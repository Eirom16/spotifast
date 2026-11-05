# Adaptador Commons experimental

Catálogo público y resolución por ID de página, sin credenciales. La búsqueda
filtra audio Ogg de la categoría CC-Zero, hasta diez resultados; resolución
vuelve a exigir licencia CC0, fuente «Own work», HTTPS y host permitido.
No se infieren derechos a partir del título, de una categoría o de una URL.
No proporciona acceso al catálogo comercial ni sustituye IDs de otro proveedor.

El subconjunto progresivo admite un único stream Vorbis, 1–2 canales,
8–192 kHz, hasta 600 s/32 MiB, según metadatos del proveedor. Duración y
parámetros se comprueban contra el contenedor/decoder. Título de archivo como
fallback cuando no hay título musical; no se inventa intérprete ni artwork.
Una búsqueda puede quedar vacía si los resultados no cumplen estas condiciones.
No hay paginación de Commons todavía.

Cada resolución crea un grant opaco, ligado a ID, URL exacta, tamaño y SHA-1,
con caducidad monotónica de diez minutos. No se serializa en la biblioteca.
Al reproducir un guardado se solicita una resolución nueva. La UI dibuja y
emite intenciones; búsqueda/resolución corren en el worker compartido con
cancelación, deadlines, generaciones y canales acotados. Crear el cliente en
un worker: los certificados del sistema pueden necesitar disco.

El lector del worker exige HTTPS sin redirecciones ni autenticación, 206,
Content-Range/Length exactos, MIME y SHA-1 de objeto publicado por Swift en
cada respuesta TLS; fija el ETag entre rangos. Esa identidad del proveedor
**no es un hash criptográfico de cada bloque recibido**. Una denegación,
identidad ausente/cambiada, 429 o un 200 ignorando Range termina el lector,
sin reintentos implícitos ni otro método de acceso. Retry-After admite segundos
y fecha HTTP. Conexión 3 s, petición 6 s, cancelación/caducidad también tras IO.

Se conserva la ruta más fuerte de la grabación auditada de Chopin por Membeth:
[music-corpus.json](music-corpus.json), tamaño/hash fijos y SHA-1 de cada rango
de 128 KiB antes del decoder. Los dos tonos auditados conservan su API de
preparación anterior; download/verify_audio no autorizan otros archivos.
No se incluyen bytes de audio ni una caché de canciones en el repositorio.

El transporte mantiene un bloque y uno entrante, máximo una petición/s.
El host reutiliza Vorbis incremental, Feed/CPAL y PCM de 8.192 frames/65.536 B.
La prevalidación de setup limita entradas/tablas Vorbis antes de crear el
codec; estos límites y los buffers no representan el RSS total del proceso.

Las pruebas unitarias son locales, sin tráfico público. El recorrido opt-in
scripts/progressive-music-qa.py admite consulta, ID explícito y duración obtenida
independientemente de los metadatos. Busca en la GUI, guarda, reproduce, hace
seek, cambia de recurso, cancela y vuelve a reproducir el guardado desde un
proceso nuevo. scripts/android-music-ui-qa.py comprueba el mismo ID en el móvil.
Las capturas/evidencia van fuera del árbol. No ejecutar contra red pública en CI.

Fuentes: [API Imageinfo](https://www.mediawiki.org/wiki/API:Imageinfo/en),
[filtros de búsqueda](https://www.mediawiki.org/wiki/Help:CirrusSearch/en),
[Vorbis I](https://xiph.org/vorbis/doc/Vorbis_I_spec.html).
