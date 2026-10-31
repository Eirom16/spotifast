# Doremi: desarrollo Android real y Linux

Base Preview validada: `7b9404c58f947d86b2ea51a6a360d3872edb6f46`.
Rama autorizada: `feature/doremi-android-real`. Sin cambios en main ni releases.

Teléfono detectado por ADB: Samsung SM-J701M, Android 9/API 28,
ARMv7 (sistema de 32 bits), 720×1280, densidad efectiva 300 dpi,
MemTotal 1.843.892 kB. Depuración USB autorizada. No registrar número de serie.
Se construyen ARMv7 para este teléfono y ARM64 para el gate de empaquetado.

Estado actual (2026-10-31): APK y Doremi Native Dev Linux `a39ce24`, descargados de CI e instalados conservando datos. La suite física completa pasó 22 capturas, Unicode/navegación/temas/cola/lifecycle, audio corto, WAV de 120 s con buffer PCM de 64 KiB y el experimento online de dos tonos Commons CC0. Un segundo proceso comprobó favoritos persistentes y reproducción por ID sin búsqueda. Las cuatro canciones de demostración siguen siendo metadatos silenciosos.

CI enfocada `41155541994`: ambos hosts aprobados; harness offline `41155541980`: aprobado. Regresión completa `41155115847` sobre `57a0205` (anterior a la corrección de creación de la muestra Android): nueve jobs verdes, incluida la Preview conservada, Windows/macOS del legado y gate de artefactos. No se atribuye ese gate completo al SHA posterior. Los cambios posteriores de código están en el generador de la muestra Android y se comprobaron mediante CI enfocada y dispositivo.

El teléfono real es ARMv7/API 28 con páginas de 4 KiB. ARM64/16 KiB tienen verificación estática de empaquetado; no se ejecutaron en este hardware. Linux se probó con salida nativa y UI Xvfb/Mesa software; no se atribuye a Wayland/GPU física ni a un nuevo host Windows Doremi.

Pendientes: selector/importación general de archivos, más formatos, streaming remoto continuo, MediaSession/background, proveedor productivo y cuenta opcional. La CPU del WAV largo en el teléfono merece optimización; no se confunde con consumo de batería. No se capturó salida acústica ni micrófono. El historial siguiente conserva pruebas, fallos y correcciones; sus pendientes corresponden a cada iteración.

Primeras iteraciones CI:
- `40769993298` (`0aac395`): quick y core Android aprobados; APK falló antes
  de compilar al pedir el paquete SDK retirado `tools`. Corregido a platform-tools.
- `40770336343` (`156fb14`): SDK/NDK y cargo-ndk instalados; falló por usar
  `-p 26` en lugar de `-P 26`. Corregido en `529494c`.
- El test local de launchers no pudo ejecutarse por falta de Ruby. CI lo ejecuta.
- `40771321286` (`2787d92`): las dos librerías nativas Rust compilaron.
  Gradle falló por clases Kotlin duplicadas entre stdlib 1.8.22 y jdk7/jdk8
  1.6.21. Se alinea con Kotlin BOM 1.8.22, sin modificar fuentes externas.
  Aún no se generó ni instaló un APK verificable.

Primer APK real: `7e0fd83`, CI `40772870294` aprobada. Descargado con gh,
SHA-256 `62f129d76892c4e242c66106ca39accf88b18b4a6a0f9e11dcc41130323d3657`.
APK 20 MB, ARM64 y ARMv7; firma, zipalign y segmentos ARM64 de 16.384 bytes
verificados en CI. Instalación ADB Success en SM-J701M. Abrió y permitió
Inicio/Biblioteca/Buscar. Capturas reales en `target/android-qa/`.
Primera apertura am start: ThisTime 1921 ms, no equivale a primer frame útil.
El teléfono usa páginas de 4 KiB. ARM64/16 KiB no ejecutados en hardware.

Fallos de dispositivo que impiden cerrar A/B: el teclado visible no entrega
texto a egui, tanto por ADB como con toques reales. La recreación de Activity
produce pantalla vacía, timeout de apertura y bucle de errores del host.
Se detuvo únicamente Doremi, sin borrar datos. La prueba instrumentada falló
al no encontrar InputConnection; no se cuenta como pasada. Se cambia solo
el host Android a Activity/GLSurfaceView/egui_glow con IME framework. Se
conservan vistas/core, eframe Linux, forks y la Preview aprobada. Pendiente
validar esta corrección instalada, con la misma firma y datos existentes.

Decisiones adicionales incorporadas al roadmap: cada estable requiere Linux,
Windows y Android con versión/commit comunes y gate global; previews separadas
permitidas, publicación siempre con autorización. Cuenta Google/YouTube opcional
separada de catálogo/credenciales/audio, OAuth/PKCE y retorno seguro a investigar
tras los hitos activos. No se implementó autenticación ni acceso a cookies.

## APK funcional comprobada en el teléfono

`cd0b9a4`, CI enfocada `40774964947`: ambos jobs Android y Linux verdes.
APK descargada/verificada/actualizada sin desinstalar: conserva la biblioteca
previa. ADB comprobó navegación, búsqueda Lluvia, selección, biblioteca,
cola editable/duplicados, Next/Previous, pausa simulada, Shuffle/Repeat,
cancelación, morado/verde, claro/oscuro, persistencia y background/foreground.
Instrumentación comprobó Café Unicode y una nueva instancia tras recreate.

`561497d`, CI enfocada `40777190887`: ambos jobs verdes. Actualizada en el
mismo teléfono, con hashes/manifest del SHA. La instrumentación ampliada pasó
entrada rápida/Unicode, recreación conservando dos ocurrencias, horizontal/
vertical y `rain -> dawn -> dawn -> cafe`, la misma secuencia del host Linux.
Capturas PixelCopy de la superficie propia: `target/android-qa/561497d5da5e`.
No se encontraron fatal/panic/ANR/shutdown timeout en el log de ese proceso.
Una captura accidental del launcher tras finalizar instrumentación fue
eliminada; las pruebas ahora verifican foco y usan PixelCopy de Doremi.

Linux `cd0b9a4`: artifact descargado, checksums correctos, sin librerías
faltantes en ldd. Repitió el QA X11/XTest en CachyOS bajo Xvfb/Mesa software.
No se atribuye ese resultado a Wayland, GPU nativa o Windows. La Preview
aprobada y `src/` no cambiaron. El ajuste de redondeo de navegación se verificó
en el siguiente job Linux y en las capturas reales horizontal/vertical Android.

Mediciones puntuales, sin extrapolación: PSS 56.279 kB (gráficos 14.240 kB),
CPU idle 0,0% en dos muestras top de un segundo; RES top 121 MB, de los cuales
105 MB compartidos. Gfxinfo registró solo 5 frames HWUI y uno janky; no mide
la cadencia completa de egui/OpenGL y no permite afirmar FPS o jank global.
am start -W del nuevo host reportó ThisTime 901 ms tras instalar. No es un
benchmark frío/caliente ni una medida del primer audio o primer frame útil.

Siguiente corte activo: checkpoint de cola JSON versionado, validado en core,
restauración pausada tras process death, archivos incompatibles conservados,
writer acotado y errores de disco visibles. `doremi-library` permite probar
ese codec/worker localmente sin compilar UI. Audio local viene después.
La CI completa del legado sobre el cambio final sigue pendiente.

Corte de restauración implementado, aún pendiente de CI/dispositivo:
`queue::checkpoint` expone datos neutrales y valida índices/permutaciones,
identidades, límites y posiciones antes de restaurar transaccionalmente.
Tres nuevas pruebas core (una recorre 2000 estados), tres de codec/worker y
Clippy core/library pasaron localmente. `doremi-library` no importa egui ni
Spotify. El host migra preferencias schema 0, preserva archivos desconocidos,
muestra fallos de disco, guarda cola/historial/orden/posición y restaura pausado.
Se corrige además una duplicación de proyección en la vista de cola: cada
ocurrencia se dibuja una vez, con edición solo de las manuales. La prueba de
render cuenta las dos copias reales, no solo el tamaño del vector del core.


## Restauración comprobada en el teléfono

`ac1a2c4`, CI enfocada [40783624535](https://github.com/Eirom16/spotifast/actions/runs/40783624535): Android y Linux aprobados. APK descargada, verificada e instalada sobre el perfil existente. El driver ADB pasó dos reinicios de proceso: conservó IDs de ocurrencia manual, canción actual, orden siguiente, Repeat, Shuffle y posición, restaurando en pausa. Evidencia local: `target/android-qa/ac1a2c4b5819/report.json` y `restored-player.png`. Se abrió el reproductor restaurado sin reiniciar el contexto.

La APK de instrumentación del mismo commit pasó entrada rápida/Unicode, recreación, horizontal/vertical y la secuencia compartida `rain -> dawn -> dawn -> cafe`. No se atribuye esta prueba a ARM64 ni a páginas reales de 16 KiB. El teléfono sigue siendo el SM-J701M/API 28 con Android ARMv7. La CI completa del legado sigue pendiente.

La iteración `40782295443` falló por un rótulo duplicado que combinaba dos tipos de error de almacenamiento. Se eliminó el rótulo obsoleto, manteniendo el aviso superior tipado, y se reconstruyó en la ejecución aprobada anterior. Audio real aún no entregado: comienza un adaptador separado con PCM local original acotado.

## Audio local: implementación en validación

Se añade `doremi-player`, separado de core y del reproductor Spotify. Dos WAV PCM16 originales de ocho segundos se generan matemáticamente y se dedican a CC0. No se reutilizan grabaciones. Decoder con límites y reloj/resampling/seek tienen tres tests locales aprobados, incluido volumen cero que sigue avanzando. El worker prepara el PCM completo, acotado a un millón de muestras; el callback propio no accede a disco/red, locks ni asignaciones. No se anuncia streaming ni biblioteca de archivos arbitrarios.

El host común interpreta efectos/eventos de la misma cola, con generación para descartar resultados antiguos. La selección por índice conserva las ocurrencias duplicadas; Shuffle usa ahora una permutación aleatoria del host conservando la selección. La UI distingue las dos muestras con audio de las cuatro canciones simuladas. Android añade contexto de aplicación válido para AAudio y foco musical; se pausa al salir o perder foco, sin servicio/MediaSession todavía. Audio físico, reproducción Linux y APK de este corte siguen pendientes de CI y hardware.

Primera salida nativa observada en SM-J701M con `a844849`: APK del job Android aprobado de `40791051067`, descargada con gh, hashes/commit verificados e instalada preservando datos. La UI original PCM avanzó a 3120 ms, 149768 frames/150 callbacks, 48 kHz, PCM preparado 1536000 bytes y 0 xruns reportados por CPAL en esa muestra. Primer PCM 38739 µs y primer callback con datos 128286 µs, muestras puntuales que incluyen preparación y foco, no benchmarks ni audio audible medido. Captura propia y estado: `target/android-qa/audio-a844849/first-output.*`. Cancelar llegó a pausa y retiró las métricas de salida. PSS puntual con audio: 51802 kB, gráficos 17752 kB. No se capturó micrófono ni salida acústica.

La ejecución completa `40791051067` no está verde: Linux pasó siete tests pero falló Clippy por un if colapsable en el worker, corregido posteriormente. La regresión ADB descubrió coordenadas de diagnóstico obsoletas tras navegar, debidas al throttle nuevo; la captura mostraba Player mientras el JSON conservaba los controles de Home. No se declara pasada esa regresión. Se corrige publicación inmediata de geometría y repaint diferido para estado final, con test específico. Se reduce el refresco de posición a 5 Hz, independiente del callback, y se evita fsync en diagnóstico efímero. Preferencias y cola siguen durables y atómicas; coalescer diagnósticos ya no puede descartar un guardado de cola aceptado. Validación de estas correcciones y controles de audio completa pendiente.


## Audio local ejecutado en Linux y Android

CI enfocada `40793923660` (`783501b`) y `40794856339` (`86aa04b`): ambos jobs aprobados. Linux `783501b` descargado, SHA/version/target verificados y ejecutado en CachyOS. `--audio-probe` pasó Play/Pause, seek en pausa, volumen cero, Next/Previous, fin de pista y cancelación sobre la salida nativa, 48 kHz y sin notificaciones xrun en esa ejecución. Pico RSS 42128 KiB; proceso 2,874 s, CPU usuario 0,136 s y sistema 0,062 s, muestras de una prueba corta, no benchmark sostenido. XTest también pasó Play/Pause/resume/Next/Previous/cancel con PCM real bajo Xvfb/Mesa software. El sink predeterminado observado es ALSA PCM (`front:0`). No se grabó salida acústica. Evidencia en `target/native-audio-783501b/` y `target/native-audio-ui-783501b/`.

Se instaló el build verificado con lanzador exclusivo `Doremi Native Dev`, en `~/.local/share/doremi/native-dev-builds/783501b7e91effb4304c3674b231b721ca46c6f1/`. Conserva la Preview, perfiles previos y licencias. El botón Inicio → Probar audio original reproduce las dos muestras, sin Spotify. El script reproducible de QA es `scripts/native-audio-qa.py`; el CLI no lee ni escribe perfiles.

Android `56e0e7d` y `86aa04b`: actualizados sin borrar datos; el driver completo de navegación, temas, cola y restauración pasó. Pruebas exploratorias ADB `a844849` pasaron los controles de audio, EOF y background/foreground, con un toque inerte para evitar el defecto ya corregido de coordenadas. Hubo notificaciones xrun intermitentes: no se declara reproducción libre de cortes. Con sondeo de posición programado cada 200 ms (no una medida de FPS), una muestra de `56e0e7d` registró PSS 74618 KiB y 55 ticks CPU en 2,086 s; `getconf` no existe en el teléfono, así que no se convierte ese contador a porcentaje sin verificar su frecuencia. El contador CPAL registra notificaciones, no el acumulado bruto de underruns del hardware.

Reproducción directa del IME: después de background/cancelar, `56e0e7d` producía `LluviaX`; `86aa04b` produce solo `X`, con el hash del APK instalado comprobado. La instrumentación extendida aún no está cerrada: se corrigieron PixelCopy antes del primer swap y una carrera de inyección antes de recuperar foco, sin solicitar INJECT_EVENTS ni cambiar seguridad. CI `40796658711` reconstruye el test Java de `40dfd4a` reutilizando únicamente librerías de `86aa04b` con fingerprint/hash idénticos. La CI completa `40795008801` terminó verde sobre `86aa04b`: nueve jobs, incluida Preview Linux, tests, Clippy, APK y gates de artefactos. Windows/macOS corresponden al baseline conservado; no prueban audio nativo Doremi en esas plataformas. Pendiente clasificar los xruns y cerrar controles/interrupciones con instrumentación.


Tras reanudar, `40795008801` se confirmó terminal/aprobada y el teléfono SM-J701M siguió autorizado. El intento instrumentado anterior tenía salida vacía y no cuenta como prueba pasada. El nuevo intento `40dfd4a` pasó por IME tras background, rotación, cola y foco (capturas propias de PCM activo y pausa por interrupción). Falló antes de capturar EOF: el timeout global de 8 s no reservaba preparación/render/publicación para una muestra que ya dura 8 s. El último estado del mismo proceso muestra la segunda muestra `tone-chord` con PCM activo a 320 ms. Se mantiene el test funcional y se asigna a ese paso un plazo de 12 s, con diagnóstico de timeout explícito. La suite completa sigue pendiente hasta repetirla. También se añade la posición PCM de las notificaciones xrun para poder clasificar arranque frente a reproducción sostenida.


Primer tramo válido del diagnóstico `9695f1d`: una notificación xrun tras 1924 frames entregados y ninguna adicional hasta 136604 frames, a 48 kHz. Un siguiente tramo perdió el foco y la superficie, por lo que no se cuenta como prueba sostenida. El log propio reveló una recreación de painter después de la limpieza de onPause: un repaint pendiente podía entrar antes de `surface.onPause()`. Se impide inicializar/dibujar el renderer cuando `resumed` es falso. No se modifica ninguna dependencia ni se suprime su aviso. La nueva instrumentación mide CPU/PSS de su propio proceso mediante la API Android, evitando suponer la frecuencia de ticks. Validación instalada de esta corrección pendiente.


## P2: corte local validado

`50030dc`, CI enfocada [40921905474](https://github.com/Eirom16/spotifast/actions/runs/40921905474) aprobada, APK SHA-256 `43fafc9e01b3ad8384473858f2fd3165fd48215122ed38ddd751f8a2cf7f6bc6`. Sus librerías provienen de `9695f1d`, reutilizadas solo tras verificar igualdad de fingerprint/hash; Java/manifest/assets corresponden al commit del APK. El driver de regresión y la suite completa pasaron sobre la instalación real. Nueve pares PixelCopy/JSON propios y logs del proceso en `target/android-qa/50030dcd1c15/instrumentation/`. Se inspeccionaron capturas de reproducción y pausa por foco.

CPU/PSS durante 1100 ms de reproducción: 236 ms CPU del proceso (21,45% de un núcleo), PSS 37794 KiB, 96008 frames entregados a 48 kHz, PCM 1536000 bytes. El contador no registró notificaciones xrun en ese tramo ni en las tres instantáneas de audio preservadas. Primer PCM 69262 µs y primer callback con datos 153851 µs en esa carga; son muestras puntuales, no latencia acústica ni benchmark sostenido. No aparecieron fatal nativo/JNI, panic, timeout de cierre o aviso de painter sin destruir en el log propio revisado. Cinco instancias de renderer cubrieron recreación y retornos de background, conservando ocurrencias y pausando correctamente.

La suite comprueba Unicode rápido, recreación con duplicados, horizontal/vertical, la traza compartida, cancelar tras background sin restaurar texto antiguo, controles PCM, EOF real, interrupción real de foco y pausa sin autorreanudación, recreación durante audio, background/foreground y cancelación. La captura acústica sigue sin realizarse; no se afirma sonido audible medido. El siguiente corte es evaluar proveedores con contenido autorizado, sin autenticación prematura.


## F2: experimento online admisible, separado de la aplicación

CI del probe `40974353021` (`fbcd0a4`) y `40974787764` (`d1ebed0`) aprobadas:
tests de core/políticas de transporte/licencia, Clippy y ejecutable Linux.
Artifact de `d1ebed0` descargado con gh y verificado por manifiesto, SHA/versión,
checksums y corpus. Evidencia local: `target/provider-live-qa/d1ebed0/report.json`.
La CI enfocada Android/Linux `40974019600` también pasó sobre `075f18f`.

Se usa Commons, no YouTube: API pública, agente descriptivo, IDs neutrales y
solo dos tonos sintéticos propios de Ubershmekel con declaración CC0. Corpus
con páginas primarias, tamaños y SHA-1 en `tools/provider-live-probe/corpus.json`.
Ninguna cuenta, cookie, perfil de cliente o credencial participa. Solo se aceptan
los originales auditados; no se conserva URI de transporte en los informes.

Ocho solicitudes completas (dos archivos × dos resolutores × dos pasadas)
aceptaron exactamente los mismos hashes. Búsqueda común nativa en ambos casos:
yt-dlp no ofrece búsqueda Commons. Medianas de cuatro muestras por resolutor,
en el Pentium N5000 local, procesos nuevos y cachés remotas no controladas:

| Medida | Rust nativo | yt-dlp 2026.10.26 |
|---|---:|---:|
| Búsqueda común | 203,88 ms | 193,58 ms |
| Resolución | 59,48 ms | 3601,09 ms |
| Primer byte después de resolver | 198,50 ms | 199,24 ms |
| Transferencia completa, sin escritura del archivo | 251,65 ms | 251,76 ms |
| Proceso completo | 555,08 ms | 4095,06 ms |
| CPU usuario / sistema del proceso y descendientes esperados | 34,96 / 17,53 ms | 2952,70 / 212,97 ms |
| Mayor RSS individual observado | 27292 KiB | 73500 KiB |

RSS no es el pico simultáneo del grupo. Las repeticiones no acreditan un
benchmark frío/caliente controlado. Con FFmpeg idéntico y los archivos locales
ya verificados, primer PCM de decodificación entre 179,83 y 209,57 ms;
se obtuvieron diez segundos mono/48 kHz/f32 no vacíos por archivo, sin abrir
salida de audio. No es latencia total al primer PCM ni primer sonido audible.

Tras observar la fase de resolución, terminar cada grupo experimental dejó
cero procesos vivos o zombies en 24,94/30,26 ms de observación; se observó
el hijo Python en el caso yt-dlp. Es cancelación de un CLI aislado, no prueba
del futuro worker integrado. Scripts: `scripts/provider-live-qa.py`.

Fallos conservados: `40974019684` no compiló por usar `Resolved` en vez de
`ProviderEvent::Resolve`; se corrigió sin cambiar el contrato. Las primeras
cuatro resoluciones yt-dlp de `fbcd0a4` fallaron por seleccionar `wikimedia`;
el nombre oficial es `wikimedia.org`. La corrección resolvió el original con
los mismos límites y sin evasión. No se cuentan esos fallos como denegaciones
remotas ni como resoluciones exitosas. Los informes originales se conservan.

Este corte favorece el adaptador nativo para el experimento Commons, pero no
decide un proveedor YouTube. El APK sigue sin permiso de red. El siguiente
corte lleva Ogg/Vorbis acotado y el recurso permitido al motor nativo; reproducción
online en UI, cancelación de worker y MediaSession siguen pendientes.

La investigación solicitada de cuenta opcional queda en
`docs/doremi/account-feasibility.md`, enlazada por el roadmap. Se inspeccionó
la referencia histórica y documentación oficial vigente, sin copiar cookies,
WebEngine ni Storage Access API y sin implementar autenticación prematura.


Revalidación física `075f18f`: APK descargada de `40974019600`, hash
`cc93799a3c3a82bd0fa7294ac14d3dc6d2876016c3cb60f1ab3589641da43c52`,
actualizada preservando datos en el mismo SM-J701M autorizado. Driver básico
aprobado y suite instrumentada completa aprobada, con nueve PixelCopy propios
inspeccionados y sin errores críticos encontrados en el log de ese proceso.
PSS 38134 KiB y CPU 215 ms/1100 ms (19,55% de un núcleo); PCM 96008 frames,
48 kHz, cero notificaciones xrun en ese intervalo. No captura acústica.
Evidencia `target/android-qa/075f18f142eb/instrumentation/summary.json`.
El nuevo wrapper `scripts/android-instrumentation-qa.py` comprueba el hash del
APK realmente instalado y exige PASS antes de recopilar archivos privados QA.
El host Linux del mismo commit también se descargó/verificó y pasó controles
nativos Play/Pause/resume/Next/Previous/cancel con XTest y Xvfb/Mesa software:
`target/native-audio-ui-075f18f/report.json`. No se atribuye a Wayland/GPU física.


## Primer flujo online con salida nativa (CLI Linux experimental)

`cd3a9ad`: CI enfocada `40976213322`, probe `40976213263` y CI completa
`40976390452` aprobadas. Nueve jobs del gate completo verdes, incluida Preview,
regresiones Windows/macOS del legado y APK. Android y Linux revalidaron los
controles PCM después de compartir buffers inmutables con el actor de salida.
Android: PSS 35206 KiB, CPU 244 ms/1100 ms, cero notificaciones xrun en la muestra;
suite completa y capturas propias en `target/android-qa/cd3a9adc65f7/`.
El decoder Rust preparó los dos tonos online a 44,1 kHz, diez segundos y
1764000 bytes PCM cada uno; se verificaron hashes y límites, incluido rechazar
un Ogg pequeño que expande a más de un millón de muestras. Symphonia 0.5.5
se reutiliza sin modificar, con MPL y enlace a sus fuentes conservados.

`ed60553`: CI enfocada `40977844848` y probe async `40977844882` aprobadas.
Se comparte el adaptador `doremi-commons`; no depende de UI/audio/credenciales.
El ejecutable descargado/verificado ejecutó `--online-audio-probe` en CachyOS:
búsqueda real, selección por IDs auditados, resolución, descarga/hash, PCM Rust,
salida nativa y los mismos controles/cola del host. Pasaron pausa, seek,
volumen cero, Next/Previous, fin de pista y cancelación. Las cuatro métricas de
salida guardadas registraron cero xruns, PCM a 48 kHz de salida y frames reales.
No se capturó sonido acústico ni se afirma audibilidad medida.

Dos recursos completos se preparan antes de reproducir: no es streaming.
Muestra única: prefetch 1515,64 ms, proceso 4177,36 ms, CPU usuario/sistema
198,79/48,49 ms y RSS máximo 49132 KiB. Los tiempos `first_pcm_us` de salida
comienzan al cargar PCM ya preparado; no incluyen red/decoder ni miden primera
salida acústica. Evidencia: `target/native-online-ed60553/report.json` y log propio.
El CLI no creó perfil y usa volumen de aplicación 0,08 para estos tonos;
no modifica el volumen global. UI offline del mismo build pasó QA XTest.
El instalador privado actualizado conservó builds/Preview/perfiles anteriores.

APK `ed60553` SHA-256 `0a7d945cbc08114b5e061ea6277679d85e3994c0be661d6eddb5672c1966b0a9` descargada e instalada
sin borrar datos: regresión básica y suite instrumentada completa aprobadas.
PSS 40392 KiB y CPU 243 ms/1100 ms, cero notificaciones xrun en ese tramo.
Nueve capturas propias y log sin errores críticos encontrados:
`target/android-qa/ed605534c519/instrumentation/summary.json`.
Android sigue offline/sin permiso INTERNET. Linux ofrece el flujo online solo
como CLI experimental; el siguiente corte añade worker/UI cancelables. Esto
no acredita un catálogo YouTube, biblioteca Music ni autenticación.


## Flujo online en la interfaz Linux y regresión Android

`b2cfb744a5d6acf01d9e70a9f7bc3f1988f96404`: CI enfocada `40983361715` y
probe `40983361717` aprobados. Doremi Native Dev instalado con manifiesto y
checksums verificados; Preview y perfiles anteriores conservados. Buscar →
Commons CC0 permite consultar y reproducir dos tonos auditados. Worker acotado,
decodificación cancelable, misma Queue/Catalog y restauración pausada sin red
automática. Biblioteca online y streaming siguen pendientes.

QA real XTest sobre Xvfb/Mesa software: búsqueda, selección, PCM nativo, pausa,
Next/Previous, duplicados, cancelación, claro/oscuro, ventana estrecha y
restauración pasaron. Evidencia: `target/native-online-ui-b2cfb74-final/report.json`.
Cancelación observada en UI: 176,41 ms; no mide aborto de red. Cierre normal
49,78 ms; cierre con consulta pendiente 42,05 ms. Muestra de 1100 ms: CPU
480 ms, RSS 131028 KiB, salida 48 kHz, 60416 frames y cero avisos xrun en ese
tramo; incluye render software y no representa GPU física ni salida acústica.

El harness corrigió una expectativa que ignoraba la prioridad del historial
forward sobre la cola manual; el núcleo no cambió. Los intentos fallidos se
conservan. Destruir externamente la ventana X11 con XDestroyWindow provoca un
panic en winit fijado; el cierre normal WM_DELETE_WINDOW pasó.

APK del mismo SHA descargada e instalada preservando datos. Suite física
completa aprobada en SM-J701M/API28/ARMv7: nueve capturas, Unicode, navegación,
cola, lifecycle, foco y audio local. Evidencia:
`target/android-qa/b2cfb744a5d6/instrumentation/summary.json`. Muestra 1100 ms:
CPU 225 ms, PSS 37513 KiB, 96008 frames a 48 kHz. Una notificación CPAL xrun
en frame 1924 y ninguna adicional hasta 96008; no se afirma ausencia de cortes
audibles. Android aún offline en esa APK. Siguiente corte: compartir el worker
online con Android y validarlo físicamente, sin adelantar autenticación.


## Primer flujo online compartido, Android físico y Linux

Código Rust `acad284c82257792a37c83e798fb6adaca116dbd`: CI enfocada
`41077946872` y CI completa `41078491263` aprobadas. Nueve jobs del gate
completo verdes, incluida Preview y regresiones Windows/macOS del legado;
esto no valida todavía Doremi Native en Windows. Preview descargada y todos
los hashes del archivo/contenido verificados en
`target/downloaded-ci/preview-acad284/verified-archive.json`.

Linux: Doremi Native Dev instalado y QA online completa aprobada en
`target/native-online-ui-acad284/report.json`. Búsqueda, selección, pausa,
Next/Previous, duplicados, cancelación, restauración pausada sin red automática
y cierre durante consulta pendiente. Claro/oscuro/384 px inspeccionados.
Xvfb/Mesa software, salida nativa predeterminada; muestra 1100 ms, CPU 490 ms,
RSS 130836 KiB y cero avisos xrun en ese tramo. No representa GPU física.
El registro del instalador se regeneró tras una redirección de stdout al
mismo JSON; binario/hash/lanzador revalidados sin cambiar perfiles.

APK `94859d8d33c7f3c053e71c0326da11818c8b651f`, CI `41143469529` aprobada,
SHA-256 `9597a4b83f1b31ed1f448ea2ef60adb5ccfe28db2a65684553bb66bcd5bb28f0`.
Descargada automáticamente e instalada sobre Doremi Dev preservando datos.
El manifiesto identifica las bibliotecas nativas reutilizadas de `acad284`;
huella de entradas Rust y bytes de ambas ABI comprobados idénticos.
ARM64 alineada estáticamente a 16 KiB; prueba física en SM-J701M/API28/ARMv7
con páginas de 4 KiB. No se atribuye una prueba ARM64/16 KiB a ese teléfono.

Suite instrumentada completa aprobada, con quince capturas propias:
`target/android-qa/94859d8d33c7/instrumentation/summary.json`. Unicode,
navegación, cola compartida, foco, recreación, background/foreground y audio
local; además búsqueda real Commons, cancelación de carga observada, PCM
nativo, pausa estable, Next/Previous y misma ocurrencia pausada tras lifecycle.
Claro/oscuro y capturas reales inspeccionados. INTERNET es el único permiso
ordinario añadido; certificados raíz incluidos en la app con licencia CDLA,
sin instalar certificados ni modificar seguridad Android.

Ambas mediciones exigen volumen de aplicación positivo 0,08 y frames nativos.
Local: CPU 276 ms/1101 ms, PSS 38465 KiB, 118012 frames. Online completamente
prebufferizado: CPU 264 ms/1100 ms, PSS 47396 KiB, 89324 frames; salida 48 kHz
y 1764000 bytes PCM. Cada muestra registró un aviso xrun en frame 0 y ninguno
adicional hasta su instantánea; no se afirma audio sin cortes. Los tiempos
de callback comienzan con PCM ya preparado, excluyen red/decodificación.
No se capturó salida acústica ni se midieron FPS o batería. Sin errores
críticos relevantes encontrados en el log del propio proceso.

Se conservaron los fallos del harness: `8b83333` no actualizó el volumen con
un par de eventos instantáneo; mantener contacto real 80 ms permitió pasar
`1e0da43`. Esa suite funcional había usado ganancia cero en la muestra online
por asumir un tamaño de handle incorrecto. El review de esa limitación sigue
en su evidencia; `94859d8` corrige coordenadas y exige explícitamente ganancia
positiva. No se atribuye audibilidad a la prueba silenciada.

Flujo disponible en ambos hosts: Buscar → Commons CC0 → sine/440/880 →
Seleccionar → PCM nativo → Pausa/Siguiente/Anterior. Sigue limitado a dos
tonos CC0 auditados de Ubershmekel, con recursos completos acotados; no es
streaming ni catálogo YouTube ni proveedor definitivo. MediaSession y
background playback siguen pendientes. Próximo corte: favoritos neutrales
compartidos para guardar estas identidades reales, conservando el esquema
y los favoritos anteriores; autenticación continúa separada y posterior.


## Biblioteca neutral persistente y reproducción desde favoritos

Rust `5a73e9b4ec4d6dac31d46ed9e72ab77a42a42653`: CI enfocada
`41146068350` y CI completa `41147141363` aprobadas, nueve jobs del gate
completo verdes. Preview descargada y hashes internos/externos verificados en
`target/downloaded-ci/preview-5a73e9b/verified-archive.json`; regresiones
Windows/macOS siguen siendo del legado, no del host Doremi Native.

La colección compartida guarda hasta 256 identidades calificadas y sus
metadatos en `library-v1.json`. Save es idempotente por proveedor/ID; quitar
un favorito no altera las ocurrencias de cola. El worker codifica snapshots
inmutables y escribe atómicamente; la UI no espera disco. La lista anterior
de preferencias se importa solo si falta el nuevo archivo y se conserva
como archivo de rollback, sin reimportar favoritos eliminados. Archivos
incompatibles quedan de solo lectura. No se guardan recursos de reproducción.
Ocho tests ligeros de persistencia y Clippy del paquete aprobados localmente;
tests de migración/namespace y host aprobados en CI.

Linux instalado: Doremi Native Dev `5a73e9b`. QA completa con guardar
duplicado/quitar/restaurar/reproducir desde Biblioteca, además de la regresión
online anterior: `target/native-online-ui-5a73e9b/report.json`. Capturas claro,
oscuro y estrecho revisadas. Render Xvfb/Mesa software; salida nativa.

Android `5a73e9b` pasó regresión ADB y el flujo completo de favoritos mediante
toques verificados: `target/android-library-smoke-5a73e9b/report.json`. Quitó
solo el favorito de prueba recién creado, conservó el anterior y comprobó
restauración en proceso nuevo sin red/audio automáticos, seguida de PCM
positivo desde Biblioteca con el buscador vacío.

APK instalada final `0be06258b888ea38b577e196c28994f50df3baec`, SHA-256
`6bb7323d3afda30f10feb86f1bc7f3b1a66a326b533c00d2c68d785baac4df2a`.
CI `41148104353` verde en intento 2: el primero falló por HTTP 500 al obtener
Gradle antes de compilar; se reejecutó solo el job fallido. Las bibliotecas
nativas reutilizadas de `5a73e9b` tienen huella y bytes idénticos comprobados.
La instrumentación inicial de `5a73e9b` perdió el primer toque de navegación;
su fallo quedó conservado. `0be0625` espera una captura PixelCopy del primer
swap y mide coordenadas de la superficie; en este teléfono su origen y el
del contenedor coinciden en [0,45]. La suite completa pasó con 18 capturas:
`target/android-qa/0be06258b888/instrumentation/summary.json`. Conservó el
favorito que ya existía; la retirada física quedó probada por el caso ADB
anterior, sin borrar otros favoritos.

Una segunda instrumentación después de force-stop pasó con PID nuevo y dos
capturas: `target/android-qa/0be06258b888/library-restore/summary.json`.
Mismos favoritos, proveedor sin inicializar y ningún PCM activo al abrir;
selección por ID guardado produjo frames nativos sin resultados de búsqueda.
La ausencia de I/O automático se refiere al proveedor/red y audio, no a las
lecturas locales necesarias para restaurar persistencia.

Ganancia verificada 0,08. Muestra local: CPU 233 ms/1101 ms y PSS 37216 KiB,
un aviso xrun temprano. Online: CPU 281 ms/1100 ms y PSS 41657 KiB, cero avisos
en ese tramo. La reproducción desde favoritos tras reiniciar registró un
aviso en frame 0; no se afirma audio libre de cortes. Sin errores críticos
relevantes encontrados en logs propios. No hay captura acústica, FPS medidos
ni soporte MediaSession/background todavía. El catálogo online continúa
limitado a los dos tonos CC0 auditados. Próximo corte viable: audio local
de mayor duración con buffering continuo acotado antes de ampliar proveedores.

## WAV largo: Linux comprobado, corrección Android en validación

Código `16e304a`, CI enfocada [41154415161](https://github.com/Eirom16/spotifast/actions/runs/41154415161) y harness offline `41154415185` aprobados. Linux descargado/verificado: reproducción continua >23 s de WAV PCM16 de 120 s, buffer PCM de 65536 bytes/8192 frames, cero vaciados propios y cero notificaciones xrun en esa muestra. Pausa, seek a ~90 s, Next/Previous, EOF y cierre con decoder activo aprobados; cierre 67,24 ms. Xvfb/Mesa software, salida nativa, sin captura acústica. Muestra puntual: RSS 124744 KiB y CPU 520 ms/1100,14 ms. Evidencia: `target/native-long-ui-16e304a/`.

El lector general también pasó `--wav-audio-probe` con un archivo estéreo original CC0 de 120 s/16 kHz, convertido a salida de 48 kHz, controles y cancelación; muestras positivas con ganancia 0,08 y cero muestras no nulas al silenciar. Pico RSS del proceso CLI 16404 KiB. Evidencia y procedencia: `target/wav-stream-fixtures/`. Instalado como Doremi Native Dev (`target/native-installed-16e304a.json`), SHA-256 `062b876b0f3d50016b5fd735aa9b4b1a050f9d8d114a918333b086abc2e845a3`. Sigue pendiente el selector/importación de archivos del usuario; el CLI es un probe, no una biblioteca general.

Android `16e304a`: APK descargado y actualizado conservando datos, SHA-256 `db109761c24bf906d4284b9eecb23061e2c7f9b0b381544d83adc7bd15d1c759`. La suite física falló en el nuevo WAV largo con `Decode(Input)`, tras los controles de audio corto. Una prueba mínima confinada a un directorio de QA propio confirmó `ln: Permission denied`; los favoritos permanecen idénticos al snapshot anterior. No se declara aprobado este corte Android. Evidencia de fallo: `target/android-qa/16e304a1cc77/`.

Corrección `a39ce24`: Android crea un archivo regular exclusivo y lo desvincula inmediatamente; el descriptor seekable se conserva hasta cancelar/cerrar y no deja un archivo parcial nombrado. No cambia políticas del teléfono ni depende de enlaces duros. Test Linux de esta ruta anónima, cancelación, 11 tests player y Clippy ligero aprobados. CI enfocada `41155541994` en curso; falta instalar y repetir la suite física antes de aceptar la corrección. El canal PCM y la lectura/resampling siguen compartidos; no se anuncia reproducción acústica medida, importación general, streaming remoto ni MediaSession.


## WAV largo aceptado en el Android físico

`a39ce24`, CI enfocada [41155541994](https://github.com/Eirom16/spotifast/actions/runs/41155541994) aprobada. APK SHA-256 `46bb6bb7a3bbb6740f3115f0f9f59115b2fda49e1c2ccb5f91bc7dc8c353d902`, instalada sin borrar datos. Suite instrumentada PASS con 22 capturas: entrega continua >23 s, seek ~90 s y al final, pausa estable, Activity recreada, Next/Previous/EOF/cancelación y regresión anterior completa. Buffer PCM 65536 bytes/8192 frames; 1189032 frames y 2377336 muestras de salida no nulas a 48 kHz en la muestra larga, con ganancia 0,08. Cero vaciados propios/cero avisos CPAL en esa muestra; la muestra online tuvo un aviso xrun inicial en frame 0. No se generaliza ausencia de cortes a todas las ejecuciones.

Muestra larga: PSS 42758 KiB, CPU 652 ms/1100 ms. Es una ventana puntual del proceso completo; se investiga el coste del decode/buffer antes de añadir complejidad. Primera apertura después de actualizar: `am start` 2053 ms, no primer frame/audio. Log propio sin errores críticos. Restauración de favoritos en proceso nuevo también PASS (2 capturas); no eliminó el favorito Commons existente. Evidencia: `target/android-qa/a39ce24877f2/{instrumentation,library-restore}/`.

Linux del mismo SHA repitió la suite WAV: >23 s, buffer de 64 KiB, pausa/seek/Next/Previous/EOF/cancelación y cierre con decoder activo (84,01 ms). Muestra: RSS 126340 KiB, CPU 550 ms/1100,11 ms, cero vaciados y cero xruns observados. Instalado con lanzador Doremi Native Dev y SHA-256 `eca3cdbbb04901e15869886527506ec1f17e7e2cde57cd7e200f789f0fb09a1e`. Evidencia: `target/native-long-ui-a39ce24/`, `target/native-installed-a39ce24.json`.

Ahora puede probarse Inicio → Probar WAV de 2 minutos en laptop y teléfono. Es una muestra original CC0 repetida, leída de un archivo real por un worker con memoria acotada, no una biblioteca de música arbitraria ni audio acústico medido. El `--wav-audio-probe PATH` de Linux sí abre un WAV PCM16 regular explícito; el selector/importación de producto sigue pendiente. La Preview y los perfiles Spotifast se conservan.
