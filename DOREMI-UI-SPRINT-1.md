# Doremi UI — Sprint 1

Base: `integration/doremi-alpha-unified`, `d6091d24647c2930085a55fd6f7d09c3a5bc85b4`.
Rama: `Eirom16/doremi-ui-sprint-1`, worktree independiente.
Commit final de código: `1d54095767c964af0d7fab3df17d44a70862697a`
(implementación principal `4377565`).

## Cambios aplicados

- Inicio: acción principal morada distinguible sobre el panel existente.
- Escritorio: selección persistente en la navegación lateral; afecta Inicio,
  Búsqueda, Biblioteca, Playlist y Cola sin cambiar sus destinos.
- Reproductor de escritorio: agrupa carátula e identidad musical en una superficie
  del tema. La barra reutiliza las bandas del reproductor heredado, con más ancho
  para progreso y límites para títulos largos.
- Reproductor móvil vertical: carátula proporcional al espacio disponible después
  de reservar controles; conserva la composición horizontal y los objetivos de 48 dp.
- Mini-player: disco de reproducción de 40 dp dentro del objetivo táctil de 48 dp;
  carátula y controles secundarios conservan su geometría.
- Carátulas del proveedor: recorte central que preserva proporciones en las tres
  superficies; sin decodificación adicional ni trabajo de red en UI.
- Estados: carga y preparación se muestran antes del estado de demostración sin
  motor. Hover/foco del botón circular conserva tamaño, sin salto de escala.
- Listas: margen inferior de 8 dp para separar la última fila de elementos fijos
  y absorber redondeo a píxeles físicos.

Reutilizados: `collection::hero`, `player_bar`, `nav_row`, `icon_button_sized`,
`icon_toggle`, `circle_button`, `thin_slider_sized`, carátulas y paletas de
`doremi-ui`, con Inter y la familia de iconos heredada. No cambian el motor,
la cola, las fuentes, dependencias ni persistencia. Acento morado predeterminado.

## Evidencia real anterior

Archivos privados, fuera del repositorio:
`/home/eirom/.local/share/doremi/product-qa/2026-11-22-ui-sprint-1/`.
Cada captura conserva el JSON de estado y los scripts de QA.

| Plataforma | Evidencia anterior | Evidencia posterior |
| --- | --- | --- |
| Linux 1240×800 | `before-desktop-wide/`: `home-dark`, `search`, `library`, `playlist-three-entries`, `full-player-dark`, `mini-player-dark` | Pendiente del binario de CI modificado |
| Linux 640×480 | `before-desktop/`: mismas superficies, reproductor horizontal y mini-player | Pendiente |
| Android físico 384×658,7 dp | `before-android/`: `startup`, `qa-mobile-results-dark`, `qa-mobile-library`, `qa-playlist-empty`, `qa-mobile-playing`, `qa-mobile-mini` | Pendiente del APK de CI modificado |
| Android físico horizontal | `before-android/qa-mobile-landscape.png`, con navegación inferior | Pendiente |
| Producto sin controles QA | `before-product-desktop/`, `before-product-android/`, claro/oscuro | Pendiente |

Las muestras del circuito interno están identificadas como demostración. La prueba
Android usa música CC0 real y métricas nativas; no se afirma captura acústica.
No se presentan capturas antiguas como evidencia de la versión modificada.

## Validación

- **Ejecutado / pasó:** checksums de los artefactos base; `scripts/check-local.py`;
  formato y `git diff --check`.
- **Ejecutado / pasó, base:** `native-qa.py` a 1240×800 y 640×480; navegación,
  búsqueda, modos, cola con duplicados, favoritos, edición/restauración de playlists.
  `product-ui-qa.py` a 1240×800 en claro y oscuro.
- **Ejecutado / falló, base:** `android-music-ui-qa.py --playlists` llega a
  selección de segunda fila de Playlist: borde inferior 493,71875 dp frente a
  límite de página 493,65625 dp. Se conservan `qa-failure.png/json` y log.
  El circuito anterior ya había capturado reproducción, seek/resume, mini-player,
  cola y rotación. El margen de contenido aborda este defecto; falta repetirlo.
- **Ejecutado / pasó, base Android:** `android-product-ui-qa.py`, 22 capturas,
  SM-J701M/API 28/ARMv7, claro/oscuro, vertical/horizontal, teclado físico y
  filtros; biblioteca normal preservada.
- **En ejecución:** tests por crate `cargo test --locked -p doremi-ui -p doremi-app --lib`.
- **Pendiente:** CI del commit modificado, QA gráfica posterior y comparación.
  La subida de la rama requiere autorización específica según `AGENTS.md`;
  se solicitó antes de cualquier push. No hay SDK/NDK local; el APK se obtiene de CI.

## Límites abiertos

- Falta verificar los cambios en píxeles y controles con los artefactos modificados;
  este informe no declara aceptado el sprint mientras esas filas estén pendientes.
- El modelo mínimo de canción no conserva créditos completos para todas las
  superficies. Se mantienen títulos y metadatos disponibles sin inventar artistas.
- Faltan capturas físicas en teléfonos de 360 y 430 dp; el dispositivo conectado
  usa 384 dp. Los tests existentes cubren esas geometrías sin cambiar ajustes del teléfono.
- Las carátulas ausentes conservan el placeholder existente.
- La reproducción se pausa al salir de Android, comportamiento previo conservado.
