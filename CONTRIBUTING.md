# Contribuir a Doremi

Doremi se desarrolla en [Eirom16/spotifast](https://github.com/Eirom16/spotifast)
a partir de [Spotifast de Carmine Paolino](https://github.com/crmne/spotifast).
El código original conserva su licencia MIT y atribuciones. Las licencias de
fuentes, iconos y dependencias también se conservan.

## Alcance

El objetivo es reproducción nativa en Rust, egui/eframe y un núcleo común
para escritorio y Android. Catálogo/resolución usan contratos neutrales:
Rust nativo y yt-dlp son candidatos a comparar, no una decisión productiva.
No se requiere Spotify en
el producto final. F0 todavía conserva su cliente Spotify y sus requisitos.

No se aceptan Electron, motores de navegador embebidos, un backend hospedado
obligatorio, evasión de DRM o controles de acceso, ni sustituciones ocultas de
audio Spotify. Solo se reproducen medios a los que el usuario tiene acceso
legítimo. El soporte de un extractor no garantiza derechos de distribución.

Consulta `DOREMI-ROADMAP.md` antes de ampliar alcance. Una función existente
solo se retira después de identificar datos, consumidores, tests y sustitución.
Las reglas antiguas que excluían otros catálogos no son política de Doremi.
Las limitaciones Spotify documentadas siguen siendo relevantes al legado.

## Cambios revisables

- Una rama y un tema por commit. No modificar ni publicar en el upstream.
- Describir comportamiento, plataformas afectadas, riesgos y pruebas reales.
- Preservar contratos de cola, EQ, visualización, accesibilidad e i18n donde
  sean reutilizables. No copiar el núcleo para Android.
- La UI y los callbacks de audio no esperan búsquedas, red, descargas,
  decodificación ni disco. Trabajo asíncrono/cancelable, buffers acotados y
  errores visibles. No ocultar fallos bajo una respuesta exitosa.
- No exponer credenciales ni tocar bibliotecas reales desde tests.

### Visual reviews

Los cambios de interfaz requieren alcance explícito y evidencia antes/después
en el PR, en claro y oscuro y tamaños representativos. No versionar capturas
de revisión. No hay rediseño de interfaz en F0.

### Settings

Preferir defaults razonables, shortcuts o comandos a nuevos ajustes.
Los ajustes nuevos requieren aprobación del mantenedor antes de implementarlos;
el check heredado acepta la etiqueta `setting-approved`. F0 no añade ninguno.

## Dependencias

Preferir crates.io. No se cambia ninguna fuente git heredada en F0.
Los siete repositorios git heredados y sus revisiones están en
`docs/doremi/dependencies.md`. Añadir o cambiar una fuente requiere aprobación
del mantenedor de Doremi, pin inmutable, justificación, licencia y revisión de
Android. El checker heredado conserva una allowlist estricta de `crmne` para
este baseline; no describe propiedad de Doremi ni autoriza nuevos forks.

## Checks

GitHub Actions es el entorno principal de compilación y validación completa.
En el Pentium Silver N5000/8 GB usar a diario este circuito ligero:

```sh
python3 scripts/check-local.py
```

No compila el cliente completo: políticas, estructura/procedencia, formato y
el core sin dependencias. No equivale a tests completos, Clippy, audio ni APK.
La CI y sus reportes/cachés se describen en `CI-REPORT.md`.

Rust está fijado en `rust-toolchain.toml`. No usar otro compilador para declarar
que pasó el baseline. Los comandos completos para reproducción bajo demanda
(no necesarios en cada iteración local) se ejecutan desde la raíz:

```sh
python3 scripts/test-f0.py
python3 scripts/test-ci.py
python3 scripts/audit-dependencies.py --check
python3 .github/scripts/check-dependency-sources.py
python3 .github/scripts/check-review-evidence.py
python3 .github/scripts/test-policy-checks.py
python3 packaging/test-launchers.py
python3 packaging/test-release-names.py
python3 packaging/flatpak/test-metainfo.py
cargo fmt --all --check
cargo test --locked -p doremi-core
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --doc
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
```

Linux necesita pkg-config, ALSA, PipeWire, X11, Wayland, xkbcommon y OpenGL
(paquetes de desarrollo), CMake, compilador C++ y libclang para MilkDrop.
`--no-default-features` excluye MilkDrop, no Spotify ni las dependencias desktop.
Las pruebas de packaging necesitan Python 3.11+ y Ruby; las de IPC, D-Bus.
No instalar herramientas de sistema sin permiso en la máquina del usuario.

Android F0 solo comprueba la biblioteca neutral, no la aplicación:

```sh
rustup target add aarch64-linux-android
cargo build --locked -p doremi-core --target aarch64-linux-android
```

Una `.rlib` no es un APK y no prueba audio, JNI ni egui en Android. El plan
Android y sus gates están en el roadmap. No afirmar que una plataforma funciona
solo por compilar. La CI nueva no despliega el sitio histórico ni publica.

Cambios en credenciales requieren el round trip nativo con datos ficticios
(`credentials::tests::native_store_round_trip`, ignorado por defecto).
Cambios en Nix requieren `nix build .#default`; traducciones,
`.github/scripts/update-translations.sh --check`; estado Flatpak,
`packaging/flatpak/test-state.sh`. El sitio histórico se valida con
`(cd docs && bundle exec jekyll build)` si se modifica.

## Issues y releases

Leer el reporte y la discusión completos. No cerrar peticiones por reglas de
producto heredadas que contradigan Doremi. Pedir un dato concreto cuando falte
una reproducción, no inferir soporte ni políticas de un issue anterior.

F0 no tiene release Doremi. La versión 0.14.2 identifica el baseline heredado,
no una publicación nueva. Antes de publicar, separar IDs de aplicación,
perfiles, actualizador, claves de firma y recetas del proyecto original.
Todo artefacto debe corresponder al mismo commit y versión verificados.

Las contribuciones se distribuyen bajo la licencia MIT del proyecto.
