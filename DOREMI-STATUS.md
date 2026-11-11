# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal YouTube Music nativo, adaptado de Tubefast MIT: búsqueda,
  artistas, álbumes, playlists e inicio con secciones reales. MusicBrainz opcional.
  Sin equivalencias ocultas de títulos o IDs entre proveedores.
- CI `43112075731`, commit `4f4e49e`: APK y hosts Doremi Linux/Windows aprobados,
  incluido auxiliar de autenticación, licencias, versión y checksums verificados.
  CI general `43112075771`: tests completos y Clippy aprobados; otros jobs en curso.
- APK `4f4e49e` instalada sin borrar datos en Samsung ARMv7: inicio con 30 elementos
  y carátulas reales; búsqueda y campo superior visible con teclado en ambos temas.
  Biblioteca conserva su hash. Artista, álbum y playlist comprobados previamente.
- Sesión: Android usa WebView temporal en proceso separado y almacén Keystore;
  escritorio usa `doremi-auth` temporal, sin enlazar el motor web al host musical.
  Cancelar destruye proceso y perfil: comprobado en CachyOS y Samsung. Keystore
  sintético comprobado tras reinicio, con integridad y borrado. Cookies redactadas.
  Importación explícita de sesión autorizada de Doremi-Rust disponible.
- Autorización real pendiente: la primera prueba de navegador automatizado fue
  rechazada por Google. El nuevo flujo Android llegó a verificación en dos pasos;
  el usuario completó la verificación y Google mostró «Something went wrong» al
  continuar. Ajuste de entrada desde YouTube Music y cookies del WebView pendiente
  de compilar/probar. Aún no hay cuenta musical ni datos personales validados.
- Windows: ejecutables nativos compilados. Arranque gráfico/audio bloqueados en
  runner por falta de OpenGL 2.0 y dispositivo de audio; falta prueba física.
- Audio YouTube: resolución por ID y streaming AAC/MP4 conectados al motor existente;
  rangos, cancelación y seek probados. Audición real pendiente: consulta pública
  recibió petición de autorización del proveedor. No se eluden negativas.
- Biblioteca, playlists propias, cola e historial conservados. CI gráfica Linux
  verifica navegación, teclado, duplicados, playlists y restauración.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-11/` y jornadas anteriores.
- Sin releases. Documentación histórica conservada.
