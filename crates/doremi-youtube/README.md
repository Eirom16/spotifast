# Diagnóstico de reproducción

El lector comparte código entre Linux, Android y Windows. Solicita bloques
de 64 KiB mediante `range=inicio-fin` en la query de la concesión de
googlevideo, como el transporte de Tubefast. Acepta `200` sin
`Content-Range` y `206` con el rango correcto. Nunca acepta más bytes que
los solicitados ni convierte una denegación en un reintento con otro cliente.
La caché conserva como máximo un bloque más un byte para detectar desbordes.
Un bloque incompleto o cancelado no se publica en la caché.

`YTM range` solo muestra estado HTTP, categoría fija y número de bytes.
`stage=read` identifica un fallo leyendo HTTP, no un rechazo del códec.
`stage=mp4` identifica la cabecera del primer bloque, no prueba que se haya
decodificado ni reproducido audio.

## Prueba explícita con audio nativo

```sh
cargo run --locked -p doremi-youtube --example stream_probe -- \
  --session PRIMER_VIDEO_ID SEGUNDO_VIDEO_ID
```

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
