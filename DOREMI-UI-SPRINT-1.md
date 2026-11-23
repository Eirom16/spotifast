# Doremi UI — Sprint 1

Base: `integration/doremi-alpha-unified`, `d6091d24647c2930085a55fd6f7d09c3a5bc85b4`.
Rama: `Eirom16/doremi-ui-sprint-1`.
Commit final de implementación y de los artefactos ejecutados:
`7d79bb2133d5d7fa51a1b85a603d32756ff3e3b5`.
Este informe se entrega en un commit posterior, sin cambios de código.

## Pantallas y correcciones

- **Inicio y navegación:** acción principal morada visible sobre el panel
  existente y selección persistente en la barra lateral de escritorio.
- **Reproductor de escritorio:** carátula e identidad musical agrupadas con el
  tema existente; bandas independientes para metadatos, transporte y volumen.
  En 1240×800, el progreso pasa de **311,88 a 397,72 puntos (+27,5 %)**.
- **Reproductor móvil:** carátula adaptada al espacio restante tras reservar
  controles. En el Samsung, pasa de **159,5 a 189,66 dp (+41,4 % de área)**;
  progreso, volumen y transporte permanecen dentro del área visible.
- **Mini-player:** disco de reproducción de 40 dp dentro de un objetivo de
  **48×48 dp**, separado de abrir reproductor y siguiente. Mantiene la barra de
  progreso y el espacio sobre la navegación inferior.
- **Carátulas y estados:** recorte central sin deformar imágenes rectangulares;
  carga/preparación antes del estado de demostración sin motor. El botón
  circular conserva su tamaño al recibir hover/foco.
- **Biblioteca y Playlist:** creación arriba de los filtros y renombrado antes
  de la carátula. El desplazamiento necesario para el teclado es inmediato,
  evitando que el campo pierda visibilidad y el IME oscile. Se conservan
  Unicode, guardar, cancelar eliminación, favoritos y la cola activa.
- **Listas:** margen inferior de 8 dp. Resuelve el último elemento que quedaba
  0,06 dp fuera del área visible por redondeo a píxeles físicos.

Se reutilizan `collection::hero`, `player_bar`, `nav_row`, `icon_button_sized`,
`icon_toggle`, `circle_button`, `thin_slider_sized`, Inter, iconos, placeholders,
paletas y workers existentes de Spotifast/Doremi. El acento predeterminado sigue
siendo morado. No cambian motor, catálogo, contratos de cola, persistencia,
dependencias ni licencias; no se integra Doremi-Rust.

## Capturas comparativas

Evidencia fuera del repositorio:
`/home/eirom/.local/share/doremi/product-qa/2026-11-22-ui-sprint-1/`.

[Comparativa interactiva de las seis superficies](</home/eirom/.local/share/doremi/product-qa/2026-11-22-ui-sprint-1/comparacion-antes-despues-7d79bb2.html>) ·
[Capturas, estados y resultados en ZIP](</home/eirom/.local/share/doremi/product-qa/2026-11-22-ui-sprint-1/capturas-ui-sprint-1-7d79bb2.zip>) ·
[Medidas verificadas](</home/eirom/.local/share/doremi/product-qa/2026-11-22-ui-sprint-1/comparison-measurements.json>).

| Plataforma | Antes | Después, commit `7d79bb2` |
| --- | --- | --- |
| Linux 1240×800, Inicio/Búsqueda/Biblioteca/Playlist/Player/barra | `before-desktop-wide/`, `before-product-desktop/` | `final-desktop-wide/`, `final-product-desktop/` |
| Linux 640×480, composición compacta y reproductor horizontal | `before-desktop/` | `final-desktop-small/` |
| Android físico 384×658,66 dp, seis superficies y navegación inferior | `before-android/`, `before-product-android/` | `final-android/`, `final-product-android/` |
| Android horizontal, player expandido y navegación | `before-android/qa-mobile-landscape.png` | `final-android/qa-mobile-landscape.png` |

Los directorios incluyen PNG originales, estados JSON, scripts y resultados.
Se comprueban ambos temas. Las demostraciones se identifican como sin audio;
los recorridos musicales usan Commons CC0 y métricas de salida PCM nativa.
No se afirma captura acústica ni se presentan capturas antiguas como finales.

## Resultados de pruebas

| Comprobación | Resultado |
| --- | --- |
| `cargo test --locked -p doremi-app --lib` | **68 aprobados**, 1 ignorado: recorrido de red de unos 20 minutos |
| `cargo test --locked -p doremi-ui --lib` | **6 aprobados**, incluidos recorte, geometría y estados de controles |
| `check-local.py`, formato, políticas y checksums | **Aprobados**; checks rápidos también ejecutados en CI |
| Linux `native-qa.py`, 1240×800 y 640×480 | **Aprobado**: búsqueda, navegación, modos, duplicados, favoritos, edición/restauración de playlists |
| Linux 1240×800 `--playlist-music` | **Aprobado**: música real, entrega PCM, reproducción desde playlist y tras restaurar proceso, sin búsqueda previa |
| Linux `product-ui-qa.py` | **Aprobado**: claro/oscuro, cuatro filtros, sin controles internos ni reproducción ficticia |
| Android `android-music-ui-qa.py --playlists` | **Aprobado**: PCM progresivo, pausa/seek/resume, mini-player, cola, duplicados, renombrado Unicode, cancelación de borrado, rotación y restauración sin búsqueda |
| Android `android-product-ui-qa.py` | **23 capturas y comprobaciones aprobadas**, incluida Biblioteca visible y sin desplazamiento al abrir el IME |
| Perfil normal del Samsung | **Preservado**: mismo hash de biblioteca antes y después |

[CI del commit final, ejecución 45221132558](https://github.com/Eirom16/spotifast/actions/runs/45221132558):
`quick`, `full-tests` (default, all-features y doctests), `preview-linux`,
`clippy`/Rustdoc, `android-core`, `android-apk` y Windows **aprobados**.
macOS no obtuvo runner por capacidad de GitHub: primer intento cancelado;
reintento solicitado y todavía sin veredicto. El gate global
`verified-artifacts` depende de macOS: **no se declara la CI completamente verde**.
El estado consultado se conserva en `ci-final-state.json`.

El primer fallo de la prueba nueva se corrigió preparando la muestra en pausa;
los fallos físicos del último elemento y del IME originaron las correcciones
anteriores. También se conservaron pruebas con timeout de proveedor y un
intento local con colisión de pantallas Xvfb; las repeticiones finales usan
pantallas separadas y pasaron. No se eliminaron ni relajaron comprobaciones.

Artefactos de desarrollo, `release: false`, verificados contra sus manifiestos:

- Linux `doremi`: SHA-256 `87327202ebaf1f69e81f189f1be778502728420fd8a610cbdf5c63cf88cb44e1`.
- APK: SHA-256 `29ab2d796b31bcda8331f402813dedb17636eeaab37a9230c97a5ff79115a58c`.

## Defectos y verificaciones pendientes

- Las carátulas ausentes mantienen el placeholder; no se inventa arte ni metadata.
- El modelo mínimo no conserva créditos completos de artista para todas las superficies.
- Validación física en un Samsung SM-J701M, API 28/ARMv7. Los tamaños de
  360/393/430 dp se cubren con tests de layout, sin afirmar pruebas físicas en
  esos teléfonos ni en tabletas.
- GUI física de Windows/macOS no ejecutada; macOS CI sigue condicionado por
  disponibilidad del runner. La reproducción Android se pausa al salir,
  comportamiento previo conservado.

`main` no se modificó. No hay fusiones a producción ni releases publicadas.
