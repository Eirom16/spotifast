# Doremi: desarrollo Android real y Linux

Base Preview validada: `7b9404c58f947d86b2ea51a6a360d3872edb6f46`.
Rama autorizada: `feature/doremi-android-real`. Sin cambios en main ni releases.

Teléfono detectado por ADB: Samsung SM-J701M, Android 9/API 28,
ARMv7 (sistema de 32 bits), 720×1280, densidad efectiva 300 dpi,
MemTotal 1.843.892 kB. Depuración USB autorizada. No registrar número de serie.
Se construyen ARMv7 para este teléfono y ARM64 para el gate de empaquetado.

En progreso: host Rust/eframe independiente, GameActivity/IME, mismo reducer
de cola y Catalog; preferencias/biblioteca privadas, worker de escritura atómica.
La reproducción inicial es simulada y está identificada como tal.

Pendiente de evidencia: compilación APK, descarga, instalación, pruebas reales,
16 KiB, CI completa, audio local y proveedores. No declarar estos gates
completados por los tests del núcleo ni por la Preview Linux anterior.

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
