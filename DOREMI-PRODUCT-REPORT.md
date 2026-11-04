# Doremi — avance de producto

Rama de producto: `feature/doremi-product-ui`, creada desde `ea71b92` tras
consultar el remoto. El informe local anterior se conserva sin incorporarlo
a los commits de implementación.

## Corte actual: presentación compartida (en validación)

- `doremi-ui` extrae de `src/theme.rs` la paleta, pesos Inter, iconos,
  controles y estilos. El legado los reexporta; no hay una segunda copia.
- Navegación extraída de `src/ui/sidebar.rs`, contenedor de barra inferior
  de `src/ui/player_bar.rs` y placeholder de `src/ui/widgets.rs`, usados por
  ambos hosts. El legado conserva sus acciones, datos y visualizadores.
- `doremi-app` compone sidebar, navegación, biblioteca y barra persistente
  sobre el mismo Queue/Engine existente. Sin Spotify, cabecera adicional ni
  motor nuevo. Ventana desktop 1240×800; navegación táctil en ventanas pequeñas.
- Alias ejecutable `doremi`/`doremi.exe`; se conserva `doremi-app` para los
  consumidores anteriores. Perfil Linux conservado y ruta propia Windows.
- CI enfocada incorpora Windows nativo del nuevo host, tests, lint, smoke
  CLI y artifact con manifiesto/hash. No equivale a reproducción Windows real.

No está terminada la migración de las vistas completas: faltan home con
secciones musicales reales, filas/tablas completas, álbum, artista y playlist,
transiciones y otras capacidades inventariadas en el roadmap. Este corte
no cierra P0 y no sustituye la Preview aprobada como evidencia visual.

## Datos, reproducción y plataformas

El catálogo continúa siendo demostración offline y experimento Commons de
dos tonos CC0. No es aún búsqueda musical general. La reproducción Commons
existente descarga el recurso antes de decodificar; **no es streaming
progresivo online**. No se añaden formatos/importadores locales.

Linux: capturas anteriores reales guardadas en `/tmp/doremi-product-before`;
compilación, capturas nuevas y audio de este corte pendientes. Windows:
job añadido, todavía sin resultado. Android: mismo código de presentación;
APK y validación física de esta iteración pendientes. ADB detectó SM-J701M;
no se han modificado sus datos ni configuración.

Checks: `cargo check --offline -p doremi-ui` pasó antes de componer el host.
Circuito ligero, Clippy del host y CI se registrarán al terminar. Capturas,
logs y evidencias quedan fuera del árbol versionado. No hay release.

Siguiente corte: terminar QA de geometría/navegación de esta composición,
y conectar búsqueda musical real con metadatos/procedencia y navegación;
revisar proveedores actuales antes de decidir resolución de audio.
