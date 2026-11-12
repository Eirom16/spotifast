# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal YouTube Music nativo, adaptado de Tubefast MIT: búsqueda,
  artistas, álbumes, playlists e inicio con secciones reales. MusicBrainz opcional.
  Sin equivalencias ocultas de títulos o IDs entre proveedores.
- CI `43123455951`, commit `f67b410`: APK y hosts Doremi Linux/Windows aprobados,
  incluido auxiliar de autenticación, licencias, versión y checksums verificados.
  CI general `43123455939` completa en verde, incluidos tests, Clippy y artifacts.
- APK `f67b410` instalada sin borrar datos en Samsung ARMv7: aislamiento y cancelación
  del login comprobados. Inicio con 30 elementos, carátulas y buscador superior con
  teclado comprobados en `4f4e49e`, en ambos temas. Biblioteca conserva su hash.
  Navegación Linux real: 20 canciones, artista con 59 elementos, álbum con 18 pistas
  y playlist con 84. Se conservan IDs del proveedor y la navegación atrás.
- Sesión: Android usa WebView temporal en proceso separado y almacén Keystore;
  escritorio usa `doremi-auth` temporal, sin enlazar el motor web al host musical.
  Cancelar destruye proceso y perfil: comprobado en CachyOS y Samsung. Keystore
  sintético comprobado tras reinicio, con integridad y borrado. Cookies redactadas.
  Importación explícita de sesión autorizada de Doremi-Rust disponible.
- Autorización real pendiente: Google volvió a atascarse tras la verificación del
  usuario en `f67b410`; el proceso musical permaneció vivo. Comparado Meld/Metrolist
  `7ab3534` (GPL-3.0): nueva adaptación con código propio de los enlaces de Android
  `intent://` y del contexto Music (visitor, cuenta y canal), sin exigir LOGIN_INFO.
  El mismo cliente Rust valida cuenta, catálogo y biblioteca y conserva el contexto
  en el almacén protegido. Local: 21 tests YouTube, incluido llavero nativo, 50 del
  host y Clippy estricto aprobados. Android adaptado pendiente de CI y prueba real;
  aún no hay cuenta musical ni datos personales validados.
- CachyOS: bundle `db4c9b0` instalado con acceso directo y auxiliar verificados;
  versiones anteriores y perfiles conservados. Cancelación y retiro de los
  procesos del navegador comprobados desde la instalación.
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
