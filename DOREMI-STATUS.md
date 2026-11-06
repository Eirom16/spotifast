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
  La conexión de cuenta Android sigue pendiente.
- Audio: resolución por ID y streaming AAC/MP4 conectados al motor existente;
  rangos, cancelación y seek probados con fixtures. Audición real de YouTube aún
  pendiente: la prueba pública recibió una petición de autorización del proveedor.
- Biblioteca, playlists propias, cola e historial previos se conservan.
- CI `42277426523`: APK aprobada; Linux/Windows detenidos por dos lints corregidos
  en `3f1cc02`. Pruebas y Clippy locales del cliente pasan; nueva CI pendiente.
- Push posterior bloqueado por la política de ejecución: exige aprobación de
  `git push`, pero el entorno tiene las aprobaciones deshabilitadas.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-06/`.
- Sin releases. Los informes históricos se conservan como referencia.
