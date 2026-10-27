# Doremi: informe F0

## Resultado y límites

Se preparó una migración incremental sin cambiar funciones de la aplicación.
**No se ha eliminado Spotify, no se ha integrado yt-dlp y no hay APK ni release
Doremi.** El gate de baseline completo no está cerrado mientras queden checks
fallidos, incompletos o pendientes en la sección de validación.

Base auditada: commit `cd40da0` de Spotifast 0.14.2. Remoto de trabajo:
`https://github.com/Eirom16/spotifast.git`; `local` es el backlink del checkout.
No se modifica ni se publica en `crmne/spotifast`.

## Alcance de auditoría

Inventario de todos los archivos versionados, manifests, lockfile, módulos,
tests, ejemplos, scripts, assets, documentación, configuración Nix y workflows.
Lectura dirigida de fronteras y contratos de app/model/backend/player/sink,
API/auth/credenciales, almacenamiento, UI y empaquetado. Esto es una auditoría
estructural y de migración, **no una revisión de seguridad línea por línea de
138.133 líneas ni de todas las dependencias transitivas**.

Snapshot legado: 120 archivos Rust, 138.133 líneas incluyendo build script,
ejemplos y tests; 1.481 atributos `#[test]`/`#[tokio::test]` contados
estáticamente. No es un número de tests ejecutados: cfg/features/plataforma,
tests ignorados y doctests cambian la suite real. `src/app.rs` tiene 32.084
líneas, backend 10.099 y demo 11.537. El coste de separación principal es
acoplamiento semántico, no mover archivos.

86 entradas de dependencias directas contando scopes de build/dev/target.
`Cargo.lock` tenía 791 entradas, 746 de crates.io, 44 de siete repositorios
git y un paquete local. F0 añade solo el paquete local `doremi-core`:
792 entradas, sin cambios de versión ni source externo. Inventario completo
de dependencias directas por target/features y patches en
`docs/doremi/dependencies.md`, reproducible y comprobado por script.

## Arquitectura observada

| Zona | Flujo y acoplamiento | Riesgo de migración |
|---|---|---|
| `src/spotifast.rs`, `src/entrypoint.rs` | CLI, shell eframe, tray/IPC, argumentos Spotify, updates y startup. | No es entrypoint Android; identidad integrada en arranque/empaquetado. |
| `src/lib.rs` | Exporta todos los módulos; sin features que aíslen Spotify. | `--no-default-features` solo quita MilkDrop, no hace portable la app. |
| `src/ui/**`, `src/model.rs`, `src/app.rs` | Vistas → `Action` → app; modelos importan `api::models::*`; app coordina auth, librería, cola y reproducción remota/local. | No separar UI copiando modelos Spotify al core. Cola optimista ligada a respuestas remotas y Spirc. |
| `src/backend.rs` | Runtime Tokio en thread dedicado, comandos/eventos y repaint; tareas bloqueantes a workers. Canal de comandos unbounded. | Buen patrón a conservar, pero acotar la nueva carga de procesos/eventos; no afirmar bloqueo de UI imposible por diseño. |
| `src/api/{client,gateway,models}.rs` | Web API, enrutamiento por cuentas/grants, cuotas y fallbacks. | Catálogo no neutral; no convertir estos endpoints a yt-dlp por sustitución textual. |
| `src/auth.rs`, `src/credentials.rs` | OAuth PKCE/loopback, grants separados, store nativo, migraciones y perfiles portables. | No necesarios para nuevo catálogo público; no borrar aún ni migrar secretos al nuevo producto. |
| `src/player.rs` | Session/librespot Player/Mixer/Spirc, SpotifyUri, Premium, reconnect, volumen y queue snapshots. | Reemplazar por motor neutral, no por un proceso yt-dlp que supuestamente reproduce. |
| `src/sink.rs`, `eq`, `limiter`, `resample`, `vis` | Adaptador librespot Sink → rodio/mixer → fastframe-audio/cpal; PCM, DSP, taps y device recovery. | DSP reutilizable tras separar imports y contratos de decoder/output. Callback no debe esperar red/disco. |
| `src/session_reads.rs`, `made_for_you`, `new_releases`, `prerelease`, `app/radio` | Spclient, protobuf, consultas privadas, Spotify IDs, estaciones, amigos y metadatos. | No equivalen a catálogo o recomendaciones yt-dlp. Sin promesa de paridad ficticia. |
| `liked`, `history`, `account_cache`, `device_history`, `settings`, `paths` | Cachés por cuenta, URIs Spotify, sesión/cola, archivo de configuración y persistencia atómica. | Identidad persistida y schemas deben versionarse; no reutilizar carpeta original para Doremi. |
| `local_tags`, `lyrics`, `playlist_cover`, `images` | Symphonia, LRC, LRCLIB/Spotify/tags, artwork y cachés/decodificación en workers. | Reusar parsers/DSP; revisar privacidad, fuente y tamaño de datos externos. |
| `cast`, `sonos`, `zeroconf` | Descubrimiento LAN, Spotify Connect y credenciales/tokens de receptores. | No son salida genérica para URLs yt-dlp. Conservar legado, evaluar aparte. |
| `skin/**`, `ui/winamp/**`, `winamp`, `milkdrop/**` | ZIP/bitmaps/layout, eq y visualización; MilkDrop GL en proceso hijo, memoria compartida y C++. | Skins/UI reutilizables con view models; MilkDrop desktop-only hasta demostrar otra cosa. |
| `bidi`, `i18n`, `emoji`, `autoscroll`, `theme`, `motion`, `notch` | Texto lógico/RTL, 17 catálogos PO, colores e interacción. | Preservar accesibilidad/i18n, verificar fuentes/paths/lifecycle Android. |
| `window`, `appearance`, `mac_*`, `thumbbar`, `power`, `single_instance`, `opener`, `system_fonts` | APIs y convenciones desktop detrás de cfg y fastframe. | Separar adaptadores, no propagar `cfg(not(windows))` como supuesto soporte Android. |
| `src/updates.rs` | Repo `crmne/spotifast`, IDs originales, firma con clave de assets. | Bloqueador de distribución Doremi. F0 no cambia runtime, advierte y archiva publicación automática. |
| `build.rs` | Compila PO; Windows resources y GLEW estático según target. | Nuevo core no tiene build script; futura lib Android no debe heredar recursos/GL de escritorio. |

Reglas existentes de cola: `docs/_reference/queue.md`, tests en app y demo.
La UI muestra intenciones de inmediato y descarta respuestas obsoletas;
una extracción debe conservar duplicados, posiciones y orden de contexto,
no limitarse al caso simple de next. Las visualizaciones consumen señal
post-EQ/pre-volumen; seeks y reemplazos reinician historia DSP pero las
transiciones naturales no lo hacen.

## Dependencias desktop y portabilidad

No todas las bibliotecas siguientes son intrínsecamente incompatibles con
Android. Se distingue plataforma exclusiva de uso/configuración desktop:

| Categoría | Dependencias/módulos identificados | Acción para Android |
|---|---|---|
| Exclusivas Linux desktop | x11rb, zbus, zbus-secret-service-keyring-store; transitivas MPRIS/mpris-server, ksni, Secret Service, X11/Wayland/portals. | No compilarlas para Android; sustituir controles/credenciales/selección por APIs nativas. |
| Exclusivas macOS | fastframe-dock, apple-native-keyring-store/keychain, objc2/AppKit/Foundation, mac_menu/mac_links/mac_notch/guard. | Adaptador macOS, nunca dependency de core ni Android. |
| Exclusivas Windows | windows-native-keyring-store, windows-sys Win32/DPAPI, winresource, thumbbar; CRT estático y recursos. | Mantener target-specific; Android Keystore/MediaSession no son equivalentes directos. |
| Shell de escritorio | fastframe-shell/tray/autostart/titlebar/graphics/update/now-playing/power/instance/dirs; rfd/open y single-instance. | Varios tienen stubs en otros SO, no integración Android. Auditar cada uso; Activity, intents, lifecycle y directorios del contexto reemplazan convenciones desktop. |
| Render/audio con backend portable potencial | eframe/egui/egui_extras, winit/glutin/glow, raw-window-handle, cpal/fastframe-audio, rodio, symphonia. | eframe fork expone android-game-activity/native-activity, pero la app no selecciona backend ni pasa AndroidApp. Elegir EGL/GLES y audio Android con NDK; validar AAC/Opus/WebM/MP4 y seek. |
| Desktop opcional pesado | projectm-sys y transitivas CMake/C++/OpenGL/GLEW, proceso MilkDrop y shared memory. | No llevar por defecto; conservar desktop. API gráfica y proceso hijo no son adaptación automática. |
| Fuentes/assets/I/O dependientes de entorno | fastframe-fonts/emoji/text/icons/theme/i18n/log/file/scroll, skrifa/memmap2, image/tiny-skia/flate2. | Parte lógica reusable; lectura de fuentes de sistema, rutas, assets empaquetados y writes necesitan contexto Android. El core no los importa. |
| Red portable, configuración por revisar | reqwest/rustls/ring/http/hyper-util, tokio, mdns-sd. | CA/proxy, DNS y reachability no se asumen iguales; permisos de Internet/multicast, batería y cancelación. `blocking` nunca en UI. mdns actual pertenece a Spotify LAN. |
| Spotify, no necesariamente desktop-only | todos los librespot/protobuf y crypto/session/auth/acceso privado. | Se retiran por desacoplamiento de producto después de reemplazos, no por fingir que cfg Android los arregla. |
| Soporte general | serde/json, anyhow/thiserror, hashes/crypto, rand, percent/urlencoding, jiff, unicode-bidi. | Mantener cuando un caso de uso real los necesite. No copiar en bloque al nuevo core. |
| Tooling no móvil | polib/directories/test-util, Nix/mise/mbx, Ruby/Jekyll/native-packages y scripts de instalador. | Build/test host; Android necesita toolchain/Gradle/SDK/NDK/firmas aparte. No confundir tests host con instrumentados. |

La revisión de los manifests/cfg de fastframe confirma integraciones para
Linux/macOS/Windows y stubs para algunas plataformas, no soporte Android
funcional. El fork eframe tiene opciones Activity; eso no demuestra que el
conjunto de fastframe, esta app o su accesibilidad sea utilizable en móvil.
Todas las fuentes git siguen exactamente como estaban. La entrada fastframe
usa tag, con commit resuelto en lock; no es un pin de revisión en el manifest.
Migrar a crates.io/pins mejores es tarea separada, con regresiones de Wayland,
RTL, clipboard y recuperación de audio preservadas.

## Android: bloqueos previsibles

1. Falta un paquete `cdylib`, AndroidApp/Activity y proyecto Gradle con
   manifiesto/permisos, package ID, API mínima/target, ABI y NDK fijados.
   Un target Rust Android no produce un APK.
2. Android/Bionic no es Linux/glibc: los binarios Linux aarch64 de yt-dlp,
   Python, JS o FFmpeg no son paquetes Android. Hay que construir/empaquetar
   por ABI y verificar tamaño de página 16 KiB en bibliotecas nativas.
3. Restricciones de ejecutar código desde directorios escribibles y políticas
   de distribución/actualización dinámica. No descargar y ejecutar un binario
   arbitrario en filesDir como solución por defecto. Probar runtime embebido
   o helper Android-compatible empaquetado, licencias y reglas de tienda.
4. yt-dlp requiere Python y, para cobertura YouTube completa según upstream,
   EJS/runtime JS. No hay Python/yt-dlp ni Termux garantizados en el teléfono.
   El spike de F2 es un gate real antes de prometer Android.
5. Archivos mediante SAF/MediaStore/content URI y FD, no solo PathBuf.
   Evitar permisos amplios de almacenamiento. Keystore y secretos no son
   keyring desktop ni JSON exportable.
6. Activity recreable, rotación, pérdida de surface, IME/teclado, back,
   insets/densidad y touch. No duplicar runtime ni cola al recrear vistas.
7. Background playback necesita servicio foreground de tipo mediaPlayback,
   notificación, MediaSession, audio focus, auriculares/Bluetooth y manejo
   de interrupciones. Revisar permisos según API; no pedir todos a ciegas.
8. Doze, red medida, desconexiones y process death exigen checkpoints y
   cancelación, no polling de escritorio constante ni asumir proceso eterno.
9. Validar egui/AccessKit/IME y GPU en dispositivo. Una compilación `.rlib`
   neutral solo cubre arquitectura/MSRV del código neutral.

## yt-dlp: límites y empaquetado

Fuentes consultadas: [README upstream](https://github.com/yt-dlp/yt-dlp#dependencies),
[licencias de sus binarios](https://github.com/yt-dlp/yt-dlp#licensing),
[EJS](https://github.com/yt-dlp/yt-dlp/wiki/EJS),
[eframe](https://github.com/emilk/egui/tree/main/crates/eframe),
[android-activity](https://github.com/rust-mobile/android-activity).
Son documentación viva, no versiones aprobadas para empaquetar. La página
Android de restricciones de ejecución no pudo leerse con fetch por redirección
OAuth; se debe verificar su regla exacta por target SDK en el spike.

- yt-dlp busca y resuelve, no decodifica ni proporciona salida de audio. No
  crea una API de catálogo con garantías de paginación, disponibilidad,
  artistas/álbumes normalizados o sincronización de biblioteca.
- `ytsearchN` y búsqueda plana reducen trabajo pero dejan datos incompletos;
  resolución separada. URLs de CDN expiran y pueden requerir headers/cookies;
  no son IDs permanentes ni datos seguros para logs.
- Extractores/sitios cambian; resultados dependen de región, autenticación,
  rate limits, borrado o restricciones. Respetar negativas, no evadirlas.
  Ninguna promesa de acceso a todo YouTube ni de sustituir canciones Spotify.
- Upstream consultado requiere Python 3.11+. Recomienda ffmpeg/ffprobe,
  yt-dlp-ejs y motor JS (Deno recomendado). FFmpeg se necesita para mezclar/
  postprocesar en esos flujos, no necesariamente para consultar JSON o
  reproducir un formato ya compatible. No añadir transcodificación a F0.
- Python/yt-dlp como proceso desktop evita acoplar ABI Python con Rust.
  Alternativa embebida reduce problemas de ejecución Android, pero aumenta
  integración JNI/packaging; decidir con prototipo Android, no teoría.
- Una app "nativa Rust" sigue necesitando estos runtimes auxiliares. Deben
  tener versiones/checksums y política de actualización explícitos, discovery
  seguro, diagnóstico de ausencia, sandbox y cuotas de salida/tiempo.
- Deshabilitar configs/plugins/componentes remotos ambientales, no shell ni
  `--exec`, no cookies del navegador automáticamente. Sin token providers,
  impersonación o workarounds para saltar controles de acceso.
- La licencia del código fuente yt-dlp es Unlicense; sus ejecutables
  PyInstaller incluyen componentes GPLv3+. FFmpeg varía por build; EJS incluye
  MIT/ISC y Python/runtime JS sus licencias. Revisar la redistribución concreta,
  avisos y fuentes correspondientes antes de empaquetar; MIT no cubre todo.
- Linux: decidir dependencia de paquete o bundle verificable. Windows/macOS:
  helper por arquitectura, firma/notarización y rutas seguras. Flatpak:
  sin asumir PATH host ni ejecutables externos. Android: runtimes y librerías
  por ABI/API, sin ejecutables glibc ni instalación manual obligatoria externa.

## Otros archivos y automatizaciones auditados

- `tests/`: branding/CLI/IPC, localización, Omarchy, iconos, bundle macOS,
  diagnóstico de startup, tray y handoff de actualización. F0 conserva todos.
- Unitarios junto a módulos: auth/rotación de grants, fallbacks/cuotas,
  sesiones/reconnect, cola/UI optimista, DSP, skins y cachés. Demo hace
  cobertura headless; no prueba audio real o GPU. Native-store round trip es
  ignorado por defecto y requiere store disponible.
- Ocho ejemplos: probes Spotify/LAN, generador de skin y updater-inspect.
  No presentarlos como herramientas Doremi sin credenciales.
- `.github/scripts/`: checks de fuentes, evidence, settings, visuales,
  scope y traducciones. Se mantienen; la allowlist `crmne` queda como
  congelación del legado. No se añaden fuentes nuevas.
- `packaging/`, `native-packages.yaml`: AUR, Homebrew, DEB/RPM/AppImage,
  Flatpak, macOS y Windows; nombres/IDs/destinos originales y tests. No
  renombrar con búsqueda global: hay update keys, receipts, URI handlers y
  perfiles instalados. `PACKAGING.md` queda señalado como histórico.
- `flake.nix`/lock, `.cargo/config.toml`, rust-toolchain y mise:
  Nix, compiler pin 1.98.0, MSVC CRT estático y builds compartidos mbx. Se
  preservan, salvo añadir workspace en Cargo. Nix lee versión literal raíz.
- Workflows anteriores: CI multi-OS/Nix/docs, release firmada, packaging de
  fixtures/release, Pages, Flatpak y bots de triage/review. Tenían permisos
  de escritura, reusable actions y políticas/destinos de crmne.
  Se archivan ocho workflows y la config de triage, con comentario de
  cuarentena. CI se reemplaza; original recuperable en Git.
- `docs/` es sitio Jekyll histórico con CNAME/dominios upstream; no se
  despliega en F0. `contrib/omarchy`, assets/traducciones y release notes se
  conservan. F0 también actualiza PR/issue templates y la
  política CodeRabbit para el alcance y mantenedor Doremi; FUNDING y el preset
  Renovate heredados quedan identificados como pendientes de identidad antes
  de abrir el fork a contribuciones públicas.
  No son canales de publicación activos en esta CI.
- MIT original y licencias de assets sin cambios. No se han añadido
  screenshots, grabaciones ni fuentes vendorizadas.

## Cambios F0 y decisiones

1. Sustituir AGENTS, CONTRIBUTING y Copilot por reglas Doremi, sin conservar
   una prohibición Spotify-only que impediría el objetivo solicitado.
2. README con estado real, atribución y advertencia del updater legado.
   Cambiar descripción y añadir `package.metadata.doremi` en Cargo, no
   nombre/bin/lib/versión. La URL repository del paquete legado se conserva:
   la UI la lee con `CARGO_PKG_REPOSITORY` para About/release notes. El primer
   intento de cambiarla produjo un fallo real de regresión (1.394 pasados,
   1 fallido, 3 ignorados); se revirtió ese cambio de URL, no el test.
3. Workspace con resolver 3, root legado y `doremi-core` sin dependencias.
   No mover funciones ni añadir traits especulativas. Lock añade solo ese
   paquete local. Rechazo probado de divergencia de versiones.
4. Inventario reproducible de dependencias y tests de estructura/procedencia.
5. Archivar automatizaciones de escritura para no publicar/triagear como
   Spotifast. Nueva CI valida Linux, core Android y pareja de artefactos
   por commit/versión/hash, con `contents: read`, sin secretos ni releases.
6. Roadmap con fases, dependencias y gates de sustitución. Las retiradas de
   capacidades exclusivas Spotify se decidirán explícitamente, no se
   esconderán como refactor.

Alternativas descartadas: renombrado global (rompe migraciones/updater/
instaladores), retirar librespot ahora (cola y locales dependen de él),
crear diez crates vacíos (apariencia falsa de arquitectura terminada) y
portar el paquete raíz directamente a Android (carga todo el escritorio).

## Validación real

Resultados finales locales de esta sesión; no implica que la CI remota haya
corrido. Las compilaciones frías agotaron varios límites de tiempo, pero los
reintentos posteriores completaron las dos suites y el build nativo. No se
eliminó ni debilitó el test que detectó el cambio inseguro de repository.

| Comando/comprobación | Resultado observado |
|---|---|
| `rustc --version`, `cargo --version` | 1.98.0, toolchain fijada instalada. |
| `cargo metadata --offline --no-deps --format-version 1` | Pasó; reconoce workspace y añadió solo paquete local al lock. |
| `cargo test --offline -p doremi-core` | Pasó: 0 unitarios, 1 doctest. Solo identidad, no prueba reproducción. La CI usa `--locked`. |
| `rustup target add aarch64-linux-android` y `cargo build --locked -p doremi-core --target aarch64-linux-android` | Pasaron; genera `.rlib`, no APK. |
| `python3 scripts/test-f0.py` | Pasó: 9 tests, estructura/inventario/artefactos y rechazos de drift, dirty/hash/SHA/versión. |
| `python3 scripts/audit-dependencies.py --check` | Pasó tras corregir una omisión del inventario. |
| `.github/scripts/check-dependency-sources.py` / `check-review-evidence.py` | Pasaron; se repiten con archivos nuevos indexados, pues consultan `git ls-files`. |
| `python3 .github/scripts/test-policy-checks.py` | Pasó: 27 tests. |
| `python3 packaging/test-release-names.py` | Pasó: 8 tests. |
| `python3 packaging/flatpak/test-metainfo.py` | Pasó: 4 tests. |
| `python3 packaging/test-launchers.py` | Falló localmente: 4 tests, 2 errores por ausencia de Ruby. Posteriormente pasó sin modificar el test en el job `quick` de GitHub, con Ruby disponible. |
| `cargo fmt --all --check`, `git diff --check` | Pasaron. |
| YAML CI, issue templates y CodeRabbit, parser PyYAML 6.0.3 | Se parsearon. CI optimizada también pasó actionlint 1.7.12 sin ShellCheck; no equivale a ejecución GitHub Actions. |
| `cargo test --locked --all-targets` (baseline previo) | Incompleto por límites de tiempo de 200 s y 1.200 s en compilación fría. No llegó a tests. |
| `cargo test --locked --workspace --all-targets` | Pasó tras corregir repository: biblioteca 1.395 pasados, 0 fallidos, 3 ignorados; CLI 14 y suites de integración verdes. Incluye MilkDrop default. |
| `cargo test --locked --workspace --all-targets --all-features` | Pasó: biblioteca 1.435 pasados, 0 fallidos, 3 ignorados; CLI 16 e integración verdes, incluido startup/demo. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Pasó (compilación fría de 14 min 31 s). |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Pasó. |
| `cargo test --locked --workspace --all-features --doc` | Pasó: 1 doctest core; 0 doctests legado. |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps` | Pasó. |
| `cargo build --locked -p spotifast --bin spotifast` | Pasó, binario nativo debug con features default. |
| Integridad del baseline | `src/`, `tests/`, `examples/`, `assets/` y `LICENSE` sin cambios byte a byte respecto de `cd40da0`. Todos los paquetes/aristas originales del lock intactos. |

Logs completos locales, no versionados: `target/f0-tests-default.log`,
`target/f0-tests-all-features.log`, `target/f0-clippy-default.log` y
`target/f0-clippy-all-features.log`. Los subprocess tests de IPC/tray
imprimen summaries adicionales: no sumarlos dos veces como cobertura nueva.
Los tres ignorados son portal desktop, store nativo y ventana X11, requieren
servicios reales. Que un test de assets macOS pase en Linux no prueba macOS.

No ejecutados: GUI real, prueba de audio, native-store ignorado,
tests nativos completos macOS/Windows, Nix, Jekyll y Flatpak runtime,
APK/NDK/dispositivo. Sí se ejecutaron builds nativos macOS/Windows con smoke
`--version`, Linux completo y core Android en CI. Los resultados remotos
finales, tiempos, fallos y caché están en `CI-REPORT.md`.
No se ejecutó ShellCheck. No hay releases públicas. El seguimiento de las
ejecuciones y artifacts de la infraestructura optimizada está en `CI-REPORT.md`.

Entorno: Linux x86_64 CachyOS, 4 CPUs, 7,6 GiB RAM; librerías ALSA/PipeWire/
Pulse/X11/Wayland detectadas por pkg-config. CMake/clang/Python/D-Bus
disponibles; Ruby/Bundler/Nix/SDK manager/cargo-ndk ausentes. yt-dlp
`2026.10.21`, FFmpeg `n9.0.2`, Deno y Node presentes. Solo se consultaron
versiones: no se buscó ni descargó un medio, no se probó reproducción yt-dlp.

### Reproducción

```sh
git rev-parse HEAD
git remote -v
python3 scripts/test-f0.py
python3 scripts/audit-dependencies.py --check
python3 .github/scripts/check-dependency-sources.py
python3 .github/scripts/check-review-evidence.py
python3 .github/scripts/test-policy-checks.py
python3 packaging/test-launchers.py
python3 packaging/test-release-names.py
python3 packaging/flatpak/test-metainfo.py
cargo fmt --all --check
cargo test --locked -p doremi-core
cargo build --locked -p doremi-core --target aarch64-linux-android
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --doc
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
```

Instalar el target Rust Android si falta. Resolver Ruby con autorización o
ejecutar en CI, donde se declara como dependencia. No quitar MilkDrop ni
modificar flags de optimización para presentar otro build como el solicitado.
Comandos de checks adicionales y dependencias en `CONTRIBUTING.md`.
Mantener outputs en `target/` de este worktree, no compartir target ni limpiar
cachés para ocultar resultados.

## Riesgos y siguiente paso

| Prioridad | Riesgo | Gate |
|---|---|---|
| Cerrado en CI | Ruby falta localmente, pero launchers y suites/build requeridos pasaron en runners. | Usar circuito ligero local y CI principal; evidencia en `CI-REPORT.md`. |
| P0 | Updater, IDs y perfiles todavía son upstream. | No distribuir como Doremi; separar antes del primer binario propio. |
| P0 | Python/JS/yt-dlp Android no demostrado ni licencias/distribución resueltas. | Spike Android de F2 temprano, no al final del producto. |
| P1 | Cola/modelos/app/session_reads demasiado entrelazados. | Caracterización y bridge neutral en F1, no refactor masivo. |
| P1 | Formatos/streams yt-dlp exceden soporte de decoder actual. | Selección explícita por capacidades y fixtures en F3a. |
| P1 | Sitios cambian o deniegan acceso. | Errores visibles y actualización verificada, jamás bypass. |
| P1 | CI nueva reduce cobertura multi-OS/Nix/docs heredada. | Recuperar jobs de validación sin publicación antes de distribuir. |
| P2 | Documentación/recetas históricas, FUNDING y preset Renovate aún enlazan upstream. | Revisión de identidad pública en fase de producto; normas/templates Doremi ya actualizados. |

Siguiente fase propuesta: **cerrar validación F0 y después F1, tipos/cola
neutral con pruebas de caracterización**. Ninguna release autorizada por
este informe. La existencia de CI o un doctest verde no cierra los riesgos.

## Infraestructura principal de validación

Tras la petición de optimizar para N5000/8 GB, la compilación pesada pasa a
runners estándar de GitHub. Se separan jobs rápidos, tests, Clippy, builds
nativos y core Android. Cachés codegen/metadata separadas, targets desktop
incompatibles sin compartir objetos, retención corta y gates de espacio.
`scripts/check-local.py` mantiene un circuito cotidiano sin compilar el cliente.
La evidencia de resultados remotos, tiempos y cachés, incluida cualquier
causa de fallo, se registra en `CI-REPORT.md`, no se infiere de tests locales.

Validación final de infraestructura: run GitHub `40563292782`, código
`12d21370dcd6e2e8b3ecac2e91c5025ae95b46f0`, intento 3 **success**,
cuatro artefactos del mismo SHA/versión verificados y caché exacta comprobada.
Tests default: 724,357 s en frío → 64,266 s con caché. El fallo inicial MSVC,
el miss por toolchains preinstaladas y la cancelación por capacidad macOS
están documentados sin ocultarlos. No cambia el alcance de F0: no se ha
implementado reproducción yt-dlp ni una aplicación Android.
