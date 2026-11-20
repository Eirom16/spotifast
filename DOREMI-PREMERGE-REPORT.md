# Doremi: validación previa a integración

> Misión P0 de fiabilidad, 2026-11-19–20: las mediciones siguientes conservan su
> SHA histórico. Las correcciones posteriores, pruebas del nuevo candidato y
> veredicto de continuidad se documentan en
> [DOREMI-STREAMING-RELIABILITY.md](DOREMI-STREAMING-RELIABILITY.md).
> Los resultados de 15 s de este informe no certifican reproducción completa.


Fecha de trabajo: 2026-11-18, America/Santo_Domingo (algunas ejecuciones
Actions ya están fechadas 2026-11-19 UTC). Rama:
`integration/doremi-premerge-validation`. Implementación en desarrollo,
sin merge a `main` ni release.

## Decisión

**No recomendar la fusión alpha.** La misión P0 probó el código
`c18786424d0a673a69ec530312b732d41d46e687` con artefactos CI verificados:
Linux, 0/3 EOF por cada canción; Samsung SM-J701M, 0/4 EOF por cada canción.
Hubo tres arranques PCM en cada plataforma, pero todos se interrumpieron por
lecturas que agotaron la recuperación. Los otros intentos fallaron antes de
PCM. Ambos workflows del SHA exacto están verdes; eso no certifica continuidad.
La matriz física de seek sigue sin aprobarse. El detalle y el veredicto P0
están en el informe de fiabilidad enlazado arriba.

Los apartados 1–10 conservan la validación histórica del 18–19 de noviembre,
con sus propios SHA y cifras. Las mejoras antiguas de arranque y sus 8/12
selecciones no son mediciones del candidato P0 final.

Este informe conserva resultados negativos y distingue artefacto probado,
script ejecutor y commit candidato. El informe histórico sigue en
`DOREMI-PLAYBACK-AUDIT.md`; su apéndice de esta misión tiene precedencia para
el estado actual, sin borrar las sesiones previas.

## 1. Referencias y preservación

Referencias verificadas con Git y `git ls-remote origin` antes de editar:

| Referencia inicial | SHA completo |
| --- | --- |
| `main`, local/remoto | `cd40da022edd35d6017b3093421b8b83071722a6` |
| Producto remoto | `cd8927f7321a5f1a51f1882c6af4c529f9a0fe9c` |
| Producto en el checkout del usuario (ancestro del producto remoto) | `0399ee9be8cf8ce15168ba5053b847d02e98a9d4` |
| Integración auditada, local/remoto | `07924d597aab5fc80b9254c8d96f773cc88b574d` |
| Fix de resolvedor, remoto | `cc8a755f40a76b950dd3d2c233f9603610ffb718` |
| Merge local ya existente | `fa669b9124bb93347947f65f95b9f167242d7f55` |

Estado publicado al cierre de la revisión del 18–19 de noviembre, antes de P0
(consultado entonces con `git ls-remote origin`):

| Referencia final | SHA completo | CI |
| --- | --- | --- |
| `main`, sin cambios | `cd40da022edd35d6017b3093421b8b83071722a6` | no ejecutada por esta misión |
| Código propuesto, `integration/doremi-premerge-validation` | `5ac877839dba269d200b5a8da5e936bc0c2faafb` | `ci.yml` `44465708503`, `android.yml` `44441447069`: success |
| Cierre documental publicado | `34af8ab3bdd9f50da0a5a50aec41b4d21f9f9de0` | `ci.yml` `44470862089`, `android.yml` `44470866179`: success (este último tras relanzar apt) |

Los commits de cierre entre `34af8ab` y `b5cda4a` solo añadían Markdown;
su diff histórico se conserva con `git diff 34af8ab..b5cda4a --stat`.
Los cambios productivos posteriores de P0 se describen en §11 y en el
informe de fiabilidad, con su propio SHA y evidencia.

Se creó un worktree independiente desde `fa669b9` mediante Orca:
`/home/eirom/orca/workspaces/spotifast/doremi-premerge-validation`.
El checkout principal tiene cambios ajenos en reglas, lockfile y archivos
sin seguimiento: se conservaron. Tampoco se alteró el worktree original de
integración. Su informe modificado se copió íntegro como punto de partida y
se respaldó fuera del árbol; SHA256 del respaldo:
`5fb918103419bfe27c50b6dacddd454eaa83b5bd46063eb3cede7e5ef7dc8d44`.

`fa669b9` ya integra `4a6a619`, `91d563f` y `cd8927f` con `07924d5`.
La comparación previa producto/audit era 3/11 commits exclusivos
(`git rev-list --left-right --count`), posterior a los 3/10 históricos.
Ambos padres son ancestros del candidato. `git merge-tree --write-tree`
reproduce exactamente su árbol `0688e10b1be45134286424d5baa27dfd9436a876`,
sin conflictos ni resoluciones manuales adicionales.
En `5ac8778`, `origin/main...HEAD` era 0/206: ningún commit de main pendiente
y 206 commits acumulados del producto/integración, no 206 cambios de esta
misión. Su amplitud requiere revisar el diff completo antes de una futura
fusión. Entre `dbc841e` y `5ac8778` solo cambia el workflow Android: las
entradas Rust de reproducción y los datos de producto son idénticos. El merge
`fa669b9`, comparado con `07924d5`, no modificaba core, player, resolvedor,
prefetch ni sesión; la optimización posterior del player está descrita en §3. La de `settings_view.rs` contra producto es vacía: conserva también
el control de cuenta y sus gates por plataforma. No se cherry-pickearon
duplicados ni se revirtieron commits.

Commits de esta misión:

| Commit | Cambio |
| --- | --- |
| `4cfa3d7` | Media e intervalos por solicitud; QA Android que no inspecciona/relanza el perfil habitual; observaciones acotadas de buffering |
| `df2ef47` | Instalación gráfica mínima, presupuesto separado y contratos Python/Android en CI principal |
| `7c080ad` | Corrección reproducida de quoting ADB: rechazar perfiles QA que ya existen |
| `b609aad` | Abortar ante diagnósticos de error de shell sin tratarlos como perfil ausente |
| `964e683` | Apertura incremental de MP4 fast-start para evitar lecturas de cola antes del PCM; mantiene seek tras abrir cabeceras |
| `6984caa` | Guard conservador temporal de MP4 fragmentado, reemplazado tras verificar índices lazy |
| `dbc841e` | Extiende apertura incremental a fragmentos; regresión CC0 de PCM y seek adelante/atrás contra el recorrido completo |
| `5ac8778` | Presupuesto del host Linux de Android basado en apt/tests medidos; conserva gates |

El SHA del cierre documental se obtiene sin ambigüedad con
`git log -1 --format='%H' -- DOREMI-PREMERGE-REPORT.md`; el SHA de ejecución
de cada plataforma está en su manifiesto/reporte, no se infiere del checkout.

## 2. Autorización, corpus y procedimiento

El usuario autorizó expresamente dos identidades:
`qQzdAsjWGPg` y `_d3SaQI_Ag4`, una sesión QA aislada, push de esta rama y
`ci.yml`/`android.yml`. No autorizó merge, force-push ni releases.

En Android se usó `files/qa/premerge-youtube-20261118/`. El usuario completó
el inicio de sesión desde el propio Doremi. La sesión queda en su almacén
AES-GCM/Keystore del dispositivo; no se leyó, imprimió ni copió su contenido.
La ruta JNI recibe el directorio QA y no el del perfil normal. Las pruebas
invitadas se preservan separadas de las autenticadas. No hubo fallback tras
una denegación. Los perfiles desktop usan `DOREMI_DATA_DIR`; no se invoca
restauración del llavero personal ni `stream_probe --session`.

Las pruebas de IDs directos restauran un checkpoint sintético con únicamente
esas dos entradas autorizadas y emiten `play`/`next` en la UI. Esto prueba la
ruta productiva cola → resolvedor → lector → decoder → callback. No demuestra
búsqueda, primera tras login ni prefetch cuando falta duración de catálogo.
La medición válida utiliza `Trace` y el RequestId del mismo snapshot. El reloj
ADB/host solo sirve para observaciones de polling, nunca se mezcla con el
origen monotónico del motor Android.

Evidencia reproducible fuera del árbol, raíz de esta misión:
`/home/eirom/.local/share/doremi/product-qa/2026-11-18/premerge-validation/`.
Contiene scripts ejecutados, respaldo inicial, checks, logs redactados,
categorías de proveedor y metadatos de los experimentos. Los viajes Android
tienen directorios hermanos `premerge-commons-b4b9cc0`,
`premerge-youtube-android-b4b9cc0` y
`premerge-youtube-android-b4b9cc0-session2`. Sus JSONL y snapshots conservan
también errores. `session/` de la primera tentativa autenticada no produjo
reproducción: falló el guard del arnés antes de acceder a la sesión.

La serie `premerge-youtube-android-b4b9cc0-warm` usó duraciones obtenidas por
el bridge de producción: 276.000/284.000 ms. Sus primeros cuatro intentos
son válidos: error/error/éxito/éxito, con el mismo proceso. Se interrumpió
después porque el runner esperaba una pista anterior al pulsar `Previous`
una sola vez con >3 s reproducidos; el núcleo reinicia correctamente la pista
actual. El intento 5 es inválido por esa expectativa del arnés, no fallo de
reproducción de una identidad equivocada. Se conserva la interrupción y su
snapshot; no se incorpora a la tasa de fallos de los cuatro intentos válidos.
No hubo descargas de artefactos concurrentes en esta serie.

## 3. Reproducción real y latencia

APK base instalado y verificado contra manifiesto, SHA256SUMS y `pm path`:
`b4b9cc0b6a5c6b467f929d41677e5522a3650f42`, Samsung SM-J701M, Android 9,
ARMv7. No atribuir estas métricas a un APK del candidato.

Sin sesión, ambas pistas fallaron en resolución con rechazo visible y sin
HTTP de audio/PCM. En el primer viaje autenticado válido, la pista 1 agotó la
verificación de cuenta (`stage=account`, categoría fija del log del mismo PID).
La pista 2 sí entregó 750.637 frames a 48 kHz y 1.497.792 muestras no nulas,
con 510 underruns y 0 xruns reportados, tras más de 15 s de PCM.

| Intervalo Android, pista 2 autenticada | Una muestra real |
| --- | ---: |
| Intención → worker | 18,867 ms |
| Cola de envío → worker | 0,260 ms |
| Espera del cliente | 0,032 ms |
| Extracción nativa completa | 17.314,579 ms |
| Primer GET → primer bloque validado | 230,267 ms |
| Primer bloque → primer PCM | 20.447,535 ms |
| Intención → primer PCM | 38.045,084 ms |
| Intención → callback no silencioso | 38.160,419 ms |

El intervalo bloque→PCM domina en esta muestra, seguido por resolución.
**Incluye red/lecturas y apertura de contenedor; no es coste CPU exclusivo
del decoder.** No se dispone aún de número/latencia de todos los GET hasta
PCM ni subdivisión cuenta/bootstrap/player/QuickJS. El intervalo de dispositivo
también incluye apertura/prebuffer, por lo que no se suma a estos costes.
La extracción Android no ejecuta Python/Deno. La espera de mutex de 32 µs no
explica los 38 s. No se cacheó `account()` ni se alteraron permisos por ello.

Una repetición posterior de la primera pista entregó 747.848 frames, sin
underruns/xruns en sus primeros 15 s de PCM, pero arrancó a 44.481.386 µs:
extracción 4.727.360 µs y primer bloque→PCM 39.141.274 µs. Ya existe audio de
ambas pistas en el APK base, con fallos conservados entre intentos; no una
pasada estable del candidato. Inspección del demuxer Symphonia fijado en el
lockfile: al abrir un MP4 seekable recorre los átomos de todo el stream antes
de volver al primer `mdat`. Es una hipótesis concreta de solicitudes extra,
no prueba de cuántas se hicieron en estas pistas. El candidato abre progresivamente solo un MP4 con `ftyp` y `moov` anteriores
a `mdat`/`moof`, tras inspeccionar como máximo 16 cabeceras y rebobinar. Reactiva
seek inmediatamente después de abrir las cabeceras. La inspección posterior verificó que Symphonia no retiene índices de
fragmentos durante ese recorrido inicial: los descubre de forma lazy en
`next_packet`/`seek`. `dbc841e` incluye también `moof`, con regresión de PCM,
seek adelante a fragmentos no decodificados, atrás y EOF contra la ruta
completa. Otros layouts o cabeceras no aptas conservan el recorrido completo.
No cambia la dependencia, buffers ni timeouts; cancelación/denegación siguen
siendo errores, sin fallback posterior. Las 22 pruebas del player con salida
nativa, Ogg y AAC pasan, incluido seek exacto y apertura sin leer un átomo
de cola de 4 MiB. Clippy del mismo conjunto también pasa. Esta regresión
sintética demuestra el comportamiento de lectura, **no una mejora real**;
la aceptación de rendimiento requiere el APK/binario CI del candidato.

El probe `mp4-header-probe.py` usa el bridge de producción invitado y solo
un bloque validado de 64 KiB por identidad; conserva grants únicamente en
memoria. Ambas cabeceras reales contienen `ftyp` (24 bytes), `moov` (735),
`sidx` (368/380), `moof` (1816) y `mdat` (160207/160120). No descarga pistas
completas ni cambia cliente tras una negativa; no es benchmark de reproducción.
La repetición Android adicional `premerge-youtube-android-baseline-paired/`
conserva seis intentos fallidos (1 frío, 3 cambios, 2 repeticiones), cinco con
primer bloque validado pero sin PCM. El log reciente contiene cinco rechazos
403 en offset 1048576 del PID 23752. No se reintenta ni se sortea ese rechazo;
evitar lecturas innecesarias no demuestra acceso al resto de la canción.
Los fallos de continuidad del candidato se conservan a continuación.

### Primer candidato físico, sin éxito todavía

El APK `dbc841ec16200d0e91f08ddf67fe9780ea94a990` se obtuvo del job APK
aprobado de `44436685168`, aun cuando el workflow global agotó su presupuesto
en host Linux. ZIP CRC, SHA256SUMS/manifiesto y hash del APK instalado fueron
verificados antes de probar. SHA256 APK:
`7b9fd635d9dae05e744b13f42969552be3577d5d2afdfcefebeb10b5c43856e8`.
Se instaló con `adb install -r`, sin borrar la sesión QA ni los datos normales.

`premerge-youtube-android-dbc841e-controls/` conserva cuatro selecciones:
**4 errores / 4 intentos, sin PCM**. Categorías del mismo PID 29192, en orden:
timeout de account, timeout de bootstrap, error de transporte inicial
(`http=0`, 0 bytes, offset 0) y lectura incompleta inicial (HTTP 200,
49.152 bytes, offset 0). El primer intento esperó 5,591 s por el cliente
mientras restauraba la sesión; el siguiente esperó 11 µs. No se atribuye esa
espera al decoder ni se omiten verificaciones de acceso. Las extracciones
que sí finalizaron tardaron 35,400 s y 12,183 s. Ninguno llegó a completar el
primer bloque de audio. Por tanto, esta pasada **no mide** el beneficio de
abrir MP4 progresivamente: ambos extremos de bloque→PCM faltan.

No se ejecutaron pausa/seek/prefetch sin audio. La aserción posterior de
cancelación del runner usaba equivocadamente `audio=false`: ese campo indica
capacidad de reproducción de la identidad, no actividad del motor. Esa prueba
queda **inválida**, no como regresión de Stop; el runner externo siguiente
comprueba `audio_metrics is None`. Se conserva el original en la evidencia.

### Candidato Android: comparación exploratoria y continuidad fallida

La serie posterior `premerge-youtube-android-dbc841e-paired/` usó el mismo
corpus, perfil, duraciones, dispositivo y seis selecciones de la serie base
`...-baseline-paired/`, sin transferencias de artefactos concurrentes. Conserva
**4 errores antes de PCM y 2 éxitos de 15 s**; PID 31073. Los cuatro errores
fueron account/player/player/player. Los éxitos fueron una repetición de la
primera pista y el último cambio a la segunda. La serie base emparejada tuvo
6/6 errores: no existe latencia base válida para calcular una mejora emparejada.

| Escenario Android emparejado | Base intentos/errores | Candidato intentos/errores | Candidato p50/p95/media/mín./máx. de éxitos |
| --- | --- | --- | --- |
| Primera tras restaurar | 1/1 | 1/1 | N/D |
| Cambio normal | 3/3 | 3/2 | 9,122112 s, todos coinciden (n=1) |
| Pista previamente reproducida | 2/2 | 2/1 | 21,722790 s, todos coinciden (n=1) |

El resumen solo condiciona las latencias al éxito de 15 s; **no** elimina los
fallos de las tasas. En el candidato son 100%, 66,67% y 50%, respectivamente.
Una media de éxito no representa el tiempo de espera de quien recibió error.
La prueba extendida hasta 80 s falló en ambas canciones: posiciones finales
40.286/52.453 ms. El mismo PID registró errores `stage=read`, HTTP 200,
0 bytes en offsets 655360/851968. Son errores de lectura, no nuevas negativas
403 en esta serie. No hubo reintento del lector ni cambio de cliente. Estado
**FAIL** de continuidad; llegar a 15 s no acredita escuchar la canción completa.

La serie warm anterior sí ofrece un éxito por identidad/escenario para una
comparación **exploratoria, no una pareja de red controlada ni un p50 fiable**:

| Intervalo real / escenario | Base warm, n=1 | Candidato, n=1 | Reducción aritmética |
| --- | ---: | ---: | ---: |
| Intención→no silencioso, primera pista repetida | 44,481386 s | 21,722790 s | 51,16% |
| Bloque→PCM, misma pista | 39,141274 s | 0,012933 s | 99,967% |
| Intención→no silencioso, cambio a segunda | 30,116026 s | 9,122112 s | 69,71% |
| Bloque→PCM, segunda pista | 25,662369 s | 0,004729 s | 99,982% |

Fórmula aplicada a cada intervalo: `(antes - después) / antes × 100`.
El JSON reproducible es `premerge-validation/android-exploratory-comparison.json`.
No se usa el estimado informal de 10 s ni se presenta una mejora estadística
de población. El coste de extracción incluso aumentó de 4,727/4,312 s a
21,213/8,928 s entre esos éxitos: la variabilidad de red/cuenta importa.
Las dos capturas del candidato tuvieron 0/0 underruns y 0/1 xruns al llegar a
15 s, y 749.768/748.436 frames, respectivamente. No prueban resuelta la
subalimentación histórica ni describen los contadores justo antes del error
extendido, cuando el motor ya fue retirado. PSS observado 92.058→66.350 y
84.758→66.138 KiB; CPU 4.722→5.205 y 6.604→7.063 ticks, sin normalizar.

La apertura MP4 tiene ahora evidencia productiva acotada además de regresiones
offline; el arranque total sigue por encima del objetivo y dominado por
resolución. No se implementan cachés de acceso ni un extractor persistente
basándose en esos dos agregados. Veinte éxitos por escenario y la estabilidad
siguen pendientes; los fallos reproducidos impiden recomendar merge.

Los probes Linux preliminares, con el artefacto completo `cc8a755` verificado
por SOURCE_COMMIT y todos sus checksums, fallaron en transporte/lectura:
primer orden `Decode(Input)` con 16.384 bytes parciales en offset 917.504;
orden inverso, 0 bytes del primer bloque. Sus 62,87/36,55 s de duración de
proceso **no son latencia de intención ni p50**. No se obtuvo una pareja de
15 s de audio en esos probes ni se seleccionó otro cliente tras los fallos.

La línea base Linux posterior sí utiliza el host productivo `df2ef47` y
ambas identidades autorizadas, sin login, en X11/perfil aislados. Verificados
el manifiesto y todos los checksums del bundle: primer callback no silencioso
41.195.791/32.220.177 µs, primer PCM 40.761.920/31.853.853 µs; extracciones
15.517.073/15.412.123 µs y bloque→PCM 25.038.364/16.296.838 µs. Ambas alcanzaron
15 s de PCM con 0 underruns/0 xruns en la captura. Evidencia en
`premerge-youtube-linux-df2ef47-sized/`. Las aserciones iniciales de seek del
runner no verificaban un cambio de posición: no se convierten en PASS de seek.
Dos ejecuciones previas del arnés no emitieron intención por geometría X11
inicial insuficiente/carrera de resize; no se cuentan como fallos del proveedor.

| Escenario | Antes p50 | Después p50 | Antes p95 | Después p95 | Intentos/fallos |
| --- | --- | --- | --- | --- | --- |
| Inicio frío Android | N/D | N/D | N/D | N/D | Android autenticado: 1/1 en esta pasada |
| Inicio frío Linux | 56,730 s (n=1) | 11,785 s (n=2) | 56,730 s (n=1) | 12,107 s (n=2) | Base 1/0; candidato 2/0 |
| Inicio caliente | N/D | N/D | N/D | N/D | NOT_VERIFIED |
| Pista repetida, éxitos exploratorios | 44,481 s (n=1) | 21,723 s (n=1) | 44,481 s (n=1) | 21,723 s (n=1) | Warm base 1/0; candidato emparejado 2/1 |
| Primera tras login | N/D | N/D | N/D | N/D | NOT_VERIFIED: se reinició/restauró el proceso QA |
| Cambio normal, éxitos exploratorios Android | 30,116 s (n=1) | 9,122 s (n=1) | 30,116 s (n=1) | 9,122 s (n=1) | Warm base 2/1; candidato emparejado 3/2 |
| Cambio normal Linux | 47,007 s (n=3) | 14,121 s (n=4) | 63,210 s (n=3) | 18,268 s (n=4) | Base 3/0; candidato 6/2 |
| Cambio con prefetch | N/D | N/D | N/D | N/D | NOT_VERIFIED |
| Seek/reanudación | N/D | N/D | N/D | N/D | Candidato Android/Linux: FAIL por transporte; base no reproducible en la misma ventana |

Para n=1, media/mínimo/máximo coinciden con el único valor; no son un gate
estadístico. La pasada autenticada inicial conserva aparte sus 38,160 s.
La serie warm añade una primera carga fallida (100%, n=1), sin primer PCM.
Los detalles y denominadores por escenario están en cada `summary.json`.
Los JSONL iniciales usaron el nombre de perfil como run; no concatenarlos
entre procesos porque sus RequestId pueden repetirse. Conservar cada serie
por separado; las futuras capturas usan un identificador de proceso único.
No se combina el viaje invitado con el autenticado, ni Android con Linux.
Los ~10 s históricos no se usan como p50. La mejora estadística sigue
**NOT_VERIFIED**; las reducciones aritméticas exploratorias y sus limitaciones
están arriba. No se implementó un extractor persistente, caché de cuenta ni cambio de bloques
o buffers sin medir sus costes respectivos.

### Línea base Linux con duraciones conocidas

Serie `premerge-youtube-linux-baseline-paired`, host `df2ef47`, invitado,
mismo corpus y sin descargas paralelas: seis selecciones que alcanzaron
15 s de PCM. No agruparla con Android ni con la pasada sin duración anterior.
Dos restart/cancelaciones de preparación por la semántica de Previous no se
capturaron como muestras correlacionadas; no son las seis selecciones de
reproducción. El runner guardado permite comprobar esta limitación y las
capturas futuras conservan por separado esos restart cancelados.

| Escenario Linux base | n / errores | p50 | p95 | Media | Mín.–máx. |
| --- | --- | ---: | ---: | ---: | ---: |
| Primer inicio tras restaurar cola | 1 / 0 | 56,730 s | 56,730 s | 56,730 s | 56,730–56,730 s |
| Cambio normal | 3 / 0 | 47,007 s | 63,210 s | 49,465 s | 38,178–63,210 s |
| Pista reproducida previamente | 2 / 0 | 30,068 s | 40,078 s | 35,073 s | 30,068–40,078 s |

Todos son exploratorios (n<20). La distribución de bloque→PCM en los cambios
es p50 22,868 s / p95 43,013 s, media 29,582 s. Los seis snapshots finales
registran underruns 159/0/0/0/825/210 y xruns 0 en todos. No confundir llegar
a 15 s de PCM con 15 s de reproducción sin pausas. Todos los intervalos,
medias y extremos están en `summary.json`; no se restaron percentiles.

### Candidato Linux `5ac8778` frente a la base `df2ef47`

Bundle nativo del job `native-host` del run `44441447069`
(`artifact_id` 14491534025), descargado por rangos con reanudación
(370,263 s de preparación, fuera de las métricas). ZIP, `SHA256SUMS`,
`manifest.json` y `SOURCE_COMMIT` verificados: `5ac8778…`; `doremi`
SHA256 `aedbf781edc3226937251facada6713ec7cd9a70ee415905ebf391de785f0c68`.
Mismo corpus, invitado, X11 y perfil aislados que la base.

Tres intentos previos del arnés quedaron **inválidos por geometría**, no por
el producto: KWin limitó la ventana a 1100x674 (pantalla 1366x768) mientras
la serie base se ejecutó con 1100x800. El arnés exigía la altura exacta; se
adaptó para aceptar la geometría real solo si es estable entre dos snapshots,
registrarla en `report.json` y seguir validando cada control dentro del
viewport. Los tres directorios fallidos se conservan. Cambio del arnés
documentado en `premerge-validation/harness-geometry-adaptation.json`
(SHA256 anterior `79d3c001…dcd6`, posterior `e697d07b…a88e`).

| Escenario Linux candidato (agregado) | n / errores | p50 | p95 | Media | Mín.–máx. |
| --- | --- | ---: | ---: | ---: | ---: |
| Primer inicio tras restaurar cola | 2 / 0 | 11,785 s | 12,107 s | 11,946 s | 11,785–12,107 s |
| Cambio normal | 4 / 2 | 14,121 s | 18,268 s | 15,578 s | 13,791–18,268 s |
| Pista reproducida previamente | 2 / 2 | 12,483 s | 13,044 s | 12,763 s | 12,483–13,044 s |

Dos series `--skip-controls` para separar latencia de los controles:
`premerge-youtube-linux-candidate-5ac8778-latency/` y `...-latency2/`, agregadas
en `...-pooled/samples.jsonl` (cada fila conserva su etiqueta `run`; el script
deduplica por `run`+`request`, así que no se cuenta dos veces una solicitud).
Total **8 éxitos / 12 intentos (66,7%)**. Los cuatro fallos, por categoría del
mismo PID: una resolución `stage=extractor`, una lectura HTTP 200 con 0 bytes
en offset 131072 y **dos 403 `stage=refused`** en offset 0, respetados sin
reintento ni cambio de cliente. En los cambios con éxito, la extracción fue
p50 13,912 s y el intervalo primer bloque→PCM p50 1 ms / p95 2 ms; underruns
por éxito 0/140/106/0/0/63/4/0 y xruns 0. Llegar a 15 s de PCM no implica
reproducción fluida: 4 de 8 éxitos registraron subalimentación.

| Intervalo Linux emparejado | Base `df2ef47` | Candidato `5ac8778` | Reducción aritmética |
| --- | ---: | ---: | ---: |
| Frío, intención→no silencioso | 56,730466 s (n=1) | 11,785 s p50 (n=2) | 79,23% |
| Cambio, intención→no silencioso p50 | 47,007009 s (n=3) | 14,121 s (n=4) | 69,96% |
| Cambio, intención→no silencioso p95 | 63,210222 s (n=3) | 18,268 s (n=4) | 71,10% |
| Pista previa, intención→no silencioso p50 | 30,068 s (n=2) | 12,483 s (n=2) | 58,48% |
| Cambio, bloque→PCM p50 | 22,867555 s (n=3) | 0,001 s (n=4) | 99,996% |
| Cambio, extracción p50 | 19,206425 s (n=3) | 13,912 s (n=4) | 27,56% |

Fórmula: `(antes - después) / antes × 100`, aplicada a cada intervalo, nunca
a percentiles de etapas distintas. Son series exploratorias (n<20) en ventanas
de red distintas (base 21:36, candidato 01:0x–01:4x local) y con un control
sin transferencias concurrentes, no una pareja de red controlada. La tasa de
éxito difiere: base 6/6 en su ventana, candidato 8/12, y la base repitió 2/2
errores en la ventana del candidato. La reducción describe los arranques que
sí llegan a PCM, no la experiencia completa. Los ~10 s históricos siguen sin
usarse como referencia.

**Controles y seek.** La serie con controles
`premerge-youtube-linux-candidate-5ac8778-run3/` reprodujo la segunda pista
(34,906 s hasta callback no silencioso, 892 underruns), y el seek al 35% de la
duración (~99,464 s) terminó en `Failed` con `stage=transport` (`http=0`,
0 bytes, offset 0), sin PCM correlacionado. Para no atribuirlo al cambio de
apertura, se repitió la **base** `df2ef47` en la misma ventana
(`premerge-youtube-linux-base-controls-now/`): **2/2 errores** antes de llegar
a los controles (lectura HTTP 200 con 0 bytes en offset 720896 y resolución
`stage=extractor`). Por tanto el fallo de seek del candidato **no se demuestra
como regresión**: el entorno/proveedor falla también en la base en esta
ventana. Seek sigue **FAIL** en el candidato y la comparación de seek queda
`NOT_VERIFIED`.

Conclusión acotada: la apertura progresiva de MP4 fragmentado reduce el
intervalo bloque→PCM a milisegundos y el arranque total en los éxitos Linux,
pero **no corrige** la continuidad, el seek, la tasa de fallos ni la
subalimentación. No autoriza a declarar resuelta la latencia ni el merge.

### Orden de costes y decisiones

| Hipótesis | Coste observado | Decisión |
| --- | --- | --- |
| Recorrido MP4 antes de PCM | 20–43 s en bases; 836 µs–12,933 ms en éxitos candidatos Android/Linux | Única optimización productiva: apertura progresiva acotada, con pruebas de seek/PCM |
| Resolución Android | 8,928/21,213 s en esos éxitos, además de timeouts previos | Dominante ahora; falta separación cuenta/bootstrap/player/QuickJS, no cachear acceso sin evidencia |
| Extracción desktop | p50 19,206 s base / 13,912 s candidato en cambios Linux; probe separado 35,700/56,468 s | Importación yt-dlp ~0,4–0,7 s y spawn Python ~63 ms no explican todo; extractor persistente no justificado |
| Primer bloque HTTP | 96,862/327,001 ms en éxitos Android candidatos; otros intentos fallan o truncan | Mantener 64 KiB y deadlines; no existe comparación controlada de tamaños |
| Transporte sostenido | Lecturas fallidas a ~40/52 s y rechazo 403 en otro viaje | Riesgo crítico abierto; no ocultar con buffers, retries de grants o cambio de cliente |
| Contención del cliente | 10–15 µs en éxitos; 3,021/5,591 s tras restauraciones fallidas | No asumir coste nulo del primer arranque ni introducir caché de cuenta insegura |

Incluso sin coste de decoder, las extracciones de 8,928/21,213 s impiden
cumplir 3 s en esas muestras. En Linux, el mejor arranque candidato fue
12,107 s con extracción 11,584 s: la apertura MP4 ya no pesa (6 ms), pero
resolución/extracción siguen por encima del objetivo. Esto explica el
incumplimiento observado; no prueba que 3 s sea técnicamente imposible en
otras condiciones. Los costes de apertura productivos posteriores justifican
el cambio pequeño; no justifican marcar estable la reproducción ni ampliar
su alcance.

### Cobertura de los relojes

| Etapa solicitada | Evidencia disponible / límite |
| --- | --- |
| Intención, envío, worker, espera del cliente | Trace monotónico de cada RequestId; captura de errores preservada |
| Verificación de cuenta | Categoría de error y agregados; sin dos marcas exclusivas: NOT_VERIFIED de duración propia |
| Extractor | Inicio/fin agregado real; Android incluye cuenta/bootstrap/player/firma, desktop marca el proceso externo después de account |
| Python, yt-dlp, Deno | Probe desktop separado descrito en §6; dos muestras, no tiempos internos del host |
| Rust/QuickJS | Ruta Android confirmada; bootstrap/player/firma no desglosados temporalmente: NOT_VERIFIED |
| URL lista, primer HTTP, primer bloque válido | Trace; sin URL ni grant persistido. FirstBytes significa bloque íntegro validado, no primer byte de socket |
| Apertura/decoder y PCM | Bloque→PCM medido; falta marcador propio de inicio del decoder para separar CPU/red/prebuffer |
| Callback y primer no silencioso | Métricas del mismo RequestId y origen; sin captura acústica |
| Transporte completo | Error/PID/offset allowlisted y primer GET medidos; no hay histogramas de todos los GET, throughput y conexiones |

No se suman intervalos anidados ni se restan percentiles independientes.
Ninguna latencia desde `adb input` se presenta como intención→callback.

## 4. Underruns y estabilidad Android

En `output.rs`, `buffer::Read::Pending` indica que el consumidor PCM está
vacío sin EOF/error. Incrementa `buffer_underruns` una vez en ese callback y
rellena con silencio lo que queda del callback. Pausa/epoch retirado, EOF y
fallo tienen otros caminos y no incrementan ese contador. No cuenta frames
perdidos ni episodios independientes: varios callbacks vacíos pueden formar
un solo corte. `reported_xruns` cuenta notificaciones CPAL, sin equivaler al
contador hardware. Un valor `buffering=false` al final no borra eventos previos.

Se preservan las 50/33 incidencias históricas. En la nueva repetición Commons
con el mismo APK se observaron **296/215** underruns, respectivamente, y
0/1 xruns; el audio sintético local tuvo 0/0. Primer PCM Commons: 4,814/3,584 s,
primer callback no silencioso: 5,336/3,632 s. Son relojes del motor Commons,
sin resolución/ADB; no se mezclan con las métricas YouTube anteriores.

La nueva serie acotada de snapshots muestra intervalos con bytes HTTP y
frames inmóviles, `buffering=true` y underruns crecientes. Por ejemplo, en
Chopin los frames se mantuvieron en 35.178 y los bytes en 197.167 durante
varias observaciones consecutivas. Las ventanas observadas de las dos pistas
duraron 19,873 y 18,882 s después de la primera observación mientras se
completaban 15 s de PCM. Esto es evidencia de subalimentación y silencio
digital insertado; no una captura acústica ni un timestamp exacto de cada
underrun. Las observaciones se tomaron por ADB, con interferencia posible de
red de laboratorio; no se usan para una comparación controlada antes/después.

El viaje Commons terminó `pass` para sus aserciones de 15 s, pausa, seek,
resume y lifecycle/restauración, **no para ausencia de cortes**. La nueva
ejecución omitió el perfil normal por diseño (`normal_profile_check:
NOT_VERIFIED`). Los resultados históricos de invariantes se preservan, pero
un sobre cifrado del mismo tamaño y magic no prueba que el plaintext sea
idéntico. No descifrar credenciales para intentar mejorar esa evidencia.

Estado de subalimentación: **FAIL, reproducida y sin corregir**. No se subieron
indiscriminadamente buffers. CPU/RAM quedan como muestras PSS/ticks en los
reportes: Commons 66.337→67.436 y 67.875→67.822 KiB PSS; YouTube verificado
63.144→65.458 KiB PSS y 2.603→3.294 ticks CPU del proceso. No hay un benchmark
de CPU normalizado ni RSS del árbol desktop.

## 5. Prefetch, cancelación y formatos

Se conserva el prefetch de una resolución, deadline 15 s, retención 45 s y
margen de lease 5 s; sin audio anticipado ni descargas completas. La intención
directa retira trabajo especulativo, y generación/identidad/sesión invalidan
publicaciones antiguas. Las pruebas existentes cubren pausa/repeat/sesión,
deadline y resultados tardíos, suspensión, continuación de cola y retiro de
un future pendiente. Su resultado de CI debe vincularse al SHA correspondiente.

Beneficio real del prefetch, caducidad bajo uso real, shuffle/reordenamientos,
prioridad bajo carga y saltos rápidos con cuenta activa: **NOT_VERIFIED**.
La prueba con metadatos de duración desconocida no entra en el disparador de
últimos 30 s; no convertir ese caso en un hit ficticio. Cancelación HTTP puede
esperar la lectura bloqueante acotada de hasta 6 s. No se cambió ese timeout
ni se afirma cancelación física inmediata. El supervisor desktop retira el
grupo/Job Object; eso sigue necesitando evidencia real del candidato.

La pasada `premerge-youtube-android-dbc841e-controls2/` conservó una primera
pista con timeout de cuenta y otra reproducción de 15 s de la segunda pista:
26,239672 s hasta callback no silencioso, bloque→PCM 8,651 ms, **21 underruns
y 1 xrun**. La pausa dejó posición/frames estables y la reanudación avanzó
más de 1 s antes de solicitar seek. El seek a ~99 s emitió un RequestId nuevo
y terminó `Failed` con aviso `Timeout`, sin PCM correlacionado. Estado de
seek **FAIL** en el candidato; no se demostró que sea una regresión frente
a base. El PID 1008 también registra 403 en offset 1048576 durante ese viaje;
ese log no lleva RequestId, así que no se atribuye a una petición específica.
Se respetó la negativa sin reintento de grant ni fallback. No hubo hit de
prefetch y no se amplió a audio comprimido especulativo.

Las dos pruebas iniciales de Stop del runner son **inválidas**: primero
exigía `audio=false` (capacidad, no actividad) y después `Stopped`, aunque
`Command::Cancel` conserva la pista en `Paused` por contrato del núcleo.
Los archivos originales se preservan con esta corrección interpretativa.
El viaje específico `premerge-youtube-android-dbc841e-cancel/` sí comprobó
el contrato: canceló una carga `phase=resolve`, quedó `Paused`, sin carga
ni `audio_metrics`, y 27 observaciones durante 8 s no mostraron PCM tardío.
**PASS acotado de cancelación de resolución**. No había primer HTTP: cancelar
lectura bloqueada y demostrar liberación de todos los recursos siguen
`NOT_VERIFIED`. Esa prueba funcional coincidió con una descarga de artefacto,
no se usa como latencia comparativa. No se interpreta como fallo de producto
una expectativa errónea del arnés ni se reescribe su evidencia.

| Verificación de prefetch solicitada | Estado al cierre de pruebas físicas |
| --- | --- |
| Reducción real del cambio | NOT_VERIFIED: ningún Trace consumió un prefetch |
| Vigencia al consumir | Guard de 45 s/lease 5 s inspeccionado; NOT_VERIFIED con red real |
| Cancelar y cambiar cola | Generación/target retirados en código; contrato offline parcial, carrera real NOT_VERIFIED |
| Shuffle/repeat | Repeat/pausa/sesión ejercitados offline; shuffle real NOT_VERIFIED |
| Cambio de cuenta | Revisión de sesión invalida resultado; prueba offline de revisión, cuenta real NOT_VERIFIED |
| Prioridad de reproducción directa | Actor aborta tarea especulativa antes de atender lane 1; NOT_VERIFIED bajo carga real |
| Identidad de pista y saltos rápidos | Checks de MediaId/RequestId/generación; NOT_VERIFIED de carrera real |
| Liberación de recursos | Future pendiente ejercitado offline; lector HTTP/QuickJS real NOT_VERIFIED |

No se cambiaron schemas de biblioteca, cola, playlists, historial ni almacenes
de credenciales, ni fuentes/versiones del lockfile. Se conservaron AAC/MP4,
posición, generaciones y la arquitectura compartida; eso no acredita seek ni
duración de las dos pistas bajo todas las condiciones.

## 6. Linux, Windows y dependencias externas

Linux local: `check-local.py` aprobado; Cargo check y Clippy offline para
core/library/player, suites Python de latencia, geometría/perfil Android,
políticas y 40 fixtures HTTP sintéticos aprobados. La prueba añadida de ADB
emula su shell con directorios temporales y demuestra existente/ausente/error;
la reproducción física confirma 0/1 con quoting correcto y 1/1 con el anterior.
Player con AAC/Ogg y salida nativa: 22 tests y Clippy pasan, incluidos
seek y cancelación. Las suites completas, host, cancelación/prefetch y preview se
delegan a Actions conforme a CONTRIBUTING; no se compiló el workspace local.

Windows: **PHYSICAL_PLAYBACK_NOT_VERIFIED**. Evidencia de compilación/pruebas
en Actions se registra por SHA; no se interpreta como salida de audio real.
El bundle no incluye Python/yt-dlp/EJS/Deno. El README del proveedor documenta
Python 3.11+, yt-dlp >=2026.10.26 con yt-dlp-ejs, Deno en PATH (`python` en
Windows, `python3` en Linux). La falta del runtime produce un error visible
`Unsupported`, sin fallback silencioso, pero no existe una matriz física de
instaladores Windows comprobada.

Versiones locales verificadas sin acceder a sesiones: Python 3.14.9,
yt-dlp 2026.10.26, yt-dlp-ejs 0.8.0, Deno 2.9.8. FFmpeg no es el reproductor
de esta ruta AAC. No se afirma que estas versiones estén empaquetadas ni que
sean todas las combinaciones soportadas. Python/Deno no se usan en Android;
su ruta Rust/QuickJS sí reprodujo ambas identidades, con fallos intermedios
y errores de continuidad en el candidato.

El candidato Linux `5ac8778` se ejecutó con ese mismo entorno externo
(Python 3.14.9, yt-dlp 2026.10.26, EJS 0.8.0, Deno 2.9.8) y el bundle nativo
verificado. Reprodujo 8 de 12 selecciones; los fallos fueron dos 403
respetados, una resolución del extractor y una lectura HTTP 200 sin datos. El
bundle no empaqueta
esas herramientas: sin ellas el error visible es `Unsupported`, sin fallback.
No hay matriz física de instaladores ni evidencia de que otras versiones se
comporten igual.

### Desglose de runtime desktop, probe real separado

`desktop-runtime-stages.py/json` ejecutó el bridge de producción (SHA256
`dd6268366017fb8aa3892733ddc996de8c4e5d55ddef16d5a17c9c5ed0dbc69e`)
con hooks de tiempo en importación/constructor/Popen y ambos IDs invitados.
No modifica la política de extracción, usa grants solo en memoria y no lee
sesiones ni guarda argv/diagnósticos privados. Es un probe instrumentado del
bridge, **no la latencia del host ni una comparación antes/después**. Se
ejecutó durante una transferencia de artefacto CI: la duración global de
red no es comparable con las series de reproducción sin esa transferencia.

| Coste observado, ms | qQzdAsjWGPg | _d3SaQI_Ag4 |
| --- | ---: | ---: |
| Spawn → entrada al script (incluye scheduling/imports iniciales) | 63,273 | 62,757 |
| Importación de yt-dlp | 695,967 | 398,457 |
| Constructor YoutubeDL | 167,048 | 161,607 |
| extract_info (incluye red y Deno) | 56.468,304 | 35.700,287 |
| Procesos Deno: vidas observadas | 119,950 y 2.827,598 | 13,294 |
| Proceso bridge completo | 57.565,831 | 36.488,088 |

La creación Popen de cada Deno costó 0,647/0,740/0,909 ms; su vida completa
incluye arranque y ejecución. No se ha separado V8 de trabajo JS, ni se
identifica retrospectivamente qué invocación fue una consulta de versión.
Los intervalos están anidados: no sumarlos a extract_info ni presentarlos
como costes CPU exclusivos. La diferencia entre pistas no prueba una caché
segura reutilizable. Estas dos muestras no justifican un extractor persistente
ni omitir account(); sí acotan una parte de los costes de Python/yt-dlp/Deno.

## 7. CI y timeout gráfico

El análisis del run `44342784124`, tres intentos, está respaldado por
`previous-ci-jobs.json` y `preview-timeout.log`. En un intento, solo descargar
37,7 MB costó 9m21s; en otro, todas las pruebas gráficas terminaron y el job
fue cancelado cerca del cierre de 10 min. No era evidencia de aprobación
global ni de un fallo de audio.

El candidato sigue reutilizando los dos binarios de `full-tests`. Instala
únicamente runtime gráfico con `--no-install-recommends`, acota preparación
a 12 min y reserva un job de 20 min: 12 instalación, 5 para pruebas medidas
en ~3 min y 3 para artefactos/reportes. Conserva las tres geometrías,
preview/native QA y el gate `verified-artifacts`. No se tocó ningún timeout
de reproducción para ocultar latencia. La corrección del presupuesto gráfico está
**demostrada en `6984caa`**: el run completo `44435616417` terminó
`success`, preview incluido (3m07s), sus tres circuitos gráficos y gate de
artefactos. `ci.yml` `44465708503` repite ese resultado en el head de código
`5ac8778` (`quick`, `clippy`, `full-tests`, `android-core`, `android-apk`,
macOS, Windows, `preview-linux` y `verified-artifacts`, todos success), junto
con `android.yml` `44441447069` en el mismo SHA. Ambos workflows quedan verdes
sobre el código que se propone integrar.

| Workflow / SHA | Evidencia |
| --- | --- |
| `ci.yml`, `79538c3` | [44251398160: success histórico](https://github.com/Eirom16/spotifast/actions/runs/44251398160) |
| `android.yml`, `79538c3` | [44251402256: success histórico](https://github.com/Eirom16/spotifast/actions/runs/44251402256) |
| `ci.yml`, `b4b9cc0` | [44342784124: cancelled, gate incompleto](https://github.com/Eirom16/spotifast/actions/runs/44342784124) |
| `android.yml`, `b4b9cc0` | [44342809444: success, APK físico usado aquí](https://github.com/Eirom16/spotifast/actions/runs/44342809444) |
| `ci.yml`, `df2ef47` | [44430719185: ejecución inicial del candidato](https://github.com/Eirom16/spotifast/actions/runs/44430719185) |
| `android.yml`, `df2ef47` | [44430723595: success global, baseline nativa Linux descargada](https://github.com/Eirom16/spotifast/actions/runs/44430723595) |
| `ci.yml`, `6984caa` | [44435616417: success global, preview y artefactos aprobados](https://github.com/Eirom16/spotifast/actions/runs/44435616417) |
| `android.yml`, `dbc841e` | [44436685168: APK/Windows success, host Linux cancelled a 35 min](https://github.com/Eirom16/spotifast/actions/runs/44436685168) |
| `android.yml`, `5ac8778` | [44441447069: success global, APK/host Linux/host Windows](https://github.com/Eirom16/spotifast/actions/runs/44441447069) |
| `ci.yml`, `5ac8778` | [44465708503: success global, preview y gate de artefactos](https://github.com/Eirom16/spotifast/actions/runs/44465708503) |
| `ci.yml`, `34af8ab` | [44470862089: success global del cierre documental](https://github.com/Eirom16/spotifast/actions/runs/44470862089) |
| `android.yml`, `34af8ab` | [44470866179: success tras relanzar apt del runner](https://github.com/Eirom16/spotifast/actions/runs/44470866179) |

El host Linux de `44436685168` agotó 35 min después de 17m28s de apt.
El run verde anterior requería 18m27s de tests y 1m02s de QA gráfica, más
checkout/artefactos: el presupuesto no cubría esa combinación. `5ac8778`
reserva 50 min (25 preparación, 21 tests, 2 gráfico, 2 artefactos), con
preparación acotada a 25 min y todos los gates intactos. No altera deadlines
de reproducción. Ese run cancelado no es aprobación global.

**Incidente de aprovisionamiento en el cierre documental.** Al publicar
`34af8ab` (solo Markdown y un comentario de script), `ci.yml` `44470862089`
volvió a aprobar todos sus jobs. `android.yml` `44470866179` falló en su
primer intento por infraestructura, no por código: `android-apk` terminó con
`E: Unable to locate package libclang-dev` tras índices apt incompletos, y
`native-host` agotó sus 25 min de preparación durante `apt` (seguía
descargando paquetes); `windows-host` aprobó. El mismo código había aprobado
`android.yml` `44441447069` en `5ac8778`. Se relanzaron los dos jobs fallidos
con `gh run rerun 44470866179 --failed`: **ambos terminaron `success`**, así
que `34af8ab` queda con `ci.yml` y `android.yml` verdes. No se amplió ningún
presupuesto ni se ocultó el fallo: la preparación gráfica depende del espejo
apt del runner y el margen de 25 min es ajustado; el primer intento se
conserva como evidencia de esa fragilidad.

La consulta final por SHA y sus reportes Actions fuera del árbol son la
fuente de verdad. Las ejecuciones sobre `df2ef47` no cubren el guard posterior
`7c080ad`. `5ac8778` y `34af8ab` tienen `ci.yml` y `android.yml` verdes. Los
commits posteriores solo añaden Markdown y enlaces: se verifican con
`git diff 34af8ab..HEAD --stat` y no se les atribuye una compilación nueva.

## 8. Matriz de requisitos y riesgos

| Requisito | Estado | Evidencia / límite |
| --- | --- | --- |
| Integridad de ambas ramas | PASS | Ancestros y árbol de merge reproducido |
| `main` intacto, sin release/force-push | PASS | SHA inicial conservado; solo nueva rama publicada |
| Corpus/sesión autorizados y aislados | PASS | Autorización explícita, login manual, directorio QA |
| Dos pistas YouTube 15 s de PCM | PASS acotado | Ambas en base y candidato; Linux candidato 8/12 y Android candidato 4/6 errores de arranque, más 2/2 fallos extendidos |
| Continuidad YouTube hasta 80 s | FAIL | Android falla a posiciones 40,286/52,453 s; Linux candidato falla en el seek a ~99,464 s |
| p50 ≤3 s, transición ≤1 s | NOT_VERIFIED (incumplido en lo observado) | Linux candidato p50 14,121 s; Android 38,16 s; n<20 |
| Mejora antes/después | Parcial y exploratoria | Linux: frío 56,730→11,785 s y cambio p50 47,007→14,121 s (n=3/4), con 4 fallos de 12; Android n=1; no es p50 de población |
| Estabilidad PCM (Android/Linux) | FAIL | Starvation repetida; Android 510 underruns; Linux candidato 4 de 8 éxitos con underruns (hasta 140) |
| Cuenta/Python/Deno/QuickJS separados | NOT_VERIFIED parcial | Probe desktop real de Python/yt-dlp/Deno; cuenta/bootstrap/QuickJS Android sin tiempos internos propios |
| Prefetch real y carreras de salto | NOT_VERIFIED | Sin hit real; resolución/seek fallidos impiden validar transición |
| Pausa/reanudación YouTube candidato | PASS acotado | Segunda pista, posición/frames estables y avance posterior |
| Seek YouTube candidato | FAIL | Android y Linux ~99 s sin PCM (`Timeout`/transporte); base no reproducible en la misma ventana |
| Cancelación de resolución | PASS acotado | 8 s sin PCM tardío; HTTP bloqueado y reaping NOT_VERIFIED |
| Perfil normal en esta sesión | NOT_VERIFIED | Se evitó su acceso; preservación histórica separada |
| Windows físico | BLOCKED | Sin entorno Windows conectado para reproducir |
| CI completa del head de código | PASS | `ci.yml` `44465708503` y `android.yml` `44441447069` verdes en `5ac8778`; el commit documental posterior solo añade Markdown y un comentario |
| CI del cierre documental `34af8ab` | PASS tras reintento | `ci.yml` `44470862089` verde; `android.yml` `44470866179` falló primero por apt del runner y aprobó al relanzar los jobs |

El riesgo dominante para la futura fusión es presentar reproducción principal
como estable con errores y grandes pausas medidos. También siguen abiertos el
retiro de HTTP bloqueante, refresh/expiración/cambio de cuenta durante carga,
pausa background frente a destrucción del renderer, herramientas externas
desktop y coste de lectura hasta abrir AAC/MP4. A ello se suma la fragilidad
del gate Android: en el cierre documental un intento no encontró
`libclang-dev` y otro agotó 25 min de `apt`; ambos aprobaron al relanzar, pero
el margen de preparación es estrecho y el merge debe exigir reintento, no
interpretar el primer fallo como código roto ni como aprobación. No se propone
una descarga completa, extractor persistente ni ampliación especulativa de
buffers.

## 9. Pasos para una futura integración, no ejecutarlos todavía

1. Reproducir las dos pistas con el APK y binario Linux del head revisado,
   verificar sus manifiestos/SHA256SUMS y separar intentos fallidos.
   *(Hecho en Linux para `5ac8778`: 8/12; Android sigue con las series
   anteriores. Repetir tras cualquier corrección.)*
2. Subdividir cuenta/bootstrap/QuickJS y todas las lecturas hasta primer PCM;
   en desktop, subdividir arranque Python, importación yt-dlp y Deno con
   relojes propios coherentes. Elegir una optimización usando esas trazas.
   *(Parcial: Python/yt-dlp/Deno medidos; el resto sigue `NOT_VERIFIED`.)*
3. Comparar base/candidato con el mismo corpus, red sin descargas de artefactos
   concurrentes y 20 intentos válidos por escenario prioritario, conservando errores.
   Verificar seek, resume, prefetch real, cancelación y ausencia de pista errónea.
   *(Parcial: Linux n=3/4; falta alcanzar 20 y el seek sigue fallando.)*
4. Corregir o explicar y acotar la subalimentación con pruebas digitales y
   físicas; no contar un viaje que llega a 15 s de PCM como reproducción fluida.
5. Obtener `ci.yml` y `android.yml` globalmente verdes del SHA final exacto;
   revisar todos los `report-*`, APK/bundles y capturas gráficas. Windows físico
   sigue separado, con limitación explícita si se acepta para alpha.
6. Volver a consultar `origin/main`, revisar cambios desde `cd40da0`, el diff
   completo, compatibilidad de datos y el informe. Preparar/revisar el PR como
   implementación en desarrollo. Requiere una decisión nueva de integración.
7. Solo con autorización futura, efectuar la fusión aprobada en GitHub y
   comprobar su CI resultante. **Esta misión no ejecuta ese paso.**

Recomendación: **no fusionar aún**. La evidencia ya demuestra reproducción
real parcial y localiza intervalos lentos; en Linux la mejora de arranque es
reproducible en los éxitos (n=3/4), pero 4 de 12 intentos fallan, el seek
queda sin PCM y hay underruns: falta la estabilidad exigida para el merge.

## 10. Cómo comprobar las cifras

Los snapshots contienen la superficie allowlisted del host, no grants de
transporte ni credenciales. Ejemplo con evidencia local, sin acceder al perfil
habitual ni volver a extraer una URL:

```sh
python3 scripts/playback-latency-summary.py summarize /home/eirom/.local/share/doremi/product-qa/2026-11-18/premerge-youtube-linux-baseline-paired/samples.jsonl
python3 scripts/playback-latency-summary.py summarize /home/eirom/.local/share/doremi/product-qa/2026-11-18/premerge-youtube-linux-candidate-5ac8778-latency/samples.jsonl
python3 scripts/playback-latency-summary.py summarize /home/eirom/.local/share/doremi/product-qa/2026-11-18/premerge-youtube-android-baseline-paired/samples.jsonl
git merge-base --is-ancestor cd8927f7321a5f1a51f1882c6af4c529f9a0fe9c HEAD
git merge-base --is-ancestor 07924d597aab5fc80b9254c8d96f773cc88b574d HEAD
git diff cd8927f -- crates/doremi-app/src/settings_view.rs
```

El resumen usa nearest-rank y calcula primero cada intervalo de una misma
solicitud. Sus percentiles de latencia condicionan al resultado `ok`; los
fallos/cancelaciones conservan sus marcas pero no se convierten en latencia
cero. La tasa `failure_rate` del script incluye cualquier resultado no `ok`: al
analizar seek, separar las cancelaciones intencionales de preparación. Los
JSONL de procesos iniciales con run repetido no se concatenan.

Los `runner.py` de las nuevas series están copiados en su propio directorio,
con los argumentos/procedencia en el reporte y el fixture de dos IDs
autorizados. Las primeras pruebas exploratorias tienen las limitaciones de
arnés descritas arriba. Acceder a la evidencia física exige acceso a esos
archivos locales; las pruebas nativas de CI tienen además artefactos públicos
con manifiesto, SHA256SUMS, reportes y capturas. No se afirma que la evidencia
física se haya publicado ni que Actions haya iniciado sesión en YouTube.


## 11. Misión P0 de fiabilidad, noviembre 2026

Se parte de `b5cda4a`; el candidato de código/tests es
`c18786424d0a673a69ec530312b732d41d46e687`. La evidencia física de
`fd36e5b`, `4b903bb` y `b39753d` se conserva como pasos intermedios, sin atribuirla al
candidato posterior. Las series de esta misión y su veredicto se encuentran en
[DOREMI-STREAMING-RELIABILITY.md](DOREMI-STREAMING-RELIABILITY.md).

Se separa el seek/indexado inicial del plazo de prebuffer y se conecta
cancelación del feed con I/O. El lector exige Range estándar/206 e intervalo
exacto. No acepta 200 ambiguos ni publica bloques truncados. Los reintentos de
interrupción/timeout se limitan a tres operaciones de la misma concesión;
conservan privadamente el prefijo de un 206 exacto y solicitan su cola faltante.
No hay renovación o cambio de cliente tras una negativa. Un único worker
anticipa hasta cinco bloques comprimidos, con generación para destinos fuera
del bloque validado; los seeks internos del demuxer conservan el anticipo.

La ventana se dimensionó contra recuperaciones reales de 15 s: seis bloques
incluido el consumidor, hasta 384 KiB + seis bytes de guarda, 256 KiB más que
el primer intento. No crece el buffer PCM de 8192 frames ni el prebuffer de
4096, ni los timeouts HTTP. Se conserva la apertura progresiva de MP4. PCM
distingue callbacks Pending, episodios contiguos y frames exactos de silencio.
La traza QA es circular, optativa y sin secretos. Los rechazos tardíos conservan
AccessDenied hasta el aviso existente de UI. En b39753d, el segundo bloque
Linux tardó 9,73 s y agotó los ~4 s iniciales: 5,64 s de silencio antes de
recuperarse. c187864 espera dos intervalos de la ventana antes del primer
byte de cada generación; añade buffering antes del audio, con cancelación,
sin ampliar memoria comprimida ni timeouts.

Pruebas locales del código final: 59 tests del proveedor (dos ignorados
históricos), 28 del player con AAC/MP4, Ogg y salida nativa, Clippy de ambos y
check-local aprobados. La matriz de seek offline incluye MP4 convencional y
fragmentado a 10/35/50/75/98 % y retorno. Estos PASS no sustituyen audio real.

`4b903bb` tuvo ambos workflows verdes y un EOF Linux de q con cinco timeouts
recuperados y cero underruns; la otra selección fue rechazada antes de PCM.
Android logró siete arranques dentro de un proceso y todos terminaron con
403 exactamente en 1048576; además hubo fallos de cuenta. Se conservan también
sus silencios, rechazos de seek y pruebas de cancelación, sin convertirlos en
reproducción completa. b39753d pasó ambos workflows, alcanzó un EOF por pista Linux (q con silencio
inicial, _d3 sin underruns), pero Android quedó 0/4 EOF por identidad y
falló seek al 35 % con 403 en 1048576. El código final se volvió a probar: Linux seis intentos, tres por pista,
0 EOF; Android ocho intentos, cuatro por pista, 0 EOF. En cada plataforma
hubo tres arranques PCM y todos fallaron posteriormente por timeout HTTP.
La serie final acumuló 23,789021 s de silencio por starvation en Linux y
53,262396 s en Android; la duración menor de intentos fallidos impide comparar
esos totales como una mejora de continuidad. Todos los intervalos aceptados
observados fueron 206 íntegros; no hubo EOF ficticio ni 403 de medios en esas
series finales. Las negativas 403 de 1 MiB pertenecen al paso b39753d.

CI final de código: ci.yml 44647361722 y android.yml 44647370690, ambos PASS
al primer intento sobre c187864. APK SHA256
282ed75d9845bafd7167ca801fd491f0bcfcd7f7a8076599adc9b7c59b1e8a07,
verificado también instalado. Binario Linux SHA256
911af2222af3f2c0cf3a2afc58d37436210b387a1e303d44bffb280b45a1cf56.
En Linux se verificaron CRC y SHA de todas las entradas utilizadas; se omitió
solo el alias no ejecutado doremi-app tras agotar la descarga completa. No se
certifica CRC del ZIP completo. Manifiestos source_commit/dirty comprobados.
El cierre documental es un commit posterior de Markdown, sin cambiar esos
payloads; no se presenta como otro build de aplicación.

Cancelación rápida Android c187864: cinco Next, Previous y Stop durante resolve,
73 observaciones/20 s sin PCM tardío. Seek/pause de la serie Android final:
bloqueados por fallos de arranque. Las fixtures de seek integrado HTTP→AAC
convencional/fragmentado pasan sin certificar reproducción física. Windows:
PHYSICAL_PLAYBACK_NOT_VERIFIED. Se mantiene la recomendación de no fusionar;
main sigue en cd40da0 según la consulta remota del 20 de noviembre y no se
modificó. El informe de fiabilidad conserva también el control Linux final
independiente, las causas demostradas y los escenarios físicos pendientes.
