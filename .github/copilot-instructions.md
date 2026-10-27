# Instrucciones de Doremi

Este repositorio es el fork Eirom16/spotifast en migración a Doremi.
Sigue `AGENTS.md`, `CONTRIBUTING.md`, `DOREMI-ROADMAP.md` y `F0-REPORT.md`.
La política Spotify-only y la automatización account-wide de crmne no
gobiernan este fork.

- Rust nativo, egui/eframe, contratos neutrales de catálogo/resolución y un núcleo
  compartido entre escritorio y Android. Nada de Electron ni backend
  hospedado obligatorio.
- No elegir proveedor antes de medir: Rust nativo y yt-dlp son candidatos;
  RustyPipe sigue como referencia por su licencia.
- No evadir DRM ni controles de acceso. No reemplazar audio Spotify
  silenciosamente por resultados de otro catálogo.
- No bloquear UI o callback de audio con red, búsqueda, descargas,
  decodificación o esperas de disco.
- Preservar funciones y tests hasta identificar consumidores y reemplazo.
  La documentación Spotify sirve para el legado, no para rechazar el nuevo
  objetivo del producto.
- No registrar secretos. No cambiar fuentes git sin revisión explícita.
  Conservar MIT, atribuciones y licencias de assets.
- Al revisar, comenzar por `User-visible UI impact: none` o enumerar cambios
  visibles. Pedir evidencia visual en el PR, no en archivos versionados.
- Distinguir tests pasados, fallidos y no ejecutados. No aprobar ni publicar
  automáticamente. No modificar el upstream ni reactivar sus workflows.
