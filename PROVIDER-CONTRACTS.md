# Doremi: contratos de catálogo y resolución F1B-1

## Decisión

El proveedor definitivo **no está elegido**. Se compararán un adaptador Rust
nativo y yt-dlp mediante experimentos reproducibles. Tubefast es referencia
de mecanismos, no una dependencia productiva; RustyPipe permanece como
referencia hasta resolver GPL-3.0-only. No se incorporó código de ninguno.

El consumidor real de estos contratos es `doremi-provider-harness`: realiza
búsqueda, selección explícita de ID, resolución y lectura de un recurso
sintético. La búsqueda y reproducción de la aplicación legado siguen con
Spotify. No se modificaron UI, DSP, credenciales ni actualizadores.

## Dominio y ejecución

`doremi-core::catalog` es un coordinador puro que recibe tiempo monotónico
inyectado y produce `ProviderCommand`. No llama a red, reloj, thread,
subproceso, JNI ni decoder. Un host ejecuta efectos en workers y devuelve
`ProviderEvent`. Nunca ejecutar el adaptador bloqueante desde UI/audio.

Identidades:

- `ProviderId` identifica la fuente lógica, no el nombre del ejecutable que
  implementa su adaptador. Cambiar de extractor no autoriza sustituir un medio.
- `MediaId` es estable y calificado por proveedor. No es un título ni una
  URL firmada. El consumidor resuelve únicamente un ID seleccionado de los
  resultados actuales.
- `SessionId`, `Ticket`, lane y generación identifican el dueño y operación.
  El host asigna sesiones nuevas, monotónicas, al reemplazar el dueño.
  No reutilizar sesiones entre instancias con callbacks vivos.
- `Router` mantiene solo fences; no contiene otra lista de resultados o cola.
  Search, Resolve y ManualEdit son lanes independientes.

`Catalog` es la autoridad de sus resultados actuales. Cancelar, nueva búsqueda
o reemplazar sesión invalida solicitudes y recursos anteriores. El adaptador
puede mantener una caché temporal no autoritativa, pero no decide qué
resultado está seleccionado ni puede reactivar una solicitud cancelada.

## Contratos implementados

| Contrato | Regla v1 |
|---|---|
| Capacidades | Search/resolve explícitos, máximo de resultados y formatos declarados; no inferir capacidades de nombres de endpoints |
| Búsqueda | Query no vacía, ≤512 bytes UTF-8, límite 1..min(capacidad, 50) |
| Respuesta | No truncar silenciosamente una respuesta sobredimensionada; rechazarla. Namespace correcto, IDs/títulos acotados |
| Resolución | ID explícito de los resultados actuales, sin equivalencias por título ni fallback silencioso |
| Deadline | Tiempo monotónico inyectado, presupuesto 1..30.000 ms, sin reloj del SO en core |
| Frontera temporal | Respuesta en el deadline o después pierde frente a Timeout; reloj regresivo se rechaza |
| Cancelación | Efecto Cancel + invalidación lógica inmediata. El host debe además interrumpir/recoger trabajo físico |
| Respuestas antiguas | Session/lane/generación y operación pendiente deben coincidir; duplicados consumidos no se aceptan |
| Errores | Cancelled, Timeout, Unavailable, AccessDenied, RateLimited con Retry-After opcional, Unsupported, UnsupportedFormat, Transport, InvalidRequest, InvalidResponse, TooLarge y Exhausted |
| Reintentos | No automáticos dentro del coordinador. Una negativa no dispara otra identidad, proveedor o método de acceso |

No se implementa paginación ilimitada, autenticación, escritura de biblioteca,
recomendaciones, selección automática de proveedores ni reproducción de audio.
Ampliar el puerto exige un consumidor y tests, no copiar un super-API Spotify/YTM.

## Recursos efímeros y secretos

`TemporaryResource` contiene ID del medio, URI HTTP(S), headers y `StreamInfo`.
Se mantiene separado de `Media`, la biblioteca y `queue::Snapshot`.
No tiene serde/Display y su Debug redacta URI y valores de headers.
Solo el transporte puede usar `expose_uri`/`expose_headers`.

Límites: URI ≤8 KiB, ≤16 headers, nombres de headers acotados con caracteres
admitidos, valores ≤8 KiB y sin caracteres de control. No se admiten
esquemas file/ftp, credenciales en authority o inyección CR/LF.
Esto es validación estructural mínima, **no una protección SSRF completa**:
un adaptador productivo debe validar hosts, DNS, redirects, permisos y política
de acceso antes de hacer la petición.

La lease distingue caducidad informada por proveedor de un límite conservador
del host. No presentar un TTL supuesto como expiry observado. Un recurso
caducado se elimina del coordinador; no se guarda como ID permanente.
Una respuesta de resolución debe corresponder al medio solicitado y formato
declarado antes de poder publicarse.

El snapshot de cola solo contiene metadatos estables. No se añade un formato
persistido ni se cambia el schema de los perfiles existentes. No registrar
objetos de transporte, cookies, queries privadas o URLs completas.

## Corte vertical de cola

`manual::remove_selected` recibe un préstamo mutable del vector manual
autoritativo de App, la proyección visible y selecciones index/key. Valida
ocurrencias, elimina en una transacción y devuelve únicamente índices para
actualizar la proyección y remapear adiciones pendientes. No retiene filas.

App sigue siendo dueño del vector existente. No se crea un `Queue` paralelo
ni una réplica autoritativa. El resto de la cola/contexto y Spirc permanecen
activos. El host sigue enviando ClearQueue + AddToQueue en el orden manual
resultante, con las mismas restricciones local/remote del legado.

La confirmación vive en core como ticket y política, no una copia del orden:
compara la respuesta con el vector autoritativo actual. Una lectura parcial
se reintenta durante la ventana de seis segundos y el presupuesto de seis
reintentos del legado. Después se conserva su fallback explícito.
El contador compartido de retries sigue en el host por compatibilidad con
otras causas de staleness (shuffle, cambio de pista y escrituras pendientes).

Una confirmación exitosa/fallback consume el reply de esa edición una vez.
Un cambio de sesión invalida el router y avanza la secuencia wire u64 para
rechazar un poll anterior incluso sin otro fetch. No se modifica el protocolo
Spirc ni se serializan tickets en perfiles.

### Límites heredados conservados

- La UI actual entrega índice + URI, no un ID global de ocurrencia. Se
  preserva la correspondencia por ordinal entre copias iguales. No se
  reconstruye una identidad que la UI no envió.
- Resume-only sigue siendo navegación de bookmarks legacy: wrap y ambigüedad
  por URI duplicada se caracterizan, no se sustituyen silenciosamente.
- El fallback tras seis reintentos puede mostrar estado remoto incoherente;
  es una compatibilidad deliberada, no confirmación fuerte del servidor.
- No se transfirió autoridad de radio/autoplay, contexto paginado, historial
  remoto ni recuperación completa de sesión al nuevo reducer.

## Harness reproducible implementado

```sh
python3 scripts/test-provider-harness.py
python3 scripts/provider-bench-summary.py target/provider-fixtures/samples.jsonl
```

El primer comando compila solo la herramienta pequeña de host y usa un
servidor loopback con WAV PCM16 generado por el propio test, declarado CC0-1.0
y acompañado de SHA-256. No usa medios de terceros, cookies reales ni internet.
Produce 40 registros JSONL: diez casos, dos temperaturas y dos muestras.

Casos: éxito 206, 403, 429/Retry-After, 404, JSON malformado, demasiados
resultados, rango ignorado/200, truncado, cuerpo declarado excesivo y timeout.
El cliente de fixture tiene límite de headers/cuerpo y deadline total; no es
un cliente HTTP productivo y rechaza destinos no loopback.

JSONL incluye revisión/dirty, caso, proveedor falso, versión de herramienta,
temperatura, cache hits, etapas search/resolve/transport/total y error tipado.
Warm/cold se refiere **solo a la caché en memoria del adaptador falso**;
no mide arranque de proceso ni caches de un proveedor real.

Primer PCM, audio audible, cancelación física, CPU y RSS están explícitamente
en null: **no se midieron**. El resumen muestra p50/p95 solo de etapas
exitosas y conserva denominadores, fallos y categorías; no oculta timeouts.
El validador rechaza métricas imposibles, secretos y afirmaciones de audio
sin medir. Con dos muestras de smoke no inferir rendimiento estadístico.

Los candidatos reales, corpus autorizado, n suficiente, CPU/RSS del árbol de
procesos y medición de primer PCM/audio requieren una siguiente etapa.
No comparar el extractor de un candidato contra el reproductor completo de otro.
