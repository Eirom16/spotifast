# Doremi Android background playback

**Veredicto: BACKGROUND_WORKING.** APK instalada y reproducción/control verificados en el Samsung físico.

## Implementación y SHA

Servicio Android `mediaPlayback` en foreground con `MediaSession`, notificación `MediaStyle`, audio focus y receptor de desconexión. Un actor Rust de proceso conserva el motor `doremi-player`, cola, caché, biblioteca y persistencia. Activity/OpenGL pueden desaparecer; comandos acotados y serializados y snapshots con generación conectan UI y sistema. Se mantienen minSdk 26 y targetSdk 36.

El descarte de un frame egui en tránsito podía terminar el actor al recrear la Activity. Se corrigió su propiedad y limpieza de texturas y se añadió una regresión para los dos cierres posibles del canal. La pasada física final verifica esa corrección.

- Rama: `feature/doremi-android-background-playback`, desde `4a774c5`, que contiene `cac4786` y el rescate de reproducción.
- **Código y APK verificados: `d9ee8681a339faedb63e36c8cc1fa3486155f8d9`.**
- SHA256 del APK instalado: `e95c24dfcec5c42441d1557cfec119c9cad0bdae05481350c532ce0ce0ad1702`, idéntico al artefacto de CI.
- Recolector final: `a72fed7`; tolera hilos retirados durante Stop y rechaza snapshots de procesos muertos. Su cambio posterior no modifica el APK.

## Archivos modificados

- Android: `AndroidManifest.xml`, `MainActivity.java`, nuevo `PlaybackService.java`, `DoremiInstrumentation.java`.
- `doremi-app/src/`: `android_native.rs`, nuevo `android_runtime.rs`, `lib.rs`, `artwork.rs`, `music_catalog.rs`.
- Núcleo: `doremi-core/src/queue.rs` conserva la petición al reanudar una carga pausada; `doremi-library/src/lib.rs` identifica el hilo de persistencia.
- Validación: `scripts/android-background-qa.py`, `scripts/verify-android.py`, `.github/workflows/android.yml`.

## Resultado físico

Samsung SM-J701M, Android 9/API 28, ARMv7. Instalación `adb install -r`, sin borrar datos, con perfiles privados de QA.

**Instrumentación y recolector final: PASS.**

- Pausa de 32 segundos: progreso detenido, WakeLock liberado y 147 ms de CPU del proceso, aproximadamente 0,46 % de un núcleo durante esa medición.
- Recreación de Activity y eliminación de su tarea de recientes sin perder reproducción. Los 63 muestreos conservaron una instancia y un propietario de reproducción; 44 observaron reproducción con display OFF.
- `qQzdAsjWGPg` hasta EOF natural y avance a `_d3SaQI_Ag4`; tramo continuo con pantalla apagada de **263,91 segundos**. Buffer PCM de 65.536 bytes, cero underruns del buffer y un xrun al inicio (frame 0).
- Play/Pause desde la notificación, Next/Previous/Seek desde MediaController, sincronización al cambiar desde UI, reapertura y Stop. Servicio/sesión/notificación liberados; controlador retirado rechazado.
- AudioManager real: pérdida transitoria, ducking, pérdida permanente y pausa voluntaria respetadas.
- El mantenedor confirmó escucha continua de esta misma APK con la pantalla apagada. No hubo grabación acústica.
- Desconexión **física Bluetooth** verificada en la pasada inicial `0d6b4ea`: pausa, progreso detenido y ausencia de reanudación espontánea. El broadcast sintético fue rechazado por Android y no cuenta como prueba superada.

Los hashes de biblioteca, listas, historial y sesión habitual coinciden antes/después. Las pasadas interrumpidas/fallidas se conservan separadas; el veredicto se apoya en la ejecución final completa.

## Checks y evidencia

[CI del código verificado](https://github.com/Eirom16/spotifast/actions/runs/45446165665): **Android ARMv7/ARM64, Linux y Windows: PASS**, incluyendo empaquetado, tests, Clippy y QA X11 de Linux. Local: circuito ligero, 38 tests/doctest del núcleo, 71 tests de app, 22 de biblioteca y Clippy; un test largo de red/audio quedó ignorado por defecto.

Evidencia fuera del repositorio: `~/.local/share/doremi/product-qa/2026-11-23-background/final-d9ee868-3/`. Contiene resultado JSON, log PASS, contadores/recursos, hashes, comprobación del APK instalado y capturas de notificación, bloqueo y reapertura. Bluetooth inicial: `bluetooth-preliminary/`.

## Límites

Llamada telefónica real y hardware Android 14–16 no probados; sí se ejercitaron las pérdidas de foco correspondientes mediante AudioManager. No se promete continuidad tras force-stop o muerte real del proceso. WakeLock acotado a carga/reproducción, con concesión renovable de diez minutos; pausa abandona foreground tras treinta segundos y cierra el servicio tras cinco minutos conservando el bookmark. No se publica una release.
