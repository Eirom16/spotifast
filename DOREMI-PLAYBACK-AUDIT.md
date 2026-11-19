# Auditoría de reproducción Doremi

> Actualización pre-merge 2026-11-18: este documento conserva el corte histórico
> y añade la sección 11 con las pruebas nuevas. Para el veredicto actual y sus
> límites, consultar también `DOREMI-PREMERGE-REPORT.md`. No atribuir las
> sesiones históricas a artefactos del nuevo head.

Fecha de cierre de este corte: 2026-11-15. Rama local:
`integration/doremi-playback-audit`.

## Veredicto

**No lista para integrar a `main`. Misión parcialmente implementada, no
completada en términos de rendimiento o reproducción real.**

Se consolidaron las correcciones, se instrumentó la ruta YouTube del host,
se retiró la red del mutex compartido de catálogo/resolución y se implementó
prefetch de una resolución. Hay pruebas offline aprobadas, no una medición
antes/después de música real. No se afirma haber reducido los diez segundos,
alcanzado las metas p50, escuchado audio ni validado paridad de plataformas.

No hubo push, release, merge commit, modificación de `main`, lectura de
cookies para QA, importación de cuenta ni cambios a bibliotecas del usuario.
No se añadieron crates, fuentes git, ajustes ni cambios estéticos propios.

## 1. Procedencia y consolidación

SHA completos comprobados con `git ls-remote origin` al comenzar:

| Referencia | SHA |
| --- | --- |
| `main` local/remoto | `cd40da022edd35d6017b3093421b8b83071722a6` |
| producto local, atrasado | `0399ee9be8cf8ce15168ba5053b847d02e98a9d4` |
| producto remoto, base efectiva | `4cdcce602dd90c7396800a82b7cb16224aa42cc8` |
| correcciones remoto | `cc8a755f40a76b950dd3d2c233f9603610ffb718` |

El checkout inicial estaba limpio en `main`. Se creó la integración desde el
producto remoto, no desde la copia local ni desde la rama antigua de fixes.

| Commit fuente | Resultado local | Decisión |
| --- | --- | --- |
| `6dc3430` | `1f445a7` | Query `range`, octetos firmados conservados, 200/206 acotados, bloques parciales no reutilizables, probe nativo y pruebas |
| `465ae2c` | omitido por redundancia | El producto ya importaba `UI_TEST_LOCK`. Conflicto resuelto conservando su archivo, sin un commit vacío |
| `bd0777c` | `8fa4ec3` | Bootstrap público comprimido y acotado, presupuesto de resolución consistente, offsets de diagnóstico seguros |
| `cc8a755` | `b8eced6` | Supervisor Python/yt-dlp, protocolo privado, Deno y retiro de descendientes, desktop sin fallback tras negativa |

README se fusionó automáticamente. No se copiaron versiones completas antiguas
de archivos UI. Se conservaron las mejoras de catálogo, navegación, sesión,
carátulas, historial, biblioteca, playlists y transporte de la base de producto.
Esto describe preservación del código, no validación funcional de cada vista.

Durante la misión el producto remoto avanzó otros tres commits:
`4a6a619`, `91d563f`, `cd8927f7321a5f1a51f1882c6af4c529f9a0fe9c`.
Se inspeccionaron sus diferencias: QA de geometría/perfiles Android y extracción
de controles de apariencia. **No se incorporaron a esta base congelada**, para
no mezclar un cambio visual concurrente con streaming. No se reemplazó ese
trabajo ni se modificó su rama. Antes de publicar/integrar, actualizar esta
rama mediante una integración revisada de esos commits o aplicar nuestros
commits sobre el nuevo producto. No hacer un push que retroceda el producto.

## 2. Arquitectura implementada

### Sesión y catálogo

`Client::fork` comparte el pool reqwest, la sesión inmutable mediante `Arc` y
el programa público cacheado. No vuelve a serializar cookies para cada cliente.
Cada worker conserva su diagnóstico y sus modificaciones de metadatos.

Catálogo y resolvedor capturan el fork bajo mutex, hacen su IO fuera del
mutex y publican mediante `Client::complete`. Una identidad de sesión opaca
invalida resultados anteriores a login/logout, incluso el caso invitado →
login → logout → invitado. Un fork no puede restaurar una sesión retirada.
La publicación no degrada un programa ya preparado con una copia más vieja.
Thumbnails se fusionan en la caché acotada original.

**Límite:** el worker de login/restauración sigue verificando cuenta bajo el
mutex. La primera reproducción puede esperar esa verificación. No se eliminó
`account()` por petición ni se redujo su garantía de acceso por intuición.

### Relojes correlacionados

`Trace` es memoria acotada, sin ID musical, URL, cookies o texto de proveedor.
El host liga el objeto al `RequestId`; la intención de cola proporciona
el origen monotónico a resolvedor, lector y `Engine::load_stream_since`.
El diagnóstico de audio expone también su request para rechazar mezcla de
una carga nueva con métricas de la pista anterior.

Etapas disponibles:

1. Intención recibida por el host de cola, origen cero.
2. Entrada en la cola pendiente de envío al actor.
3. Recepción por el task del worker.
4. Inicio de resolución.
5. Inicio/fin de espera del cliente compartido.
6. Inicio/fin de extracción, tras la comprobación de cuenta en desktop.
7. Concesión validada estructuralmente y con lease vigente. **No prueba HTTP
   de acceso al audio** ni adelanto artificial de `Ready`.
8. Primer GET de audio.
9. Primer bloque completo validado disponible para el decoder. No es primer
   byte del socket; la lectura sigue esperando un bloque de hasta 64 KiB.
10. Primer PCM preparado por el feed del motor.
11. Descubrimiento del dispositivo y salida construida. El intervalo total
    incluye apertura/decodificación/prebuffer, no solo coste de CPAL.
12. Primer frame entregado y primera muestra post-gain/conversión no silenciosa
    en el callback. Silencio inicial o volumen cero no cuentan como no-silencio.

No se cambió `Ready` para anticiparlo. Las etapas no observadas son `null`,
no cero ni éxito. Un hit de prefetch no inventa una extracción posterior al
click. Otros proveedores conservan sus relojes anteriores: no mezclarlos
con el origen completo de YouTube.

**Falta:** medir input físico → intención del host, subdividir bootstrap,
cuenta, importación Python y Deno; medir resume con un reloj nuevo. Resume
no crea otro request, por lo que no debe reutilizar el tiempo de la carga
original. Tampoco se mide la latencia física del SO/DAC/altavoz.

### Estadística reproducible, no telemetría

`scripts/playback-latency-summary.py` captura solo campos permitidos de un
snapshot y resume JSONL. Rechaza requests mezclados, duplicados, secretos
como campos extras, valores negativos y relojes inconsistentes. Separa
commit, plataforma, escenario y evidencia sintética/real. Informa n,
cancelaciones, tasa de no-éxito, p50/p95/mínimo/máximo por timestamp.
Usa nearest-rank y marca menos de veinte éxitos como exploratorio.

La captura es explícita por el operador. Registrar un snapshot final por
intento, incluidos errores/cancelaciones, antes de iniciar el siguiente.
Los snapshots intermedios de una solicitud no son muestras independientes.
Frío/caliente/primera tras login requieren protocolo externo; el script no
adivina esas condiciones. Ver comandos en `crates/doremi-youtube/README.md`.

## 3. Evidencia de cuellos de botella

| Hallazgo | Evidencia y cambio | Qué NO está demostrado |
| --- | --- | --- |
| Serialización catálogo/audio | Antes, `fetch().await` y `resolve().await` retenían el mismo guard. Ahora IO en forks y publicación breve | Su fracción de los ~10 s en la conexión del usuario |
| Inicialización repetida desktop | Cada resolución ejecuta Python/yt-dlp, con caché de disco desactivada | Que Python, Deno o descarga del player dominen el tiempo |
| Comprobación de cuenta | Sigue siendo una petición previa en cada resolución autenticada | Coste real o conveniencia de cachearla |
| Transporte | Un GET por bloque de 64 KiB, cliente/pool reutilizado dentro del lector | RTT/throughput, tamaño óptimo o beneficio de ampliar bloques |
| Apertura/seek | Decoder puede necesitar varios bloques; seek crea nueva carga y resolución | Latencia real de seeks repetidos o reutilización segura de resolución actual |
| Buffer PCM | Capacidad 8192 frames, arranque con 4096 | Que cause underruns evitables en música/red reales |

No es posible ordenar los costes por dominancia sin trazas reales. La única
optimización de contención se apoya en la regla de propiedad observable del
código, no en atribuirle artificialmente todo el retraso. No se adoptó un
extractor persistente ni se ajustaron buffers a ciegas.

## 4. Latencia antes/después

Los ~10 s son una observación proporcionada en la misión, sin corpus,
denominador, percentiles ni origen correlacionado. **No son un p50 medido.**

| Escenario | n real antes/después | p50 antes/después | p95, mínimo, máximo | Tasa de fallos |
| --- | --- | --- | --- | --- |
| Frío | 0 / 0 | N/D / N/D | N/D | N/D |
| Caliente | 0 / 0 | N/D / N/D | N/D | N/D |
| Primera tras login | 0 / 0 | N/D / N/D | N/D | N/D |
| Siguiente | 0 / 0 | N/D / N/D | N/D | N/D |
| Antes reproducida | 0 / 0 | N/D / N/D | N/D | N/D |
| Preparada | 0 / 0 | N/D / N/D | N/D | N/D |
| Reanudar | 0 / 0 | N/D / N/D | N/D | N/D |
| Seek | 0 / 0 | N/D / N/D | N/D | N/D |

No hay evidencia de mejora cuantitativa de música real en este corte.
Los veinte números del test estadístico son datos de prueba de nearest-rank,
no tiempos medidos. Las cuarenta muestras del harness son HTTP sintético.

## 5. Estado del prefetch

- Máximo una concesión, solo en memoria y solo YouTube. No hay una nueva
  caché persistente ni descargas de audio en segundo plano.
- Se prepara al entrar en los últimos 30 s de una pista con duración conocida.
  `Queue::next_on_end` predice la continuación sin mutar selección, incluyendo
  repeat, shuffle, manual y forward-history. Prueba diferencial contra EOF.
- Deadline de 15 s, retención de 45 s y margen mínimo de lease de 5 s.
  El actor retira concesiones viejas aun sin otro comando.
- Generaciones independientes. Cambios de cola, pausa, seek, suspensión y
  sesión invalidan la preparación. Primero se consume una preparación válida
  para una nueva carga coincidente; después se invalida el antiguo candidato.
- Un Source explícito aborta trabajo especulativo antes de resolver. Se
  conserva una concesión entre reintentos del canal lleno, sin perderla ni
  etiquetar como hit una resolución de reemplazo.
- No hay bucle de refresco/reintentos de prefetch. Un fallo no selecciona
  otra pista, cliente o proveedor ni publica `Ready`.

Límites: no prepara saltos tempranos, duraciones desconocidas ni Commons.
No hay buffer comprimido anticipado, ni decoder/output preparado de antemano,
ni garantía de transición <1 s. En Android usa el camino Rust existente,
no Python/Deno; su coste y cancelación física necesitan APK y dispositivo.
El proceso Python cancelado se retira de forma supervisada, pero el permiso
puede seguir ocupado hasta ese retiro, no una promesa de cancelación instantánea.

## 6. Streaming, memoria y recursos

Se conservan 64 KiB por bloque HTTP más un byte de detección de exceso
(65.537 B de capacidad propia del lector) y 8192 frames PCM estéreo f32
(65.536 B). El prebuffer de 4096 frames representa ~85,3 ms a 48 kHz.
Son cálculos estáticos de buffers propios, **no RSS medida**. No incluyen
reqwest/TLS, Symphonia/MP4, pila, thumbnails, Python, QuickJS o Deno.
No se aumentaron buffers ni se implementó buffering adaptativo sin experimento.

Prefetch retiene una URL acotada a 32 KiB, user-agent hasta 4 KiB y metadatos
pequeños. No retiene audio. El programa público se comparte por `Arc`.
QuickJS conserva sus límites anteriores: heap 64 MiB, stack 512 KiB y
presupuesto de ejecución 8 s. No equivalen a un límite de RSS del proceso.

**CPU/RAM de reproducción:** no medidas. No presentar memoria del compilador
ni datos de antiguos informes como consumo del nuevo producto.

Riesgo abierto: HTTP bloqueante tiene timeout total de 6 s. La cancelación
se comprueba entre lecturas, pero una lectura ya bloqueada puede tardar hasta
su timeout y retener al worker de audio. No se ha demostrado recuperación
con red lenta, cortes, seeks rápidos o ausencia de underruns físicos.
El callback sigue sin red, asignaciones pesadas, mutex ni joins.

## 7. Validación por plataforma

| Plataforma | Ejecutado en este corte | No ejecutado / límite |
| --- | --- | --- |
| Linux | Compilación de metadatos del host/resolvedor/player, Clippy, core, 43 tests YouTube, 19 player, checks Python | Sin dos canciones nuevas de 15 s, salida física, captura acústica, ni medición de latencia real. Tests del host compilados por Clippy, no ejecutados |
| Windows | Inspección de cfg y workflow preparado; evidencia previa de CI del fix fuente | No compilación/ejecución del commit nuevo en Windows, ni dispositivo físico |
| Android | Núcleo puro y contratos locales; sesión física 2026-11-15 con el APK base `4cdcce6` (sha256 verificado contra lo instalado) en SM-J701M/Android 9: viaje ADB `original-native` y `pause` superados, 0 xruns, PSS estable, y perfil normal estable salvo checkpoint de cola y re-sello de sesión (causa fundamentada en código, ver 7.1); 2026-11-18: control base con red que materializó una vez el timeout de 6 s con negativa visible y, con el APK de integración `b4b9cc0` (sha256 verificado contra el artefacto CI y contra lo instalado), pasada completa con dos pistas Commons, pausa/seek/resume, ciclo de vida y perfil preservado | Sin dos pistas YouTube de 15 s (autorización explícita pendiente), sin captura acústica ni latencia click-to-callback; el ciclo de vida de Android destruye el renderer en `onPause`, así que la pausa en segundo plano se valida por el camino destruir/restaurar, no aislada de la pérdida de foco |

No se invocó el probe con una sesión real ni se eligió arbitrariamente una
pareja YouTube fuera de un corpus autorizado en el repositorio. El corpus
existente contiene Commons, no dos IDs YouTube aprobados. El probe integrado
acepta dos IDs seleccionados explícitamente por el operador y exige 15 s de
PCM por pista y pausa/reanudación, pero no reemplaza la ruta de UI/prefetch.
No se atribuye a estas modificaciones la reproducción física histórica de
Commons ni los problemas de acceso YouTube documentados previamente.

### 7.1 Sesión física Android (APK base `4cdcce6`, 2026-11-15)

Evidencia fuera del árbol:
`/home/eirom/.local/share/doremi/product-qa/2026-11-15/playback-android-4cdcce6/`
(el guion del viaje queda copiado dentro de cada corrida como `qa-script.py`;
la copia del repositorio es `scripts/android-playback-device-qa.py`).

Dispositivo: Samsung SM-J701M, Android 9 (SDK 28), `armeabi-v7a`. El APK
instalado se verificó por `pm path` + `sha256sum` contra el artefacto:
`8ed178daec1ef8b52f76d882288a8b4b359c3ad912aa61ef7a9ad11317ef7f86`
(`installed_apk_matches: true`). Es el APK del commit base de producto
`4cdcce6`, **no** del commit de integración `23b1a5f`
(`integration_apk_available: false`): la sesión valida el punto de partida y
el arnés QA físico, no los cambios de esta rama.

**Viaje ADB (`adb-playback/`):**

- `original-native` (demo, 15 s por UI, camino Rust nativo): superado. Primer
  PCM propio observado a 553.340 µs desde el comando ADB; primer callback a
  665.608 µs. El alcance temporal es «comando ADB → primera instantánea no
  silenciosa», no click-to-callback. 741 callbacks, 726.728 frames,
  1.452.986 muestras no nulas, 0 xruns reportados, `buffering=false`,
  salida 48 kHz.
- `pause`: superado con `position_ms=17620`.
- PSS total 57.422 → 53.440 KiB durante los 15 s (pid 12456); CPU
  411 → 828 ticks.
- El resto del viaje abortó con `Own QA surface geometry changed; no input
  injected`: la aserción de geometría del arnés (superficie QA estable por
  captura) falló con el editor de búsqueda enfocado (`input.focused: true`
  en `failure-state.json`), coherente con el IME desplazando el contenido.
  Es una propiedad de seguridad del arnés —no inyectar toques contra una
  superficie desplazada—, no un defecto del producto. La aserción demostró
  ser demasiado estricta para flujos con teclado y se relajó en
  `scripts/android-playback-device-qa.py`: los cambios de escala o anchura
  abortan (invalidan el mapeo de coordenadas) y el cambio de altura del IME
  se tolera y se registra como `geometry_events` en el reporte; la
  validación de rect por tap contra el viewport vivo permanece, y
  `control()` sigue exigiendo un rect asentado en dos lecturas. La lógica
  tiene verificación offline (`scripts/test-android-device-qa-geometry.py`,
  5 casos, sin dispositivo); el viaje completo con este arnés relajado está
  pendiente de validación en el dispositivo.

**Corridas instrumentadas (`instrumentation-failed/`, `music-chopin/`):**

- `instrumentation-failed/`: `FAIL: tap:row-more:dawn` — el control
  `row-more:dawn` no estaba presente en la instantánea QA al tocar;
  `qa-input-delivery.json` retiene la entrega del último toque y el destino
  intentado.
- `music-chopin/`: la búsqueda «Chopin» llegó a `search_status: "Ready"` con
  resultado seleccionable (`select:26896924` y `save:26896924` visibles),
  pero `saved_media` permaneció vacío: el guardado nunca se confirmó. Fallo
  registrado: `AssertionError: Not observed: fresh Rust state; current=dawn
  playback=Paused position_ms=0`.

**Perfil normal (hashes de tres capturas; `normal-profile-before.sha256`,
`adb-playback/normal-{before,after}.sha256`):**

| Fichero | Inicio de sesión | Antes del viaje ADB | Tras el viaje ADB |
| --- | --- | --- | --- |
| `files/library-v1.json` | `b5d5ead5…` | `b5d5ead5…` | `b5d5ead5…` |
| `files/playlists-v1.json` | `0312125d…` | `0312125d…` | `0312125d…` |
| `files/history-v1.json` | `75a13620…` | `75a13620…` | `75a13620…` |
| `files/preferences-v1.json` | `17fc71ed…` | `17fc71ed…` | `17fc71ed…` |
| `files/queue-v1.json` | `18233f90…` | `eeceda87…` | `829b454d…` |
| `files/youtube-session-v1.bin` | `40095817…` | `5e50f35d…` | `5e50f35d…` |

El reporte marca `normal_profile_preserved: false` por exigir igualdad
estricta de todos los ficheros. La lectura correcta, fundamentada en código:

- Biblioteca, playlists, historial y preferencias quedaron **bit a bit
  estables** en las tres capturas; la biblioteca conservada se corrobora en
  `music-chopin/normal-library-preserved.json`.
- `queue-v1.json` es el checkpoint de cola: el trabajador de persistencia
  (`crates/doremi-library`) solo escribe cuando los bytes serializados
  difieren, y el checkpoint lleva `request_watermark`, `position_ms` y
  `next_entry` (`crates/doremi-core/src/queue/checkpoint.rs`). Un lanzamiento
  que restaura la cola y una reproducción la avanzan: deltas esperados.
- `youtube-session-v1.bin` es el almacén cifrado de la sesión
  (`android/app/src/main/java/dev/eirom/doremi/YoutubeSessionStore.java`):
  AES/GCM con `setRandomizedEncryptionRequired(true)`, es decir, un IV
  aleatorio de 12 bytes nuevo por escritura, y el actor de sesión Rust
  re-sella la cookie tras cada `sign_in` exitoso, incluida la restauración
  por lanzamiento (`crates/doremi-app/src/youtube_session.rs`,
  `credentials::store` después de validar; Android llama
  `session.restore()` en cada creación de renderer,
  `crates/doremi-app/src/android_native.rs`). Por diseño, texto claro
  idéntico produce cifrado distinto en cada lanzamiento: el cambio de hash
  entre lanzamientos no indica deriva de datos de sesión. No se descifró ni
  volcó el fichero; la verificación de contenido se limita a este análisis de
  código y a la estabilidad de los demás ficheros.

**Qué valida y qué no la sesión física:** valida el arnés QA físico y la
reproducción demo por camino Rust nativo con audio no silencioso y pausa con
posición persistida. La tercera sesión (abajo) valida además el APK del commit
de integración con dos pistas Commons, seeks y ciclo de vida, sin alterar los
datos reales del usuario. No valida dos pistas YouTube de 15 s, ni captura
acústica, ni latencia click-to-callback; el camino Python/Deno no aplica en
Android por diseño.

**Segunda sesión (2026-11-18, arnés relajado, sin red en el dispositivo):**

Evidencia: `/home/eirom/.local/share/doremi/product-qa/2026-11-18/playback-android-4cdcce6-relaxed/`
(perfil `playback-adb-20261118`). Mismo dispositivo; el artefacto base
`4cdcce6` se re-descargó del run Android `43742870337` y se verificó byte a
byte contra el sha256 retenido antes de ejecutar.

- El arnés relajado superó la barrera del 15-N: completó el smoke
  (`original-native` 15 s con `first_pcm_us=1.267.923`,
  `first_callback_us=1.374.769`, 748 callbacks, 0 xruns, PSS
  56.448→56.985 KiB; `pause` en `position_ms=17.380`) y llegó a escribir
  «Chopin» en el buscador con el IME abierto y cerrado correctamente: la
  aserción de geometría ya no aborta la jornada.
- La búsqueda terminó en `search_status="Failed(Transport)"` con aviso
  visible «Operación cancelada.»: **el dispositivo no tenía red** — Wi-Fi
  deshabilitado (`wifi_on=0`, `Active default network: none`, `ping`
  inalcanzable). Es la negativa visible correcta ante red inaccesible, no
  una sustitución silenciosa de proveedor. La pasada Commons completa sigue
  pendiente de una ventana con red.
- Perfil normal: solo cambió `queue-v1.json` (`fb87503c…`→`89fee5de…`); los
  otros cinco ficheros quedaron bit a bit estables. La cola real se verificó
  por lectura directa en el dispositivo: 6 entradas de YouTube Music
  intactas (current «Saquen Bocina», `next_entry=805`, `position_ms=0`,
  `request_watermark=1231`). El `normal_profile_preserved: false` retenido
  corresponde a la regla estricta anterior al refinamiento. La regla final
  exige igualdad byte a byte de los cuatro ficheros de datos (library,
  playlists, history, preferences), contenido idéntico del checkpoint (todo
  salvo `position_ms`, `next_entry` y `request_watermark`, contabilidad
  volátil documentada) y un sobre de sesión válido de tamaño estable (el
  re-sello GCM cambia los bytes por diseño). Registra
  `queue_checkpoint_delta` y `session_resealed`. Se aplicó en la tercera
  sesión con hashes idénticos en la ventana capturada; el caso de re-sello
  está cubierto por los tests offline. Verificación:
  `scripts/test-android-device-qa-profile.py` (13 casos) y
  `scripts/test-android-device-qa-geometry.py` (5 casos).

**Tercera sesión (2026-11-18, APK de integración y red activa):**

Evidencia:
`/home/eirom/.local/share/doremi/product-qa/2026-11-18/playback-android-4cdcce6-full/`
(control con el APK base), `playback-android-b4b9cc0-full/` (primera pasada
de integración, que llegó al ciclo de vida con un fallo del arnés) y
`playback-android-b4b9cc0-full2/` (segunda pasada, `pass`). Perfiles
`playback-adb-20261118-base`, `-int` y `-int2`.

- **Artefacto e instalación.** `android.yml` `44342809444` (success sobre
  `b4b9cc0`) publicó `doremi-android-dev-b4b9cc0…`; la descarga desde el
  blob de Actions falló repetidamente por resets de conexión y solo completó
  al cuarto intento del bucle. El manifiesto declara
  `source_commit=b4b9cc0b6a5…`, `source_dirty=false`, `release=false`; el
  sha256 del APK es `4d77b2f9…`. El viaje verifica además el sha256 del APK
  **instalado** (`pm path` → `sha256sum`) antes de inyectar entrada, así que
  la sesión no puede atribuirse a otro binario.
- **Control con el APK base `4cdcce6`.** El smoke y la búsqueda «Chopin»
  funcionaron (`search_status="Ready"`), pero al seleccionar `26896924` el
  primer rango de audio agotó el presupuesto total de 6 s
  (`REQUEST_TIMEOUT_MS`) con `retired_network {requests:1,
  received_bytes:0}` y el producto pasó a `Failed` con el aviso visible «La
  fuente agotó el tiempo de espera. Puedes volver a seleccionar la pista.».
  No se cambió el timeout a ciegas: minutos después, el mismo dispositivo y
  la misma red descargaron las dos pistas Commons completas; el enlace medía
  7,1 MiB/s por TCP contra el host, el puerto 443 de `upload.wikimedia.org`
  conectaba y el host obtenía `206` en 0,19 s con la petición exacta. Fue un
  atasco transitorio de la primera petición tras habilitar Wi-Fi, no un fallo
  reproducible; la negativa visible es la conducta correcta y la cola real
  quedó intacta (`request_watermark [1233,1234]`).
- **Pasada de integración (segunda ejecución, `pass`).** Smoke
  `original-native` (`first_pcm_us=196.444`, 0 underruns, 1 xrun reportado en
  el frame 0, PSS 58.089→58.742 KiB) y `pause` en 17.516 ms. Después, por la
  UI: búsqueda «Chopin» → `select:26896924` (observado en 4.820 ms,
  `first_pcm_us=2.037.545`), 15 s con 50 underruns; pausa, seek a 83.858 ms,
  resume. Búsqueda «Mendelssohn» → `select:17687586` (3.189 ms,
  `first_pcm_us=1.202.407`), 15 s con 33 underruns; pausa, seek a 65.054 ms,
  resume. Ambas pistas son CC0 de Commons y el motor reporta
  `streaming=true`, proveedor `wikimedia-commons`.
- **Ciclo de vida.** En Android, `MainActivity.onPause` ejecuta
  `nativeDestroy()` y el QA state deja de dibujarse: la espera de `Paused`
  antes de volver a primer plano leía un estado congelado y falló en la
  primera pasada (`full/`). El arnés corregido vuelve a primer plano, exige
  el mismo pid, `Paused` y posición no reiniciada: `foreground-paused` con
  pid 7792, `position_ms=66.654`, aviso «Operación cancelada.»; tras
  `force-stop` + relanzamiento, pid 9288, `Paused`, `audio_metrics=null`,
  `worker_initialized=false`, aviso «Sesión restaurada en pausa.» — la
  restauración no inicia red. La pausa en segundo plano se valida por el
  camino destruir/restaurar del renderer nativo; la rama de pérdida de foco
  (`native_focus`) no se aísla en esta prueba.
- **Perfil normal.** La primera pasada de integración cambió
  `youtube-session-v1.bin` (`5a79f4c8…`→`0ee9c7bd…`), y solo ese fichero; el
  relanzamiento normal de limpieza del arnés restaura la cookie y la
  re-sella. La causa está en código: tras restaurar la cookie, `sign_in`
  re-sella con `credentials::store` y `YoutubeSessionStore` usa AES-GCM con
  un IV de 12 bytes nuevo (`MAGIC 'DYM',1`), así que los bytes cambian por
  diseño aunque el contenido sea el mismo. La regla se corrigió: los cuatro
  ficheros de datos exigen igualdad byte a byte; la cola exige contenido
  idéntico y conserva sus campos volátiles; la sesión exige un sobre válido
  (magic, tamaño ≥32) de **tamaño estable** y registra `session_resealed`
  cuando el digest cambia. La segunda ejecución terminó con
  `normal_profile_preserved: true`, hashes antes/después idénticos y
  `queue_checkpoint_delta {request_watermark: [1237,1238]}`.
- **Frescura del perfil.** El chequeo `run-as … test -e` devolvía 1 incluso
  para un perfil existente en este dispositivo («Permission denied»), así
  que la aserción era vacua. Se sustituyó por
  `run-as … sh -c 'test -e …'`, validado en vivo (existente=0, ausente=1).

## 8. Checks y CI

### Aprobados localmente

- `python3 scripts/check-local.py`: políticas, inventario, formato, core,
  biblioteca y decoder básico. Core: 37 tests + 1 doctest; biblioteca: 22.
- `python3 scripts/test-provider-harness.py`: 40 casos HTTP sintéticos.
- `python3 scripts/test-playback-latency.py`: 4 tests de captura/estadística.
- `python3 .github/scripts/test-policy-checks.py`: 27 tests.
- `cargo test --locked -p doremi-core`.
- `cargo test --locked -p doremi-youtube --lib`: 43 aprobados, 2 ignorados
  (llavero nativo y programa público suministrado explícitamente).
- `cargo test --locked -p doremi-player --features native-output,ogg-vorbis,aac-mp4 --all-targets`:
  19 aprobados, no necesitan salida de audio física.
- `cargo check --locked -p doremi-youtube --lib`.
- `cargo check --locked -p doremi-player --features native-output,ogg-vorbis,aac-mp4 --lib`.
- `cargo check --locked -p doremi-app --lib`.
- `cargo clippy --locked -p doremi-app --all-targets -- -D warnings`.
- `cargo clippy --locked -p doremi-youtube --all-targets -- -D warnings`.
- `cargo clippy --locked -p doremi-player --features native-output,ogg-vorbis,aac-mp4 --all-targets -- -D warnings`.
- `cargo fmt --all --check` y `git diff --check`.

Hubo invocaciones locales interrumpidas por el límite del terminal durante
compilación fría (200/600/300 s). No se contaron como tests aprobados.
Posteriormente las suites YouTube/player anteriores terminaron. El coste
confirmó que la validación pesada restante debe hacerse en Actions, no seguir
reconstruyendo el host completo localmente.

### Evidencia CI ajena al nuevo commit

- Fix `cc8a755`: runs `43625343199` (CI) y `43625343053` (Android), ambos success.
  No acreditan la combinación ni los cambios nuevos.
- Base producto `4cdcce6`: Android development `43742870337`, success.
- CI `43742870291` de esa base terminó **cancelled**; `full-tests` y
  `preview-linux` sí habían terminado success. El paso de producto prueba
  384×760, 682×360 y 1100×800. El fallo anterior `product-ui-682x360` de
  `43627743182` fue timeout de estado esperado, no un error de audio.
  No sigue fallando en ese paso de `4cdcce6`. No se rediseñó UI para ocultarlo.

### CI del commit de integración (2026-11-18, push autorizado)

- `ci.yml` run `44251398160` y `android.yml` run `44251402256`, ambos
  **success** sobre `79538c3` (`integration/doremi-playback-audit`). Cubren
  el código de esta rama hasta ese commit; los commits posteriores solo
  cambian scripts de QA y documentación, no el código del producto.
- `playback.yml` no se pudo disparar: GitHub exige que el fichero del
  workflow exista en la rama por defecto y solo existe en esta rama (se
  añadió en `fede768`). Será dispatchable tras integrar en `main`; mientras
  tanto, sus suites se ejecutan dentro de `ci.yml`.
- `android.yml` run `44342809444` (success sobre `b4b9cc0`) publicó
  `doremi-android-dev-b4b9cc0…`; manifiesto y sha256 (`4d77b2f9…`)
  verificados antes de instalar, y el viaje comprobó el sha256 del APK
  instalado. La prueba física está en 7.1.
- `ci.yml` run `44342784124` sobre `b4b9cc0`: `quick`, `clippy`,
  `full-tests`, `android-core`, `android-apk` y `desktop-build` (Windows y
  macOS) **success**; `preview-linux` fue cancelado por su
  `timeout-minutes: 10` en los tres intentos y `verified-artifacts` falló
  como gate al faltar ese reporte. No es un fallo del código: en el run
  verde `44251398160` el primer paso de `preview-linux` (solo
  `apt-get update/install` + `chmod`) consumió 5m57s de los 10 min; en
  `44342784124` los intentos 1 y 2 murieron en el empaquetado y el 3 en ese
  mismo paso. El delta con `79538c3` es solo scripts/docs. No se debilitó
  ningún test ni se amplió el presupuesto sin revisión.

Se añadió `playback.yml`, manual, Linux/Windows, solo core/resolvedor/player y
estadística offline. Evita compilar Spotifast, GTK/WebView y el host gráfico
para iteraciones aisladas de esos crates. **No sustituye** `ci.yml` ni
`android.yml`; este cambio de host necesita además ambos gates completos.
No se debilitaron tests ni permisos. La allowlist de workflows se amplió
explícitamente y se añadió regresión que conserva el gate completo.

### Comandos pendientes, solo después de autorizar publicación de esta rama

Antes, incorporar/revisar el avance concurrente del producto y ejecutar los
checks de nuevo. Confirmar remoto `origin` y revisar exactamente qué commits
van a publicarse:

```sh
git log --oneline origin/feature/doremi-product-ui..integration/doremi-playback-audit
git diff --stat origin/feature/doremi-product-ui...integration/doremi-playback-audit
# Únicamente con autorización explícita:
git push origin integration/doremi-playback-audit
gh workflow run playback.yml --repo Eirom16/spotifast --ref integration/doremi-playback-audit
gh workflow run android.yml --repo Eirom16/spotifast --ref integration/doremi-playback-audit
gh workflow run ci.yml --repo Eirom16/spotifast --ref integration/doremi-playback-audit
```

No hay comandos para aterrizar en `main`. Una CI completa verde tampoco
supliría la autorización específica de landing ni la prueba física.

## 9. Distribución y seguridad

Desktop requiere Python 3.11+, yt-dlp >=2026.10.26 con yt-dlp-ejs y Deno
en PATH. En esta máquina se comprobaron yt-dlp 2026.10.26 y Deno 2.9.8;
no es una matriz de versiones soportadas. El bundle actual no los incluye.
FFmpeg no es el reproductor ni una nueva dependencia productiva de esta ruta:
el audio AAC se decodifica en Rust. Linux además requiere los backends nativos
documentados; Windows usa CPAL y Credential Manager.

Android no presupone Python/Deno. Conserva resolver Rust, bootstrap público,
QuickJS y TLS con raíces webpki. No se sustituyó una denegación por otro
cliente ni se añadieron tokens de atestación, bypass de DRM o verificación.

Supervisor existente integrado: argv fijo sin shell, Python aislado,
stdin/stdout privados, entorno acotado, configuraciones/plugins personales
deshabilitados, límites stdout/stderr y proceso/grupo o Job Object retirados
al cancelar. Las pruebas de procesos usan Python y datos sintéticos.
No se midió RSS del árbol Python/Deno ni se demostró distribución autónoma.

Las nuevas trazas no incluyen secretos ni cambian formatos del llavero,
Keystore, playlists o biblioteca. No se ejecutó el round-trip del llavero
real. Es imprescindible una revisión de concurrencia y prueba de cambio de
cuenta mientras catálogo, reproducción y prefetch están activos.

## 10. Commits locales y pendientes por prioridad

Commits creados antes de este informe:

1. `1f445a7`: rangos y probe.
2. `8fa4ec3`: presupuesto/bootstrap.
3. `b8eced6`: supervisor yt-dlp.
4. `824bf5a`: trazas del proveedor/motor y forks ligados a sesión.
5. `a59e9ad`: host independiente y prefetch de una resolución.
6. `fede768`: captura privada/estadística y CI manual enfocada.

El commit documental final se identifica con
`git log -1 --format='%H %s' -- DOREMI-PLAYBACK-AUDIT.md`.
No había identidad Git efectiva en este clone. Se usó por comando la identidad
de los commits Codex existentes, `Codex agent <codex@localhost>`, sin cambiar
configuración global ni persistir una configuración nueva.

### P0: antes de recomendar integración

- CI del commit exacto combinado, incluyendo tests del host, APK y legado:
  `android.yml` está verde en `b4b9cc0` y los jobs de código de `ci.yml`
  también; `preview-linux` agota su presupuesto de 10 min en `apt-get` (ver
  8) y deja el gate `verified-artifacts` en rojo. Repetir el disparo sobre el
  head final y revisar el presupuesto de ese job.
- Dos pistas YouTube explícitamente autorizadas, 15 s de PCM no silencioso
  cada una por la UI/producto, pausa/resume/seek/next/previous y prefetch.
- Al menos veinte intentos por escenario, mismo corpus y red, línea base y
  candidato. Conservar todos los fallos. Ordenar costes por trazas reales.
- Probar cancelación durante red/extracción y saltos rápidos, sin audio obsoleto.
- Revisar/integrar los tres commits concurrentes de producto, sin reemplazar UI.
- Prueba física Windows y APK Android **del commit de integración**. El APK
  `b4b9cc0` ya se instaló y pasó el viaje completo en 7.1 (dos pistas
  Commons, seeks, ciclo de vida y perfil preservado); falta Windows y las dos
  pistas YouTube con autorización explícita. El ciclo de vida se validó por
  el camino destruir/restaurar del renderer; la rama de pérdida de foco no se
  aisló.

### P1: rendimiento y cobertura pendientes

- Reloj de resume y subdivisión de bootstrap/cuenta/Python/Deno.
- Comparación equivalente Rust vs yt-dlp, CPU/RSS del árbol y tasa de fallos.
  No probar otra ruta automáticamente después de una negativa de acceso.
- Probar hits caducados, canal lleno, timeout y reordenamientos con prefetch
  completo mediante fixtures de host, no solo compilación de sus tests.
- Medir 64 KiB frente a alternativas y latencia de lecturas canceladas.
  Valorar buffering adaptativo solo con trazas de underruns/red lenta.
- Definir distribución y preflight de Python/EJS/Deno por plataforma.

### P2: siguiente misión, no implementado aquí

- Servicio multimedia/background Android. La suspensión actual retira
  prefetch y conserva el contrato de pausa al salir; no se rediseñó Activity.
- Preparación de fragmentos comprimidos/output si resolución-only no alcanza
  la meta y las mediciones justifican su memoria y coste de cancelación.

La recomendación final sigue siendo **no integrar a `main` todavía**. Este
corte entrega implementación y regresiones offline verificables, no una
certificación de streaming rápido o de reproducción multiplataforma real.

## 11. Misión pre-merge, 2026-11-18: evidencia acumulada

Se leyó primero el informe local más reciente de
`/home/eirom/.local/share/opencode/worktree/10a8af/doremi-playback-audit/`,
incluida su modificación documental sin commit. Se respaldó antes de editar.
La sección 7.2 ya contiene `playback-android-b4b9cc0-full2` y sus resultados
50/33 underruns; se conserva completa. Ningún resultado anterior se retiró.

El estado Git real era posterior al de la misión: integración `07924d5`,
producto `cd8927f` y merge local `fa669b9` con ambos como padres. Este último
incorpora los tres commits UI pendientes. La misión continúa en
`integration/doremi-premerge-validation`, un worktree nuevo. La reconstrucción
del merge con `git merge-tree` produce exactamente su árbol, sin pérdida de
resolvedor, prefetch, instrumentación, control de cuenta o perfiles QA. `main`
sigue en `cd40da0`. Los checkouts previos y sus cambios locales se preservan.

El usuario autorizó expresamente `qQzdAsjWGPg` y `_d3SaQI_Ag4`, el login QA
aislado y publicar únicamente la nueva rama y ejecutar CI. Se probó primero
sin sesión: ambas fallaron en resolución, sin primer GET de audio ni PCM.
El usuario completó después el inicio de sesión de Doremi en el Samsung,
dentro de `files/qa/premerge-youtube-20261118/`. No se exportaron secretos,
no se importó la sesión normal y no hubo fallback tras rechazos.

**Viaje autenticado inicial, APK verificado `b4b9cc0`:**

- Primera pista: fallo visible `Timeout` durante cuenta, confirmado con
  categoría `stage=account` del mismo PID. No hay muestra de éxito.
- Segunda pista: 750.637 frames a 48 kHz, 1.497.792 muestras no nulas,
  510 underruns y 0 xruns reportados; más de 15 s de PCM. Primer PCM a
  38.045.084 µs y primer callback no silencioso a 38.160.419 µs desde la
  intención de cola del host. Extracción nativa: 17.314.579 µs; primer
  GET→bloque validado: 230.267 µs; bloque→PCM: 20.447.535 µs.
- Es una sola muestra exploratoria de Android autenticado, no un p50 de
  población ni una mejora antes/después. Bloque→PCM incluye lecturas de red
  y apertura/decodificación; no demuestra veinte segundos de CPU del decoder.
  No hay todavía subdivisión cuenta/bootstrap/player/QuickJS.

Evidencia:
`~/.local/share/doremi/product-qa/2026-11-18/premerge-youtube-android-b4b9cc0-session2/`.
El viaje anterior `...-session/` se detuvo por una aserción del arnés antes de
reproducir: no se cuenta como fallo del producto ni como sesión validada.

**Repetición Commons, mismo APK y un perfil QA nuevo:**

`~/.local/share/doremi/product-qa/2026-11-18/premerge-commons-b4b9cc0/`.
Completó dos pistas, pausa, seek, resume, background y reapertura. Los
underruns fueron 296 y 215; xruns 0 y 1. Primer PCM 4.813.958/3.583.612 µs,
callback no silencioso 5.336.209/3.631.564 µs. No mezclar ese origen de motor
Commons con el de intención YouTube. El smoke local tuvo 0 underruns/0 xruns.

La investigación de `output.rs` confirma que un underrun significa callback
con PCM vacío y silencio insertado hasta el siguiente callback, no un frame
perdido ni una transición de pausa esperada. Los nuevos snapshots acotados
correlacionan aumentos del contador con frames y bytes HTTP inmóviles y
`buffering=true`. No existe captura acústica; tampoco se puede llamar fluida
a una reproducción solo porque llega finalmente a 15 s de PCM. El problema
está **reproducido y sin resolver**. No se aumentaron buffers ni timeouts.

**Corrección adicional del arnés:** la afirmación histórica de frescura en
7.2 era insuficiente. Aunque añadía `sh -c`, ADB concatena argv y perdía el
agrupamiento: el guard aún devolvía 1 para un perfil existente. Se reprodujo
con dos directorios QA (existente/ausente: 1/1), frente al comando citado
correctamente (0/1). `7c080ad` preserva ese quoting y añade una regresión que
emula la shell ADB con un directorio temporal. No se interpreta la ausencia
de esa validación histórica como evidencia positiva de perfiles frescos.

El nuevo modo `--isolated-only` no lee cola, hashes o sesión del perfil normal
y no lo relanza al limpiar. En estos viajes sus invariantes quedan
`NOT_VERIFIED`, sin sustituir los resultados históricos. El chequeo histórico
de magic/tamaño del sobre cifrado tampoco demuestra identidad del plaintext.
Una sesión cifrada del mismo tamaño puede contener otros datos; no se leyó
para intentar suplir esa limitación.

El resumen ahora calcula media y duraciones por solicitud antes de obtener
percentiles, manteniendo `null` cuando falta un extremo. Conserva errores y
cancelaciones y separa plataformas/commits. Los tests de medición, geometría
y perfil QA se ejecutan también en el job `quick` de CI principal.

La corrección de `preview-linux` conserva todos los gates, usa instalación
sin paquetes recomendados, un presupuesto de preparación de 12 min y job
de 20 min justificado con los tiempos reales de apt de `44342784124`.
No se ha convertido esa edición YAML en una aprobación CI. Los runs iniciales
del candidato son `44430719185` (CI) y `44430723595` (Android), SHA `df2ef47`;
no acreditan el guard `7c080ad` ni el cierre documental posterior.

Ver tablas actualizadas, procedencia, límites de plataforma y pasos pendientes
en `DOREMI-PREMERGE-REPORT.md`. Sigue sin demostrarse una reducción de los
~10 s históricos; la nueva muestra real es significativamente lenta. El
veredicto permanece **NOT_READY**, sin autorización ni recomendación de merge.

### 11.1 Repeticiones y corrección de apertura bajo evaluación

La serie autenticada `premerge-youtube-android-b4b9cc0-warm` añadió audio
real de la primera pista (44.481.386 µs hasta callback no silencioso) y
otra muestra de la segunda (30.116.026 µs). Los cuatro intentos válidos
fueron error/error/éxito/éxito. El quinto se interrumpió por una expectativa
incorrecta del runner sobre Previous tras >3 s: se conserva como prueba
inválida, sin atribuir reproducción equivocada al núcleo.

El host Linux `df2ef47`, bundle verificado, también entregó 15 s de PCM de
ambas identidades: callback no silencioso 41.195.791/32.220.177 µs, sin
underruns/xruns durante esa captura. La extracción tardó 15,517/15,412 s y
el intervalo bloque→PCM 25,038/16,297 s. Evidencia:
`~/.local/share/doremi/product-qa/2026-11-18/premerge-youtube-linux-df2ef47-sized/`.
No se cuentan como pruebas de seek sus aserciones iniciales incompletas.

Los commits `964e683` y `6984caa` evalúan una apertura incremental acotada
a MP4 fast-start no fragmentado: evitar recorrer colas remotas al abrir y
restaurar seek al terminar de leer cabeceras. Otros layouts mantienen su
recorrido completo. Las 21 pruebas del player y Clippy con AAC/salida nativa
aprobaron, incluyendo seek y cancelación; el beneficio real sigue pendiente
del artefacto CI, sin convertir una prueba sintética en evidencia de latencia.

La inspección real de un solo bloque por canción mostró `ftyp/moov/sidx/moof/mdat`.
El guard conservador de `6984caa` no optimizaba ese layout. Tras verificar en
Symphonia que los índices se construyen de forma lazy tanto al reproducir
como al hacer seek, `dbc841e` permite también la apertura fragmentada. La
regresión CC0 compara muestras con el recorrido completo y seek adelante a
fragmentos no leídos, atrás y EOF: las 22 pruebas y Clippy pasan. El beneficio
real sigue pendiente del APK CI nuevo. La serie base adicional
`premerge-youtube-android-baseline-paired/` conserva seis errores; cinco ya
habían obtenido un primer bloque válido sin llegar a PCM. No se ocultan ni
se reintentan las negativas de transporte.

### 11.2 Primera prueba física del candidato y CI gráfica

El APK `dbc841e`, comprobado por manifiesto/checksums y hash instalado,
se instaló con `adb install -r` conservando el login QA. La serie
`premerge-youtube-android-dbc841e-controls/` tuvo cuatro errores antes de PCM:
account, bootstrap, primer transporte sin bytes y lectura inicial incompleta
(49.152 bytes). Los dos últimos resolvieron en 35,400/12,183 s, pero no
completaron el primer bloque. El beneficio de apertura MP4 permanece
`NOT_VERIFIED`. Pausa, seek y prefetch no se aprobaron sin audio. El intento
de Stop del runner externo tenía una aserción inválida sobre el campo de
capacidad `audio`: se conserva como prueba inválida, no fallo del producto.

`ci.yml` en `6984caa`, run `44435616417`, sí terminó globalmente aprobado:
preview (3m07s), navegación del host y gate de artefactos incluidos. Esto
verifica la corrección del presupuesto gráfico para ese SHA, no acredita
CI del cierre posterior. `android.yml` en `dbc841e` aprobó APK y Windows,
pero agotó 35 min en host Linux después de 17m28s de apt. `5ac8778` reserva
50 min con preparación acotada, según apt + 18m27s de tests y 1m02s gráfico
medidos anteriormente; mantiene todos los gates y deadlines de reproducción.

### 11.3 Audio del candidato y continuidad fallida

La serie `premerge-youtube-android-dbc841e-paired/` conserva seis selecciones:
cuatro errores de resolución y dos éxitos de 15 s. Primer callback no
silencioso de las dos pistas: 21.722.790/9.122.112 µs; extracción
21.212.557/8.928.407 µs; bloque→PCM 12.933/4.729 µs. Underruns 0/0 y xruns
0/1 en esos snapshots de 15 s. No mezclarlos con los resultados históricos.

La base emparejada de seis intentos tuvo seis errores, así que no aporta
una latencia válida emparejada. Frente a los éxitos warm previos de cada
identidad, las reducciones aritméticas observadas de arranque son 51,16% y
69,71%, **n=1 por identidad/escenario, exploratorias y sin control estricto
de red**. No son prueba estadística de un p50 ni cumplimiento de 3 s. El
informe pre-merge conserva las medias, extremos, tasas y límites.

La comprobación extendida hasta 80 s falló en ambas canciones, a posiciones
40.286/52.453 ms: lecturas HTTP 200 sin datos en offsets 655360/851968 del
mismo PID. No se reintentaron grants ni se alteraron timeouts. Estado de
continuidad **FAIL**. La apertura corta observada no subsana ese fallo ni la
subalimentación anterior; el veredicto sigue **NOT_READY**.

### 11.4 Controles reales y cancelación acotada

`premerge-youtube-android-dbc841e-controls2/`: primera pista falla en cuenta;
la segunda alcanza 15 s de PCM a 26.239.672 µs de arranque, con 21 underruns
y 1 xrun. Pausa/reanudación observadas; seek con nueva generación a ~99 s
termina `Timeout` sin PCM. También hay un 403 en offset 1048576 del mismo
PID, sin correlación RequestId para asignarlo a un GET concreto. No se
sorteó esa negativa. Seek queda **FAIL** del candidato; regresión frente
a base no demostrada. Beneficio de prefetch sigue `NOT_VERIFIED`.

El segundo intento de Stop del runner también tenía expectativa inválida:
el núcleo conserva la pista en `Paused` al cancelar. La prueba específica
`premerge-youtube-android-dbc841e-cancel/` sí pasó: cancelación en resolución,
PCM retirado y ocho segundos sin publicación tardía. No demuestra HTTP
bloqueado ni liberación total de recursos. `android.yml` de `5ac8778`, run
`44441447069`, aprobó globalmente APK, host Linux y host Windows.

### 11.5 Candidato Linux verificado y comparación con la base

El bundle `doremi-native-linux-5ac8778` del run `44441447069`
(`artifact_id` 14491534025) se descargó por rangos con reanudación y se verificó
contra `SHA256SUMS`, `manifest.json` y `SOURCE_COMMIT`
(`5ac877839dba269d200b5a8da5e936bc0c2faafb`; `doremi` SHA256
`aedbf781edc3226937251facada6713ec7cd9a70ee415905ebf391de785f0c68`).
Mismo corpus autorizado, invitado, X11 y perfil aislado que la base `df2ef47`.

Dos intentos iniciales del arnés fueron inválidos por geometría: KWin limitó
la ventana a 1100x674 (pantalla 1366x768) y la comprobación exigía 1100x800,
la altura de la serie base. Se adaptó para aceptar la geometría real solo si
es estable entre dos snapshots, registrarla y seguir validando cada control
dentro del viewport. No se cuentan como fallos del proveedor.

Dos series `premerge-youtube-linux-candidate-5ac8778-latency/` y `...-latency2/`
(sin controles), agregadas en `...-pooled/samples.jsonl` conservando la
etiqueta `run`: **8 éxitos / 12 intentos**. Frío p50 11.785.000 µs (p95
12.107.000); cambios p50 14.121.000 / p95 18.268.000 µs; extracción p50
13.912.000 µs; bloque→PCM p50 1.000 / p95 2.000 µs. Underruns por éxito
0/140/106/0/0/63/4/0 y xruns 0. Fallos del mismo PID: una resolución
`stage=extractor`, una lectura HTTP 200 con 0 bytes en offset 131072 y dos 403
`stage=refused` en offset 0, respetados sin reintento.

Frente a la base Linux emparejada (6/6 éxitos): frío 56.730.466→11.785.000 µs
(79,23%), cambio p50 47.007.009→14.121.000 µs (69,96%), p95
63.210.222→18.268.000 µs (71,10%), bloque→PCM p50 22.867.555→1.000 µs
(99,996%) y extracción p50 19.206.425→13.912.000 µs (27,56%). Son series
exploratorias, en ventanas de red distintas y con un control sin descargas
concurrentes; la tasa de éxito empeoró (6/6 → 8/12). No es un p50 de población
ni acredita cumplir 3 s: el mejor arranque observado fue 11,785 s.

La serie con controles `...-candidate-5ac8778-run3/` reprodujo la segunda pista
(34.906.428 µs, 892 underruns) y el seek al 35% (~99.464 ms) terminó `Failed`
con `stage=transport` (`http=0`, 0 bytes, offset 0), sin PCM. Para descartar
una regresión se repitió la base `df2ef47` en la misma ventana
(`premerge-youtube-linux-base-controls-now/`): **2/2 errores** antes de los
controles (lectura sin datos en offset 720896 y resolución `stage=extractor`).
El fallo de seek no se demuestra como regresión; la comparación de seek queda
`NOT_VERIFIED` y el seek del candidato **FAIL**. La subalimentación persiste
en Linux (4 de 8 éxitos con underruns, hasta 140) además de Android.
