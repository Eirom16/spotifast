# Doremi Preview 0.1 — informe de implementación y verificación

## Estado: Preview Linux descargable y validada

Rama `feature/doremi-preview-0-1`, base `331ffa1fef5edc2711b98eb1bf85cab1595a71d9`.
**SHA compilado y validado: `a982806d68293e30d3ce44503c3ba3324b1c8123`.**
CI [40622061151](https://github.com/Eirom16/spotifast/actions/runs/40622061151),
intento 1: **success**, ocho jobs aprobados.

- [Portable Linux x86_64, Preview 0.1.0](https://github.com/Eirom16/spotifast/actions/runs/40622061151/artifacts/12789140067).
- [18 capturas reales, estados y logs de QA](https://github.com/Eirom16/spotifast/actions/runs/40622061151/artifacts/12788268760).
- [Reporte agregado de CI](https://github.com/Eirom16/spotifast/actions/runs/40622061151/artifacts/12788163590).

El portable se descargó de Actions, se verificaron todos sus checksums y se
ejecutó mediante `run.sh` en CachyOS, sin compilar Rust. Pasó las mismas
18 pruebas gráficas; también abrió y capturó Home en Wayland nativo.
Este informe se registra después de CI: el SHA de evidencia/documentación
no se presenta como otro SHA de ejecutable. No se creó release pública.
Los artifacts requieren sesión GitHub y caducan el **4 de noviembre de
2026 a las 02:58 UTC**; no constituyen alojamiento permanente.

El usuario autorizó específicamente el push de esta rama y la ejecución
de Actions. Se verificó árbol limpio y circuito local antes de publicar
`21d277b76f7af7356518905b7efc0e354f37016d` exclusivamente en
`Eirom16/spotifast`. No se hizo merge a main ni publicación de release.
La evidencia verde de F1B-1 no se presenta como evidencia de Preview.

### Primera CI real de Preview y correcciones

Run [40619158651](https://github.com/Eirom16/spotifast/actions/runs/40619158651),
SHA `21d277b`: tests default/all-features, Clippy/Rustdoc, macOS, Windows
y Android core pasaron. Falló preview-linux; el gate rechazó correctamente
la ejecución. El run automático del push fue cancelado por concurrencia al
lanzar este dispatch del mismo SHA; no es un fallo de compilación adicional.

Causa gráfica confirmada en log: faltaba `libxkbcommon-x11.so`, cargada
dinámicamente y no detectada por `ldd`. Se añade el runtime Ubuntu
`libxkbcommon-x11-0` y su equivalente CachyOS a las instrucciones.

Se descargó el binario exacto de ese run, sin recompilar Rust en CachyOS.
El QA encontró además un problema de su automatización de teclado:
Ctrl+A se enviaba antes del frame que entrega foco tras Ctrl+F. El script
ahora espera ese frame y envía input XTest a la ventana enfocada.
Con esa corrección pasaron localmente 18 capturas/estados reales, cambio
de color, restauración, claro/oscuro, búsqueda y navegación con clic real
a Settings y atajo a Home. La segunda CI y el portable descargado cerraron
posteriormente estos gates para `a982806`.

### Evidencia final del SHA entregado

| Gate | Resultado real |
|---|---|
| quick | Políticas, licencias/procedencia, formato, tooling y harness pasaron; también launchers con Ruby |
| full-tests default | App: 1.407 pasados, 0 fallidos, 3 ignorados; CLI legado: 14 pasados |
| full-tests all-features | App: 1.450 pasados, 0 fallidos, 3 ignorados; CLI legado: 16 pasados |
| Preview | 3 tests de preferencias/aislamiento, 1 test del capturador y 1 test CLI pasaron |
| Core y doctests | 32 tests y 1 doctest pasados |
| clippy | Default/all-features y Rustdoc con `-D warnings` pasaron |
| desktop-build Windows/macOS | Ambos builds nativos legacy y smoke CLI pasaron |
| android-core | Build arm64 `.rlib` y check de tests pasaron; no APK |
| preview-linux | Empaquetado, `ldd`, 18 capturas/input/persistencia y checksums pasaron |
| verified-artifacts | Todos los jobs y cuatro payloads legacy/core del mismo SHA/versión verificados |

Los tres tests ignorados de servicios nativos no se cuentan como pasados.
No duplicar los summaries de tests subprocess al sumar resultados.

Tiempos reales por comando en el runner, incluyen compilación cuando procede:

| Comando | Segundos | Exit |
|---|---:|---:|
| Tests default | 944,762 | 0 |
| Tests all-features | 79,969 | 0 |
| Clippy default / all-features | 425,676 / 16,400 | 0 |
| Rustdoc / doctests | 26,678 / 1,605 | 0 |
| Build Windows / macOS | 1.289,938 / 800,632 | 0 |
| Build core Android | 20,261 | 0 |
| Empaquetado Preview | 10,782 | 0 |
| QA gráfico Preview | 58,944 | 0 |

Los cuatro jobs que restauran caches registraron `cache_exact_hit=false`.
No atribuirles el acierto observado en F0. Snapshot final por API:
12 caches, **7.841.152.854 bytes**, sin crecimiento ni borrados remotos.
Se mantuvo la admisión de guardados bloqueada por superar 6 GiB.
Target tests: 3.406.648.498 bytes, bajo 4.096 MiB; metadata:
1.243.371.046 bytes, bajo 2.048 MiB. El job Preview no compila ni cachea
otro árbol Rust. Estos tiempos no son latencia musical.

## Resultado visible y límites

- Nuevo ejecutable `doremi-preview`, versión visible **0.1.0**, feature `demo`.
  El ejecutable, versión 0.14.2, identidad y updater de Spotifast se conservan.
- Icono original Doremi con fondo para la ventana, sin estirar, recortar
  ni tintar. Por indicación del usuario se retiró la franja de marca
  añadida inicialmente: no se añade logo ni cabecera al área de contenido
  y no se ocupa espacio adicional sobre las vistas. El nombre permanece
  en el título nativo. Los tres archivos se copian byte a byte
  de `Eirom16/Doremi-Rust`, SHA `1e44a21f883810a8337b31180ada9bf0fb0d4daf`.
- Morado **#9966E8** solo en Preview; verde original conserva sus valores
  dark/light. Configuración reutiliza filas, secciones y botones existentes.
  Preferencias de acento y claro/oscuro en `preview.json`, escritura atómica
  asíncrona con el Writer existente. Tinta oscura en botones morados, no
  blanco de contraste insuficiente. No se rediseñan las vistas.
- Home, Search, biblioteca, playlists, artista, álbum, cola y barra de
  reproducción conservan sus componentes y datos demo. Nueva búsqueda
  filtra canciones falsas en memoria, por nombre/artista y máximo 20.
  El resultado no es una equivalencia entre catálogos.
- **No hay reproducción musical real.** Datos y dispositivo indican demo/
  simulación/no audio. Play y operaciones no soportadas muestran aviso.
  Carátulas sin red usan placeholders existentes; no se atribuyen imágenes
  ficticias a álbumes reales.
- Preview no tiene worker Spotify, engine, keyring, restauración de grants,
  descubrimiento, updater, autostart, IPC, MPRIS ni tray. Su backend bloquea
  todos los comandos excepto Shutdown y usa HTTP no disponible para artwork.
  El allowlist de acciones permite navegación/apariencia, no mutaciones
  de cuentas, perfiles, instalación, navegador ni actualizaciones.
- El arranque es propio, no llama a `entrypoint::run` ni a la interceptación
  de actualizaciones heredada. Rutas explícitas separadas, sin migración/
  copia del perfil instalado. No se modifica DSP, credenciales, ajustes ni
  recursos del verde legado.

La app sigue enlazando dependencias desktop del paquete legado. Una demo
sin transporte no equivale a haber eliminado librespot del árbol. No hay
proveedor real nuevo, benchmark musical ni APK.

## Logos, licencias y procedencia

La licencia encontrada en Doremi-Rust es **GPL v3**, no MIT. Se conserva
íntegra en `assets/doremi/LICENSE`, con sus imágenes originales y hashes
en `assets/doremi/PROVENANCE.md`. Son assets separados; no se copió código
del repositorio antiguo ni se modificó ese repositorio. No se afirma
haber auditado la autoría de los gráficos. La MIT y atribución original
de Carmine Paolino permanecen intactas en la raíz.

## CI y paquete portable

`full-tests` construye y prueba el nuevo binario como dependencia real de
`tests/preview_cli.rs` en all-features. Exporta ese binario y manifest:
no añade un `cargo build` completo repetido.

Nuevo job `preview-linux` en Ubuntu 24.04:

1. Descarga únicamente el binario del mismo SHA de full-tests.
2. Verifica commit/hash/target/estado limpio. Empaqueta launcher, logos,
   MIT, licencias de assets, Cargo.lock, versión/SHA, manifest, dependencias
   reales `ldd`, checksums internos y del tar.
3. Ejecuta el binario real con Mesa/Xvfb. El script de QA usa ratón/teclado
   X11, no modifica imágenes ni dibuja composiciones.
4. Comprueba cambio Purple/Green, restauración, claro/oscuro, búsqueda con
   teclado, vistas pedidas y Settings a 900 px. Los PNG y JSON/logs van a
   artifacts de reportes, nunca al árbol versionado.
5. Publica `doremi-preview-0.1.0-linux-x86_64-<SHA>` solo si pasa el job
   gráfico. Retención siete días, sin releases ni actualizaciones.

`verified-artifacts` exige también este job. Los cuatro payloads baseline
y sus verificaciones siguen separados: Preview no entra por error en el
patrón `payload-*`. Se mantienen default/all-features, Clippy, Rustdoc,
Linux/Windows/macOS, Android core, políticas, MIT/procedencia y budgets.
Preview no añade caché, target/perfil incompatible ni otro árbol Rust.
No se eliminaron caches remotas.

## Verificación local y fallos observados

Entorno real: CachyOS x86_64, kernel `7.2.7-1-cachyos`, sesión Wayland,
Xvfb disponible, sin Ruby ni xdotool instalados globalmente. Para repetir
el QA final se extrajo xdotool de un paquete oficial dentro de target;
su SHA-256 coincidió con la base pacman local. No se instaló software
global. La validación final del portable no recompiló Rust.

- `cargo check --locked --all-features --all-targets`: pasó.
- `cargo clippy --locked --all-features --all-targets -- -D warnings`: pasó.
  Primer comando conjunto llegó al límite local de 200 s durante Clippy;
  no se contó como éxito. La ejecución separada terminó en 94 s.
- `python3 scripts/check-local.py`: pasó con los checks existentes y core
  (32 tests y 1 doctest). El circuito ahora incorpora cinco regresiones
  ligeras de Preview, incluido el guard de geometría.
- `cargo test --locked --all-features --test preview_cli`: pasó, 1 test.
  Compilación/enlace incremental local: 5 min 26 s; no es latencia musical.
  Verifica versión propia y rechazo de flags de actualización/control legacy.
- `cargo test --locked --all-features --lib preview::tests`: pasó,
  3 tests (selección/persistencia, defaults y bloqueo de acciones).
  3 min 14 s de compilación del test App; no se afirma suite completa.
- Políticas: 27 tests pasados. Nombres de releases: 8 pasados.
  Metainfo: 4 pasados. actionlint con `-shellcheck=` pasó.
- `packaging/test-launchers.py`: **falló localmente**, 2 errores por Ruby
  ausente; 2 tests pasaron. No se rebajó el test ni se instaló Ruby global.
  Su ejecución obligatoria pasó posteriormente en quick de Actions.
- Primera comprobación de compilación encontró el nombre antiguo
  TopBottomPanel (egui fijado usa Panel). Se corrigió antes del check verde.

### Evidencia gráfica real y corrección del capturador

Se ejecutó el binario local inicial y se guardó un framebuffer PNG de Settings:
logo original, Doremi, morado y layout legible a 1240 px. Esa versión incluía
una franja de marca que posteriormente se retiró por indicación del usuario;
sus capturas no acreditan la geometría final sin franja. En Wayland de
CachyOS el framebuffer observado fue 1240×768; en X11/Xvfb, 1240×800.
No se estiran ni falsifican capturas para imponer una resolución.

La primera prueba de input no hallaba ventana en Xvfb: winit 0.30 eligió
Wayland por el entorno real. `WINIT_UNIX_BACKEND` no basta en esta revisión;
el harness y la instrucción de fallback eliminan WAYLAND_DISPLAY/SOCKET.

La prueba X11/XTest real cambió a verde y verificó su restauración al
reiniciar. Al cambiar Light encontró un bug del capturador inicial:
aceptaba también Screenshot interno de la animación de tema y cerraba
prematuramente. No se considera aquella captura un tema claro validado.
Se añadió marcador propio, filtro de respuesta por dueño/viewport y test
de regresión, sin desactivar la animación ni alterar el capturador legacy.
La verificación posterior al arreglo pasó en la CI final y en la repetición
del portable descargado, manteniendo la animación.

Una ejecución local inicial de la suite completa all-features y del circuito
local fue interrumpida por el usuario antes de concluir. No se cuenta como pasada.
Los checks posteriores a retirar la franja se registran aparte; no se
atribuyen resultados de compilaciones anteriores a esa modificación.

Tras retirarla: `cargo fmt --all --check`, `cargo check --locked
--all-features --bin doremi-preview` (13,17 s), `scripts/test-preview.py`
(5 tests) y `git diff --check` pasaron. El guard verifica que el renderer
delegue directamente a las vistas originales, sin panel, imagen ni labels
adicionales, y mantenga el icono nativo original de Doremi. Después de ese
check se reconstruyó y capturó la versión definitiva mediante Actions.

Las capturas iniciales están bajo `target/preview-visual-local`. Las
definitivas descargadas y repetidas están bajo `target/preview-final`.
No están versionadas. La prueba local inicial usó libXtst directamente;
la repetición final usó exactamente `scripts/preview-qa.py` con xdotool.
Se inspeccionaron imágenes de Home/Search/Library/Settings, claro/oscuro,
verde y Settings a 900 px: sin franja adicional ni solapamientos nuevos.
El desplazamiento horizontal de las estanterías Home se conserva.

Se comprobó también el título `Doremi Preview 0.1 — demo · no audio` y la
propiedad nativa X11 `_NET_WM_ICON` de 1080×1080, coherente con el PNG
original. No se colocó el logo dentro del contenido. Esta prueba no acredita
la integración del icono en todos los docks/compositores Wayland.

## Reproducción y uso en CachyOS

Instrucciones de usuario incluidas en `packaging/preview/README.md`.
Descargar el ZIP del portable enlazado arriba, extraerlo y ejecutar en
la carpeta extraída:

```sh
sha256sum -c SHA256SUMS
tar -xzf doremi-preview-0.1.0-linux-x86_64.tar.gz
cd DoremiPreview-0.1.0
sha256sum -c SHA256SUMS
./run.sh
```

No requiere Rust, Python ni Spotify Premium. Requiere sesión gráfica,
glibc compatible y librerías de sistema; el paquete no es AppImage.
El `ldd` del portable descargado no mostró dependencias ausentes en CachyOS:
PipeWire, ALSA, libstdc++, libgcc_s, libm y glibc. El paquete incluye
`runtime-libraries.txt` con el `ldd` real de Ubuntu 24.04; se comprobó
adicionalmente el de CachyOS al ejecutar. OpenGL/EGL/X11/Wayland y
`libxkbcommon-x11` se cargan dinámicamente y siguen siendo requisitos.
Si el escritorio no los trae:

```sh
sudo pacman -S --needed alsa-lib pipewire libxkbcommon libxkbcommon-x11 wayland libx11 mesa libglvnd
```

SHA-256 verificado del tar:
`a76cde7000cf00713859a2b60b4591bf6afeb87326c371787b0d55294357805a`.
Manifest limpio: Preview 0.1.0, legacy 0.14.2, target
x86_64-unknown-linux-gnu, features demo/milkdrop, SHA `a982806...`,
release=false y audio=false. Launcher y ejecutable conservaron modo 0755.
El ZIP artifact observado mide 36.055.362 bytes.

Para descargar sin usar el navegador (opcional, con gh autenticado):

```sh
gh run download 40622061151 --repo Eirom16/spotifast \
  --name doremi-preview-0.1.0-linux-x86_64-a982806d68293e30d3ce44503c3ba3324b1c8123 \
  --dir doremi-preview-download
```

Comandos de validación:

```sh
python3 scripts/check-local.py
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
xvfb-run -a -s "-screen 0 1440x1000x24" \
  python3 scripts/preview-qa.py target/ci/debug/doremi-preview target/preview-qa
```

Los comandos pesados pertenecen principalmente al runner. El circuito
cotidiano sigue siendo `scripts/check-local.py`. Para ejecutar el binario
local ya construido, sin recompilar:

```sh
target/debug/doremi-preview --data-dir target/preview-profile
```

## Alcance aceptado y pendientes de producto

La entrega visual Linux está validada: artifact descargable, CI completa,
capturas definitivas y ejecución del portable en CachyOS sin Rust.
Solo se publicó la rama autorizada. No se fusionó main, no se crearon
releases y no se modificó Doremi-Rust ni crmne/spotifast.

No ejecutados/no implementados: audio musical real, proveedores online,
APK/dispositivo Android, pruebas nativas completas Windows/macOS,
servicios reales de los tres tests ignorados, packaging Nix/Flatpak runtime,
auditoría exhaustiva de licencias/vulnerabilidades transitivas y GUI a
todos los DPI/compositores. La demo no promete esas capacidades.

Siguiente hito: **Android Preview 0.1 / F1B-2**, APK offline instalado con
misma identidad y núcleo Rust, navegación táctil, IME/teclado, rotación y
lifecycle. No se bloquea Linux por Android; una `.rlib` no es esa APK.
