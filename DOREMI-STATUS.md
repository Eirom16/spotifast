# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal: YouTube Music nativo, adaptado de Tubefast MIT. MusicBrainz
  queda disponible como fuente opcional de metadatos; sin equivalencias ocultas.
- APK `13c83e3` instalada sin borrar datos en Samsung ARMv7: búsqueda de 20 canciones,
  miniaturas reales y buscador superior con teclado, comprobados en ambos temas.
  Artista, álbum y playlist ya comprobados en `ffb1fe9`. Biblioteca conserva su hash.
- Cuenta: Google rechazó el navegador automatizado en la prueba real. Login sin
  validar. Conexión explícita a una sesión autorizada de Doremi-Rust desde su
  llavero implementada localmente; no importa ni modifica el origen automáticamente.
  Android incorpora una Activity de autenticación temporal en proceso separado,
  con entrega al cliente musical Rust y cierre del WebView/proceso; conserva el
  selector de sesión autorizada y Keystore. APK y autorización real pendientes.
  Escritorio incorpora `doremi-auth`, WebView nativo en proceso temporal con perfil
  privado, IPC autenticado, cancelación y limpieza; el host musical no enlaza el
  motor web. Comprobación Linux y tests pasan; build Windows y autorización real
  pendientes. Payloads de CI incluyen auxiliar, licencias, versión y checksum.
  Keystore Android probado con datos sintéticos: reinicio, cifrado, integridad y
  cierre de sesión. El nuevo login y la redacción de cookies aún no están en la APK.
- Buscador móvil fijado arriba de los resultados; geometría en 360/393/430 dp y
  prueba real del teclado en 384 dp, con ambos temas, aprobadas.
- Audio: resolución por ID y streaming AAC/MP4 conectados al motor existente;
  rangos, cancelación y seek probados con fixtures. Audición real de YouTube aún
  pendiente: la prueba pública recibió una petición de autorización del proveedor.
- Biblioteca, playlists propias, cola e historial previos se conservan.
- CI `42378699254`: APK aprobada; hosts detenidos por formato, corregido localmente.
  Local: host 46 tests, YouTube 14 (uno nativo ignorado), Clippy estricto incluido
  auxiliar, check-local y 27 checks de política aprobados. Nuevos builds pendientes.
- Push bloqueado: política de ejecución exige aprobación, deshabilitada en el entorno.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-06/` y `2026-11-07/`.
- Sin releases. Los informes históricos se conservan como referencia.
