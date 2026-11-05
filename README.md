> Doremi Android: host de desarrollo independiente en [`android/`](android/README.md).
> Estado actual de producto: [`DOREMI-PRODUCT-REPORT.md`](DOREMI-PRODUCT-REPORT.md).
> Evidencia histórica del host: [`DOREMI-GOAL-REPORT.md`](DOREMI-GOAL-REPORT.md).
> APK instalada y navegación/restauración probadas en Android real. Las muestras de audio local ya funcionan en ambos hosts. La Preview Linux se conserva.

# Doremi

Reproductor musical nativo en Rust, en migración desde
[Eirom16/spotifast](https://github.com/Eirom16/spotifast), fork de
[crmne/spotifast](https://github.com/crmne/spotifast).

**Estado: host Doremi independiente para Linux/Android, catálogo público
MusicBrainz y nuevo ejecutable Windows compilado en runner nativo.
La búsqueda musical está probada en Linux y SM-J701M/API 28; MusicBrainz aporta
metadatos, no audio. El primer stream musical CC0 está probado en Linux;\nla selección reproducible sigue siendo pequeña y no hay release estable.**

El objetivo es usar egui/eframe y contratos neutrales de búsqueda/resolución,
con escritorio y Android construidos sobre el mismo núcleo. No usaremos
Electron, un backend hospedado obligatorio ni evasión de DRM o controles de
acceso.

## Qué funciona ahora

Se conserva el ejecutable `spotifast` 0.14.2 y su biblioteca Rust. Su
reproducción sigue usando Spotify/librespot y requiere las autorizaciones y
Premium del baseline. La estructura nueva no elimina ese acoplamiento.
`doremi-core` contiene tipos neutrales y una máquina de cola determinista.
El bridge conserva la integración manual del legado. El host independiente
`doremi-app` usa el core para selección, duplicados, cola, modos y restauración
pausada y almacenamiento propio. `doremi-ui` extrae componentes reales del
legado para la presentación neutral. MusicBrainz permite buscar y explorar
grabaciones/artistas/ediciones y guardar sus IDs; no se les asigna audio de otra
fuente. En Buscar → Commons → Chopin puede seleccionarse una interpretación
CC0 de Membeth: el motor nativo reproduce bloques HTTP verificados sin descargar
la pieza completa. La selección Commons también conserva dos tonos de prueba.
Los datos demo siguen separados y silenciosos.
Estado, plataformas y límites: [DOREMI-PRODUCT-REPORT.md](DOREMI-PRODUCT-REPORT.md).
Rust nativo y yt-dlp siguen en evaluación para resolución musical admisible.

El legado mantiene sus IDs persistidos, keyrings, rutas, protocolos, iconos y
actualizador. El host Doremi usa identidad y datos propios. **El ejecutable legado conserva su actualizador dirigido al
upstream y su clave pública. No usarlo para distribuir Doremi ni aceptar sus
actualizaciones como actualizaciones de Doremi.** El roadmap conserva los gates de identidad y distribución.

## Desarrollo

Consulta [CONTRIBUTING.md](CONTRIBUTING.md) para dependencias y checks.

```sh
python3 scripts/check-local.py
cargo test --locked -p doremi-core
# Baseline visual con datos de prueba, no reproducción yt-dlp:
cargo run --locked -p spotifast --features demo -- --demo
```

GitHub Actions es el entorno principal para compilar el cliente completo.
La CI separa checks rápidos, tests, Clippy y builds Linux/macOS/Windows del
baseline. Un circuito enfocado genera los nuevos hosts Linux/Windows y el APK Android ARM64/ARMv7,
con manifiestos y checksums. Las estables futuras requieren Linux, Windows y
Android con versión/commit comunes y gate global. No hay publicación automática de releases ni
del sitio heredado. Estado real de ejecuciones y cachés: [CI-REPORT.md](CI-REPORT.md).

## Documentación

- [Roadmap y arquitectura objetivo](DOREMI-ROADMAP.md)
- [Auditoría, resultados reales, pendientes y reproducción](F0-REPORT.md)
- [Dominio/cola y caracterización F1A](F1A-REPORT.md)
- [Integración y resultados F1B-1](F1B1-REPORT.md)
- [Contratos y harness sintético](PROVIDER-CONTRACTS.md)
- [Rotación de cachés propuesta, sin borrar](CACHE-ROTATION.md)
- [Evaluación de Tubefast y otros proveedores](TUBEFAST-ASSESSMENT.md)
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
