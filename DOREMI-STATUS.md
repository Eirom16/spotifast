# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal YouTube Music nativo, adaptado de Tubefast MIT: búsqueda,
  artistas, álbumes, playlists e inicio con secciones reales. MusicBrainz opcional.
  Sin equivalencias ocultas de títulos o IDs entre proveedores.
- CI `43117255787`, commit `db4c9b0`: APK y hosts Doremi Linux/Windows aprobados,
  incluido auxiliar de autenticación, licencias, versión y checksums verificados.
  CI general `43117255808` completa en verde, incluidos tests, Clippy y artifacts.
- APK `db4c9b0` instalada sin borrar datos en Samsung ARMv7: aislamiento y cancelación
  del login comprobados. Inicio con 30 elementos, carátulas y buscador superior con
  teclado comprobados en `4f4e49e`, en ambos temas. Biblioteca conserva su hash.
  Navegación Linux real: 20 canciones, artista con 59 elementos, álbum con 18 pistas
  y playlist con 84. Se conservan IDs del proveedor y la navegación atrás.
- Sesión: Android usa WebView temporal en proceso separado y almacén Keystore;
  escritorio usa `doremi-auth` temporal, sin enlazar el motor web al host musical.
  Cancelar destruye proceso y perfil: comprobado en CachyOS y Samsung. Keystore
  sintético comprobado tras reinicio, con integridad y borrado. Cookies redactadas.
  Importación explícita de sesión autorizada de Doremi-Rust disponible.
- Autorización real pendiente: la primera prueba de navegador automatizado fue
  rechazada por Google. El nuevo flujo Android llegó a verificación en dos pasos;
  el usuario completó la verificación y Google mostró «Something went wrong» al
  continuar. Entrada desde YouTube Music y cookies entre sitios del WebView ya
  compiladas; autorización real de este ajuste aún no probada. Se corrige además
  la espera innecesaria por una navegación: las cookies propias de Music se pueden
  entregar aunque siga visible Google; validación musical obligatoria en Rust.
  Esta última corrección está pendiente de CI y prueba real.
- Windows: ejecutables nativos compilados. Arranque gráfico/audio bloqueados en
  runner por falta de OpenGL 2.0 y dispositivo de audio; falta prueba física.
  Almacén nativo y compatibilidad UTF-16 de Python keyring comprobados con dos
  pruebas sintéticas ejecutadas en Windows; sin leer credenciales reales.
- Audio YouTube: resolución por ID y streaming AAC/MP4 conectados al motor existente;
  rangos, cancelación y seek probados. Audición real pendiente: consulta pública
  recibió petición de autorización del proveedor. No se eluden negativas.
- Biblioteca, playlists propias, cola e historial conservados. CI gráfica Linux
  verifica navegación, teclado, duplicados, playlists y restauración.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-11/` y jornadas anteriores.
- Sin releases. Documentación histórica conservada.
