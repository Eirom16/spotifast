# Rescate de reproducción Doremi

**WORKING** para los criterios mínimos del corpus autorizado en Linux y Android.
Código y artefactos probados: `507022eac6aaa2eaa3e7a4d2ec47bf9837e58f3b`,
versión `0.1.0-dev`. Este informe se añade después de probar ese corte.

## 1. Implementación integrada

Backend compartido en `crates/doremi-youtube/src/rescue.rs`: resolución pública
primaria y descarga completa AAC/MP4 por bloques de 1 MiB mediante `ureq`,
antes de reproducir. Límite de 64 MiB por archivo, cancelación, plazos y
continuación de lecturas interrumpidas desde el último byte del mismo grant.
Las denegaciones de acceso son terminales; no hay cambio de cuenta o cliente
para sortearlas.

## 2. Código reutilizado

Petición primaria y transporte por rangos adaptados de `src/ytm.rs` y
`src/player.rs` de Tubefast, revisión
`c0813fc0619e670096fc51a9d944a9ca4e4b2e42`, MIT, Yiğit Bozyaka.
Aviso en `crates/doremi-youtube/LICENSE-TUBEFAST`; licencias Tubefast, ureq y
del resto de componentes incluidas en ambos payloads. Se conservan Symphonia,
las salidas nativas y la UI Doremi. No se copió la UI Tubefast.
El arnés del motor original decodificó ambas canciones hasta EOF y realizó
cuatro seeks por canción; esa comparación no midió salida acústica.

## 3. Componentes sustituidos

La ruta predeterminada prepara todo el audio antes del decoder en lugar de
depender de HTTP durante la reproducción.
`DOREMI_PLAYBACK_BACKEND=legacy` recupera la ruta anterior.
La caché temporal retiene las dos canciones usadas más recientemente
(hasta 128 MiB en la caché, además de buffers activos/preparación).
Seek, repeat y navegación hacia una canción retenida comparten los bytes,
sin volver a descargarlos ni copiar el archivo en la UI. No se persiste audio
ni URLs firmadas. Cola, biblioteca y presentación siguen conservadas.

## 4. Linux comprobado

Cuatro EOF naturales por salida nativa, con tiempo real y muestras no nulas.
Pausa, seek a 60 s, reanudación, next/previous y cancelación aprobados por el
probe. La GUI X11 también aprobó seek en pausa, navegación, cancelación sin
audio tardío y conservación de su biblioteca sintética.
Instalado como **Doremi Native Dev** en el menú de aplicaciones, conservando
las versiones anteriores y los datos existentes.

## 5. Android comprobado

APK del mismo commit instalado en Samsung SM-J701M, API 28, ARMv7.
Cuatro EOF naturales, pausa, seek a media canción, reanudación y cancelación
aprobados mediante instrumentación en `rescue-cache507022e-pass1`.
Hashes de `library-v1.json` y `youtube-session-v1.bin` habituales idénticos
antes/después; se restauró el lanzamiento con el perfil normal.

## 6. Reproducción completa y seek

| Identidad | Duración | EOF Linux | EOF Android |
|---|---:|---:|---:|
| `qQzdAsjWGPg` | 4:36 | 2/2 | 2/2 |
| `_d3SaQI_Ag4` | 4:44 | 2/2 | 2/2 |

Cero underruns del buffer en las ocho reproducciones. Linux: cero xruns.
Android: un xrun en el frame 0 en cada arranque de `_d3SaQI_Ag4`, dos en total;
ninguno adicional durante esas reproducciones. Los seeks y la reanudación
pasaron en ambos hosts. No hubo captura acústica ni evaluación de escucha.

## 7. Errores y límites pendientes

Los xruns de arranque Android quedan registrados; no se afirma ausencia de
silencios audibles. La descarga inicial puede tardar decenas de segundos o
fallar por transporte/proveedor. La caché solo conserva dos pistas durante
la vida del worker. Esta aceptación cubre dos identidades públicas autorizadas,
no la disponibilidad del catálogo completo.

## 8. Commits y CI

`b59c365`: integración y probes; `7420afe`: transporte ureq;
`3a384ec`/`3f0b337`: recuperación y plazos;
`56dac7f`: corrección del arnés Android al desaparecer el player tras EOF;
`507022e`: caché compartida y regresión de seek sin resolver de nuevo.

[CI general, nueve jobs aprobados](https://github.com/Eirom16/spotifast/actions/runs/44969264679).
[Android y hosts nativos, tres jobs aprobados](https://github.com/Eirom16/spotifast/actions/runs/44969266796),
en el segundo intento: el primer empaquetado Android falló por HTTP 429 de
Maven; el APK usado pasó en la CI general del mismo commit.
Local: 53 tests de la app aprobados, un probe de red/audio ignorado en la
suite ordinaria y ejecutado después mediante el binario de CI;
Clippy con `-D warnings`, circuito ligero y 40 casos sintéticos aprobados.
Sin merge ni release pública.

## 9. Uso diario y evidencia

La versión verificada está instalada en Linux y en el Samsung. Queda evaluar
catálogo más amplio, arranque con distintas redes y uso prolongado/integración
del SO, incluida reproducción en segundo plano Android; esta prueba no los
certifica.

Evidencia privada fuera del repositorio:
`~/.local/share/doremi/product-qa/20261121-rescue/`:
`linux-cache507022e-result.json`, `linux-cache507022e-gui/report.json`,
`linux-cache507022e-native.log`, `android-cache507022e-pass1/`,
`cache-ci-result.json` y `cache-android-ci-result.json`.
APK verificado: `target/rescue-cache507022e/android/app-debug.apk`.
