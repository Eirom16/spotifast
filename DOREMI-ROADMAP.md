# Doremi: roadmap de migración

## Estado y objetivo

F0 validó el baseline y la infraestructura del fork Eirom16/spotifast.
F1A introdujo dominio/cola neutral. F1B-1 integra edición manual/confirmación
y contratos de proveedor con harness falso, no un nuevo motor.
La aplicación preservada todavía es Spotifast 0.14.2, con Spotify/librespot.
Hay APK de desarrollo instalada y validada en Android real, audio PCM local
y un flujo online experimental de dos tonos Commons CC0 en Android/Linux
(ver `DOREMI-GOAL-REPORT.md`). No hay proveedor productivo definitivo.
Preview 0.1 añade un ejecutable
offline independiente que reutiliza las vistas, no un motor musical nuevo.
Su gate de distribución/CI se registra por separado en `DOREMI-PREVIEW-REPORT.md`.
Estado del núcleo: `F1B1-REPORT.md`;
evaluación de proveedores: `TUBEFAST-ASSESSMENT.md`.
El resultado verificable, los checks fallidos/no ejecutados y los riesgos
están en `F0-REPORT.md`. Una fase no se acepta solo porque exista su código.

Objetivo: reproductor nativo Rust, egui/eframe, sin Spotify obligatorio,
búsqueda/resolución por puertos neutrales, escritorio y Android con el mismo núcleo.
La elección Rust nativo frente a yt-dlp es experimental, no anticipada.
Sin Electron, motores de navegador, backend hospedado obligatorio ni evasión
de DRM o controles de acceso. No hay sustitución de audio Spotify: los nuevos
medios tienen identidad y procedencia propias, elegidas por el usuario.

## Decisión de arquitectura

Se elige una migración por cortes verticales, no reescribir todo ni mover
130 mil líneas de una vez. El legado se ejecuta y prueba por separado hasta
que cada reemplazo cumpla su contrato. Conservar compilación no equivale a
haber conservado comportamiento.

### Workspace objetivo

| Paquete propuesto | Responsabilidad y límites | Dependencias internas |
|---|---|---|
| `doremi-core` | IDs neutrales con namespace de proveedor, metadatos, intenciones/eventos, errores, máquina de cola y estado. Sin I/O, UI, SO ni secretos. | Ninguna |
| `doremi-catalog` | Casos de uso de búsqueda y resolución, capabilities, caché temporal, cancelación y generaciones. URL de página estable separada del recurso reproducible efímero. | core |
| `doremi-player` | Transporte, buffering acotado, decodificación, reloj, seek, DSP, eventos y puertos de salida. Sin catálogo ni UI. | core |
| `doremi-library` | Biblioteca, playlists propias, historial, repositorio versionado y migraciones. No conserva URLs firmadas como IDs. | core |
| `doremi-ui` | Vistas egui, estado de presentación y emisión de intenciones. Sin procesos, descargas, disco ni decodificación. | core |
| Adaptadores candidatos | Rust nativo o yt-dlp, sujetos a benchmark y licencia. No incorporar todavía dependencias productivas; RustyPipe solo referencia. | core, catalog |
| `doremi-platform-desktop` | Procesos, rutas, selección de archivos, keyring, audio, notificaciones, media keys, tray. Expone capacidades, no finge soporte móvil. | core, puertos player/library |
| `doremi-platform-android` | JNI/Activity, contexto Android, SAF/MediaStore, audio focus, MediaSession/servicio, lifecycle, almacenamiento y runtime empaquetado. | core, puertos player/library |
| `doremi-desktop` | Composición eframe y workers. Único entrypoint nuevo de escritorio. | UI y servicios/adaptadores |
| `doremi-android` | `cdylib` + proyecto Gradle mínimo, composición eframe/Activity, sin duplicar reglas del núcleo. | Misma UI y servicios, adaptador Android |
| `spotifast` (legado temporal) | Paquete raíz actual, congelado funcionalmente; no es dependencia de los nuevos paquetes. | Árbol existente |

**F0 creó `doremi-core`, con constantes de identidad y un doctest.**
F1A añade medios/IDs, reducer, snapshots en memoria, contratos y reglas
compartidas de edición/prefijo. No dirige todavía la cola Spotify completa.
La tabla es diseño, no lista de módulos implementados. Los puertos y crates
restantes nacen con su primer caso de uso/test, no como APIs especulativas.
En F1B se evaluará centralizar versión/MSRV una vez adaptados los lectores de manifiesto
de Nix, packaging y tests. F0 conserva la versión literal raíz para no romperlos
y un test rechaza divergencias con el núcleo.

Regla de dependencia: composición → servicios/adaptadores → core.
La UI recibe snapshots y envía comandos; no conoce el motor ni yt-dlp.
Las traits de puertos viven junto al consumidor; ningún adaptador inyecta
tipos Spotify o JNI al dominio. Tests con adaptadores falsos en ambos hosts.

### Concurrencia y contratos antes de implementar

- egui consume eventos sin espera y emite intenciones mediante `try_send`.
  Backpressure es visible/coalescible, no un `send().await` ni un
  `block_on` en UI. Canales de comandos/eventos con capacidad explícita.
- Runtime Tokio para red, subprocess pipes y timers. Workers separados
  acotados para lectura de tags, imágenes, disco y decodificación bloqueante.
  El audio callback solo consume PCM ya preparado de un buffer acotado.
- Cada búsqueda/resolución lleva ID de solicitud y generación. Cancelar,
  borrar búsqueda o cambiar pista invalida respuestas tardías. Un timeout
  cancela y recoge el proceso y sus hijos, no solo deja de esperar el future.
- Drenar stdout y stderr concurrentemente, con límites independientes.
  Errores tipados: binario/runtime ausente, formato incompatible,
  red/deadline, cancelación, medio no disponible, acceso denegado y fallo
  del extractor. No reintentar denegaciones indefinidamente.
- Una referencia estable al medio no es una URL de CDN. El recurso resuelto
  incluye protocolo/formato, headers permitidos, capacidad de seek y
  caducidad cuando se conoce. Resolver cerca del inicio de reproducción.
  No loguear URLs firmadas, cookies o respuestas enteras.
- Persistir IDs/metadatos y posición, no sesiones Spotify ni secretos en
  JSON. Cambios de esquema con backup, versión y tests de rollback.

### Preservación y sustituciones

| Capacidad heredada | Dependencias identificadas | Estrategia y prueba requerida antes de retirar |
|---|---|---|
| Cola, next/previous, shuffle/repeat, duplicados, orden y resume | app/model, tipos API, Spirc y session.json | Extraer máquina pura con IDs de ocurrencia, tests de contratos existentes y replay del mismo escenario en desktop/Android. |
| EQ, limiter, resample, espectro y osciloscopio | sink, rodio Source, librespot Sink, AudioTap | Conservar DSP, separar PCM de Spotify. Fixtures de impulsos/silencio/seek y señal post-EQ/pre-volumen. |
| Búsqueda, catálogo y artwork | API/gateway/session_reads, auth, images y modelos Spotify | Puerto neutral; proveedor experimental explícito, resultados acotados, cancelación y errores sin login Spotify. |
| Playlists, favoritos e historial | API, liked/history/account_cache/settings | Biblioteca propia versionada. Importación explícita de metadatos si se aprueba, nunca equivalencias automáticas por título. |
| Archivos locales, tags, carátulas y letras | lookup `spotify:local:` en librespot, symphonia/local_tags y lyrics | Fuente de archivo neutral. Desktop paths y Android content URIs/FD, sin exigir Spotify. |
| UI, navegación, teclas, i18n, bidi, skins y accesibilidad | app/model, fastframe, assets | Reutilizar vistas gradualmente con view models neutrales, headless/demo y evidencia visual. Gestos/touch aparte, no cambiar escritorio silenciosamente. |
| MilkDrop | projectm-sys/CMake/C++/GL, proceso hijo, memoria compartida | Mantener desktop opcional. Sin promesa Android; decidir tras medición de energía/GPU y lifecycle. |
| Tray, autostart, IPC, menús, notch y thumbbar | fastframe + APIs SO | Adaptador desktop; Android usa integración nativa distinta. Ausencia declarada como capability. |
| Letras | Spotify, LRCLIB, tags y caché | Parser/tag local reutilizable; acceso LRCLIB opcional, explícito y documentado, nunca backend Doremi obligatorio. |
| Spotify Connect, Cast y Sonos actuales | sesiones/tokens Spotify, zeroconf y protocolos de receptor | No son streaming genérico de yt-dlp. Conservar en legado; no prometer paridad. Gate de retirada con decisión documentada; no inventar reemplazo. |
| Radio, Made for you, amigos, releases, podcasts y guardados Spotify | APIs privadas/session_reads y perfil autenticado | Datos exclusivos sin equivalencia garantizada. Conservar legado, definir sustitución local o exclusión aprobada antes de quitar vistas/datos. |
| Actualización, packaging, protocolos y perfiles | IDs Spotifast, upstream releases, clave pública y recetas | Separación obligatoria antes del primer binario Doremi distribuible; fixtures de actualización y migración sin tocar perfiles originales. |

## Fases, dependencias y criterios medibles

### F0: preparar y demostrar el baseline

Sin dependencias previas.

- [x] Inventario estructural, fuentes de dependencias y mapa de acoplamientos.
- [x] Normas Doremi, MIT/atribuciones preservadas y separación documental.
- [x] Workspace mínimo, sin borrar ni modificar funciones del legado.
- [x] CI de validación Linux y core Android sin permisos de publicación.
- [x] Baseline Rust/gates de CI verdes: run `40563292782`, SHA `12d2137`.
  Builds desktop y core Android no equivalen a tests nativos/GUI/audio/APK.

Aprobación F0: comandos de `CONTRIBUTING.md` verdes o bloqueo claramente
identificado y resuelto, aplicación baseline compilada, fuente externa sin
cambios, core portable compilado, cero releases. No declarar terminado el
gate de baseline si no se llegó a ejecutar su suite.

### F1: extraer contratos y una cola neutral

Depende de cerrar el gate F0.

1. Caracterizar la cola actual con escenarios de duplicados, reordenación,
   clear manual, contexto, cambio de pista y restauración.
2. Extraer tipos neutrales y reducer de comandos/eventos en core, mantener
   bridge de legado. Separar view models del API Spotify.
3. Introducir composición de desktop propia, sin migrar perfiles por defecto,
   con modo de pruebas/fake y sin importar `spotifast` como dependencia.
4. Centralizar versiones y adaptar lectores de Nix/packaging conscientemente.

Aceptación: mismos vectores deterministas ejecutados en host y núcleo;
0 dependencias Spotify, egui, JNI o SO en core; fallos/respuestas obsoletas
no alteran la intención actual; builds legados siguen verdes. Cero bloqueos
de I/O en la UI nueva, canales con capacidad probada.

#### F1A: dominio mínimo y caracterización

Entrega: namespaces/medios, IDs por ocurrencia y solicitud, reducer de cola,
shuffle como permutación explícita, repeat/previous/next, cancelación,
respuestas tardías, snapshots pausados y tests. Bridge sin estado duplicado
para prefijo manual y plan de reorder, preservando el resto del legado.
No copiar Tubefast ni incorporar RustyPipe. CI y estado final en `F1A-REPORT.md`.

Decisiones:

- El contrato de código/tests actual tiene prioridad sobre texto histórico:
  cambiar canción a mano termina Repeat Track y deja Context; reiniciar/
  seek/fin natural conservan Track cuando corresponda.
- Nada de RNG, reloj, transporte o entidades Spotify dentro del reducer.
  Un resultado de proveedor se enruta por dueño/RequestId; no es autoridad
  para reemplazar la cola con una respuesta antigua.
- Biblioteca/perfiles continúan sin cambios; Snapshot no define todavía
  encoding de persistencia. No centralizar manifests/versiones como refactor
  incidental de esta misión.
- El historial oído de core se acota a 256. No afirmar equivalencia de
  historia remota, autoplay/radio, resume-only wrap o cursores privados.
- Tubefast aporta ideas, no una integración aprobada. RustyPipe estudiado es
  GPL-3.0-only, no MIT. No elegir proveedor por cifras de otro equipo.

#### F1B: ampliar autoridad con un corte verificable

Depende de gates F1A verdes. Criterios medibles:

1. Extender fixtures compartidas a selección por ocurrencia, respuestas
   parciales, navegación resume-only y reemplazo de sesiones. Cada diferencia
   de F1A debe tener decisión de preservación o límite explícito, no desaparecer
   del informe.
2. Extraer un caso de uso completo (por ejemplo edición manual y su
   confirmación), con un único dueño de estado. Cero segunda cola autoritativa
   permanente; sin reescritura de App. Gates originales y matriz CI verdes.
3. Adaptador de tokens/epoch y capacidades distingue local/remote,
   confirmación optimista y fallback de seis reintentos del legado. Probar
   restore de la misma instancia y reemplazo con callbacks todavía vivos.
4. Definir v1 del puerto search/resolve con el primer consumidor real:
   ProviderId/MediaId, request owner, cancelación/deadline, páginas/capabilities
   y errores tipados. URLs/headers efímeros fuera del snapshot/biblioteca;
   sin correspondencias automáticas por título.
5. Harness/dataset autorizados de benchmark, SHAs y schema JSONL reproducibles,
   fixtures de fallos/rangos y resultados separados de compilación. No
   anunciar rendimiento de reproducción antes de medirlo.
6. Preparar APK mínimo con proveedor falso y mismo core: entrada/IME,
   rotación, lifecycle y trazas host equivalentes. Registrar NDK/API/ABI y
   ejecución en emulador/dispositivo. Cross-check de Rust no cierra este gate.
7. Revisar caches por ID/ref/tamaño; rotación solo con autorización. Cero
   fuentes git externas nuevas ni publicaciones automáticas.

#### F1B-1: un corte real y puerto con consumidor

Implementación/evidencia: `F1B1-REPORT.md`. El vector manual existente es
única autoridad, prestado al núcleo para eliminación por ocurrencia.
Confirmación core usa session/generation y compara ese vector, sin sombra.
Se preservan Spirc, seis reintentos, ventana temporal y restricciones legacy.
Caracterizar selecciones stale, writes parciales, restauración y resume-only.

`Catalog` produce requests/events y es consumido por el harness de host.
Capacidades, límites, deadlines, errores, leases y redacción están en
`PROVIDER-CONTRACTS.md`. No hay proveedor productivo ni persistencia de
URLs/headers. Los experimentos actuales son sintéticos y loopback.
La propuesta de caches es `CACHE-ROTATION.md`; no ejecutar DELETE sin permiso.

#### Preview 0.1 Linux: entrega visual antes del proveedor

Prioridad inmediata de entrega: binario `doremi-preview` independiente,
identidad/rutas propias, sin credenciales, updater ni integraciones nativas
del legado. Conserva las vistas originales y su binario. El acento inicial
de Preview es #9966E8; verde original y claro/oscuro siguen disponibles.
La preferencia vive en `preview.json`, no en los perfiles Spotifast.

El catálogo y el reproductor son demostraciones explícitas, no audio real.
Búsqueda local acotada y navegación, no proveedor elegido anticipadamente.
Las carátulas sin red usan los placeholders existentes.
Los logos originales se preservan con procedencia y su licencia GPL v3
separada; no se importó código de Doremi-Rust ni se alteró MIT del baseline.

Gates: paquete portable con ejecutable Linux, assets originales, launcher,
versión/SHA/checksums; ejecución real y capturas Home/Search/Library/Settings,
claro/oscuro, cambio con ratón y persistencia de acento, búsqueda con teclado;
CI completa del SHA, sin releases. No declarar estos gates verdes por un
`cargo check`, ni atribuir un run anterior a Preview.
El job gráfico reutiliza el ejecutable construido por las pruebas CLI
all-features; no inicia otro build completo ni otra caché desktop.

#### Android Preview 0.1 / F1B-2: APK offline y lifecycle mínimo

Depende de gates F1B-1 verdes. APK offline `561497d` validada en Samsung
SM-J701M/API 28 (ARMv7): Unicode, navegación, dos temas/acento, persistencia
de preferencias, recreación con duplicados, rotación y secuencia compartida.
CI enfocada `40777190887` verde. El host final usa GLSurfaceView/egui_glow:
GameActivity/eframe se probó en hardware y falló en IME y recreación.
Actualización: restauración de proceso y audio local inicial pasaron en el
mismo teléfono, hasta `50030dc`. CI completa del legado `40795008801` verde
sobre `86aa04b`; CI enfocada `40921905474` verde. ARM64 y 16 KiB se verificaron
en el paquete, no en hardware; API 26/36 runtime siguen sin probarse.
El propietario Rust y las mismas vistas se usan en el host eframe Linux.

Alcance original, conservado como contrato:

Es el siguiente hito tras Preview Linux; no bloquea la entrega Linux.
Debe conservar nombre/logo/acento seleccionable, navegación táctil y
teclado/IME funcionales. Reusar el mismo `doremi-core`, no portar el árbol
desktop ni duplicar su estado. La demo de Linux no demuestra paridad móvil.

1. GameActivity/eframe con el mismo core y fake en memoria, sin importar el
   paquete desktop ni sus integraciones fastframe. Sin Internet/login/Spotify.
2. Un único dueño de core fuera de Activity; rotación/recreación no duplica
   cola o runtime. SessionId nuevo al reemplazar dueño, no en cada frame.
3. Teclado/IME Unicode, búsqueda falsa, selección explícita y trazas iguales
   al host. Invalidar callbacks antiguos al destroy/recreate/process death.
4. Gradle/AGP/JDK/SDK/NDK/cargo-ndk fijados. Candidato comprobado por docs:
   JDK 17, Gradle 8.13, AGP 8.13.x y API 36; minSdk 26 coherente con CPAL
   fijado. NDK r28+ fijado y comprobación de librerías con página 16 KiB.
5. APK construido, instalado y ejecutado en API mínima y actual; capturas
   y logs redactados como artifacts/PR, no en el repositorio.
6. Audio local es un gate posterior: puerto handle/asset, PCM preasignado,
   focus, auriculares y MediaSession/servicio según API. No afirmar que
   eframe o un backend CPAL ya resuelven lifecycle/audio Android.

No tratar una `.rlib` o `cargo check --tests` como este gate cumplido.
SDK/NDK/Gradle no se instalan en el N5000 como requisito diario.

### F2: evaluación experimental de proveedores y viabilidad Android

Depende del puerto y gates de F1B. Desktop y spike Android pueden avanzar en
paralelo, pero ambos deben cerrar antes de comprometer un lanzamiento móvil.

- Comparar un adaptador Rust nativo con yt-dlp para un corpus autorizado;
  RustyPipe solo referencia hasta resolver licencia. Decidir con evidencia
  de búsqueda/resolución, errores, recursos y empaquetado, no cifras ajenas.
- Si se experimenta con yt-dlp, supervisor sin shell: executable validado, argv fijo,
  `--ignore-config`, `--no-plugin-dirs`, `--no-remote-components`,
  `--skip-download`, JSON y límite de resultados. Separar búsqueda plana y
  resolución; metadatos de búsqueda pueden estar incompletos.
- Sin argumentos arbitrarios del usuario, `--exec`, configuración/plugins
  ambientales ni importación automática de cookies. Validar esquemas y
  hosts admitidos, rechazar `file:` y recursos de red local no autorizados.
  No asumir que flags estándar convierten todo extractor en uso permitido.
- Fakes ejecutables prueban timeout, cancelación, pipes llenos, JSON inválido,
  salida enorme, exit no cero y ruta ausente. Suites sin internet.
- Validación real separada, manual, con medio público propio/autorizado:
  búsqueda → resolución JSON → formato usable. Registrar versión yt-dlp,
  runtime JS, sistema y resultado, sin secretos.
- Spike Android: eframe de la revisión usada, GameActivity/IME, Gradle,
  cargo-ndk y un ABI arm64. Solo si se evalúa yt-dlp, probar Python/JS
  empaquetados. Comparar
  bridge Python embebido frente a ejecutable Android-compatible empaquetado.
  Elegir por prueba, licencias, ABI y restricciones de ejecución, no por
  suponer que un binario Linux aarch64 funciona en Bionic.
- Evaluar Tubefast/RustyPipe solo tras licencia y política de acceso. No
  copiar su perfil VISIONOS, refresh remoto mutable, mecanismos Botguard/DRM
  ni fallbacks para eludir negativas. Criterios y benchmark fijados en
  `TUBEFAST-ASSESSMENT.md`; no hay resultados de audio comparativos en F1A.
- Ampliar el harness real de F1B-1, no sustituir muestras sintéticas por
  resultados musicales. JSONL con SHA/versiones, cold/warm explícito,
  denominadores y fallos; p50/p95 búsqueda, resolución, primer PCM y audio
  audible solo con instrumentación, CPU/RSS del árbol, cancelación física.
  Todo campo no medido sigue null. Sin cookies/URLs firmadas en registros.

Aceptación propuesta para supervisor: ≤2 procesos activos, 20 resultados por
búsqueda, 30 s de deadline, stdout ≤4 MiB y stderr retenido ≤64 KiB,
cancelación con recogida del proceso ≤2 s. Valores iniciales, ajustar con
evidencia antes del merge. Tests verifican límites, no solo happy path.
Android: APK de prueba instalable con búsqueda/resolución real y teclado en
dispositivo o emulador, sin Termux, Python del sistema ni servicio externo
obligatorio. Si no es viable legal/técnicamente, se bloquea Android y se
documenta, no se crea un segundo núcleo ni un backend alojado para ocultarlo.

### F3a: reproductor independiente

Depende de F1; medios de yt-dlp dependen de F2.

Primer corte local ejecutado: `doremi-player` + CPAL, dos WAV originales CC0,
controles/seek/EOF/cancelación y foco/lifecycle Android probados; Linux probado
con salida nativa y XTest. Datos y límites en DOREMI-GOAL-REPORT.md. No cierra
F3a completo: formatos arbitrarios, streaming, EQ e integración de sesiones
siguen pendientes. Audibilidad acústica no capturada.

Crear motor con fuente local y HTTP autorizada, symphonia/rodio/salida por
puertos, sin Spirc. Seleccionar formatos que realmente se pueden decodificar,
no descargar todo ni transcodificar por defecto. MP3/FLAC no cubren por sí
solos todos los contenedores AAC/Opus/WebM/MP4 que devuelve yt-dlp.

Aceptación: fixture de audio con play/pause/seek/next/fin/error, EQ/volumen y
resume; buffers acotados incluso con red lenta; limpieza tras cancelar;
prueba auditiva y medición en Linux y Android; focus loss, desconexión de
auriculares y cambio de salida no dejan audio huérfano. Reproducir con
Spotify ausente y sin sus credenciales.

### F3b: biblioteca independiente

Depende de F1; puede ir en paralelo con F2/F3a.

Playlists, favoritos, historial, metadatos y persistencia propios. Decidir
JSON versionado frente a SQLite con volumen de datos/consultas medidos,
no añadir la dependencia solo por anticipación. SAF/FD para Android.

Aceptación: CRUD y persistencia tras reinicio, duplicados preservados,
migración/rollback de fixtures, escrituras atómicas fuera de UI, biblioteca
de 10.000 entradas sin trabajo de disco en un frame. No modificar ni
importar automáticamente el perfil Spotifast.

### F4: integrar producto desktop/Android compartido

Depende de F2, F3a y F3b.

UI neutral con catálogo → cola → reproducción → favoritos/resume. Adaptar
shell y capacidades por plataforma; conservar vistas/DSP/skins viables.
MediaSession y foreground service Android para background playback:
destruir la Activity no destruye ni duplica la cola.

Aceptación: flujo completo sin login Spotify, misma secuencia de comandos
con mismo estado de dominio en ambos hosts; no congelación con proveedor
simulado bloqueado 10 s; objetivo p95 frame ≤16,7 ms en hardware documentado
y ninguna espera de red/proceso/disco en UI. Capturas claro/oscuro a tamaños
desktop estrecho/normal y teléfono, lector de pantalla, IME, rotación,
background/foreground y process death probados.

### F5: retirar el legado y separar identidades

Depende de F4 y decisión explícita de paridad/exclusiones de la tabla.

Quitar fuentes Spotify solo cuando ningún nuevo paquete ni test las importe.
Separar binario, app ID, keyring, paths, URL schemes, desktop/MPRIS IDs,
Info.plist/MSI/Flatpak, autostart, caché, updater y claves Doremi. El nuevo
producto no usa la clave ni releases de crmne como canal de actualización.
Conservar MIT, crédito e historial.

Aceptación: `cargo tree` del producto sin librespot/Spotify, fixture sin
credenciales, 0 peticiones a endpoints Spotify durante el flujo completo;
instalar al lado de Spotifast no altera sus archivos ni IPC; datos no
migrables permanecen recuperables; matriz desktop y Android verde.

### F6: empaquetado y distribución verificada

Depende de F5 y del gate Android si se ofrece Android.

Construir Linux y APK/AAB desde un checkout/SHA/versión únicos, toolchains y
dependencias externas fijados. SBOM/licencias de Rust, Python, yt-dlp,
runtime JS y FFmpeg si se distribuye. Checksums y firma por identidad nueva.
No confundir compartir versión con reproducibilidad bit a bit.

Aceptación: artefactos descargados e instalados en entornos limpios,
hashes/versión/commit verificados; pruebas offline tras instalación, smoke
online autorizado, actualización y desinstalación; ARM64/NDK/API/16 KiB de
página comprobados; permisos y políticas de tienda revisados; CI completa
en el commit de versión. Publicar requiere autorización específica aparte.

## CI: hoy y evolución

F0 activa solo `.github/workflows/ci.yml`. Linux ejecuta políticas, tests de
packaging, fmt/core en un job rápido; tests default/all-features en otro;
Clippy/rustdoc en un tercero. El ejecutable Linux se reutiliza de los tests
de integración, antes de activar demo; no se repite un build standalone.
Jobs nativos macOS/Windows generan ejecutables de debug; Android genera una
`.rlib` para `aarch64-linux-android`, sin NDK porque no enlaza un binario.
Cada payload lleva JSON con SHA, versión, target, tipo, estado dirty y SHA-256.
El gate dependiente de todos rechaza discrepancias o fuentes sucias.
Son artefactos de validación, no instaladores Doremi. Cachés y reportes reales:
`CI-REPORT.md`. El circuito local cotidiano no compila la app completa.

F1/F2 añade compilación real del shell Android y Gradle/NDK fijados; F3/F4,
tests instrumentados y dispositivo. Los builds macOS/Windows no sustituyen
sus suites de tests nativos; recuperarlas, junto con Nix, sin destinos de
publicación. No declarar soporte probado solo por un build.
Reactivar Pages, triage, releases, AUR/Homebrew o firma exige revisión separada.

## Decisiones adicionales de producto: estables y cuenta opcional

### Gate global de una versión estable

Las plataformas obligatorias de **cada versión estable** son Linux, Windows
y Android. Todas usan el mismo número de versión y el mismo commit de
lanzamiento. Las previews experimentales pueden distribuirse por separado,
como los artifacts de desarrollo actuales; no son releases estables.

La futura CI de releases tendrá una matriz de plataformas con artefactos
independientes y un gate global que depende de todas sus validaciones:

| Plataforma | Evidencia obligatoria antes de una estable |
|---|---|
| Linux | Build, contratos compartidos, ejecución nativa, reproducción y paquete instalable verificado |
| Windows | Build nativo, contratos compartidos, reproducción, persistencia y paquete instalable verificado |
| Android | APK/AAB, ABI/SDK/páginas, firma, instalación, IME/lifecycle, audio y pruebas en dispositivo |

Cada manifiesto identifica versión, commit, plataforma, arquitectura, formato,
hash y resultado de gates. El gate global rechaza versiones/commits distintos,
artefactos ausentes o checks fallidos/no ejecutados en cualquiera de las tres.
Compilar una plataforma no sustituye sus pruebas obligatorias. macOS puede
seguir en regresiones del legado, pero no sustituye Windows en esta matriz.
El gate deja un conjunto revisable; **publicar exige autorización explícita**.
No se reactiva la publicación histórica ni se configura una release automática.

### Cuenta Google/YouTube opcional, después de Android/audio

Se preserva la experiencia del login histórico de `Eirom16/Doremi-Rust`:
conectar, volver automáticamente a Doremi, mostrar estado y cerrar sesión.
Los archivos de referencia son `src/doremi/ui/dialogs/login_dialog.py` y
`src/doremi/api/youtube_music.py`; su mecanismo no es un contrato a copiar.
No interceptar cookies, manipular Storage Access API, pedir contraseñas ni
extraer sesiones del navegador.

La prioridad sigue siendo APK real, núcleo compartido y audio. Después se
investigarán por separado API oficial de YouTube, funciones específicas de
YouTube Music no oficiales y sus límites de políticas, seguridad, licencia
y estabilidad. OAuth de Google no implica acceso a una API oficial de YT Music
ni autoriza reproducción o sustitución de audio por sí solo.

Fronteras previstas, sin introducir autenticación prematura:

- Identidad: cuenta opcional, estado de sesión, revocación y cierre de sesión.
- Credenciales: adaptador de almacén seguro por plataforma, permisos mínimos,
  sin tokens en core, UI, logs, artifacts, CI ni servidores ajenos.
- Catálogo personal: capacidades explícitas por fuente; sincronización en
  workers cancelables, opcional y sin bloquear UI ni catálogo público/local.
- Reproducción: puerto separado, IDs neutrales y recursos permitidos. Una
  sesión autenticada no prueba que un medio esté disponible para reproducir.

Se prioriza navegador del sistema, OAuth 2.0 con PKCE y retorno seguro;
Custom Tabs en Android cuando corresponda. El registro de cliente, redirect,
scopes, almacén, revocación y capacidades reales se aprobarán con evidencia
oficial vigente antes de implementar. La entrega deberá demostrar datos
realmente soportados y errores explícitos; no simular colecciones ausentes.

Investigación posterior al primer corte P2:
[`account-feasibility.md`](docs/doremi/account-feasibility.md) separa datos
oficiales, capacidades Music no oficiales, políticas y fronteras de seguridad.
La guía Google vigente excluye custom schemes Android y loopback móvil;
el retorno Desktop con PKCE no se traslada automáticamente a Android.
Se propone estudiar AuthorizationClient con tokens cortos en Android sin
backend obligatorio. Login todavía no implementado ni probado con una cuenta.


Validación de restauración P1: `ac1a2c4`, CI enfocada `40783624535`, APK instalada y probada en SM-J701M. Reinicio de proceso conserva identidades de duplicados, selección, orden, modos y posición en pausa. Unicode, recreación y rotación repetidos con instrumentación del mismo SHA. Evidencia y limitaciones en DOREMI-GOAL-REPORT.md. Siguiente corte activo: audio PCM local original; autenticación permanece posterior. CI completa del legado pendiente.


Avance de audio local: Linux `783501b` pasó probe de salida nativa y controles mediante XTest en CachyOS, y tiene lanzador de desarrollo propio. Android entrega PCM y pasó controles exploratorios; `86aa04b` pasó regresión ADB e IME después de background. Siguen abiertos el cierre instrumentado de foco/lifecycle, la clasificación de notificaciones xrun. El gate completo del legado pasó en `40795008801` sobre `86aa04b`. Reproducción acústica no capturada; MediaSession/background y proveedores online no implementados aún. La Preview se conserva. Detalles y evidencia en DOREMI-GOAL-REPORT.md.


Cierre del primer corte P2: APK `50030dc`, suite completa de controles PCM/foco/lifecycle aprobada en SM-J701M; sin avisos de painter perdido tras impedir draw mientras pausado. Instantánea propia: PSS 37794 KiB, CPU 236 ms/1100 ms y PCM a 48 kHz. Linux tiene reproducción local nativa y lanzador propio. Se continúa F2 con experimentos autorizados; aún no hay proveedor definitivo, reproducción online ni autenticación implementada.


Experimento F2 ejecutado: `d1ebed0`, CI `40974787764`, ocho flujos de búsqueda
común/resolución/descarga de dos tonos CC0 auditados. Rust y yt-dlp resolvieron
los mismos originales; tiempos y límites en DOREMI-GOAL-REPORT.md. No acredita
YouTube ni reproducción online integrada. Próximo corte: Ogg/Vorbis acotado y
motor nativo, después worker/UI cancelables. APK `075f18f` revalidada físicamente
tras la reconexión, con suite completa y capturas propias; Preview conservada.


Primer flujo online experimental ejecutado en Linux: `ed60553`, flujo CLI real de
Commons CC0 hasta callback nativo, con pausa/seek/cambio/EOT/cancelación sobre
el mismo Model/Queue. Los dos recursos se prebufferizan completos; aún no hay
UI online ni streaming. `doremi-commons` extrae transporte async compartido,
con política/tamaños/hashes auditados. CI completa `40976390452` verde sobre
`cd3a9ad`; CI enfocada y dispositivo físico verdes sobre `ed60553`.
La siguiente prioridad es worker/UI cancelables; autenticación sigue posterior.


Interfaz online Linux validada: `b2cfb74`, CI `40983361715` y `40983361717`
aprobadas; Doremi Native Dev instalado. Búsqueda real de dos tonos Commons CC0,
PCM nativo, controles, duplicados, cancelación y restauración pausada sin red
automática verificados mediante XTest. APK del mismo commit revalidada en el
SM-J701M con suite completa; un aviso xrun temprano en la muestra física.
Evidencia y límites en DOREMI-GOAL-REPORT.md. Prioridad activa: portar ese
worker/UI compartido a Android, añadir únicamente INTERNET y probar en teléfono.
Autenticación y biblioteca online permanecen posteriores.


Flujo online compartido verificado: Rust `acad284`, CI completa `41078491263`
con nueve jobs verdes; APK `94859d8`, CI enfocada `41143469529` verde e instalada
en SM-J701M. Suite física completa con quince capturas y volumen positivo 0,08,
PCM local/online, cancelación y lifecycle; un aviso xrun temprano en cada muestra.
Linux Native Dev actualizado y QA online aprobada; Preview conservada y
artefacto descargado/verificado. Evidencia y correcciones del harness en
DOREMI-GOAL-REPORT.md. Próximo corte: biblioteca propia con identidades neutrales,
metadatos persistidos sin URLs efímeras y conservación de favoritos existentes.
Autenticación, streaming y MediaSession siguen pendientes.
