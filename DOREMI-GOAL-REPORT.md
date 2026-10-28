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
