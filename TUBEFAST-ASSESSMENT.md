# Evaluación técnica: Tubefast, RustyPipe y yt-dlp

## Conclusión

**No integrar Tubefast en F1A ni cambiar de proveedor por una supuesta ventaja
de rendimiento.** Su diseño nativo pequeño y separación de decodificación
son referencias útiles, pero no constituyen evidencia de menor latencia o
soporte Android. Mantener yt-dlp como primer adaptador previsto, tras los
gates de acceso legítimo, empaquetado y benchmark de F2. Evaluar un adaptador
Rust nativo independiente después, sin acoplarlo a la cola ni copiar la UI.

RustyPipe no es una alternativa MIT intercambiable: la revisión estudiada es
**GPL-3.0-only**. No añadirla como dependencia ni copiar su implementación a
este producto MIT sin una decisión explícita de licencia/distribución. Una
frontera de procesos no resuelve automáticamente obligaciones de licencia.

No se ha ejecutado búsqueda, descarga, streaming, autenticación ni benchmarks
online con ninguno de los tres proveedores. Las cifras que siguen son
constantes del código o resultados de pruebas de dominio, no mediciones de
reproducción. No hay APK ni evidencia de compatibilidad móvil.

## Revisión y fuentes fijadas

| Proyecto | Revisión examinada | Fuentes |
|---|---|---|
| Tubefast 0.5.0 | `c6c55dd6b1ef60f0339737e7279cda2ff3b433e2` | [ytm.rs](https://github.com/yigitbozyaka/tubefast/blob/c6c55dd6b1ef60f0339737e7279cda2ff3b433e2/src/ytm.rs), [player.rs](https://github.com/yigitbozyaka/tubefast/blob/c6c55dd6b1ef60f0339737e7279cda2ff3b433e2/src/player.rs), [Cargo.toml](https://github.com/yigitbozyaka/tubefast/blob/c6c55dd6b1ef60f0339737e7279cda2ff3b433e2/Cargo.toml), [clients.json](https://github.com/yigitbozyaka/tubefast/blob/c6c55dd6b1ef60f0339737e7279cda2ff3b433e2/clients.json), README y LICENSE |
| RustyPipe, manifest 0.11.4 | `c93bafd8416843fc68251f9ce8830a89504c185e` | [manifest](https://codeberg.org/ThetaDev/rustypipe/src/commit/c93bafd8416843fc68251f9ce8830a89504c185e/Cargo.toml), README, `src/client/mod.rs`, `src/client/player.rs` de esa revisión |
| yt-dlp | `f264a6cbd1f630894da0f368df00f8aae50140e0` | [README](https://github.com/yt-dlp/yt-dlp/blob/f264a6cbd1f630894da0f368df00f8aae50140e0/README.md), requisitos/opciones/licencias de esa revisión |

SHA Tubefast obtenido por API GitHub, RustyPipe por `git ls-remote` a Codeberg,
yt-dlp por API GitHub. El intento inicial de localizar RustyPipe en
`Theta-Dev/rustypipe` de GitHub dio 404; se corrigió al repositorio canónico
Codeberg. No se añadió ninguna de estas fuentes a Cargo ni se ejecutó su código.
Solo se consultaron archivos públicos. No se importaron cookies, tokens ni
datos del usuario.

## Tubefast: catálogo y resolución

### Búsqueda

`Client::search` (`ytm.rs:404-412`) hace una petición síncrona `music("search")`
y transforma renderers JSON en secciones/items. `music` (`553-590`) usa
Innertube de YouTube Music, contexto WEB_REMIX, versión del cliente y cabeceras
propias del servicio. El idioma está fijado a inglés. No es una API pública
documentada con garantías de estabilidad, cuota, paginación o esquema.

`parse_page`/`collect`/`card` (`793+`) y el deserializador `Lean` (`689-748`)
descartan claves conocidas como innecesarias antes de construir el árbol JSON.
Puede reducir asignaciones respecto de conservar toda la respuesta, pero no
se midió ese efecto. Las demás claves/arrays aún se materializan; no existe en
`read` un límite total de bytes, profundidad o número de resultados equivalente
a un presupuesto de respuesta. La búsqueda no es una paginación neutral completa.

La biblioteca, favoritos, radio y letras reutilizan endpoints/continuations
específicos de YouTube Music. No son sustitutos de los datos Spotify ni tipos
que deban pasar al dominio. La autenticación usa cookies y firma de sesión;
no está estudiada como mecanismo de login a adoptar en Doremi.

### Resolución y `clients.json`

`stream` (`487-505`) llama al endpoint player. Si no obtiene audio, puede
invalidar visitor y repetir; si cambió la configuración remota puede repetir
de nuevo. `ensure_visitor` puede añadir otra petición previa. Por tanto,
“Rust directo” no significa necesariamente un único round trip.

`audio` (`755-766`) exige playability OK, formato con itag 140 y URL directa.
Lee contentLength/duración como strings, usando cero si no puede parsearlos.
No ofrece negociación general por codecs/contenedores ni un camino completo
para URL cifrada, DASH/HLS o duración/tamaño desconocidos.

El archivo público `clients.json` contiene versión REMIX y perfil player:
ID, user agent y contexto de un cliente VISIONOS. No es un catálogo ni una
base de pistas, ni son credenciales de Doremi. No se copia ese perfil.
`Clients::parse` (`160-168`) comprueba campos no vacíos, no una política de
hosts/capacidades o una firma de proveedor.

`refresh_clients` (`197-216`) consulta **master remoto mutable** de GitHub:
límite de lectura 16 KiB y reintento cada diez minutos, sin pin de revisión,
firma de configuración ni aprobación del cambio. Además mantiene el mutex
del control de refresh mientras espera la red. Esto permite corregir drift
sin reconstruir, pero crea una dependencia operacional externa y un cambio
de comportamiento de red fuera de la release verificada.

La selección de identidades de cliente, cabeceras de dispositivo y flags de
comprobación de contenido no se adopta como método para superar negativas
de acceso. Doremi no implementará suplantaciones/fallbacks para saltar DRM,
edad, región, login, pago o controles antibot. Un formato ausente o acceso
denegado debe producir un error visible, no desencadenar una escalada de acceso.

## Tubefast: reproducción, buffering y recursos

Referencias a `player.rs` de la revisión fijada:

| Mecanismo | Evidencia estática | Evaluación |
|---|---|---|
| Descarga progresiva | `Fetch`/`Reader`, líneas 76-230; peticiones con parámetro range | Evita exigir archivo completo para empezar. Hay que validar status/longitud y semántica de rangos, no asumirlas. |
| Ventana comprimida | rango 1 MiB, ventana objetivo 8 MiB, retención detrás 1 MiB, ahead calculado 6 MiB | Son constantes, **no RSS medido ni límite duro garantizado**. El bucle de lectura añade bytes sin cap por respuesta; un servidor que ignore el rango puede exceder la ventana. |
| Decodificación | `Track::open/next_chunk`, 246-309; Symphonia AAC/MP4, hint m4a | No decodifica en egui. Usa copia de PCM por chunk; hay que medir coste y canales/rate. |
| Backpressure PCM | `sync_channel(24)`; `try_send` y chunk pendiente, 429-548 | Acota el número de chunks, no su duración o tamaño máximo. Requiere presupuesto en frames/bytes, no solo contar mensajes. |
| Cancelación | generaciones para load/fetch; epoch para PCM/seek | Buena separación conceptual. Es cooperativa; una lectura HTTP puede seguir hasta timeout. No hay límite global de workers ni recogida completa de threads. |
| Resolución anticipada | `preload`, 652-655, llama solo `resolve` | Precarga URL/metadatos, **no PCM ni primeros paquetes**. No demuestra gapless ni latencia cero al saltar. |
| Caché de URL | `resolve`, 332-348: 64 entradas, TTL fijo tres horas; al llenarse borra todas | TTL es una hipótesis, no caducidad extraída del recurso. No retiene autenticación/headers por stream. Rediseñar invalidación y política de errores. |
| Reintentos | hasta cinco stalls sin progreso, espera creciente; connect 10 s/read 30 s en Client | No hay deadline total único de la operación. Un trickle o múltiples pasos pueden durar más; cancelación no garantiza un tiempo absoluto. |
| Dispositivo | `follow_default_device`, cada dos segundos; reapertura conservando estado | Reutilizable como idea, no copiar sobre el sink/DSP actual. El método de apertura puede bloquear al llamador. |
| Audio callback | `PcmSource::refill`, 373-395 | Usa mutex sobre receiver, sustituye/desecha Vec y llama wake. No cumple por sí solo el contrato realtime de Doremi de no esperar locks ni asignar/liberar memoria pesada. |
| Lifecycle | `decode_loop` recibe el canal y retiene su propio Sender para respuestas; sin Stop/Drop/join explícitos en Player | La desconexión del sender externo no basta para terminar el loop. Verificar salida/recogida; no adoptar como servicio móvil de larga vida sin rediseño. |

Los errores suelen ser strings; no distinguen de forma estable cancelación,
rate limit, permiso, timeout, formato incompatible y fallo de dispositivo.
El resultado de seek se consume dentro de `Track::seek` sin devolver error
estructurado al llamador. El dominio debe conservar la intención y hacer
visible el fallo, no fingir un seek confirmado.

El `selftest` (`706-750`) sirve de punto de partida para smoke de búsqueda,
apertura, decodificación y seeks. No mide primer audio audible, p95/p99,
underflows, RAM/CPU, consumo energético ni lifecycle. El README da cifras
propias de una release Windows en otra máquina: **no se usan como resultados
de Doremi ni para afirmar que gana a RustyPipe/yt-dlp**.

## Comparación de proveedores

| Dimensión | Tubefast fijado | RustyPipe fijado | yt-dlp fijado |
|---|---|---|---|
| Catálogo | YT Music, renderers/continuations propios | API async Rust más amplia, YT/YTM, parámetros y modelos tipados | Extractores de múltiples sitios, búsqueda/playlist; no biblioteca musical normalizada |
| Resolución | Visitor + player + posibles repeticiones, un formato directo | Player por clientes, selección de streams, deobfuscación JS, errores propios | Resolución por extractor; JSON para proceso externo; formatos/metadatos variables |
| Latencia potencial | Sin arranque Python, pero varios HTTP/locks posibles | Sin proceso Python, pero HTTP, JS/config/cache también cuestan | Arranque de proceso/runtime y extracción; separar frío/caliente y resolución diferida |
| Latencia **medida aquí** | No medida | No medida | No medida |
| Audio/buffer | Incluye su reproductor desktop y ventana/PCM propios | Client no equivale a salida de audio; downloader separado | No es salida de audio ni decoder; FFmpeg para ciertos flujos, no obligatorio para todo JSON |
| Recursos | Vec/JSON/threads/cache/ventana; no RSS verificado | reqwest/Tokio/rquickjs y caches; no RSS verificado | Python y potencial JS/FFmpeg; medir árbol de procesos, no solo el padre Rust |
| Fallos/drift | Strings, refresh de perfil remoto | Parser y player con fallback de clientes; mantenimiento y errores de API interna | Amplia cobertura mantenida, pero roturas externas/403/429/expiraciones igualmente posibles |
| Runtime auxiliar | No Python en ese camino | rquickjs; documentación describe helper Botguard externo | Python 3.11+, EJS/JS para cobertura YouTube completa según README; FFmpeg/ffprobe según tarea |
| Android | No APK/entrypoint Android observado; dependencias/config desktop | No prueba móvil realizada; rquickjs/TLS/procesos/helper requieren validación | No binario Linux-glibc intercambiable con Bionic; empaquetar runtimes por ABI/API |
| Licencia principal | MIT, con atribución y licencias de dependencias/assets | GPL-3.0-only en manifest examinado | Fuente Unlicense; distribución concreta cambia por bundle/dependencias |

RustyPipe referencia generación de PO tokens mediante un helper con entorno de
navegador simulado y contiene caminos de DRM/deobfuscación. **No se ejecutaron
ni se proponen para superar controles de acceso.** No es una forma garantizada
de eliminar runtimes ni una dependencia admisible sin revisión de licencia.
La existencia de un método en una librería no autoriza una capacidad de producto.

yt-dlp también incluye opciones y fallbacks que no son política de Doremi.
Se debe limitar a contenidos/operaciones autorizados y rechazar negativas;
no habilitar plugins, cookies, componentes remotos o flags arbitrarios.
Su README distingue fuente/wheel Unlicense de ejecutables PyInstaller con
componentes GPLv3+, EJS con MIT/ISC y FFmpeg de licencia dependiente del build.

## Qué reutilizar y qué no

- **Reutilizar como diseño, sin copiar código ahora:** IDs estables distintos
  de URLs, generaciones de carga y epoch de PCM, resolver cerca de Play,
  separar worker de fetch/decoder/UI, backpressure y tests de reapertura.
- **Candidatos MIT tras pruebas y atribución:** parsers puros de renderers o
  metadatos/lyrics y deserialización selectiva, con fixtures, límites y API
  neutral. Primero probar compatibilidad de schema y errores, no prometerlo.
- **Rediseñar:** HTTP y cancelación total, límites de respuesta/bytes/frames,
  pool de tareas, errores tipados, expiración de URL, cierre/join, selección
  de formato, RT callback y lifecycle Android.
- **No importar:** UI completa (egui 0.33 frente al fork 0.36.2 de Doremi),
  perfiles, autenticación/cookies, master `clients.json`, identidades de
  cliente para evadir negativas, ni su sink en sustitución del DSP legado.
- **No reutilizar RustyPipe como si fuera MIT:** mantenerlo como comparador
  externo de investigación, con revisión de licencias antes de distribuir
  cualquier herramienta que lo incorpore.

Tubefast lleva `Copyright (c) 2026 Yiğit Bozyaka`, MIT. Si se copia una porción
sustancial en otra fase, incluir ese copyright y permiso en el código/
notices distribuidos y conservar la MIT de Carmine Paolino. MIT no obliga a
publicar modificaciones, pero tampoco concede derechos sobre contenido de
YouTube, marcas, servicios ni todas las dependencias/assets (fuentes OFL,
iconos y crates). En F1A no se copia ni integra Tubefast.

## Frontera de proveedor

F1A entrega `ProviderId` + `MediaId` estable, `Media`, ocurrencia `EntryId`,
intenciones y efectos con `RequestId`. Ningún Item/Stream Tubefast, protobuf,
token, cliente YouTube, egui o objeto JNI entra en esa frontera.
`Effect::Load` pide un medio; **no contiene una URL de reproducción**.
Una respuesta solo vuelve al dueño de la solicitud y se invalida por cambio,
cancelación o restore. Los IDs de solicitud son del Queue/host de esa sesión,
no credenciales ni IDs globales intercambiables entre instancias.

F1B debe concretar el puerto de catálogo/resolver con el primer consumidor:
búsqueda acotada/paginada, resolución de un ID ya elegido, capabilities y
errores (Cancelled, Unavailable, AccessDenied, RateLimited, Timeout,
UnsupportedFormat, Transport). El adaptador posee `ResolvedStream` efímero
(URL/headers sensibles, codec/contenedor, duración/tamaño opcionales y caducidad),
no la biblioteca ni el snapshot de cola. No diseñar un super-API con todas las
capacidades Spotify/YTM. No convertir títulos Spotify en búsquedas ocultas.

## Diseño de benchmark reproducible (no ejecutado)

### Corpus, versiones y aislamiento

1. Dataset versionado de IDs de medios propios o con permiso explícito y
   licencia registrada. Casos cortos/largos, voz/música, silencio, duración
   desconocida e inaccesibles. Consultas Unicode y resultados vacíos. No
   usar cookies reales o elegir silenciosamente el primer resultado como
   identidad equivalente entre proveedores.
2. Fijar los tres SHAs anteriores, lockfiles, Rust 1.98.0, versiones de
   Python/JS/FFmpeg si se usan, OS/kernel, CPU/RAM, audio device, flags y
   SHA256 de fixtures. Archivar manifests/SBOM/notices. No benchmark de
   binarios bajados de `latest`.
3. Dos pruebas distintas: **catálogo/resolver** hasta recurso utilizable,
   y **transport/decoder común** hasta PCM/audio. No enfrentar el reproductor
   completo Tubefast contra solo `yt-dlp --get-url` y llamar a eso latencia de
   reproducción. El reproductor común aún no existe: esa parte bloquea F2.
4. Runs fríos (proceso y caches de aplicación nuevos, conexión nueva) y
   calientes (mismo catálogo/ID resuelto, con TTL/estado registrados).
   No vaciar caches del SO global ni reiniciar servicios del usuario.
   Orden de proveedores alternado, mínimo 30 repeticiones por celda,
   concurrencia 1 y 2; registrar región/red y hora sin IP personal.

### Instrumentación y criterios

Reloj monotónico y timestamps: intención admitida, worker iniciado, respuesta
de búsqueda parseada, ID seleccionado, resolución terminada, primer byte,
primer paquete decodificado, PCM en ring y primer callback no silencioso.
Audio audible necesita loopback/calibración, no confundirlo con “URL obtenida”.

Reportar distribución p50/p95/p99, n, cold/warm, fallos/timeout por categoría,
CPU user/system y pico RSS/PSS **del árbol de procesos**, bytes de red,
máximos buffers comprimidos/PCM, underflows, wakeups y cancelación/recogida.
Contar también operaciones que fallan o alcanzan el deadline, no borrarlas de
los percentiles sin declarar censura. Separar tiempo de compilación/CI de audio.

Fixtures HTTP locales para 206 correcto, rango ignorado/200 con cuerpo grande,
416, 403, 429 con Retry-After, 5xx, trickle, corte de conexión, tamaño corrupto,
URL caducada, JSON grande/malformado y respuestas fuera de orden. No probar
antibot/DRM evadiéndolos. Deadline inicial 30 s, máximo dos operaciones,
respuesta JSON ≤4 MiB, stderr ≤64 KiB y cancelación objetivo ≤2 s:
**son criterios propuestos, no resultados**. Buffer PCM debe medirse en frames
y bytes; cero I/O/locks de espera en UI y callback.

Formato por muestra: JSONL con schema/version, SHA por componente, case ID,
provider, cold/warm, tiempos por etapa, resource counters y error tipado.
Nada de cookies, respuestas completas ni URLs firmadas. Un validador debe
rechazar muestras sin versión, error/timeout oculto o etapas imposibles.
Publicar resultados y scripts como artifacts de CI manual, no ejecutar
tráfico online de terceros en cada PR.

Reproducción de lo efectivamente ejecutado en F1A:

```sh
cargo test --locked -p doremi-core
cargo clippy --locked -p doremi-core --all-targets -- -D warnings
python3 scripts/check-local.py
```

La futura harness de proveedores y dataset autorizado deben entregarse en F1B;
no se presenta aquí un comando inexistente como un benchmark ya reproducido.

## Spike Android temprano preparado para F1B/F2

**Prueba actual:** el mismo core se compila en CI como `.rlib` y sus contratos
se comprueban con `cargo check --tests --target aarch64-linux-android`. Eso no
enlaza un APK ni acredita reproducción móvil.

Spike propuesto, acotado y separado de la app/identidades Spotifast:

1. Proyecto Gradle + GameActivity/eframe mínimo, cargo-ndk, NDK/API/ABI arm64
   fijados. Un solo core por dependencia de path. Proveedor falso primero:
   trazas F1A, IME/búsqueda, rotación y respuestas tardías; mismos resultados
   que host. Ningún fork del reducer en Kotlin.
2. Prototipo de catálogo/resolver aislado para **medios autorizados**. Medir
   alternativa Rust nativa sin importar perfiles Tubefast, frente a
   Python/yt-dlp/JS empaquetados. No asumir Python del sistema ni Termux.
   No introducir helper de navegador/Botguard ni bypass como solución.
3. APK offline instalable con fixtures antes de internet; luego resolución
   online autorizada. Verificar Bionic, TLS/CA, permisos mínimos, ejecución
   de helpers desde empaquetado permitido y licencias por ABI. El cfg
   `all(unix, not(macos))` de Tubefast incluiría Android y arrastraría MPRIS:
   no es un adaptador móvil.
4. Ring PCM preasignado y worker cancelable; audio focus, MediaSession,
   foreground service, auriculares y activity/process death. Cero callbacks
   a una Activity destruida; no duplicar runtime/cola al recrearla.
5. Evidencia requerida: build APK, instalación en API mínima y actual,
   dispositivo arm64/emulador y comprobación 16 KiB de página para librerías,
   logs redactados, métricas por etapa/recursos y cierre de tareas. Comparar
   RAM/batería/latencia solo con mediciones de ese entorno.

Gate de decisión: si un proveedor exige controles de acceso no admisibles,
una licencia incompatible o un runtime no distribuible, se rechaza ese camino
y se documenta el bloqueo. No se encubre con backend hospedado, dos núcleos
o una declaración de soporte móvil basada en cross-compilar.
