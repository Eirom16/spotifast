# Doremi

Reproductor musical nativo en Rust, en migración desde
[Eirom16/spotifast](https://github.com/Eirom16/spotifast), fork de
[crmne/spotifast](https://github.com/crmne/spotifast).

**Estado: F0, preparación y auditoría. Todavía no es un reproductor
independiente de Spotify. No hay APK ni release Doremi.**

El objetivo es usar egui/eframe y yt-dlp para búsqueda y resolución de medios,
con escritorio y Android construidos sobre el mismo núcleo. No usaremos
Electron, un backend hospedado obligatorio ni evasión de DRM o controles de
acceso.

## Qué funciona ahora

Se conserva el ejecutable `spotifast` 0.14.2 y su biblioteca Rust. Su
reproducción sigue usando Spotify/librespot y requiere las autorizaciones y
Premium del baseline. La estructura nueva no elimina ese acoplamiento.
`doremi-core` es únicamente el punto inicial neutral del workspace, todavía
sin catálogo, motor ni biblioteca propios.

No renombramos aún IDs persistidos, keyrings, rutas, protocolos, iconos ni
actualizador. **El ejecutable legado conserva su actualizador dirigido al
upstream y su clave pública. No usarlo para distribuir Doremi ni aceptar sus
actualizaciones como actualizaciones de Doremi.** El roadmap exige separar
estas identidades antes del primer ejecutable Doremi.

## Desarrollo

Consulta [CONTRIBUTING.md](CONTRIBUTING.md) para dependencias y checks.

```sh
python3 scripts/check-local.py
cargo test --locked -p doremi-core
# Baseline visual con datos de prueba, no reproducción yt-dlp:
cargo run --locked -p spotifast --features demo -- --demo
```

GitHub Actions es el entorno principal para compilar el cliente completo.
La CI separa checks rápidos, tests, Clippy y builds Linux/macOS/Windows, más
una `.rlib` neutral para Android, del mismo commit y versión. Una `.rlib` no
es una aplicación Android. No hay publicación automática de releases ni
del sitio heredado. Estado real de ejecuciones y cachés: [CI-REPORT.md](CI-REPORT.md).

## Documentación

- [Roadmap y arquitectura objetivo](DOREMI-ROADMAP.md)
- [Auditoría, resultados reales, pendientes y reproducción](F0-REPORT.md)
- [Inventario de dependencias](docs/doremi/dependencies.md)
- [Reglas para agentes](AGENTS.md)

`docs/_guide/`, `docs/_reference/` y [PACKAGING.md](PACKAGING.md) conservan
documentación histórica de Spotifast. Sus dominios, recetas de instalación y
capacidades no son una oferta de distribución ni promesas de Doremi.

## Procedencia y licencia

Spotifast fue creado por **Carmine Paolino**. Se conserva sin cambios su
[licencia MIT y copyright original](LICENSE), así como el historial Git.
Doremi es un fork independiente, no una versión oficial del upstream.

El baseline utiliza [librespot](https://github.com/librespot-org/librespot),
[egui](https://www.egui.rs), [fastframe](https://fastframe.dev), Inter (OFL),
Noto Emoji (OFL) y Lucide (ISC). Las licencias de assets están junto a ellos.
Los binarios de yt-dlp y FFmpeg tienen condiciones de redistribución propias;
la MIT de Doremi no las reemplaza.
