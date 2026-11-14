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
de 64 KiB mediante `range=inicio-fin` en la query de la concesión de
googlevideo, como el transporte de Tubefast. Acepta `200` sin
`Content-Range` y `206` con el rango correcto. Nunca acepta más bytes que
los solicitados ni convierte una denegación en un reintento con otro cliente.
La caché conserva como máximo un bloque más un byte para detectar desbordes.
Un bloque incompleto o cancelado no se publica en la caché.

`YTM range` solo muestra estado HTTP, categoría fija, número de bytes y offset.
`stage=read` identifica un fallo leyendo HTTP, no un rechazo del códec.
`stage=mp4` identifica la cabecera del primer bloque, no prueba que se haya
decodificado ni reproducido audio.

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
