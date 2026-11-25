# Doremi — precarga y reproducción continua

Veredicto: **PREFETCH_WORKING**. Preparación y reutilización verificadas en Linux
y Samsung físico, con transición natural completa en ambas plataformas.

Rama: `feature/doremi-playback-prefetch`, desde el HEAD remoto verificado
`83e3cd64f173a2fdefe7ee2d777b9ef965feb075`.
Código/artefactos finales: `7ddc52acad99feed9921955a79421d5625a16d65`.
Implementación: `e6fc2f4`; corrección del colector Java: `7ddc52a`.
Este informe se entrega en un commit documental posterior, sin cambiar esos binarios.

Se conserva la descarga completa de Tubefast/ureq, Symphonia y el único motor
Rust. `Queue::next_on_end()` consulta el sucesor efectivo sin consumir la cola:
manual, historial de Previous, shuffle, Repeat One y vuelta de Repeat All.
Al aceptar Ready empieza la preparación completa de ese sucesor, sin esperar
al final de la canción. Se reutilizan `online::Worker`, su única tarea de
fuente, cancelación/generaciones y `EncodedCache` de dos canciones.

Una selección/Next adopta la generación en curso y la vincula a la nueva
petición de la cola; un resultado de fuente nunca decide qué reproducir.
Los cambios de sucesor invalidan el trabajo anterior. Fallo/timeout de precarga
no publica Failed ni cambia el aviso de la canción actual; no hay reintentos
especulativos en bucle. La carga principal puede intentarlo normalmente.
Pausa/Cancel/Stop cancelan la precarga; la caché temporal existente puede
reutilizarse al reanudar. El actor Rust Android la ejecuta sin Activity ni GL;
no se añaden servicios, WakeLocks, permisos ni otro reproductor.

No cambia el presupuesto: dos canciones comprimidas, máximo 64 MiB por
recurso; Arc compartido con el decoder y buffer PCM de 65.536 bytes. No se
precarga PCM completo ni se escriben archivos de audio. Las dos pistas usadas
ocupan juntas **9.079.428 bytes (8,66 MiB)**. No se aumentó ningún límite.

| Medición | Referencia anterior | Precarga nueva |
|---|---:|---:|
| Linux: inicio primera pista, UI | 3.153 ms | 1.944 ms |
| Linux: preparación siguiente | al EOF | 1.403 ms, durante la primera |
| Linux: EOF → PCM observado, UI | ≈1.948 ms | **≈306 ms** (actor: 279 ms) |
| Linux: Next con caché, UI | 477 ms | 477 ms |
| Linux: Next sin caché/en preparación | no medido | 1.822 ms; operación promovida |
| Android: inicio primera pista | no instrumentado exactamente | 2.940 ms |
| Android: preparación siguiente | transferencia al EOF: 3.866 ms | 2.892 ms |
| Android: EOF actor → PCM observado | no instrumentado exactamente | **438 ms** |
| Android: Next con caché, MediaSession | no instrumentado exactamente | 623 ms |

Las mediciones son observaciones del callback/actor, no captura acústica ni
latencia del DAC. El colector Linux de UI muestrea aproximadamente cada
200 ms; el temporizador nuevo empieza al consumir Ended y termina cuando se
observa PCM no nulo. En Android anterior el muestreo externo tuvo un intervalo
de 13 s alrededor del EOF: no se convierte en una cifra precisa de transición.
Los 3.866 ms proceden de los timestamps de transferencia de logcat. Son dos
IDs autorizados (`qQzdAsjWGPg`, `_d3SaQI_Ag4`), en esta conexión y dispositivo;
no se extrapola al catálogo. La variación del primer inicio y de descarga
no se atribuye a la precarga: la red no fue idéntica entre pasadas.

Samsung SM-J701M/API 28/ARMv7: **PASS**, pista completa hasta EOF natural y
siguiente con pantalla apagada y Activity retirada. Caché preparada mientras
seguía la primera, dos descargas iniciadas y un cache hit en la transición.
MediaSession/notificación, Next/Previous/Seek, recreación/reapertura con un
solo propietario, pérdidas de audio focus reales y Stop sin servicio ni
precarga pendiente: PASS. Cero underruns; un xrun en frame 0 de arranque tanto
antes como después. Hashes de biblioteca, sesión, playlists e historial
habituales idénticos antes/después. APK instalada idéntica a CI:
`15ecb3d362f678ba6d407fa2f88619252346f56664ae4b3bee8fec8b2ede0622`.

Memoria Android durante la primera pista, muestreo `dumpsys meminfo`:
PSS anterior 41,37–46,05 MiB (mediana 41,65); nueva 35,99–65,43 MiB
(mediana 36,74). Son rangos muestreados de una pasada con recreación/GC,
no prueba de una reducción de RAM ni máximos absolutos de asignación.
El presupuesto existente bastó para estos recursos.

Linux N5000/8 GB, salida HDA Intel PCH/ALSA mediante PipeWire y UI X11:
**PASS**. EOF natural, audio preparado antes del final, reutilización con dos
descargas y un cache hit; cero underruns y cero xruns. Next iniciado mientras
B seguía descargándose mantuvo dos descargas (adopción de la operación).
Cambiar a Repeat One sustituyó el próximo destino B por A: generación 2 → 4,
sin cambiar la ocurrencia que sonaba y con progreso PCM continuado. La
preparación antigua no alteró la reproducción. Las otras ediciones de cola
están cubiertas por los tests deterministas y de generaciones, no por una
pasada física individual de cada permutación.

RSS Linux durante la primera pista: antes 139,44–142,69 MiB (mediana 142,61),
después 141,90–146,39 MiB (mediana 146,32): +3,70 MiB de mediana mientras
la siguiente ya está preparada. Máximo RSS muestreado de todo el recorrido:
151,33 MiB antes / 146,39 MiB después. Incluye UI/renderer y variación del
asignador; no se presenta como ahorro. No hizo falta cambiar el presupuesto.
Ejecutable verificado contra manifiesto y todos los SHA256SUMS:
`758760261afff340dea2d1ce0f3c107fbde2902f576e42a57ecfe5ac21674da3`.

Validación local: circuito `check-local.py` aprobado; app 75 tests aprobados,
un test largo anterior ignorado; Clippy de app/core con `-D warnings` aprobado;
harness offline: 40 muestras sintéticas, incluidos errores esperados.
Los tests nuevos cubren sucesor frente al EOF real del reducer, promoción de
Next, respuestas obsoletas, cancelación, timeout, fallo silencioso y ausencia
de reintento especulativo. Los tests de transporte existentes cubren corte de
transferencia, deadline y rechazo sin reintentar una denegación.

[CI del código final](https://github.com/Eirom16/spotifast/actions/runs/45460259081):
**Android ARMv7/ARM64, Linux y Windows: PASS** (tests, Clippy, builds y
QA X11 donde corresponde). No equivale a audio físico Windows. El primer intento falló por una
excepción comprobada de JSON en el nuevo predicado Java; corregida sin
rebajar el test. `verify-android.py` completo pasa en CI; local bloqueado por
la ausencia de SDK/aapt. El recolector local verifica hashes, assets de SHA,
identidad y bibliotecas del APK antes de instalar. No se publicó una release.

Evidencia externa: `~/.local/share/doremi/product-qa/2026-11-24-prefetch/`:
`android-before-retry02/`, `android-after/`, `linux-before-retry01/`,
`linux-after/`, `linux-controls-next/`, `linux-controls-cancel-repeat01/`,
`checks/`. Los JSON incluyen tiempos, contadores, memoria y snapshots; los
artefactos y licencias están en `target/prefetch-7ddc52a/`.
Se conservan aparte la pasada Android interrumpida por desbloqueo tardío y
el primer colector Linux fallido al consultar un campo inexistente. No se
cuentan como aprobadas ni se ocultan. Dos intentos auxiliares del colector
Linux de menús no completaron la navegación y tampoco cuentan como PASS;
la invalidación física aprobada utiliza el control Repeat One.

Límites: no se promete gapless de muestras. Precarga automática del backend
comprimido YouTube; los experimentos Commons/legacy conservan su contrato.
Un read ureq cancelado termina al siguiente read o deadline existente; no
se mata un hilo de HTTP. Red lenta/cortes/denegaciones se prueban con fixtures,
no desconectando físicamente la red en esta pasada. Bluetooth desconectado,
llamada real y Android moderno no repetidos en esta misión. No se ensayaron
recursos de 64 MiB ni colecciones fuera del corpus autorizado.
