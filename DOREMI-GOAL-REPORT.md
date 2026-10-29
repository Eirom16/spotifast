# Doremi: desarrollo Android real y Linux

Base Preview validada: `7b9404c58f947d86b2ea51a6a360d3872edb6f46`.
Rama autorizada: `feature/doremi-android-real`. Sin cambios en main ni releases.

Teléfono detectado por ADB: Samsung SM-J701M, Android 9/API 28,
ARMv7 (sistema de 32 bits), 720×1280, densidad efectiva 300 dpi,
MemTotal 1.843.892 kB. Depuración USB autorizada. No registrar número de serie.
Se construyen ARMv7 para este teléfono y ARM64 para el gate de empaquetado.

Estado actual (2026-10-29): APK `50030dc` instalada y suite completa ADB/instrumentada aprobada en este teléfono. Comparte vistas egui, core y persistencia con el host eframe Linux. El host Android usa Activity/GLSurfaceView, no el GameActivity fallido inicial. P0/P1 y el primer corte de audio local P2 tienen evidencia ejecutada: dos muestras originales CC0, Play/Pause/seek/volumen/Next/Previous/EOF/cancelación, foco y lifecycle. Las cuatro canciones de metadatos siguen siendo simuladas y silenciosas.

Linux `783501b` está instalado como **Doremi Native Dev**, con probe y clics XTest sobre salida nativa aprobados. La CI completa `40795008801` de `86aa04b` aprobó nueve jobs, incluida la Preview conservada. ARM64/16 KiB se verificaron en empaquetado; el hardware Android probado es ARMv7/API 28 con páginas de 4 KiB. No se atribuyen pruebas a API 26/36, ARM64 o Windows nativo Doremi.

Pendientes: reproducción acústica medida (no se usó micrófono), audio arbitrario/streaming, MediaSession/background, proveedores y cuenta opcional. El historial siguiente conserva resultados y fallos de cada iteración; sus notas de pendiente se refieren a aquel momento.

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
