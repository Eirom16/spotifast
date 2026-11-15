# Auditoría de reproducción Doremi

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
| Android | Núcleo puro y contratos locales; `adb devices` ejecutado sin dispositivos | No APK de este commit, NDK local no instalado, sin lifecycle/ANR/memoria/audio físico de este cambio |

No se invocó el probe con una sesión real ni se eligió arbitrariamente una
pareja YouTube fuera de un corpus autorizado en el repositorio. El corpus
existente contiene Commons, no dos IDs YouTube aprobados. El probe integrado
acepta dos IDs seleccionados explícitamente por el operador y exige 15 s de
PCM por pista y pausa/reanudación, pero no reemplaza la ruta de UI/prefetch.
No se atribuye a estas modificaciones la reproducción física histórica de
Commons ni los problemas de acceso YouTube documentados previamente.

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
- No se ejecutó CI del nuevo commit: rama local sin autorización de push.
  No afirmar regresión completa Spotifast aprobada en esta integración.

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

- CI del commit exacto combinado, incluyendo tests del host, APK y legado.
- Dos pistas YouTube explícitamente autorizadas, 15 s de PCM no silencioso
  cada una por la UI/producto, pausa/resume/seek/next/previous y prefetch.
- Al menos veinte intentos por escenario, mismo corpus y red, línea base y
  candidato. Conservar todos los fallos. Ordenar costes por trazas reales.
- Probar cancelación durante red/extracción y saltos rápidos, sin audio obsoleto.
- Revisar/integrar los tres commits concurrentes de producto, sin reemplazar UI.
- Prueba física Windows y APK Android del commit exacto cuando haya hardware.

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
