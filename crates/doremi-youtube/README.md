# Diagnóstico de reproducción

## Extracción de escritorio

Linux y Windows usan **yt-dlp instalado** como extractor desde el principio.
El catálogo YouTube Music y la reproducción/decodificación Rust no cambian.
Requisitos: Python 3.11+, yt-dlp 2026.10.26 o posterior con `yt-dlp-ejs`,
y Deno en `PATH`. En Linux se invoca `python3`; en Windows, `python`.
El runtime no está incluido en estos artefactos de desarrollo.

El proceso recibe la sesión ya autorizada por una tubería privada, con cookies
limitadas a `.youtube.com`. No usa archivos de cookies ni argumentos secretos,
configuraciones personales, plugins de usuario o componentes remotos. Devuelve
solo una concesión AAC/MP4 para la identidad solicitada. El supervisor acota
stdout/stderr, tiempo y concurrencia, y recoge también Deno al cancelar.
Una negativa es un error visible; no activa el extractor nativo como fallback.

La comparación real con yt-dlp 2026.10.26 mostró que `web_music` exige un PO
Token GVS y descarta formatos no utilizables. La extracción normal, con la misma
sesión, obtuvo AAC accesible más allá de 1 MiB para ambas canciones. No se
generan tokens de atestación ni se implementan mecanismos de YouTube aquí.

`Client::resolve_native` conserva el experimento anterior para diagnóstico.
Android sigue usando ese camino: **la integración de yt-dlp en Android todavía
no está implementada**. No afirmar paridad móvil por compilar la APK.

## Transporte y diagnósticos

El lector comparte código entre Linux, Android y Windows. Solicita bloques
de 64 KiB con `Range: bytes=inicio-fin` desde la primera petición, retirando
cualquier query `range` heredada sin recodificar los octetos firmados.
Exige `206`, `Content-Range` exacto y longitud/cuerpo íntegros. Un `200`
representa un rango ignorado y falla aunque tenga el tamaño solicitado.
No existe fallback de protocolo, cliente o concesión tras un rechazo.

Dos reintentos como máximo, sobre la misma concesión, recuperan timeout,
interrupción o truncamiento; backoff de 50/100 ms cancelable. Si un `206`
identifica exactamente el intervalo, un prefijo incompleto queda privado y
el siguiente intento solicita solo su cola faltante. Cada cola vuelve a
validar rango y longitud. Solo el bloque íntegro llega al demuxer; agotar
los tres intentos o recibir una negativa descarta el prefijo. Un cuerpo
del tamaño completo sin terminación HTTP válida tampoco se acepta ni
produce una petición vacía: se vuelve al prefijo del intento anterior.
401/403, 429, rango inconsistente y desborde son definitivos. El presupuesto
individual sigue siendo 6 s; la cancelación retira el future HTTP en intervalos
de 20 ms. DNS del sistema queda sujeto al comportamiento del runtime.

Un worker anticipa como máximo cinco bloques comprimidos; la generación
de seek invalida el trabajo anterior. Cuatro respuestas en cola y una pendiente
de entrega retienen, junto al bloque del consumidor, como máximo 384 KiB
(más seis bytes de comprobación y los buffers del cliente HTTP). Son 256 KiB
adicionales frente al anticipo de un bloque, sin otro thread ni cambios al
buffer PCM de 8192 frames. No espera llenar la ventana para entregar el primer
bloque. Las respuestas ya validadas siguen disponibles mientras HTTP espera
otro intervalo; seek/Drop retiran también un sender bloqueado por backpressure.
Ningún bloque incompleto se publica. Una negativa anticipada queda terminal.

La ventana responde a una recuperación real observada de 15 s: un bloque de
AAC a 128 kb/s cubre unos 4 s; cinco cubren el presupuesto existente de
3 × 6 s + 150 ms. No garantiza continuidad cuando la red queda por debajo
del bitrate durante más tiempo. Medir memoria, seek y starvation por separado;
el límite calculado no sustituye una medición de PSS en ARMv7.

No hay logs detallados permanentes de HTTP. El snapshot QA activa una traza
circular de 256 operaciones en memoria y la exporta explícitamente como
`online.latency.http_reads`, vinculada al RequestId del mismo objeto.
Incluye secuencia, offset, longitud, tiempos monotónicos, estado HTTP,
bytes recibidos, validez de longitud/rango y categorías fijas de resultado.
No conserva URLs, cookies, encabezados privados ni texto de errores externos.

El supervisor conserva el presupuesto de resolución existente de 120 s.
Cancelar o cambiar de pista retira el future y el proceso sin esperar ese plazo.
No se amplían permisos ni se reintentan denegaciones.

## Prueba explícita con audio nativo

```sh
./stream_probe --session PRIMER_VIDEO_ID SEGUNDO_VIDEO_ID
```

El ejecutable viene en el artefacto `doremi-native-linux-<commit>` de
`android.yml` (en Windows, `stream_probe.exe`). Compilar y ejecutar los tests
en GitHub Actions, no en el equipo de prueba.

Usa dos identidades distintas elegidas por el operador y la sesión existente
del llavero, sin importarla ni modificarla. Sin `--session` no lee credenciales.
No rota clientes después de una negativa. No guarda URLs ni datos de biblioteca.
Usa el mismo lector HTTP, decodificador AAC y salida nativa de la aplicación,
con volumen de aplicación 0.08. Exige al menos 15 segundos de frames PCM en la
salida por pista, pausa estable, reanudación y cambio de pista. No sustituye
la prueba de UI ni la prueba de la APK en Android físico.

Las métricas de callback con muestras no nulas no son una captura acústica:
hay que comprobar también que el audio se oye. Un timeout de resolución o
descarga es un fallo de la prueba, nunca reproducción verificada.

## Preparación y medición en la rama de integración

Catálogo y resolución trabajan sobre snapshots con el mismo pool HTTP y la
misma sesión inmutable en memoria. El mutex solo protege captura/publicación,
no esas peticiones de red. Un cambio de sesión invalida publicaciones antiguas,
también invitado → login → logout → invitado. Login/restauración aún verifican
la cuenta bajo el mutex; su espera se mide, no se oculta.

El host prepara **solo una resolución YouTube** en los últimos 30 segundos de
una pista con duración conocida. No descarga fragmentos de audio ni canciones.
Consulta la continuación real de la cola, incluido repeat/shuffle/historial.
La preparación tiene un deadline de 15 s, retención máxima de 45 s y margen de
caducidad de 5 s. No se refresca en bucle. Pausa, edición de cola, seek,
logout/cambio de sesión y suspensión invalidan el trabajo. La petición directa
retira el prefetch en curso; una concesión ya terminada solo se consume para el
ID y sesión correctos. Saltar temprano o desconocer la duración puede no tener
prefetch. No cambia qué pista selecciona la cola.

`online.latency` y `audio_metrics` del diagnóstico del host incluyen el mismo
RequestId y origen monotónico desde la intención recibida por la cola.
`first_pcm_us`, `first_callback_us` y `first_nonzero_callback_us` de YouTube
incluyen ahora espera y resolución, no solo `Engine::load_stream`.
`first_bytes_us` significa primer bloque HTTP completo validado, no primer
byte de socket. `device_start_us` inicia descubrimiento y `device_ready_us`
termina construcción de salida, después del prebuffer del decoder.
No se añaden logs de URLs/cookies ni persistencia de concesiones.

Para cada intento, conservar fuera del árbol un snapshot final y capturar un
registro sin consultas, nombres, cabeceras, bibliotecas ni URLs:

```sh
python3 scripts/playback-latency-summary.py capture /ruta/qa-state.json \
  --commit SHA_COMPLETO_DEL_BINARIO --platform linux --scenario cold \
  --run proceso-01 --outcome ok >> /ruta/samples.jsonl
python3 scripts/playback-latency-summary.py summarize /ruta/samples.jsonl
```

El commit debe ser el del artefacto verificado, no otro checkout. Usar un
`--run` diferente por proceso. Registrar también `error` y `cancelled`,
sin volver a capturar una solicitud como muestra nueva. El resumen separa
commit/plataforma/escenario/evidencia sintética, informa denominadores,
p50/p95/mínimo/máximo y marca menos de 20 éxitos como exploratorio.
La clasificación fría/caliente o primera tras login la decide el operador:
el snapshot no demuestra el estado de las cachés del SO ni el login previo.
Reanudar no crea otra solicitud y todavía necesita un reloj de intención de
resume separado; no reutilizar los tiempos originales como latencia de resume.
Este colector es explícito, no añade telemetría ni acceso automático a sesiones.

La comparación cuantitativa y los gates pendientes están en
[`DOREMI-PLAYBACK-AUDIT.md`](../../DOREMI-PLAYBACK-AUDIT.md).

El resumen incluye media, p50/p95 y límites de cada timestamp y de intervalos
calculados por solicitud (`intervals_us`); nunca resta percentiles de etapas
distintas. Un intervalo sin ambos extremos se conserva desconocido. La tasa
de no-éxito incluye errores y cancelaciones, con contadores separados. Los
intervalos pueden solaparse: descubrimiento de dispositivo/prebuffer incluye
trabajo del decoder; no sumarlos como costes exclusivos. Python, yt-dlp,
Deno, cuenta y QuickJS aún requieren subdivisión adicional antes de atribuir
el coste interno del extractor. No inferir sus tiempos del arranque de un
proceso vacío ni de fixtures sintéticos.

Protocolo pendiente de autorización: fijar dos IDs aprobados y una sesión
de QA aislada, verificar manifiesto/SHA256 del artefacto, anotar red y orden
del corpus fuera del árbol, y capturar una instantánea final por intento
(incluidos fallos). Alternar base/candidato bajo las mismas condiciones,
20 intentos por escenario y versión. Separar primera carga del proceso,
restauración/login, siguiente sin reiniciar, pista repetida, prefetch y seek.
Los escenarios `next` y `prefetched` deben usar el mismo par ordenado. Resume
no tiene un reloj propio todavía y queda `NOT_VERIFIED`. El probe separado
no demuestra la cola ni prefetch del host. No usar el llavero personal para
suplir una sesión de QA no autorizada.

Para la misión de continuidad, `stream_probe --full qQzdAsjWGPg _d3SaQI_Ag4`
realiza tres intentos por pista hasta `EngineEvent::Ended` o fallo, sin leer
credenciales. `EOF_REACHED` describe solo finalización; no acredita seek,
ausencia de underruns ni audio acústico. El JSON por segundo informa posición,
frames de silencio, episodios, underruns y xruns por separado. Los controles
y las pruebas físicas Android se validan desde el host con perfil QA aislado.
