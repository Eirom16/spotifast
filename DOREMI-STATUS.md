# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal: YouTube Music nativo, adaptado de Tubefast MIT. MusicBrainz
  queda disponible como fuente opcional de metadatos; sin equivalencias ocultas.
- APK `ffb1fe9` instalada sin borrar datos en Samsung ARMv7: búsqueda de 20 canciones,
  artista, álbum con 32 pistas y carátula real, playlist con 25 pistas, comprobados.
  La biblioteca habitual conserva su hash; evidencia fuera del árbol.
- Cuenta: Google rechazó el navegador automatizado en la prueba real. Login sin
  validar. Conexión explícita a una sesión autorizada de Doremi-Rust desde su
  llavero implementada localmente; no importa ni modifica el origen automáticamente.
  Android incorpora una Activity de autenticación temporal en proceso separado,
  con entrega al cliente musical Rust y cierre del WebView/proceso; conserva el
  selector de sesión autorizada y Keystore. APK y autorización real pendientes.
  El callback localhost del borrador de escritorio no captura cookies de YouTube
  y queda fuera del flujo productivo; la ventana nativa de escritorio sigue pendiente.
  El fallback de pegado oculta la cookie y la excluye de diagnósticos persistidos.
- Buscador móvil fijado arriba de los resultados; geometría comprobada en
  360/393/430 dp con teclado simulado, scroll y ambos temas. Nueva APK pendiente.
- Audio: resolución por ID y streaming AAC/MP4 conectados al motor existente;
  rangos, cancelación y seek probados con fixtures. Audición real de YouTube aún
  pendiente: la prueba pública recibió una petición de autorización del proveedor.
- Biblioteca, playlists propias, cola e historial previos se conservan.
- CI `42378699288` detenida por formato del borrador de autenticación; corregido
  localmente. Host: 46 pruebas aprobadas y Clippy estricto; comprobación Java
  sintáctica y manifiesto aprobados, sin equivaler a compilación o QA Android.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-06/`.
- Sin releases. Los informes históricos se conservan como referencia.
