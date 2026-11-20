# Doremi: fiabilidad del streaming P0

Corte de investigación: 2026-11-19–20. Rama `integration/doremi-premerge-validation`.

**El candidato no cumple la aceptación de reproducción completa. No recomendar fusión alpha.**

## Procedencia y alcance

- Inicio local/remoto: `b5cda4a340372bde3d95a09003be09c91867cc14`.
- `main` inicial/remoto: `cd40da022edd35d6017b3093421b8b83071722a6`.
- Candidato de código y tests: `c18786424d0a673a69ec530312b732d41d46e687`.
- Paso físico previo: `b39753db45df92d2c57ab311fb2e85efc8a17934`, no certifica c187864.
- Candidato intermedio físico: `4b903bb01bdf3b17a2ee7bf158339f183d249157`.
- Baseline físico instrumentado de esta misión: `fd36e5b7a05518242b71545b3658f946fe5df593`; sus resultados no certifican el candidato posterior.
- Worktree limpio inicial: `/home/eirom/orca/workspaces/spotifast/doremi-premerge-validation`.
  El checkout de producto y sus cambios anteriores no se modificaron.
- Autorización vigente: las dos identidades `qQzdAsjWGPg` y `_d3SaQI_Ag4`,
  perfil QA Android `premerge-youtube-20261118`, push de esta rama y ambos
  workflows. Consta en §2 de `DOREMI-PREMERGE-REPORT.md` y en esta misión.
  Sin merge, force-push, release, nuevos clientes ni exportación de secretos.

Evidencia fuera del árbol:
`/home/eirom/.local/share/doremi/product-qa/2026-11-19/streaming-reliability/`.
Las cifras del informe anterior son históricas, no resultados de este candidato.

## Defectos investigados y cambios

| Hipótesis / defecto | Evidencia y corrección | Límite |
| --- | --- | --- |
| Un 200 sin Content-Range identifica suficientemente el intervalo | No lo acredita. Ahora se envía Range estándar desde la primera petición y se exige 206/Content-Range exacto; 200 se rechaza incluso con tamaño exacto | Los probes externos no demuestran por qué el servidor emitió los 200 vacíos históricos |
| Un corte HTTP equivale a EOF | No: solo se publica un bloque íntegro validado. Dos reintentos máximos de la misma concesión, backoff 50/100 ms; prefijo privado de 206 exacto y reanudación solo de su cola faltante | No se renuevan grants ni se reintentan denegaciones |
| La cancelación tiene que esperar seis segundos | El antiguo bloqueo era una decisión del adaptador. Ahora cada future de petición/cuerpo se conserva y comprueba cancelación cada 20 ms; se retira al cancelar | DNS del sistema no queda certificado por las fixtures de loopback |
| El productor puede esperar cada GET sin agotar PCM | El buffer de 8192 frames cubre 170,7 ms a 48 kHz / 185,8 ms a 44,1 kHz. Una recuperación real de 15,17 s agotó un bloque anticipado y dejó 11 s sin PCM. El mismo worker anticipa hasta cinco bloques comprimidos | La red sostenidamente insuficiente sigue siendo un límite; medir el candidato físicamente |
| El timeout de seek prueba un defecto de Symphonia | El plazo de 3 s de prebuffer incluía el seek/indexado inicial. Se prepara el resampler/seek antes de comenzar ese plazo; se conserva la apertura progresiva MP4 | No demuestra que esa fuera la única causa del seek real a 99 s |
| Un timeout de prebuffer retira el I/O | Antes el stop del feed no alcanzaba el lector. Ahora ambos comparten un token; se cancela antes del join del productor | Validado con fixture cancelable; verificar también la ruta física |

`c187864`: prebuffer de dos intervalos antes del primer byte y tras cambios de
generación, usando la ventana ya existente. La pareja física b39753d llegó a
EOF en ambas pistas: q tuvo 5,637 s de silencio inicial y _d3 cero underruns.
En q, el segundo bloque tardó 9,728 s mientras el primero daba ~4 s. El nuevo
control desplaza esa espera a buffering, sin ampliar buffers/timeouts. Un
recurso de un bloque no espera otro; errores conocidos se conservan antes de
entregar bytes. Cuatro fixtures nuevas verifican espera inicial, seek nuevo,
cancelación <250 ms, EOF corto y rango inválido sin audio. En el candidato final Android se verificó que los tres arranques PCM esperaron
al segundo intervalo validado. Después volvió a agotarse la alimentación: la
precarga inicial no resuelve una transferencia sostenidamente insuficiente.

`bd62bdb`: seek/prebuffer, cancelación del feed y métricas PCM.
`8b67add`: I/O cancelable, recuperación acotada, traza HTTP y lectura anticipada.
`a1e5ec1`: Range estándar/206 obligatorio y contexto Tokio al crear send().
`7b09099`: regresión de anticipación y negativa terminal.
`7fe440f`: `inspect_err` en dos observadores de error; corrección de Clippy, sin cambiar la política HTTP.
`b39753d`: conserva AccessDenied del productor PCM hasta el error existente de UI, también después del prebuffer.
`3d1f619`: conserva anticipación en seeks del demuxer dentro del bloque validado.
`0b581ce`: ventana anticipada acotada, recepción disponible mientras otro GET espera, retirada de sender bloqueado.
`4b903bb`: reanudación por cola faltante, sin publicar prefijos ni aceptar un cuerpo sin terminación HTTP válida.
`fd36e5b`: rechazo visible de un reloj AAC discontinuo, en vez de cambiar
la posición lógica del PCM o prolongar la última muestra al agotar el input.

La primera ejecución local del cambio async detectó creación de `send()` fuera
 del contexto Tokio; siete fixtures fallaron. Se corrigió antes de las pruebas
 del candidato final. No se atribuye éxito a esa versión intermedia.

## Contrato de transporte

Un solo worker posee HTTP. `Read`/`Seek` requieren acceso exclusivo al lector.
Cada intervalo se alinea a 64 KiB y se limita por la longitud de la concesión.
Se retira únicamente una query `range` heredada, preservando los demás octetos
firmados; no se cambian identidad, cliente, credenciales o proveedor.

Se aceptan exclusivamente 206 con Content-Range exacto (inicio, fin y total),
Content-Length coherente cuando exista y cuerpo íntegro. Un bloque parcial,
cancelado, desbordado o de otra posición nunca entra en caché. El offset solo
avanza al copiar bytes ya validados. El último intervalo tiene tamaño exacto.
401/403, 429, rango inconsistente y desborde no activan recuperación. Una
negativa durante anticipación queda terminal incluso si llega otro seek.

La política concuerda con [RFC 9110, rangos](https://www.rfc-editor.org/rfc/rfc9110.html#section-14)
y el [descargador HTTP de yt-dlp](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/downloader/http.py).
Commons ya exigía 206, Content-Range/Length exactos e identidad del objeto;
no se sustituyó su transporte ni se cambió su pacing/cooldown.

La anticipación retiene como máximo seis bloques comprimidos: consumidor, cuatro
respuestas en cola y una operación/entrega del worker. Son hasta 393222 bytes
(384 KiB + seis bytes de guarda), además de los buffers del cliente HTTP.
El incremento frente al baseline instrumentado es 256 KiB; no añade otro worker.
Cinco bloques AAC a aproximadamente 128 kb/s cubren unos 20 s, dimensionados
contra la recuperación observada de 15 s y el presupuesto existente de 18,15 s.
No aumenta los 8192 frames PCM ni el prebuffer de 4096 frames. c187864 exige
dos intervalos comprimidos antes del primer byte de cada generación, dentro
de esa misma ventana. No exige llenarla entera. La espera se realiza antes
de abrir la salida y conserva cancelación cada 20 ms; un error conocido
impide publicar el bloque guardado. El coste del nuevo
thread/runtime, CPU y ARMv7 se midió con las limitaciones de muestreo indicadas en los resultados físicos.
Un seek fuera del bloque validado incrementa generación y cancela la operación anticipada; el resultado
anterior no se reutiliza en la nueva generación. Drop cancela y recoge el worker.

## Diagnósticos y contabilidad de audio

El snapshot QA activa explícitamente una traza circular de 256 operaciones en
memoria. `online.latency.http_reads` comparte el RequestId del objeto contenedor.
Campos: secuencia, offset, longitud solicitada, inicio/fin monotónicos, status,
bytes recibidos, Content-Length numérico, validez de longitud/rango, primer dato,
resultado fijo, cancelación, timeout, final del recurso e incompleto. No registra
URLs, tokens, cookies, cuerpos ni encabezados privados. No hay logging detallado
HTTP permanente. Al reemplazar un request, QA debe conservar su snapshot: no
existe un archivo automático de trazas retiradas.

PCM conserva `buffer_underruns` como callbacks con Pending, y añade episodios
contiguos, frames exactos de silencio por Pending, máximo episodio, buffer
observado al entrar al callback, frames producidos, máximo intervalo de producción
por lotes de 256 frames, esperas por buffer lleno y máximo intervalo de callbacks.
Silencio por pausa, fin o generación retirada no se cuenta como starvation.
La duración digital es `1000 * starvation_silence_frames / output_rate` ms.
Las pausas reinician el reloj de intervalos de callback. Xruns siguen separados.
La métrica de intervalo del productor incluye backpressure y solo cierra un
intervalo al producir de nuevo: una espera terminal puede faltar en su máximo.
Pending/episodios y HTTP permiten observar esa espera. No demuestra CPU
lenta sin correlacionar sus esperas con HTTP y callbacks. No hay I/O, locks ni
asignaciones pesadas nuevos en el callback.

## Probes reales de HTTP, sin decodificación

Los scripts externos invocan el bridge productivo con sesión invitada, mantienen
la concesión solo en memoria y exportan únicamente campos permitidos. No son
artefactos de reproducción ni pruebas de PCM. No hubo fallback tras una negativa.

| Protocolo | qQzdAsjWGPg | _d3SaQI_Ag4 |
| --- | --- | --- |
| Query range histórico, probe nuevo | Primer 200: 65536 bytes, 10,520 s; timeout en 65536 | Primer 200: 65536 bytes, 12,664 s; timeout en 65536 |
| Range estándar, bloques de 32 bytes | Extracción agotó 120 s; sin GET | 206 exactos en 0, 655360, 851968, 1048576; 471/726/14/13 ms |
| Range estándar, bloques de 64 KiB | 32 bloques 206 exactos hasta 2097152 bytes; timeout en ese offset | 4 bloques 206 exactos hasta 262144 bytes; timeout en ese offset |

Son concesiones/momentos distintos: no es una comparación pareada de rendimiento,
ni demuestra caducidad, ancho de banda global o causalidad de los cortes antiguos.
No se han atribuido los 403 históricos a una causa nueva ni se han sorteado.

## Regresiones ejecutadas

| Check | Resultado |
| --- | --- |
| `check-local.py` | PASS; inventario de features actualizado, sin cambiar Cargo.lock |
| `cargo test --locked -p doremi-youtube --lib` | 59 PASS, 2 ignorados históricos (requieren insumos externos explícitos) |
| Player AAC/MP4 + Ogg + salida nativa | 28 PASS, sin cambios posteriores en player |
| Clippy player/all-targets con AAC/MP4/Ogg/native-output | PASS con `-D warnings` |
| Harness de proveedor offline | 40 casos HTTP sintéticos PASS |
| Clippy Youtube/all-targets | PASS local y workspace CI del SHA final con `-D warnings` |

Fixtures cubren 200 vacío/truncado/tamaño exacto, 206 íntegro/inconsistente,
rango ignorado, conexión cortada, retry sin duplicación, último bloque/EOF,
timeout con tres intentos, cancelación bloqueada en cabeceras/cuerpo y respuesta
tardía, seek con anticipación activa, 403 terminal y anticipo sin demanda.
La retirada bloqueada debe completar en <250 ms en la fixture; el seek durante
anticipación en <500 ms. Son cotas de fixture, no p95 de población o Android.

Seek offline: MP4 convencional y fragmentado, 10/35/50/75/98 %, vuelta a 35/10/0,
256 frames posteriores comparados con PCM de referencia. Apertura progresiva,
seek a fragmentos no decodificados y retorno a fragmentos anteriores se conservan.
Un probe offline adicional usa la biblioteca player del candidato y una señal
CC0 continua de 120 s codificada como AAC fragmentado: 5761024 frames, seek
inicial a 99 s, 10/35/50/75/98 %, vuelta a 35 %/0; 256 frames por destino
coinciden con la referencia. No usa HTTP ni salida nativa. La primera
fixture larga por copia de paquetes introducía catorce duraciones de 2048
para AAC de 1024 frames: no sirve como referencia de PCM por índice lineal.
Se conserva su diagnóstico como fallo de generación y evidencia del contrato
de timeline no uniforme. Una regresión altera la duración del segundo
fragmento y exige UnsupportedFormat, sin EOF ficticio. Ese es el límite
explícito del lector: no implementa edición/relleno de gaps AAC.

Las fixtures del feed verifican seek inicial >3 s sin falso timeout de prebuffer,
y cancelación antes del join al agotar el prebuffer.

Experimento integrado adicional sobre la producción c187864: servidor loopback
206 → ReadAhead con prebuffer → Symphonia/AAC, con las dos fixtures CC0
existentes, convencional y fragmentada. Las dos entregaron 385024 frames
idénticos a la referencia y coincidieron los 256 frames de cada seek a
10/35/50/75/98/35/10/0 %. El arnés QA temporal no cambia la producción ni el
payload CI: `http-aac-probe-append.rs`, log y SHA256 de fuente/arnés están
fuera del árbol; se restauró exactamente el archivo de c187864 después del
experimento. Es un PASS integrado sintético, no reproducción física ni un
nuevo test permanente contado entre los 59.


## Baseline instrumentado que motivó la ventana y la reanudación

Estas seis ejecuciones físicas son de `fd36e5b`, anteriores a `0b581ce` y
`4b903bb`. Se dejó terminar cada intento, sin limitarlo a 15/80 s. Un checkpoint
con una sola pista, repetición desactivada y ninguna entrada siguiente permite
reconocer el EOF natural por el paso automático a Stopped y frames/posición
al final; Failed nunca cuenta como EOF. Hubo salida al dispositivo HDA Intel /
Realtek ALC257, sin captura acústica. La UI se ejecutó en Xvfb propio, software GL,
perfil nuevo y DBus aislado del llavero personal. Volumen 0,08.

| Pista / repetición | Resultado | Posición final ms | Callbacks Pending | Episodios | Silencio insertado s | Xruns |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| q / 1 | EOF real | 276000 | 1501 | 5 | 31,943 | 0 |
| _d3 / 1 | EOF real | 284000 | 0 | 0 | 0 | 0 |
| q / 2 | FAIL lectura | 202292 | 1630 | 6 | 34,678 | 0 |
| _d3 / 2 | FAIL lectura | 36292 | 561 | 2 | 11,942 | 0 |
| q / 3 | FAIL lectura | 48460 | 756 | 2 | 16,095 | 0 |
| _d3 / 3 | EOF real | 284000 | 541 | 4 | 11,494 | 0 |

En q/1, el intervalo 1179648 necesitó 15,173 s: dos timeouts de 6 s
con 32768/49152 bytes recibidos y un tercer intento completo. Coincide con
un período sin producción PCM de 11,203 s y un episodio de 11,032 s de
silencio; el máximo intervalo de callbacks fue 25,214 ms, no once segundos.
Eso demuestra agotamiento de los datos anticipados mientras I/O se recupera.
El buffer PCM no es la causa única; un bloque comprimido cubría apenas unos
4 s. Por eso se dimensionó una ventana comprimida acotada de cinco bloques.

En q/2, los tres intentos del bloque 3276800 recibieron 32768, 16384 y
49152 bytes antes de agotar sus plazos. Todos identificaban correctamente
el mismo intervalo por 206/Content-Range/Length. Se transfirieron 96 KiB,
pero sus prefijos se descartaban y se volvían a pedir: no hubo progreso
conservado. Reanudar la cola evita esa retransmisión; la fixture reproduce
cortes/timeout con los mismos tamaños, exige offsets nuevos y compara cada
byte. No demuestra por sí sola que la red real vaya a recuperar su velocidad.

Baseline seek: q pasó pausa/resume y 10 % (28120 ms); a 35 % recibió 403
en el primer GET del nuevo grant, offset 0, sin PCM. _d3 pasó pausa/resume,
10/35/50 % (28974/99656/142514 ms); 75 % recibió el mismo rechazo inicial.
Los destinos restantes quedaron bloqueados por esas negativas. El seek a
unos 99 s sí entregó PCM en una de las dos pistas; no se atribuye el 403
al índice MP4 ni se declara la matriz completa aprobada. Stop durante
resolución produjo 40 observaciones/8 s sin PCM tardío en Linux.

`linux-full-fd36/summary.json`, `linux-controls-fd36/` y los timelines
conservan la relación entre RequestId, bytes, posición y métricas. El muestreo
parcial de recursos cubrió las últimas cuatro ejecuciones: PSS Playing
111539–125526 KiB y CPU mediana 85,45 % de un núcleo, incluyendo software GL.
No es CPU exclusiva de decodificación ni comparación pareada contra el candidato.

## Evidencia intermedia `4b903bb`: progreso y límites nuevos

Los dos workflows de este SHA terminaron PASS. `ci.yml` necesitó un segundo
intento exclusivamente para macOS, tras la anotación explícita de falta de
capacidad; el gate multiplataforma pasó. No se cambiaron los workflows ni sus
pruebas. Artefactos físicos verificados:

| Plataforma | Run / artifact | SHA256 del binario/APK |
| --- | --- | --- |
| Linux | 44604501827 / 14571640945 | e0874f42edca3bff5e5dec9a80c3cab0848e08a7e197b3588e26ba1e141f09aa |
| Android | 44604507106 / 14571285051 | bb89fcd2d461ba1f210daa4f4e162b0f6cac3fcfbcf0130aa16caa25d2579b8e |

Android, seis procesos fríos: seis FAIL/account Timeout, sin GET de medios,
PCM o EOF. PSS 58931–66637 KiB y CPU mediana 36,24 % de un núcleo durante
carga. La frecuencia de ticks se obtuvo de AT_CLKTCK del propio proceso shell
(100 Hz), no se supuso. La geometría fresca del contenido de la ventana
confirmó origen [0,45], 384×658,65625 puntos y escala 1,875. El primer arnés
confundió el frame completo de la ventana con su contenido y abortó antes de
inyectar entrada; se conservó el diagnóstico y se corrigió usando content.

Después, un único proceso con la misma sesión/cliente y las mismas dos pistas
produjo siete arranques reales y un fallo previo de cuenta. No se añadió caché
ni se cambió de cliente tras una negativa. Las selecciones independientes
Next conservaron los fallos; ninguna petición 403 se reintentó ni se empalmó
con otra concesión.

| Pista / repetición | Resultado | Posición final ms | Pending | Episodios | Silencio s | HTTP terminal |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| q / 1 | FAIL cuenta | — | no medido | no medido | no medido | sin GET |
| _d3 / 1 | FAIL | 45909 | 481 | 1 | 9,871 | 403 en 1048576 |
| q / 2 | FAIL | 46055 | 0 | 0 | 0 | 403 en 1048576 |
| _d3 / 2 | FAIL | 46118 | 1807 | 6 | 37,082 | 403 en 1048576 |
| q / 3 | FAIL | 46216 | 0 | 0 | 0 | 403 en 1048576 |
| _d3 / 3 | FAIL | 45657 | 178 | 1 | 3,642 | 403 en 1048576 |
| q / 4 | FAIL | 46296 | 0 | 0 | 0 | 403 en 1048576 |
| _d3 / 4 | FAIL | 45775 | 0 | 0 | 0 | 403 en 1048576 |

El servidor permitió los intervalos anteriores y denegó repetidamente el de
1 MiB, antes del EOF conocido y con la comprobación local de concesión aún
vigente. Eso demuestra una negativa del servidor en esa operación, no su
motivo. No se atribuye a atestación, región o caducidad sin evidencia adicional.
El contrato es detener y mostrar el rechazo; no renovar en bucle ni cambiar
cliente/cuenta para acceder al resto. Este límite impide certificar Android.

También se demostró recuperación real: en _d3/1, GET 0 recibió 32768 bytes y
agotó 6 s; GET 32768 pidió solo 32768 y los validó. Más tarde, GET 65536
recibió 49152 bytes y la cola 114688 pidió/validó solo 16384. El prefijo
permaneció privado hasta completar cada bloque. La pista entregó PCM después
de ambas recuperaciones; eso no convierte el posterior 403 en éxito.

La misma traza descubrió el defecto de anticipación: el demuxer buscó dentro
del primer bloque validado y canceló GET 65536 a los 20,366 s. El siguiente
GET de ese intervalo comenzó a los 24,216 s, al agotar el bloque, introduciendo
3,850 s de espera evitable. `3d1f619` conserva el anticipo cuando el seek
permanece dentro del bloque, incluido su final; el siguiente intervalo sigue
siendo correcto. La nueva fixture falla en el código anterior por cierre del
socket y pasa después. Se mantienen invalidación/cancelación para destinos
fuera del bloque y la negativa terminal incluso con datos anteriores en caché.

Los rechazos tardíos perdían además su tipo al cruzar el feed PCM y mostraban
un error genérico de lectura. `b39753d` conserva AccessDenied con un atomic
privado del worker; el callback no añade I/O, locks o asignaciones. Las
regresiones cubren fallo antes/después del prebuffer y conservan Input para
errores genéricos. La UI ya tenía el mensaje de acceso denegado; no se rediseñó.

Controles Android intermedios: pausa/resume y seek 10 % pasaron para ambas
pistas; 35 % falló por 403. Los demás destinos quedaron bloqueados. Stop tras
Next/Previous activos dejó 65 observaciones/20 s sin PCM tardío; otra parada
de esa revisión dejó 62/20 s. PSS con PCM 67684–77602 KiB; CPU mediana 33,76 % de un
núcleo en tres ventanas con el mismo RequestId en ambos extremos. Incluye UI
y red; no es consumo exclusivo del decodificador ni de una pista completa.

## Evidencia intermedia b39753d: artefactos y condiciones

Código `b39753db45df92d2c57ab311fb2e85efc8a17934`. APK de
[android.yml 44639581494](https://github.com/Eirom16/spotifast/actions/runs/44639581494),
artifact `14585828747`, SHA256
`b07f3f458eb8113d71e239c161f991461142241675797cbf3160b17480b7289d`.
ZIP/CRC, todos los SHA256SUMS, manifiesto `source_dirty=false`, ambos ABIs y
commit nativo verificados. Instalación `-r -t`, SHA256 de base.apk instalada
idéntico; sin borrar datos. Samsung SM-J701M, ARMv7, Android 9/API 28,
perfil autorizado sellado `premerge-youtube-20261118`, Wi-Fi, alimentación USB,
volumen QA 0,08. No se leyó ni lanzó el perfil habitual. La geometría/foco de
la ventana propia se verificó antes de inyectar controles.

Linux: artifact `14586533401` de ci.yml 44639577653, manifiesto
`source_commit=b39753d`, `source_dirty=false`, todos los SHA256SUMS y CRC
verificados. Binario `doremi` SHA256
`31133006e7f0cf5e2ad3363473517674d5e259cd77b387372f090fc88a8002f6`.
La pareja física terminó con EOF en ambas identidades, una ejecución cada
una: q 275984 ms, 265 Pending/un episodio/5,636938 s de silencio; _d3
284000 ms, cero Pending/silencio. Ambas tuvieron cero xruns. Se detuvo esa
serie tras la pareja para corregir el hueco inicial de q, documentado en
`superseded.json`; no son seis intentos ni una certificación repetida. Hubo
69/71 intervalos validados y 4/6 timeouts recuperados respectivamente.
Los runs no se solaparon con descargas de artefactos ni audio de otra plataforma.
Linux usa salida física HDA Intel PCH / Realtek ALC257, Xvfb propio y software
GL; volumen QA 0,08, perfiles nuevos sin acceso al llavero personal por DBus.
No hay captura acústica: PCM no nulo y callbacks de la salida física son la
evidencia instrumental, no una valoración subjetiva de lo oído.

## Evidencia intermedia b39753d: reproducción completa Android

`android-warm-full-b397/`: ocho intentos en un proceso, cuatro por identidad.
Tras un fallo, Next inicia la siguiente selección planificada de estas dos
identidades, con el mismo cliente/sesión. No se reintenta un 403 ni se cambia
de cliente, credencial o proveedor. Cada intento espera EOF real o error; no
se detiene a los 15/80 s. La prueba de EOF exige transición automática sin
input, posición/frames al final y último intervalo HTTP validado.

| Pista / intento | Resultado | Última posición con PCM ms | Pending | Episodios | Silencio s | Xruns |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| q / 1 | FAIL cuenta Timeout, sin GET/PCM | — | — | — | — | — |
| _d3 / 1 | FAIL 403 en 1048576 | 49944 | 661 | 1 | 13,615 | 0 |
| q / 2 | FAIL 403 en 1048576 | 45760 | 0 | 0 | 0 | 0 |
| _d3 / 2 | FAIL 403 en 1048576 | 45680 | 0 | 0 | 0 | 1 |
| q / 3 | FAIL 403 en 1048576 | 46095 | 0 | 0 | 0 | 1 |
| _d3 / 3 | FAIL 403 en 1048576 | 46240 | 0 | 0 | 0 | 1 |
| q / 4 | FAIL 403 en 1048576 | 45695 | 0 | 0 | 0 | 1 |
| _d3 / 4 | FAIL 403 en 1048576 | 62294 | 992 | 3 | 20,586 | 1 |

Resultado: **0/4 EOF por identidad, siete arranques con PCM**. Las siete
negativas ocurrieron en el mismo offset, con cero bytes recibidos, tras 16
intervalos 206 completos por intento. No se observó vencimiento del reloj de
la concesión; no se ha demostrado el motivo remoto del rechazo. Las siete
UI terminales conservaron el error de acceso: «El origen denegó el acceso al
audio de esta pista». En `4b903bb` se perdía como error genérico de lectura.
El logcat filtrado en memoria exportó solo `stage=account` una vez, sin
atribuir a peticiones individuales otras marcas del mismo proceso.

La primera _d3 recuperó el bloque 262144: recibió 16384 bytes antes del
timeout; pidió 49152 desde 278528, recibió 32768; pidió los 16384 restantes
desde 311296 y los validó. Ventana monotónica 39,000839–57,125950 s: 18,125 s
para 64 KiB, velocidad útil muy inferior al consumo AAC seleccionado.
Ningún prefijo se publicó como bloque completo y no hubo GET cancelado por
los seeks internos de apertura. La ventana anticipada todavía no estaba
llena: hubo starvation. No se certifica ausencia de interrupciones evitables
ni se aumenta arbitrariamente el prebuffer para ocultar esta red lenta.

PSS con PCM: 52129–67130 KiB (50,9–65,6 MiB), diez muestras. CPU mediana
32,42 % de un núcleo en tres pares del mismo PID/RequestId con PCM, con
AT_CLKTCK=100 medido en el dispositivo. Incluye UI, I/O y decodificación;
no es comparación pareada de rendimiento ni CPU exclusiva del códec.

Control independiente `android-local-full-b397/`: señal CC0 local continua de
120 s, mismo APK intermedio y salida física ARMv7, perfil sintético propio sin
sesión. EOF natural observado, último snapshot PCM 119600 ms/5743140 frames,
cero Pending, episodios, silencio y xruns observados. No hay HTTP ni YouTube
en este control y es una sola ejecución: no certifica las dos canciones.

## Evidencia intermedia b39753d: seek, pausa y cancelación

Android, mismo proceso de la serie, controles registrados aparte de EOF:

| Pista | Pausa/resume | 10 % | 35 % | 50/75/98 %, vuelta a 35 % |
| --- | --- | --- | --- | --- |
| q | PASS: reloj y frames pausados, nuevos frames al reanudar | PASS 27463 ms | FAIL 96631 ms, 403 | BLOCKED tras la negativa |
| _d3 | PASS | PASS 28259 ms | FAIL 99432 ms, 403 | BLOCKED tras la negativa |

El seek crea otro RequestId y exige PCM de ese request y posición ±3,5 s;
una posición mostrada sin PCM nuevo no cuenta como éxito. El 35 % de _d3
reproduce el fallo próximo a 99 s. No se atribuye la negativa al índice MP4
ni se considera aprobada la matriz por las fixtures offline. Stop dejó
65 snapshots durante 20 s en Paused, sin loading ni métricas/PCM tardíos;
el proceso QA se cerró al acabar. Otra serie de b39753d inició resolución y pulsó
cinco Next, Previous y Stop: 65 observaciones/20 s sin PCM tardío, estado
Paused y loading=false. Stop observado en 2129 ms, incluyendo ADB/controles.
`android-cancel-active-b397/` conserva los snapshots. Esto no es una medida del tiempo del future
HTTP ni prueba identidad del audio mediante captura acústica.

Las restantes situaciones de cancelación física no están certificadas. Las
fixtures prueban cancelación de cabeceras/cuerpo bloqueantes y resultados
tardíos, seek con GET activo y sender lleno. Los pasos físicos históricos se
conservan por separado; no sustituyen las pruebas del APK/binario final.

## Alcance de cancelación y recuperación

| Situación | Evidencia disponible | Límite físico final |
| --- | --- | --- |
| Cambio durante extracción / Next rápido / Stop esperando proveedor | Samsung c187864: Play con loading activo, cinco Next, Previous, Stop; 73 observaciones/20 s sin PCM tardío | No es una medición del future HTTP |
| Cambio/seek mientras llega HTTP, respuesta tardía, sender lleno | Fixtures con conexión bloqueada, generación y cancelación; retirada <250/500 ms en loopback | No se certifica esa cota en red real/ARMv7 |
| Pausa/resume durante reproducción | Linux c187864: q PASS; Android c187864 BLOCKED. Ambas identidades Samsung b39753d PASS intermedio | Pausa durante buffering pendiente |
| Recuperación de conexión cortada / timeout | Fixtures byte a byte y reanudación real de colas en Samsung c187864 | Pérdida/restauración deliberada de Wi-Fi no ejecutada |
| Error de pista seguido de otra selección | Tres arranques c187864 Android después de fallos, mismo proceso/perfil | Ninguno alcanza EOF; no es PASS de continuidad |
| Previous con pista sonando / seek seguido de Stop inmediato | Reglas de cola/generación conservadas por tests; secuencia física rápida incluye Previous | No hay prueba física aislada de esos dos escenarios |
| Cambiar/cerrar sesión mientras hay trabajo | No se cambiaron garantías de sesión/RequestId | No ejecutado físicamente; la sesión QA se conserva sellada |

Las posiciones y RequestId comprobados no sustituyen una captura de identidad
acústica. No se ha certificado la matriz F completa. Las cotas sintéticas y
los controles parciales nunca se convierten en PASS de reproducción completa.

## Candidato final c187864: artefactos y CI

Los paquetes usados se obtuvieron de los workflows del SHA exacto, con
`source_dirty=false` en sus manifiestos. No hay build completo local.

| Plataforma | Run / artifact | SHA256 del payload utilizado |
| --- | --- | --- |
| Linux | 44647361722 / 14591755263 | 911af2222af3f2c0cf3a2afc58d37436210b387a1e303d44bffb280b45a1cf56 |
| Android | 44647370690 / 14590600199 | 282ed75d9845bafd7167ca801fd491f0bcfcd7f7a8076599adc9b7c59b1e8a07 |

APK: CRC del ZIP completo, SHA256SUMS, manifiestos y bibliotecas ARMv7/ARM64
comprobados. El SHA256 de base.apk instalado coincide. Linux: la descarga
completa agotó sus reintentos al 41,9 %. Se recuperaron por rangos las entradas
usadas, verificando CRC individual y todos sus SHA256 contra el manifiesto,
incluidos ejecutable, auth helper, Cargo.lock y licencias. Solo se omitió el
alias duplicado no ejecutado `doremi-app`; **no se certifica CRC del ZIP completo**.
`selection-verification.json` conserva esa distinción. Ninguna descarga de
artefactos estuvo activa durante las series de audio.

[ci.yml 44647361722](https://github.com/Eirom16/spotifast/actions/runs/44647361722)
y [android.yml 44647370690](https://github.com/Eirom16/spotifast/actions/runs/44647370690)
terminaron PASS en el primer intento sobre `c18786424d0a673a69ec530312b732d41d46e687`.
Incluyen quick, Rust default/all-features/doctests, Clippy/Rustdoc, android-core,
macOS, Windows, native-host, windows-host, APK de ambas ABI, Preview Linux,
QA gráfica automatizada y gates de artefactos. Los JSON de jobs y la salida de `scripts/summarize-ci.py` se conservan.
No se cambió ningún workflow, gate, lint ni test antiguo para aprobar.

El macOS de 4b903bb necesitó repetir un job sin capacidad disponible: fue
infraestructura, no un defecto Rust ni un fallo apt. Los runs cancelados de
0b581ce/3d1f619 no son PASS; los errores locales y de aprovisionamiento anteriores
permanecen en la evidencia. Windows: **PHYSICAL_PLAYBACK_NOT_VERIFIED**.

## Candidato final c187864: Android físico

Samsung SM-J701M, ARMv7/API 28, mismo perfil QA sellado y cliente durante la
serie, red Wi-Fi disponible sin descargas de artefactos concurrentes. No se
inspeccionaron secretos ni se modificó el perfil habitual. Geometría de contenido
y checksum instalados verificados. Una selección Next planificada después de
un fallo conserva el resultado anterior; no reintenta su rechazo ni cambia
cliente, credenciales o proveedor.

Ocho intentos hasta EOF/error: cuatro por identidad. **0/4 EOF por pista**.
Tres entregaron PCM; cuatro fallaron durante extracción y otro agotó la lectura
antes del primer bloque completo. No se observaron 403 en esta serie: falló
antes de llegar al offset de 1 MiB denegado en b39753d.

| Pista / intento | Resultado terminal | Última posición PCM ms | Pending | Episodios | Silencio s | Xruns |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| qQzdAsjWGPg / 1 | Timeout extracción | — | — | — | — | — |
| _d3SaQI_Ag4 / 1 | Timeout HTTP tras PCM | 7964 | 480 | 1 | 9.832562 | 0 |
| qQzdAsjWGPg / 2 | Timeout HTTP tras PCM | 24125 | 1211 | 3 | 25.094646 | 1 |
| _d3SaQI_Ag4 / 2 | Timeout HTTP antes de PCM | — | — | — | — | — |
| qQzdAsjWGPg / 3 | Timeout HTTP tras PCM | 16044 | 901 | 3 | 18.335187 | 1 |
| _d3SaQI_Ag4 / 3 | Timeout extracción | — | — | — | — | — |
| qQzdAsjWGPg / 4 | Timeout extracción | — | — | — | — | — |
| _d3SaQI_Ag4 / 4 | Timeout extracción | — | — | — | — | — |

Los tres arranques PCM esperaron al segundo intervalo validado. Ejemplos:
_d3/1 validó ese intervalo a 58,105781 s y produjo PCM 13,828 ms después;
q/2 a 23,394356 s, PCM 19,799 ms después. La precarga desplaza la espera
inicial a buffering; los bloques posteriores siguen agotándose. En _d3/1,
el bloque 131072 recibió 16384 bytes, la cola 147456 agotó un intento sin
bytes y el último solo recibió 32768/49152. Se conservaron prefijos privados,
pero no se publicó el bloque incompleto ni se marcó EOF. Después de tres
operaciones se notificó el fallo visible. El conjunto insertó 53,262396 s de
silencio por falta de PCM en siete episodios; no se etiqueta como reproducción
continua ni como un defecto exclusivo de CPU o tamaño del buffer PCM.

Controles posteriores: ambas selecciones fallaron antes de PCM; pausa/resume
y la matriz física 10/35/50/75/98 % y vuelta quedan **BLOCKED**, no PASS offline.
Stop produjo 65 observaciones/20 s sin PCM tardío. La serie local CC0 de b39753d
es un control intermedio del motor, no certificación física de este APK.

Recursos con PCM: cuatro muestras PSS 65417–84732 KiB (63,9–82,7 MiB).
Un solo par de muestras del mismo PID/RequestId dio 31,014 % de un núcleo,
AT_CLKTCK=100 medido; incluye UI/I/O/decoder. No hay percentil poblacional ni
comparación causal de RAM. Logcat se filtró en memoria por PID y campos fijos:
account/player tres marcas cada uno, sin atribución por RequestId ni prueba
de cobertura completa. No se exportó el log bruto.

Cancelación adicional del APK c187864, 2026-11-20: Play en loading, cinco
Next, Previous y Stop durante `phase=resolve`. Resultado PASS de ese control:
73 observaciones/20 s en Paused, loading=false, sin métricas/PCM tardíos.
Stop observado en 2066 ms, incluyendo ADB y controles; no es latencia del
future HTTP. `android-cancel-active-c187/` conserva arnés y snapshots.
El cierre usa force-stop únicamente sobre la app QA después del control.

## Candidato final c187864: Linux físico

Tres intentos por identidad, binario exacto verificado, salida ALSA/PulseAudio
HDA Intel / Realtek ALC257 activa y sin mute. Sesión invitada y perfil nuevo
por intento, DBus aislado del keyring personal, Xvfb propio con software GL,
volumen 0,08, sin captura acústica. Wi-Fi activo; cuatro CPU, 7941556 KiB RAM.
Sin descargas de artefactos ni builds durante las mediciones. La calidad de la
ruta externa no fue controlable; no se presupone una red estable solo por
estar conectado. Cada intento esperó EOF/error, con deadline de duración+155 s.
Ninguno alcanzó ese deadline: todos terminaron con error real.

**0/3 EOF por canción**. Tres arranques PCM, dos timeouts previos de resolución
y una negativa de acceso durante extracción, sin GET de medios. Esa negativa
no equivale a un status 403 observado del lector. No hubo 200 aceptado,
HTTP 403 de medios ni EOF real en esta serie final.

| Pista / intento | Resultado terminal | Última posición PCM ms | Pending | Episodios | Silencio s | Xruns |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| qQzdAsjWGPg / 1 | Timeout resolución | — | — | — | — | — |
| _d3SaQI_Ag4 / 1 | Timeout resolución | — | — | — | — | — |
| qQzdAsjWGPg / 2 | Timeout HTTP tras PCM | 7964 | 468 | 1 | 9.976896 | 0 |
| _d3SaQI_Ag4 / 2 | Acceso denegado en extracción | — | — | — | — | — |
| qQzdAsjWGPg / 3 | Timeout HTTP tras PCM | 36292 | 283 | 1 | 6.032563 | 0 |
| _d3SaQI_Ag4 / 3 | Timeout HTTP tras PCM | 7964 | 365 | 1 | 7.779562 | 0 |

Los tres arranques esperaron al segundo intervalo validado; no basta para una
red incapaz de sostener el flujo posterior. q/2 agotó tres operaciones en
131072, con 0/0/16384 bytes: el bloque no se publicó. q/3 recuperó el bloque
524288 pidiendo su cola 573440; después, 589824 agotó tres operaciones sin
bytes y acabó a 36,292 s con 6,033 s de starvation. _d3/3 guardó 16384 del
bloque 131072, pero los dos intentos de cola 147456 no progresaron. Ningún
prefijo se convirtió en bloque completo ni EOF. Los callbacks máximos de
esos tres arranques fueron 23,948/23,398/23,797 ms: los episodios de silencio
no se explican por un callback detenido seis o diez segundos.

Recursos: 40 muestras Playing, PSS 98601–100596 KiB (96,3–98,2 MiB), CPU
mediana 84,09 % de un núcleo. Incluye software GL/UI/decoder/I/O; no es coste
exclusivo de audio ni comparación pareada con Android o el baseline.
`linux-full-c187/` conserva timelines, arnés, report, invariantes HTTP,
latencias y muestras de recursos; no se ha reducido ninguna ejecución a 15 s.

## HTTP antes/después y latencia observada

La tabla cuenta operaciones observadas, no probabilidad de fallo del proveedor.
El baseline fd36e5b ya incorpora la primera instrumentación estricta. Las
series difieren en fecha, concesiones y condiciones externas: no son un A/B
pareado. Menos silencio total con canciones que terminan antes **no demuestra
mejor continuidad**.

| Serie / SHA | Intentos / PCM / EOF | Intervalos o colas validados | Timeouts | Cancelados HTTP | Negativas HTTP | Silencio total s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| linux-full-fd36 | 6 / 6 / 3 | 282 | 22 | 5 | 0 | 106.152125 |
| linux-full-b397 | 2 / 2 / 2 | 140 | 10 | 0 | 0 | 5.636938 |
| android-warm-full-b397 | 8 / 7 / 0 | 112 | 9 | 0 | 7 | 34.200938 |
| linux-full-c187 | 6 / 3 / 0 | 13 | 13 | 0 | 0 | 23.789021 |
| android-warm-full-c187 | 8 / 3 / 0 | 12 | 25 | 0 | 0 | 53.262396 |

Todos los intervalos/colas aceptados del candidato final observados tuvieron
206, rango exacto y cuerpo completo; los fallos de lectura no se marcaron
éxito. Las fixtures comparan además cada byte y el PCM resultante. El 200
vacío/truncado histórico se rechaza en fixtures; no se observó de nuevo en
la serie final. Los siete 403 de b39753d permanecen visibles y no se reintentaron.
La mejora de categoría AccessDenied se demostró en ese SHA intermedio y en
fixtures finales, sin fingir un 403 físico de c187864 que no ocurrió.

Marcas del candidato final, desde la aceptación del mismo RequestId (segundos).
Solo las seis ejecuciones con PCM tienen latencia al primer callback no
silencioso; los ocho fallos anteriores a PCM conservan ausencia de medición.

| Plataforma / pista / intento | Resuelto | Primer GET | Primer dato | Primer intervalo íntegro | Segundo intervalo | Primer PCM | Callback no silencioso |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Linux / qQzdAsjWGPg / 2 | 110.949975 | 110.997366 | 115.490430 | 122.684642 | 131.715004 | 131.726472 | 131.770139 |
| Linux / qQzdAsjWGPg / 3 | 90.751481 | 90.778127 | 90.823549 | 90.831193 | 90.837546 | 90.853000 | 90.886359 |
| Linux / _d3SaQI_Ag4 / 3 | 35.398990 | 35.432812 | 37.652574 | 38.282319 | 43.003003 | 43.022123 | 43.089166 |
| Android / _d3SaQI_Ag4 / 1 | 35.004216 | 35.039776 | 42.033830 | 42.476978 | 58.105781 | 58.119609 | 58.254588 |
| Android / qQzdAsjWGPg / 2 | 8.419980 | 8.429605 | 17.321138 | 17.700190 | 23.394356 | 23.414155 | 23.458761 |
| Android / qQzdAsjWGPg / 3 | 8.635866 | 8.642531 | 19.428020 | 23.689288 | 33.484914 | 33.501930 | 33.586022 |

Comparación contextual b39753d: Linux primer callback 19,167941 y 11,087500 s
(una ejecución por pista); Android 3,240293–27,442110 s en siete arranques,
ninguno con EOF. No se atribuye al nuevo prebuffer el aumento completo de
latencia final: q/2 Linux ya tardó 110,949975 s en resolver antes del GET.
El coste observado de esperar del primer al segundo intervalo final fue
0,006353–15,628803 s. Del segundo intervalo a PCM: 11,468–19,799 ms; PCM a
callback no silencioso: 33,359–134,979 ms. Son muestras condicionadas al
arranque, sin p50 poblacional. No se cumple el objetivo de tres segundos.

## Candidato final c187864: controles Linux y aceptación

Control aparte de la serie de EOF, con el mismo binario SHA256 verificado y
otro perfil QA aislado:

| Pista | Pausa / resume | Seek 10 % | 35/50/75/98 %, vuelta |
| --- | --- | --- | --- |
| qQzdAsjWGPg | PASS: reloj/frames quietos, PCM nuevo al resume | FAIL: nuevo RequestId, tres timeouts en offset 0; 0/0/32768 bytes, sin PCM nuevo | BLOCKED tras el fallo |
| _d3SaQI_Ag4 | BLOCKED: negativa en extracción antes de GET/PCM | BLOCKED | BLOCKED |

La posición visible 27971 ms del seek fallido de q no cuenta como éxito:
`audio_metrics=null`, ningún intervalo íntegro y estado Failed. No se ha
demostrado un fallo final de índices Symphonia; la lectura inicial del nuevo
request no completó. Las fixtures de 10/35/50/75/98 % y retorno sí pasan,
con PCM idéntico a referencia, pero la matriz física sigue sin aprobarse.

Previous seguido de Stop durante resolución pasó: 40 observaciones/8 s en
Paused, sin loading ni PCM tardío. Antes había un hijo Python de este proceso
QA; después no quedó ninguno. El proceso y su Xvfb cerraron normalmente,
sin kill forzado. No quedaron procesos Doremi/Xvfb de estas series al cierre.
No es una certificación de toda la matriz F ni una comparación acústica de
identidad. `linux-controls-c187/` conserva snapshots/RequestId y arnés.

| Criterio | Estado al cierre |
| --- | --- |
| Identidad HTTP y bloques íntegros | Fixtures PASS y metadatos observados correctos en 25 intervalos/colas finales aceptados; no se certifica todo proveedor futuro |
| Retry/reanudación acotados, negativa visible | PASS de fixtures y recuperaciones observadas; persistieron errores terminales reales |
| Continuidad completa repetida Linux | FAIL, 0/3 EOF en cada pista |
| Continuidad completa repetida Android | FAIL, 0/4 EOF en cada pista |
| Seek físico completo | FAIL en Linux 10 % por transporte; resto y Android BLOCKED |
| Subalimentación evitable eliminada | NO DEMOSTRADO: 3/3 arranques de cada serie final tuvieron starvation |
| Cancelación y cambios de pista | PASS de controles limitados y fixtures; escenarios físicos faltantes enumerados |
| Rust/Clippy/MP4/AAC/HTTP/cancelación/Preview/gates | PASS automatizado sobre c187864; no equivale a reproducción física |
| Audio real Windows | PHYSICAL_PLAYBACK_NOT_VERIFIED |

## Límites de atribución y riesgos pendientes

- Los 200 vacíos históricos no se reprodujeron con la misma concesión. La
  identificación explícita de intervalos y el rechazo determinista corrigen
  el contrato; no demuestran el motivo remoto de aquellos cuerpos vacíos.
- Los 403 de Android intermedio son negativas explícitas. Permanecen terminales;
  no se identificó un mecanismo documentado de renovación aplicable y no se
  implementaron renovaciones, fallback de cliente o cambio de identidad.
- La red observada agota repetidamente seis segundos aun transfiriendo solo
  16–48 KiB. Hay evidencia de esperas HTTP coincidentes con starvation, pero
  no de su causa externa exacta. No se certifica ancho de banda controlado
  de extremo a extremo ni se atribuye todo underrun a la red exclusivamente.
- La ventana comprimida añade como máximo 256 KiB frente al primer diseño.
  La precarga de dos intervalos añade una espera medida hasta el segundo
  intervalo, dentro del presupuesto de operaciones existente. El buffer PCM,
  la cola de usuario y los plazos HTTP no se ampliaron. La muestra ARMv7 no
  permite estimar una regresión causal de CPU/PSS ni garantizar red lenta.
- Los índices MP4 progresivos pasan seek integrado/offline; la matriz física
  queda condicionada por acceso/transporte. Un timeline AAC discontinuo se
  rechaza visiblemente; no se implementan edición, relleno ni timestamp falso.
- Stop/generación y las fixtures protegen contra publicación obsoleta, pero
  no hay certificación física completa de cambio de sesión/logout activo,
  pérdida/restauración deliberada de red, Previous mientras suena ni seek
  inmediatamente cancelado en ambas plataformas. No se tocaron biblioteca,
  playlists, esquemas persistidos ni reglas de autorización.
- La cancelación de futures se observa cada 20 ms en los workers. La retirada
  del DNS del sistema/runtime y sus cotas en red real siguen sin certificarse;
  las cotas de loopback no son promesas de UI o de audio físico.

Se pospone la optimización del extractor y el objetivo p50 ≤3 s. No se añadió
extractor persistente ni caché de cuenta. Las marcas gruesas de resolución,
GET, dato, intervalo validado, PCM y callback no separan de manera fiable
Account/Bootstrap/Player/QuickJS/Python/yt-dlp/Deno en toda la muestra. Las
mediciones históricas siguen como comparación contextual, no línea base
universal. Una fusión futura debe revisar el diff de los once commits y repetir
reproducción/seek física, especialmente ARMv7 y red recuperada, conservando los
rechazos del proveedor. Verde en CI cubre regresiones automatizadas, no esas
condiciones de aceptación.

## Estado de Git y veredicto

Inicio `b5cda4a340372bde3d95a09003be09c91867cc14`; código final
`c18786424d0a673a69ec530312b732d41d46e687`, once commits pequeños y enfocados.
El cierre documental posterior solo contiene este informe y la actualización
de premerge; las mediciones y los dos CI anteriores corresponden exactamente
al SHA de código, no a un supuesto build del cierre Markdown. No cambian
Cargo.lock, workflows, reglas de autorización, licencias ni la rama main.
La consulta remota del 20 de noviembre antes de publicar los documentos confirmó
main en `cd40da022edd35d6017b3093421b8b83071722a6` e integración en c187864.
El estado remoto de cierre y el SHA documental se conservan en la evidencia
externa; no se fusionó, publicó release ni abrió un PR sustituyendo validación.
Los cambios ajenos del checkout de producto permanecen fuera de este worktree.

**RELIABILITY_FAIL**

La respuesta a la pregunta de aceptación es **no**. Este candidato no ha
reproducido completas las dos canciones repetidamente en Linux ni Android,
y no supera el seek físico requerido. Hay correcciones verificadas de
integridad, cancelación, anticipación, reanudación de colas y propagación de
rechazos; persisten timeouts de extracción/lectura y starvation real. No se
ha probado la causa externa exacta de esas denegaciones o transferencias
lentas ni la ausencia de toda interrupción interna evitable. No recomendar
una fusión alpha. La siguiente evaluación necesita resolver o acotar esos
fallos y repetir EOF/seek físicos con artefactos del nuevo SHA, sin sortear
ningún rechazo del proveedor.
