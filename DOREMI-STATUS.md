# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; desarrollo nativo compartido Linux/Windows/Android.
- Último recorrido Linux/Android validado: `1e3a53b87faef25925e9f1fed2b835672b5bdf2a`.
- Funciona: búsqueda pública, artista/álbum, carátulas, biblioteca, playlists,
  cola, reproducción progresiva de Commons CC0 e historial de escucha real.
- Historial: probado en Linux y Samsung ARMv7, incluidos reinicio y reproducción
  sin buscar; perfiles habituales conservados.
- Navegación artista → ediciones → canciones: `fa02cff`, CI de tres plataformas
  y recorrido Linux aprobados; lanzador Linux y APK actualizados. Recorrido
  físico detenido por respuestas de límite de MusicBrainz; USB conectado.
- CI nativa: https://github.com/Eirom16/spotifast/actions/runs/42245806312
- Regresión del legado: https://github.com/Eirom16/spotifast/actions/runs/42240539196
  Tests/Clippy/Preview/Linux/Windows/Android aprobados. Gate global sin cerrar:
  GitHub abandonó macOS sin asignar runner en dos intentos.
- Límite: runner Windows sin OpenGL/audio suficientes; falta prueba en hardware.
- MusicBrainz aporta metadatos; no concede audio. YouTube nativo sin vía autorizada.
- Siguiente objetivo: guardar artistas/álbumes y reabrirlos por ID tras reiniciar;
  implementado localmente, pruebas y recorrido Linux aprobados; CI/APK pendientes.
- Pendientes posteriores: integración multimedia, DSP y reproducción en segundo plano.
- Evidencia fuera del árbol: `~/.local/share/doremi/product-qa/2026-11-06/`.
- Informes históricos conservados como referencia; no se actualizan rutinariamente.
