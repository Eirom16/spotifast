# Doremi — avance de producto

Rama feature/doremi-product-ui, desde ea71b92. Objetivo completo abierto; sin
release, merge a main ni cambios a upstream. Informe previo y skills locales
del usuario conservados fuera de commits.

## Funcionalidad disponible

- Presentación neutral que reutiliza paleta, Inter, iconos, sidebar, filas,
  sliders, player y cabeceras de colección del legado. Inicio, Buscar,
  Biblioteca, Cola y Ajustes comparten Queue/Engine. La migración visual completa
  y las funciones avanzadas del legado continúan pendientes.
- MusicBrainz público: grabaciones, artistas, ediciones, grabaciones de artista,
  canciones de álbum, paginación/caché acotadas y guardado por ID. Solo metadatos:
  sin audio/artwork y sin sustituir una grabación por búsquedas en otro sitio.
- Commons: selección auditada CC0 del Nocturno op. 9 n.º 2 por Membeth, más dos
  tonos. Buscar «Chopin»/«Membeth» muestra la interpretación real. Esto todavía
  no es un catálogo reproducible amplio. Demo sigue etiquetada y sin audio.
- Música progresiva por rangos verificados de 128 KiB; Vorbis, Feed/CPAL y cola
  existentes. Play/pause, seek, next/previous, buffering, cancelación y expiración.
  Primer PCM sin esperar al archivo completo. UI/callback no hacen red/decodificación.
- Móvil: navegación por iconos, mini-player condicional de 72 pt, player dedicado,
  composición horizontal en dos columnas, filas de 56 pt con favoritos/menú y
  Atrás al origen. Temas claro/oscuro y morado/verde; placeholders cuando falta art.
- Playlists propias, código **8315fad**: crear/renombrar, añadir desde menús,
  duplicados ordenados, selección de la aparición concreta, subir/bajar/quitar,
  confirmar/cancelar eliminación y abrir desde Biblioteca/sidebar. Editarlas no
  modifica favoritos ni la cola en curso. JSON v1 independiente, snapshots
  inmutables, escritura privada exclusiva/atómica fuera de UI y conservación de
  documentos futuros. Primer límite: 32 listas/256 entradas totales; no cierra
  el gate de escala de F3b. Sin ampliar formatos/importadores locales.
- Los campos de nombre comparten la ruta de editor con búsqueda; Android incluye
  destino/generación para impedir que texto tardío cambie otro campo. El writer
  conserva sus APIs anteriores y publica locators antes de referencias nuevas.

## Evidencia local

32 tests del host, 19 de biblioteca y 7 de Commons aprobados; Clippy focalizado,
Rustdoc estricto, Java y circuito ligero aprobados. Los fixtures cubren rangos,
200/403/429, alteraciones, caducidad durante IO, duplicados, namespace, snapshots,
datos futuros, temporales desconocidos y fallo del índice antes de publicar refs.

GUI XTest de playlists aprobada en escritorio y a 360×640, 393×852 y 430×932:
nombre Unicode, añadir duplicados, seleccionar el segundo, reordenar/quitar,
renombrar, conservar la cola, reabrir proceso sin IO automático, reproducir sin
búsqueda y eliminar conservando favoritos/cola. Capturas claro/oscuro revisadas.
El recorrido con Commons real también pasó con salida nativa positiva; sin
captura acústica. Son builds locales, con hashes/driver en la evidencia, no
artifacts oficiales de CI.

El stream anterior pasó seek HTTP pausado a unos 120 s, reanudar, siguiente a
otro recurso CC0, anterior a música y cancelación. Primer PCM observado con
197.167/4.260.399 bytes recibidos. PCM 65.536 B/8.192 frames; buffers comprimidos
propios 262.146 B, además de HTTP/TLS/decoder. No se presenta como RSS total.
Catálogo: Björk Unicode, artista/álbum, vacío, cancelación, guardar y reabrir sin
alterar playback también se probaron en GUI.

Se conservan fallos reales de 429/timeout y de geometría. El timeout de rangos
pasó de 2 a 6 s, conexión 3 s, tras fallar sin recibir bytes. Sigue acotado y
fuera de UI; cancelación puede drenar hasta ese límite. Cada error queda
latcheado, sin retry implícito; ritmo máximo una petición/s y Retry-After
numérico/fecha. Un fixture de 2,2 s y el recorrido real posterior pasaron.

## Plataformas y CI

CI completa [42086375132](https://github.com/Eirom16/spotifast/actions/runs/42086375132),
**3201c4b: todos los jobs aprobados**, incluidos tests/lints/Rustdoc, GUI Linux,
Windows nativo, macOS, ambas ABI Android y artifacts verificados del mismo SHA.
El fallo Clippy Windows de 41969111007 se corrigió con cfg; no se silenció.

CI de playlists [42095842479](https://github.com/Eirom16/spotifast/actions/runs/42095842479),
**8315fad: fallida**. APK ARMv7/ARM64 aprobada; Windows detectó un sync de
directorio Unix aplicado en el adaptador de copia existente. Se corrigió en
e0d1744 sin omitir la prueba: flush de archivo en todas las plataformas y sync
de directorios Unix, como el writer previo. Linux agotó 20 minutos descargando
paquetes, antes de tests; d56ac45 amplía a 35 min y evita recomendaciones APT.

Validación corregida [42118302950](https://github.com/Eirom16/spotifast/actions/runs/42118302950),
**d56ac45: en curso**, con los tres jobs. Incluye paquetes modificados, Windows,
GUI Linux, Rustdoc y ambas ABI Android. El legado no cambió en este corte.

Linux: launcher aún en artifact 10f80b5, abierto en CachyOS Wayland sin autoplay,
con manifiesto/checksums y datos conservados. Falta actualizarlo al artifact nuevo.
La música/playlists actuales se verificaron en los builds locales descritos.

Android: **8315fad instalada y recorrido completo PASS físico** en SM-J701M,
API 28/ARMv7. PNG/JSON propios revisados: mini-player, claro/oscuro, búsqueda y
nombres Unicode, guardar, PCM antes del archivo completo (197.167/4.260.399 B),
pausa/seek/reanudar, Atrás, recreación y controles horizontales. Playlists:
crear, añadir duplicados, seleccionar el segundo, ordenar/quitar, renombrar y
cancelar eliminación sin alterar la cola. Reapertura en proceso nuevo: playlist
restaurada y música nativa positiva sin buscar otra vez. Sin captura acústica.
La biblioteca normal mantuvo exactamente su hash antes/después; el perfil QA
está separado y se restauró el lanzamiento normal. Los fallos previos de timeout,
toque de prueba y desconexión USB se conservan, sin atribuirles PASS.

Windows: artifact 3201c4b descargado y checksums verificados; tests/build/CLI y
Clippy nativos aprobados. GUI/audio smoke fallaron por OpenGL insuficiente y
NoDevice. Sin prueba física Windows; no se atribuye a Windows el resultado Linux.

Evidencia fuera del árbol: /home/eirom/.local/share/doremi/product-qa/2026-11-04/
y /home/eirom/.local/share/doremi/product-qa/2026-11-05/. Esta última incluye
playlists-real-music-bounded/, playlists-final-desktop/, playlists-phone-{360,393,430}/,
android-mobile-{ece1375,3201c4b}/, android-playlists-8315fad/device-reconnected/
y windows-3201c4b/. Los fallos se conservan.
No se repitieron las suites físicas extensas de formatos locales.

## Pendiente

Cerrar la CI corregida y actualizar el launcher Linux con su artifact;
comprobar la actualización Android del mismo código. Ampliar el catálogo
reproducible y artwork. Siguen pendientes historial completo, cuenta opcional
compatible, EQ/DSP/visualizadores, integración multimedia y background Android.
Audius sigue investigado, sin integrar; sus condiciones de caché/sesión y acceso
requieren adaptación. No hay integración YouTube ni promesa de acceso universal.
El inventario del legado y los gates completos permanecen en el roadmap.
