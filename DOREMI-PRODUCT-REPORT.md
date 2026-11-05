# Doremi — avance de producto

Rama feature/doremi-product-ui, desde ea71b92. Objetivo completo abierto,
sin release estable ni cambios a main/upstream. Informe previo y skills locales
del usuario preservados fuera de commits.

## Funcionalidad disponible

- Presentación neutral con paleta, Inter, iconos, sidebar, slider, botones,
  player y cabeceras de colección extraídos del legado. Artista/álbum/grabación
  usan la cabecera compartida; el legado conserva artwork, bidi, i18n y enlaces.
- Inicio, Buscar, Biblioteca, Cola, Ajustes y player comparten Queue/Engine.
  Navegación táctil, claro/oscuro y acentos morado/verde.
- MusicBrainz público: grabaciones, artistas, ediciones, grabaciones de artista,
  pistas de álbum, paginación/caché acotadas, generaciones y guardado por ID.
  **Solo metadatos:** no audio, carátulas ni sustitución por búsquedas de otro sitio.
- Buscar → Commons → «Chopin» o «Membeth» devuelve una interpretación CC0
  auditada del Nocturno op. 9 n.º 2, de casi cuatro minutos. La licencia, identidad,
  tamaño y hash se verifican otra vez al seleccionar. No representa todo Commons.
- El lector HTTP progresivo entrega bloques de 128 KiB autenticados antes de
  decodificar. Symphonia, Feed/CPAL, cola, gain y cancelación son los existentes.
  Seek por rangos y estado de buffering; UI/callback no hacen red ni decodifican.
  Los dos tonos Commons mantienen su camino preparado. Demo permanece explícita
  y sin audio. No se añadieron formatos locales.

## Evidencia y pruebas

Linux, build local probado en GUI mediante XTest y salida nativa: **PASS** para
buscar música real, guardar ID, emitir PCM antes del archivo completo, pausa con
reloj estable, seek HTTP en pausa a unos 120 s, reanudar, next a otro recurso CC0,
previous a la música y cancelar salida/red. No hubo captura acústica.

Primera observación con PCM no nulo: 197.167 bytes de audio recibidos de
4.260.399 totales; buffer PCM 8.192 frames estéreo/65.536 bytes. El límite de
buffers comprimidos propio es 262.146 bytes, además de buffers de HTTP/TLS y
decoder; no se presenta como RSS total. El ejecutable exacto, driver y hash
del build local están preservados en la carpeta gui-final de la evidencia.
Son una prueba local de desarrollo, no un artifact oficial de CI.

Antes hubo un fallo real de seek por HTTP 429. Se conserva el fallo. El lector
ahora termina tras cualquier error y no permite reintentos implícitos del
demuxer; limita ritmo a una petición/s y respeta Retry-After numérico/fecha.
El recorrido completo posterior pasó. También se corrigió el preroll Vorbis
tras seek; sus muestras coinciden con el decoder de referencia.

25 tests del player con todas sus features, 28 del host y 5 de Commons pasaron.
Clippy de host/player/Commons con all-targets/all-features, Rustdoc estricto
y circuito ligero pasaron. Los fixtures HTTP cubren 206, rango ignorado/200,
403, 429, Content-Range incorrecto, bytes modificados, cancelación y caducidad.
No ejecutan tráfico público en la CI ordinaria.

UI de catálogo anterior: GUI Linux PASS a 1240×800 oscuro y 384×658 claro;
Björk Unicode, artista, álbum, vacío, cancelación, guardar y reabrir sin alterar
playback. Geometría comprobada también a 360, 393 y 430 pt, paisaje e IME.
Persistencia y reglas de duplicados/cola continúan sobre el núcleo compartido.

## Plataformas y CI

CI [41959750650](https://github.com/Eirom16/spotifast/actions/runs/41959750650),
código 05e91b7: **todos los jobs aprobados**, incluidos lints/Rustdoc,
tests completos, Linux gráfico, nuevo ejecutable Windows, macOS y APK.

CI del stream [41964907308](https://github.com/Eirom16/spotifast/actions/runs/41964907308),
código 00cf440: **en curso**. Debe completar la nueva matriz y producir artifacts
del mismo código antes de instalar/verificar el stream en Android/Windows.

Linux instalado desde artifact 10f80b5 con manifiesto/checksums completos,
conservando versiones y datos. Launcher «Doremi Native Dev» abierto en CachyOS
Wayland, sin autoplay. Ese launcher aún debe actualizarse al artifact del stream.

Android: búsqueda MusicBrainz probada físicamente en SM-J701M, API 28/ARMv7;
APK 10f80b5 instalada conservando biblioteca. El teléfono vuelve a estar conectado.
**Stream y nueva UI móvil aún pendientes de APK/validación física.**

Windows: builds/tests/CLI del nuevo doremi.exe aprobados en runner nativo.
El smoke 10f80b5 no pudo abrir GUI/audio: OpenGL insuficiente y NoDevice.
No hay prueba física Windows ni se atribuye el resultado Linux a esa plataforma.

Evidencia persistente fuera del árbol:
/home/eirom/.local/share/doremi/product-qa/2026-11-04/.
El corte musical está en progressive-chopin/; capturas de catálogo en
hero-final-dark/ y hero-final-light-phone/. Se conservan los fallos.

## Pendiente y siguiente corte

La selección reproducible es pequeña. Faltan catálogo musical amplio, artwork,
playlists propias, historial completo, cuenta opcional, EQ/DSP/visualizadores,
integración multimedia y segundo plano Android; el inventario del legado
permanece en el roadmap. Audius sigue como candidato investigado, sin integrar;
sus condiciones de caché/sesión y controles de acceso requieren adaptación.

Se aplica la skill local doremi-mobile-ui: el player expandido al navegar y la
barra de cinco botones grandes siguen pendientes. Siguiente composición:
navegación compacta con iconos, mini-player de contenido activo y player dedicado,
preservando desktop y las intenciones existentes. Primero se descargará/verificará
la APK del stream y se probará en el dispositivo disponible.
