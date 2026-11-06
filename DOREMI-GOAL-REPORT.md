# Doremi: desarrollo Android real y Linux

Base Preview validada: `7b9404c58f947d86b2ea51a6a360d3872edb6f46`.
Rama autorizada: `feature/doremi-android-real`. Sin cambios en main ni releases.

Dispositivo actual de pruebas: Lenovo TB-X306X, Android 10/API 29,
ARM64, 800×1280 físicos, 160 dpi, páginas de 4 KiB y 2.796.952 KiB de RAM.
USB autorizado, sin registrar número de serie. APK `3479575` instalada y
abierta con biblioteca ARM64. WAV y FLAC `3479575` aprobados físicamente
con siete capturas por formato y restauración en procesos nuevos; la suite
general está en repetición tras corregir sincronización de ventanas del harness.
El Samsung SM-J701M anterior conserva su validación WAV `02b686b`, separada
por modelo/SHA; no está conectado.

Estado actual (2026-10-31): Linux Native Dev `ec06121` permite importar WAV y
FLAC mediante selector y arrastre, con controles y restauración comprobados.
Android WAV fue validado en Samsung; WAV y FLAC ya están probados en el Lenovo.
Ambos usan el mismo Model/cola/índice y canal PCM acotado.

CI enfocada `41247046616` sobre `ec06121`: ambos hosts aprobados (15 tests
library, 23 app, 16 player, Clippy y empaquetado). Regresión completa
`41248095448` aprobada: nueve jobs, incluida Preview y legado Windows/macOS.
Preview instalada conservada; archivo de ese mismo SHA descargado y verificado.
CI enfocada `41261387968` sobre `3479575` aprobada en ambos hosts.
Rust reutilizado de `ec06121`
solo tras comprobar igualdad de todas sus entradas; no incorpora MP3.

ARM64 se ejecuta ahora en el Lenovo, con páginas de 4 KiB. La compatibilidad
con 16 KiB tiene comprobaciones estáticas de empaquetado; no hay ejecución
sobre hardware de 16 KiB. Linux usa salida nativa y UI Xvfb/Mesa software;
no se atribuye a Wayland/GPU física ni a un nuevo host Windows Doremi.

Pendientes: más formatos, MediaSession/background, streaming remoto continuo, proveedor productivo y cuenta opcional. La CPU del WAV largo en el teléfono merece optimización; no se confunde con consumo de batería. No se capturó salida acústica ni micrófono. El historial siguiente conserva pruebas, fallos y correcciones; sus pendientes corresponden a cada iteración.

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


## CPU del audio y seek entre ocurrencias

`03ef929`, CI enfocada `41211398464` y harness `41211398435` aprobados; APK descargada/verificada/instalada, SHA-256 `4602c6f7e43c35d7770ce3017c4b29183175c9b3c63809b1c32eca40d4e9cf8e`. La suite física volvió a pasar 22 capturas. La optimización específica de doremi-player en dev y el timestamp registrado una sola vez bajaron la muestra de CPU del WAV largo de 652 ms/1100 ms a 289 ms/1101 ms; PSS 44953 KiB, sin vaciados propios ni avisos CPAL en esa ventana. Son muestras independientes, no batería ni benchmark térmico controlado. El driver limitado a los hilos propios observó doremi-decode en 80/71/93 ticks antes y 16/16/17 después, en ventanas de ~2,7 s; no presupone frecuencia de ticks. Ambos estados pausados no acumularon ticks en esas ventanas. Datos: `target/android-qa/wav-optimization-comparison.json` y sus reportes fuente.

Linux de este SHA pasó el probe de archivo estéreo; el primer QA de UI falló esperando el siguiente tema después del seek al final y una repetición pasó. No se oculta la intermitencia: un caso específico manteniendo el puntero en EOF durante 3 s reprodujo consistentemente que el gesto pasaba al siguiente tema y lo saltaba. Evidencia previa: `target/native-seek-boundary-before-03ef929/`; el fallo y la repetición general se conservan en `target/native-long-ui-03ef929*`. Por ello no se acepta este SHA como cierre del gesto de seek aunque la suite física ordinaria pasase.

Corrección `539fdbf` en validación (`41212891370`): identidad del slider vinculada a ocurrencia e identidad neutral del medio, para que un gesto anterior no se aplique al siguiente. Incluye test egui de puntero mantenido con cambio de ocurrencia y driver XTest específico. Falta aprobar CI y repetir el caso en Linux y Android antes de cerrarlo. La importación/selección general de archivos sigue siendo el siguiente corte funcional; no se implementa autenticación prematuramente.


## Seek mantenido corregido y entrega instalada

`539fdbf`, CI [41212891370](https://github.com/Eirom16/spotifast/actions/runs/41212891370) aprobada. APK SHA-256 `a4e1a782f2f4e061efd053ab4145b2d75c5185a6f93bd823ba122c320dd9b73b`, instalada sin borrar datos. Suite ordinaria PASS con 22 capturas; restauración de biblioteca en proceso nuevo PASS con 2 capturas. La muestra larga entregó 1184648 frames, PCM 65536 bytes, PSS 38146 KiB, CPU 298 ms/1101 ms, cero vaciados propios/cero avisos CPAL en esa ventana. El primer callback de esa ejecución tardó 1192755 µs e incluye generar/sincronizar el WAV original: no equivale al tiempo de un archivo ya existente ni a audio acústico.

El caso ADB adicional mantuvo el seek en EOF 3000 ms y comprobó la siguiente ocurrencia reproduciéndose a 2480 ms, con muestras de salida positivas. El mismo caso antes (`03ef929`) había terminado en Stopped saltando ambas pistas. Evidencia: `target/android-qa/{03ef929fc912,539fdbf2f8ba}/thread-held-seek-*.json`. El caso corregido tuvo un aviso CPAL en frame 1924 y la restauración un aviso en frame 0; no se ocultan ni se equiparan a vaciados del canal PCM propio. Sin micrófono, acústica ni batería medidos.

Linux pasó tanto el caso específico de gesto mantenido como el recorrido WAV completo. La captura antes muestra Stopped sin selección; después muestra el siguiente arpegio a ~2 s/Playing. Evidencia: `target/native-seek-boundary-{before-03ef929,after-539fdbf}/`, `target/native-long-ui-539fdbf/`. Instalado como Doremi Native Dev, SHA-256 `cd3f45cb7565c317eddcfb0e4376de32e5ed7b7f31fd00d871dc8e3f175d8708`. Ya puede probarse Inicio → Probar WAV de 2 minutos en ambos hosts. Siguiente corte activo: entrada de archivos WAV reales y sus localizadores privados persistentes, separados de IDs/metadatos/cola. El índice nuevo está en desarrollo local; aún no se anuncia importación funcional.

## Importación WAV real en Linux y nueva validación física

Código `d07e76a9885f3f0fff3e37287c0d49059ca4fa03`, CI enfocada
[41221845085](https://github.com/Eirom16/spotifast/actions/runs/41221845085)
aprobada. `23d8996` falló por usar el campo antiguo de DroppedFile; `b594aed`
corrigió el acceso al método de egui fijado y la publicación privada del índice.
No se rebajaron los gates ni se modificó la Preview.

El selector Linux y la inspección WAV corren en un worker acotado, cancelable
y con generación/deadline. El Model compartido conserva la única cola y
biblioteca; el índice separado guarda localizadores privados, no URLs ni
credenciales. Las escrituras del índice (0600) preceden a sus referencias y
un error impide publicar datos colgantes. El callback sigue sin IO.

Prueba reproducible: `scripts/native-file-import-qa.py`, evidencia
`target/native-local-import-d07e76a-drag2/report.json`. Se generó exclusivamente
una derivación de la escala original CC0: WAV estéreo, 16 kHz, 120 s,
SHA-256 `c7d146d2625aab23837fe07411c3a3437c129baee9ac4193343b8f9cbe8b8ecf`.
Se comprobaron selector nativo y texto Unicode, Escape/cancelación y recolección
del hijo propio, WAV inválido sin mutación, PCM real, pausa/seek, duplicados por
ocurrencia, Previous/forward, quitar favorito sin borrar fuente ni cola,
reutilización de identidad, reinicio sin apertura/red automática, temas y
arrastre GTK/Xdnd real mediante XTest hasta salida PCM. El original conserva su
hash. Capturas revisadas en el directorio de evidencia, fuera de git.

Los primeros intentos de QA fallaron al heredar el portal de la sesión y al
retornar el puntero desde GTK sin movimiento posterior a CursorEntered.
Se aisló el bus/display/datos GTK de QA y se verificó posición egui antes del
toque. El mismo caso inválido-primero pasó con `b594aed`, anterior a los
nuevos diagnósticos, descartando atribuir el arreglo a un cambio del botón.
El primer helper de arrastre importó Gdk 4 por defecto; se fijó Gdk/Gtk 3 y
se repitió con éxito. No se alteró la configuración de seguridad del sistema.

Reconexión física: mismo Samsung SM-J701M autorizado, API 28/ARMv7. APK
SHA-256 `89f113710cf35e3a89af4b50d5b2343fdff5bb621f9d11362a57cdd3cb61ebf0`,
actualización ADB exitosa sin borrar datos. Suite ordinaria y restauración:
`target/android-qa/d07e76a9885f/{instrumentation,library-restore}/summary.json`,
ambas PASS. Favoritos previos preservados; el caso de quitar uno preexistente
se omite deliberadamente. Muestra WAV largo: 1.192.328 frames, volumen 0,08,
48 kHz, 65.536 bytes PCM, cero buffer_underruns y un aviso CPAL en frame 1924.
Restauración online: un aviso CPAL en frame 0. Sin errores críticos detectados
en logs del proceso; no hay medición acústica ni promesa de ausencia de cortes.

Linux instalado como Doremi Native Dev, SHA-256 del binario
`708deaf406bdf0a0b09a17e3a7d6206e34eaeab579cbfecd4952faddbf54e311`.
La UI se verificó en Xvfb/Mesa software, no en Wayland/GPU física. ARM64/16 KiB
continúan como gate estático, no ejecutado en este teléfono ARMv7/4 KiB.
Siguiente corte: importación Android mediante selección explícita del usuario,
sin permiso de almacenamiento general, sin recorrer archivos personales y
reutilizando el Model/Index/engine compartidos. Cuenta opcional sigue posterior.

Regresión completa del software instalado: `41223684373`, nueve jobs verdes
sobre `d07e76a`. Preview del mismo commit descargada con gh y verificada:
`target/downloaded-ci/preview-d07e76a/verified-archive.json`; archivo externo
SHA-256 `5f29ff800a05e0305270223d16a6994178e8feb018ab40605d224714b05318d2`,
binario `c96bd474cc590fd7e8c40088b88f2da98f2910ac23fc92b216bb705c32279b30`.
Hashes internos, commit, ELF64 x86_64, versión 0.1.0/audio false/release false
correctos. No se sustituyó la Preview instalada. QA Linux del WAV largo
`target/native-long-ui-d07e76a-regression/report.json` PASS: >23 s de PCM,
65536 bytes, pausa/seek/Next/Previous/EOF/cancelación y cierre normal; muestra
Xvfb/Mesa software, sin medición acústica ni de GPU física.

Candidato Android `e144f20`, aún en CI `41239277138`: selector del sistema,
lector JNI solo en worker y copia privada de documento regular, no URI
persistida. Identidad por SHA-256 del contenido, sin sobrescribir copias
existentes ni originales; límite 256 MiB, scratch 8 KiB y cleanup exclusivo
del staging propio. `sha2` 0.10.9 MIT/Apache-2.0 ya estaba fijado en Cargo.lock;
se revisó su origen crates.io y actualizó el inventario al hacerlo dependencia
directa de library. Quince tests library, Clippy y checks ligeros aprobados.
La selección y el audio Android de este candidato todavía NO están validados.
Se prepara QA del selector con carpeta que solo contiene fixtures CC0 propios,
sin capturas del selector ni enumeración de Recents personales.


Candidato base `e144f20`: CI `41239277138` con ambos hosts aprobados.
APK descargado con gh, hashes/manifest/ELF verificados, SHA-256
`91a70758382acd52ccd5b2b801237c5a168fc866bff8cd79f641c97dae1babee`.
Actualización ADB y lanzamiento correctos; suite física completa PASS, 22
capturas, sin errores críticos detectados y favoritos anteriores conservados:
`target/android-qa/e144f2056f52/instrumentation/summary.json`. Captura propia de
Biblioteca revisada: el botón Importar WAV y su alcance PCM16/256 MiB aparecen
correctamente. No se atribuye a esa suite una selección SAF que aún no ejecuta.

Linux del mismo `e144f20` pasó la suite real de importación/arrastre/restauración:
`target/native-local-import-e144f20/report.json`, y quedó instalado como Native
Dev conservando datos y builds anteriores. La regresión completa del legado y
Preview verde sigue correspondiendo a `d07e76a`, no al candidato posterior.

`4e23cb1` añadió QA específica y conservación del resultado pendiente; su CI
`41240158536` se canceló por la siguiente corrección. `6ea9fa6` evita que
suspender una carga online cancele un intento independiente de documento, con
un test sin IO/red. CI vigente `41240283311` en curso; checks ligeros locales
aprobados. La prueba específica del selector se ejecutará solo con su APK
verificado y una carpeta nueva con muestras CC0 propias. No se publican releases.


Cierre de CI Rust `6ea9fa6`: `41240283311` aprobada (15 tests library,
23 app, 13 player y gates de ambos hosts). La primera prueba SAF física
`target/android-qa/6ea9fa688890/documents/` FALLÓ al abrir ZArchiver como
selector predeterminado: el test no encontró su carpeta y no seleccionó ni
capturó archivos ajenos. Se cerró únicamente la solicitud abierta por Doremi.

`2dfb68a` selecciona un manejador de sistema para OPEN_DOCUMENT, prefiriendo
DocumentsUI, con una consulta de paquetes limitada a esa acción. No modifica
valores predeterminados ni añade permisos de almacenamiento. Reutiliza las
bibliotecas Rust verificadas `6ea9fa6` con comprobación obligatoria del
fingerprint; CI `41241330682`, job Android aprobado. En el teléfono el test
sí encontró su carpeta y fixtures; falló al esperar que un solo Back cerrara
un selector que primero navega a la carpeta superior. Biblioteca/índice no
cambiaron; evidencia `target/android-qa/2dfb68ae7d6b/documents/`.

`e5b452a` corrige solo QA: Back acotado mientras la ventana pertenece al
selector propio y eventos de archivo con SOURCE_TOUCHSCREEN explícito.
CI actual `41242019455`, Android aprobado y host Linux todavía en curso.
La ejecución automática de cada push Java se reemplazó por una dispatch con
`native_run=41240283311`, para verificar/reutilizar el Rust idéntico y no
recompilar ambas ABI innecesariamente. Los runs cancelados no se registran
como aprobados. Las capturas SAF siguen limitadas a la superficie de Doremi.


Importación física funcional `e710e95` (APK con Rust `6ea9fa6` reutilizado y
fingerprint verificado), CI `41242616062` aprobada. El modo `document-import`
pasó: selector de sistema/carpeta propia, Back acotado, rechazo de WAV PCM24 sin
mutar favoritos, nombre Unicode, copia privada, PCM estéreo 16 kHz de 120 s,
pausa/seek, misma identidad al reimportar, recreación pausada y claro/oscuro.
Cinco capturas propias en
`target/android-qa/e710e951a1c2/documents/document-import/`. La muestra de
1101 ms midió 214 ms CPU total del proceso y PSS 38.760 KiB; 65.536 bytes PCM,
86.580 frames, cero vaciados y cero avisos CPAL en esa ventana. Otro estado
posterior al reimportar registró un aviso CPAL en frame 0, sin ocultarlo.

El guard del modo `document-restore` falló al buscar el título en `saved_media`,
un resumen diagnóstico que solo contiene proveedor/ID. El nuevo proceso sí
conservaba el ID y el índice, estaba pausado y sin audio ni worker de red.
Una comprobación independiente ADB, con APK/hash verificados, geometría propia
y guardas de foco, cerró y reabrió únicamente Doremi, comprobó ausencia de IO
automática y volvió a entregar PCM desde ese mismo ID al pulsar Play:
`target/android-qa/e710e951a1c2/manual-document-restore/report.json` PASS.
La copia interna coincide con SHA-256
`c7d146d2625aab23837fe07411c3a3437c129baee9ac4193343b8f9cbe8b8ecf`,
modo 600. El original de prueba conserva su hash. Sin captura acústica.

La captura del WAV rechazado mostró un defecto visible: el mensaje quedaba
fuera de pantalla debajo de las tarjetas. `02b686b` lo coloca junto al botón de
importación en ambos hosts, conservando layout automático/touch 48/gap 8.
También corrige el guard de restauración para consultar la colección durable
y requerir el mismo ID en el modelo, y exige geometría visible del error.
Checks ligeros aprobados; CI `41243539529` en curso. Todavía no se declara
aprobada la suite específica completa de ese candidato ni su corrección visual.


## Importación WAV cerrada en ambos hosts

`02b686be38bd44030bee2017f4de7322ccd362ff`, CI enfocada
[41243539529](https://github.com/Eirom16/spotifast/actions/runs/41243539529)
aprobada. APK SHA-256 `d81478fa7ac55a3d31b19fcc4517283a4bdc28f3a03871401fa688fcae36dbf5`,
verificado/actualizado en SM-J701M sin borrar datos. Suite SAF completa PASS:
`target/android-qa/02b686be38bd/documents/summary.json`, siete capturas propias.
Cancelación, rechazo PCM24 con error visible, selección Unicode, salida real,
pausa/seek, recreación, identidad única al repetir, copia privada por hash y
restauración en proceso nuevo sin IO automática aprobadas. El guard ahora
consulta metadatos durables y exige el ID en el modelo; no asume títulos en
el resumen de diagnóstico. Original intacto, mismos favoritos preexistentes,
sin permisos de almacenamiento ni cambios de valores predeterminados.

La regresión física ordinaria también pasó con 22 capturas y sin errores
críticos detectados:
`target/android-qa/02b686be38bd/instrumentation/summary.json`.
En la muestra del WAV importado: 277 ms CPU total de proceso/1101 ms,
PSS 33.130 KiB, 84.488 frames, PCM 65.536 bytes, cero buffer_underruns y un
aviso CPAL en frame 0. No se interpreta como CPU exclusiva del decoder,
benchmark acústico, FPS ni batería. Capturas revisadas de error, claro y oscuro
y reproducción tras reinicio; la limitación de captura acústica permanece.

Linux: `target/native-local-import-02b686b/report.json` PASS, selector GTK,
arrastre real Xdnd, errores, controles, duplicados, restauración y cierre normal.
Instalado como Doremi Native Dev; binario SHA-256
`044fa4dd3083aa27744a74eedd6dc36c087ecd39e520f777aabc537b46090f13`.
Xvfb/Mesa software y salida nativa, sin atribuirlo a GPU física o Wayland.
Regresión completa `41244383738` en curso; no se publica ninguna release.
Siguiente prioridad viable: ampliar formatos locales y controles de plataforma,
con el mismo dueño de cola/identidad/persistencia y datos anteriores protegidos.

Cierre de regresión WAV: CI completa `41244383738` aprobada, nueve jobs sobre `02b686b`. La Preview del mismo commit se descargó para verificación; su instalación previa se conserva. El siguiente trabajo FLAC permanece experimental y aún no se atribuye al software instalado.


Preview `02b686b` descargada/verificada sin sustituir la instalada:
`target/downloaded-ci/preview-02b686b/verified-archive.json`, SHA-256 del archivo
`de3af4c5086819cde01997bcfbd4bf87693b263ad27c732058182e4209bb4c60`,
binario `681095368214788003400ba5bb24ea5f5a60c14a48b6976abf29f9e43d1d7dc6`.
Coinciden hashes externos/internos, commit, versión 0.1.0 y ELF64 x86_64;
audio false/release false. CI completa `41244383738` verde.

Corte FLAC experimental `ec06121`, CI enfocada `41247046616` en curso.
Symphonia 0.5.5/MPL-2.0 sin modificar, metadatos acotados, un bloque por vez,
la misma interpolación y canal PCM fijo, detección por contenido y los mismos
IDs/cola/persistencia. Quince tests player con FLAC+Ogg y Clippy, más checks
ligeros, aprobados localmente. Paridad exacta con el WAV CC0, seek/remuestreo,
EOF y truncación probados; no se afirma todavía reproducción FLAC nativa.
ADB dejó de detectar el teléfono tras cerrar el hito WAV; se continúa con CI
y Linux y se reanudará Android cuando vuelva a estar disponible. No se ha
instalado el candidato FLAC ni se considera validado físicamente.


FLAC Linux `ec06121`: CI enfocada `41247046616` aprobada. QA real
`target/native-local-flac-ec06121/report.json` PASS: FLAC estéreo/16 kHz/120 s
original CC0, selección Unicode, arrastre GTK/Xdnd, pausa/seek, duplicados,
Previous/forward, conservación de fuente/cola/favoritos y restauración en
proceso nuevo. Capturas revisadas; render Xvfb/Mesa software y salida nativa,
sin medición acústica ni de GPU física. La regresión WAV del mismo binario
`target/native-local-wav-ec06121/report.json` también PASS.
FLAC SHA-256 `be57f0f12ef2216bbaf315fa86ed936953620ad334109d0aed474446e6a89735`,
derivado exclusivamente del original CC0; no se descargó música ajena.

Linux instalado: binario SHA-256
`6ee5a9779d166e44769ed290116551d5c5c248dca6dcfb0156633d6a7168ed78`,
con datos/builds previos conservados. El canal de salida sigue en 8192 frames;
FLAC añade buffers de codec/conversión acotados a un bloque y 64 KiB de
entrada, no una copia PCM completa. No se presenta 64 KiB como RAM total del
proceso ni de todos los buffers del decoder.

APK `ec06121` descargado automáticamente, hash/manifest/ABI/alineación
verificados: SHA-256
`c86663014e5b7244c8497e82ea59fe0f8cb1e5e963e6e407d25b1d28cb9353f6`.
No instalado ni ejecutado: no hay dispositivo USB accesible. El teléfono
mantiene la APK WAV `02b686b` probada; se retomará FLAC sin borrar datos cuando
regrese. Regresión completa `41248095448` en curso, sin releases publicadas.


Nuevo dispositivo Lenovo TB-X306X (ARM64/API 29, páginas 4 KiB):
APK `3479575`, nativo `ec06121`. CI enfocada `41261387968` PASS en ambos
hosts; CI completa del nativo `41248095448` PASS, nueve jobs.
Instalación con firma de desarrollo y actualización conservando datos, sin
permisos de almacenamiento generales. Selector del sistema detectado:
`com.google.android.documentsui`; se amplió exclusivamente la allowlist de QA
que antes reconocía `com.android.documentsui`.

Evidencia real WAV: `target/android-qa/34795754129e/documents-lenovo-audio/summary.json`
PASS, siete capturas. FLAC: `target/android-qa/34795754129e/documents-flac-lenovo/summary.json`
PASS, siete capturas. Selección mediante el selector oficial, cancelación,
rechazo del WAV de 24 bits, identidad estable al reimportar, tema claro/oscuro,
seek/pausa, recreación y restauración con proceso nuevo sin IO automático.
Originales CC0 y copias privadas verificadas por SHA-256; favoritos existentes
conservados. Capturas reales revisadas en claro y oscuro, modo horizontal.

Mediciones breves del proceso completo (no CPU aislada del decodificador):
WAV 398 ms CPU / 1100 ms, PSS 32100 KiB; FLAC 426 ms / 1101 ms, PSS 33740 KiB.
Ambos a 48 kHz de salida, volumen propio 0,08, canal de 8192 frames/65536 bytes,
PCM no nulo, sin buffer-underruns ni xruns reportados durante esas muestras.
El campo `condition` del harness antiguo aún dice WAV para FLAC; el formato
real se comprueba mediante corpus/hash/decoder y se corrige la etiqueta en la
siguiente iteración. No hay captura acústica ni medición de batería/FPS.

Fallos conservados: primer intento sin foco; suite general posteriormente
falló al tocar inmediatamente tras recreación/rotación (sin crash nativo).
Primer documento falló porque una selección simulada previa carecía de slider
de audio. Se repitió tras seleccionar la muestra real mediante ADB, con PASS
de WAV y FLAC. QA `c35a9c1` añade geometría GL coincidente, espera de transición
y precondición de audio real para no depender de otra suite. Su CI
`41262110044` está aprobada: APK y reutilización del gate nativo `ec06121`,
con coincidencia de todas las entradas Rust. `native-host` se omite en esta
iteración Java: el run de origen debe tener ambos gates aprobados y el
paquete se rechaza si cambia una sola entrada Rust. No se declara aprobada
la regresión general del Lenovo todavía.

Preview `ec06121` verificada sin reinstalar: 73 hashes internos, origen/versión
y ELF x86_64; archivo SHA-256 `56394e84e3c8ee9893cff389d192ee256fe8420c92ec2d06c0805bb67a4a4b41`.
Prueba ADB adicional: `target/android-qa/34795754129e/lenovo-adb-smoke/summary.json`
PASS con 110600 frames/221174 muestras no nulas y pausa con reloj estable.


Lenovo, comprobaciones adicionales: `manual-lenovo/report.json` bajo
`target/android-qa/34795754129e/` PASS: navegación, búsqueda/teclado, favoritos,
cola con duplicados/edición, controles demo, acentos, tema, background y
restauración tras cerrar el proceso. `accent-pixel-check.json` confirma por
píxeles que el texto de acento cambia verde/morado y se restaura; no se
acepta únicamente el booleano del diagnóstico. Lanzamiento COLD observado:
610 ms según `am start -W` (no tiempo hasta audio ni primera interacción).

APK `c35a9c1` instalada, nativo `ec06121`.
`target/android-qa/c35a9c15379e/lenovo-streaming-adb/summary.json` PASS: WAV
largo original, más de 23 segundos/1107976 frames, canal 65536 bytes, cero
underruns/xruns reportados, seek en pausa, background/foreground y proceso
nuevo restaurado sin audio automático. PSS estable observado 51272 KiB.
Muestra de CPU de hilos propios: 1202,7 ms de pared, decoder 40 ms, hilo GL
principal 140 ms (contadores `/proc` propios, CLK_TCK del dispositivo).
Condiciones distintas a las muestras instrumentadas anteriores; no son
un benchmark controlado ni una medición acústica. Logs de ambos procesos
sin crash/panic/JNI crítico.

La regresión general `c35a9c1` volvió a fallar al cancelar búsqueda después
de restaurar la orientación original horizontal. La espera de foco/geometría
no basta: sigue abierta la diferencia entre inyección/prueba y host. Capturas
reales vertical/horizontal conservadas; no se declara resuelta la rotación.
QA `ea71b92` cambia a inyección explícita touchscreen con confirmación y
captura del fallo antes de destruir la Activity. Referencia inspeccionada:
[Instrumentation de Android 10](https://android.googlesource.com/platform/frameworks/base/+/refs/tags/android-10.0.0_r47/core/java/android/app/Instrumentation.java).
La confirmación de inyección nunca sustituye las aserciones sobre el estado.

Candidato MP3 `2ca3a85` en compilación remota: CI enfocada `41262350061` aprobada en ambos hosts; completa `41262358298`
aún en curso. MP3 Linux ya está probado e instalado; Android pendiente de instalación. Lectura de
frames acotada para duración exacta CBR/VBR, límites de metadatos/archivo,
preroll acotado de seek para reconstruir el reservorio, entrada cancelable
y mismo resampler/canal PCM compartido. Veintidós tests locales de player
(FLAC/MP3/Ogg sin salida nativa), Clippy y circuito ligero aprobados; los
tests del host y la salida MP3 real están pendientes de CI/hardware.


MP3 Linux `2ca3a85`: `target/native-local-mp3-2ca3a85/report.json` PASS.
Fuente original CC0 estéreo/16 kHz/120 s, selector GTK real y arrastre Xdnd,
PCM nativo acotado, pausa/seek, duplicados, previous/forward, favorito/ID,
claro/oscuro y proceso nuevo sin audio automático; original preservado.
La regresión WAV del mismo binario (`target/native-local-wav-2ca3a85/`) PASS;
FLAC sigue en ejecución. Instalado y disponible como Doremi Native Dev,
binario SHA-256 `ad342cfc2565ec5577111b40ecdeb54177392d0e78e967e459c2ff34e84c32b4`.
Preview, perfiles y builds anteriores conservados. APK `2ca3a85` descargado
y verificado: `85d71411a96b1a2c0732cf24f8d9aaa12e73d42e1138bd487c3c0c3a1a935612`,
min 26/target 36, ARM64 y ARMv7. Se instalará la iteración Java `ea71b92`,
con ese mismo Rust tras verificación de entradas idénticas; CI `41263029245`
PASS en APK/reutilización, host omitido como reutilización explícita.

Diagnóstico adicional de rotación `rotation-injection-diagnostic` no llegó
a rotar: dos ejecuciones fallidas habían dejado cuatro ocurrencias demo:dawn
y la precondición esperaba dos. No hubo toque externo ni resultado aceptado
de esa ejecución. Se retiraron exclusivamente esas cuatro ocurrencias mediante
botones de cola, tras verificar provider=demo/id=dawn en el snapshot propio;
before/after en `prepare-owned-queue`, sin borrar datos ni editar archivos
persistentes directamente. La próxima regresión completa empieza sin esos
fixtures residuales; sus resultados siguen pendientes.
