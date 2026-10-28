# Doremi Preview 0.1 — informe de implementación y verificación

## Estado: implementación en validación, no distribución completada

Rama `feature/doremi-preview-0-1`, base `331ffa1fef5edc2711b98eb1bf85cab1595a71d9`.
El SHA definitivo y el enlace descargable de Actions se deben registrar
tras ejecutar la CI de estos cambios. **No hay todavía evidencia remota
de Preview ni se declara verde su gate.**

`AGENTS.md` exige autorización específica antes de hacer push. Se ha
solicitado publicar únicamente esta rama en `Eirom16/spotifast` y ejecutar
CI; no se ha hecho push, merge a main ni publicación de release.
La evidencia verde de F1B-1 no se presenta como evidencia de Preview.

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
Xvfb disponible, sin Ruby ni xdotool. Se reutilizaron dependencias ya
compiladas en target; no se instaló toolchain ni software global.

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
  Su ejecución obligatoria queda en quick de Actions.
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
La verificación posterior al arreglo debe registrarse antes de cerrar QA.

La última ejecución de la suite completa all-features y el circuito local
fue interrumpida por el usuario antes de concluir. No se cuenta como pasada.
Los checks posteriores a retirar la franja se registran aparte; no se
atribuyen resultados de compilaciones anteriores a esa modificación.

Tras retirarla: `cargo fmt --all --check`, `cargo check --locked
--all-features --bin doremi-preview` (13,17 s), `scripts/test-preview.py`
(5 tests) y `git diff --check` pasaron. El guard verifica que el renderer
delegue directamente a las vistas originales, sin panel, imagen ni labels
adicionales, y mantenga el icono nativo original de Doremi. No se ha
reconstruido/capturado todavía el ejecutable final después de este cambio.

Las capturas locales están bajo `target/preview-visual-local`, no
versionadas. El runner usa xdotool; la prueba local usó libXtst/XTest:
**no son la misma ejecución ni evidencia de CI**.

## Reproducción y uso en CachyOS

Instrucciones de usuario incluidas en `packaging/preview/README.md`.
Una vez haya un artifact aprobado, descargar su ZIP de Actions y ejecutar:

```sh
sha256sum -c SHA256SUMS
tar -xzf doremi-preview-0.1.0-linux-x86_64.tar.gz
cd DoremiPreview-0.1.0
sha256sum -c SHA256SUMS
./run.sh
```

No requiere Rust, Python ni Spotify Premium. Requiere sesión gráfica,
glibc compatible y librerías de sistema; el paquete no es AppImage.
El `ldd` local no mostró dependencias ausentes: PipeWire, ALSA, libstdc++,
libgcc_s, libm y glibc. OpenGL/EGL/X11/Wayland/xkbcommon se pueden cargar
dinámicamente y siguen siendo requisitos de ejecución. El `ldd` de CI
debe adjuntarse tras construir allí, no sustituirse por el local.

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

## Pendientes antes de aceptar la entrega

- Autorización de push, ejecutar Actions y registrar SHA/run/conclusiones
  reales de todos los jobs, tiempos/cache y artefacto descargable.
- Revalidar capturas/input tras el arreglo del capturador, inspeccionar
  Home/Search/Library/Settings claro/oscuro/estrecho y adjuntar PNG/reportes.
- Probar el paquete descargado de Actions en CachyOS, no solo el binario local.
- No ejecutados todavía para Preview: CI nueva, suite default completa,
  Rustdoc, matrices nativas nuevas, paquete CI, audio real, APK.

Siguiente hito: **Android Preview 0.1 / F1B-2**, APK offline instalado con
misma identidad y núcleo Rust, navegación táctil, IME/teclado, rotación y
lifecycle. No se bloquea Linux por Android; una `.rlib` no es esa APK.
