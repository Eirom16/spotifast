# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; desarrollo nativo compartido Linux/Windows/Android.
- Último commit validado: `1e3a53b87faef25925e9f1fed2b835672b5bdf2a`.
- Funciona: búsqueda pública, artista/álbum, carátulas, biblioteca, playlists,
  cola, reproducción progresiva de Commons CC0 e historial de escucha real.
- Historial: probado en Linux y Samsung ARMv7, incluidos reinicio y reproducción
  sin buscar; perfiles habituales conservados.
- CI nativa: https://github.com/Eirom16/spotifast/actions/runs/42240490038
- Regresión del legado: https://github.com/Eirom16/spotifast/actions/runs/42240539196
  Linux/tests/Clippy aprobados; Windows/Preview en curso, macOS cancelado.
- Límite: runner Windows sin OpenGL/audio suficientes; falta prueba en hardware.
- MusicBrainz aporta metadatos; no concede audio. YouTube nativo sin vía autorizada.
- Siguiente objetivo: artista → álbumes/ediciones paginados → canciones,
  con navegación de regreso y validación Linux/Android/Windows.
- Pendientes posteriores: integración multimedia, DSP y reproducción en segundo plano.
- Evidencia fuera del árbol: `~/.local/share/doremi/product-qa/2026-11-06/`.
- Informes históricos conservados como referencia; no se actualizan rutinariamente.
