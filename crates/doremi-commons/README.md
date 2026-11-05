# Adaptador Commons experimental

Transporte async compartido, extraído del experimento ejecutable después de
verificar los dos tonos CC0 reales. Conserva la allowlist de corpus, IDs,
licencia, hashes, HTTPS/hosts, límites, agente transparente y errores tipados.
No depende de egui, audio, Spotify, credenciales, procesos o un backend propio.
El núcleo sigue sin dependencias y decide tickets, selección y leases.

Crear el cliente en un worker: cargar certificados del sistema puede acceder
a disco. Búsqueda/resolución/descarga son futuros; el host debe cancelarlos al
reemplazar una intención y descartar completions por ticket/generación.
`download` verifica formato, lease, ausencia de headers de autenticación,
identidad auditada, tamaño y hash antes de devolver bytes al decoder separado.
No declara autorización para descargar todos los resultados CC0 de búsqueda.

El CLI `tools/provider-live-probe` consume este mismo adaptador. El subproceso
yt-dlp permanece exclusivamente en ese laboratorio y no forma parte de la
biblioteca. El host Linux incorpora un worker/UI experimental en validación:
una búsqueda y una preparación concurrentes, generaciones, deadlines, colas
acotadas y dos PCM retenidos. La cola sigue en core; el adaptador solo consume
el ID explícito del efecto Load y nunca sustituye por título. Sus pruebas UI
y de cancelación reales se registran separadamente del probe CLI.


El corte musical añade una interpretación CC0 de Chopin por Membeth.
Buscar «Chopin» o «Membeth» consulta la identidad auditada en el proveedor;
no promete el catálogo completo de Commons. Los metadatos/licencia/tamaño/hash
se vuelven a validar al resolver. La fuente y los hashes están en
[music-corpus.json](music-corpus.json); no se distribuyen sus bytes de audio.

El lector progresivo obtiene rangos HTTPS de 128 KiB, verifica Content-Range,
tamaño y hash por bloque antes de entregar bytes. Mantiene un bloque y un bloque
entrante, con ritmo máximo de una petición por segundo. Un 200 ignorando Range,
denegación, 429 o bloque distinto terminan el lector; las siguientes lecturas
no vuelven a pedir red. Retry-After admite segundos y fecha HTTP. Cancelación y
caducidad se comprueban entre operaciones; cada petición tiene timeout de 2 s.
El cliente blocking se crea/reutiliza en el worker, nunca en UI o callback.

El host adapta Read/Seek a Symphonia y reutiliza el Feed/CPAL existentes.
El PCM mantiene el buffer original de 8.192 frames estéreo/65.536 bytes.
La API antigua download y el CLI mantienen su alcance de dos tonos preparados.
El recorrido opt-in de música, con búsqueda, seek y salida nativa, se ejecuta
con scripts/progressive-music-qa.py; no genera tráfico público en la CI ordinaria.
