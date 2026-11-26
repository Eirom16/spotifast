# Doremi · recuperación de Spotifast

Rama `feature/doremi-legacy-features`. Base remota verificada
`00e540355e6509ef4d6f85b3f8bbf5996c23e2af`, que incluye el catálogo validado
en `3366307db46fc190d66e4bf106c79e239aefd42e`. Último commit de código
`a96d701de7a3ab5e35e3dd840f3a3f95a1a11d27`. Los binarios y pruebas nativas de
este informe corresponden a `fa523a48aa85ded638f8976f95d757f65f3966f8`.
Este informe se registra en un commit documental posterior.

## Funciones

| Función | Estado | Evidencia y límites |
| --- | --- | --- |
| Ecualizador | WORKING | Diez bandas, preamplificador, presets, bypass y restablecimiento. Ajustes modificados durante audio real en Linux y el Samsung. En Linux se comprobó el preset Club y el restablecimiento de todas las bandas. El callback de Linux a 48 kHz midió **+12,00 dB a 1 kHz** y **+0,00 dB tras el bypass**, con cero underruns y xruns. Con todos los realces al máximo, la prueba de señal midió un pico de 0,8913 de escala completa. |
| Letras sincronizadas | WORKING | Carga, coincidencia estricta de metadatos, caché, cancelación por generación y línea activa. Con «I Feel Fantastic» se verificó el seek por línea en Linux y el Samsung. Linux recuperó la transcripción del disco en un proceso nuevo con el proxy HTTPS inaccesible. «Code Monkey» quedó sin letras al devolver LRCLIB candidatos incompatibles del mismo álbum. |
| Letras normales | PARTIAL | La vista muestra texto plano cuando la coincidencia carece de marcas. La prueba física usó una transcripción sincronizada. |
| MPRIS en Linux | WORKING | Play, Pause, Next, Previous, Stop, seek absoluto y relativo y Quit con `playerctl` y D-Bus. Verificados título, artista, álbum, duración, posición y carátula local. El servicio de teclas multimedia de KDE Plasma pausó y reanudó Doremi en Wayland. La pantalla de bloqueo de KDE quedó sin prueba directa. |
| Controles nativos de Windows | PARTIAL | Integración SMTC de fastframe con carátula de la caché. Pasaron build y smoke de Windows en CI para `fa523a4`. Interacción física pendiente. |
| Mini reproductor de escritorio | PARTIAL | Ventana de 350 × 148 con controles, créditos y carátula. Desde la bandeja oculta se abrió en Wayland y XWayland con el mismo proceso, posición de reproducción y cola. En XWayland se guardó la posición de la ventana. El compositor decide la colocación y el nivel en Wayland. Plasma XWayland dejó el atributo de ventana superior sin confirmar. El intercambio directo con la ventana principal necesita repetir QA después del parche de cierre. |
| Bandeja del sistema | PARTIAL | Verificada en Plasma Wayland la continuidad de PCM al ocultar con la preferencia de cierre a bandeja activa, y pausa y reanudación desde su menú. El binario de CI presenta un fallo al devolver eframe la notificación de cierre. El código posterior conserva la primera decisión del shell. |
| Espectro y osciloscopio | WORKING | Ambos mostraron PCM real en Linux. El osciloscopio mostró PCM real en el Samsung. El tap toma audio post-EQ y previo al volumen, con caducidad al dejar de dibujar la vista. El límite de 20 FPS en Android pertenece a `a96d701` y queda sin medición física. |
| MilkDrop y projectM en escritorio | BLOCKED | La integración heredada requiere su proceso gráfico, ring estéreo compartido y backend projectM. Doremi integra el tap mono para los visualizadores básicos. La implementación heredada permanece disponible en el árbol. |
| Biblioteca | WORKING | Filtro por título, artista y álbum. Orden por título, artista, álbum o duración. Linux verificó el orden por título, reprodujo solo la vista filtrada y reordenó la segunda de dos ocurrencias duplicadas en su posición original. Se reutilizaron favoritos, historial, acciones de fila, edición y persistencia. |
| Preferencias | PARTIAL | Tema y acentos existentes, con morado predeterminado. Se añadieron EQ, visualización, cierre a bandeja, posición compacta y publicación multimedia aplicable al reiniciar. El Samsung recuperó EQ y osciloscopio al reiniciar. Wayland no informa coordenadas globales. |
| MilkDrop, mini reproductor y bandeja en Android | NOT_APPLICABLE | Son superficies de escritorio. Android usa MediaSession y su reproductor móvil. |
| Funciones exclusivas de Spotify, casting y archivos locales | NOT_APPLICABLE | Fuera del alcance de esta recuperación. |

## Código reutilizado

- `src/eq.rs` aportó bandas, presets, biquads y ajuste de respuesta entre
  bandas. Se adaptó la rodilla suave de `src/limiter.rs` sin añadir
  latencia de anticipación al reloj PCM. El worker calcula los coeficientes y
  los publica al callback.
- `src/lyrics.rs` y `src/ui/lyrics.rs` aportaron parser, línea activa, consulta,
  caché y desplazamiento. Se verifican título, artista, álbum si existe y
  duración con tolerancia de dos segundos. [API de LRCLIB](https://lrclib.net/docs).
- `src/app.rs` y `src/entrypoint.rs` aportaron transporte, menú y ciclo de
  ventanas a través de los mismos pins de fastframe.
- `src/vis.rs` aportó AudioTap, FFT, espectro y osciloscopio.
  `src/ui/collection.rs` aportó el filtrado por metadatos y la separación
  entre orden mostrado y posición almacenada.
- Las adaptaciones conservan la procedencia MIT de Carmine Paolino. Las
  licencias de la aplicación, los assets y los medios de prueba se conservaron.

## Regresiones y rendimiento

Pasó `native-audio-qa.py` con PCM real, pausa, seek, volumen, anterior,
siguiente y cancelación. El probe de precarga de Linux reprodujo hasta EOF
natural, entregó 13.260.160 frames de la primera pista con cero underruns
o xruns, y tardó 91 ms en pasar a la segunda desde caché al pulsar Siguiente.
Los dos medios de esa prueba fueron los autorizados en el informe del catálogo.

Pasó `android-background-qa.py --prefetch` en el Samsung. El teléfono
reprodujo con la pantalla apagada durante 264,7 s hasta el cambio natural de
pista. La siguiente entregó PCM en 632 ms desde el cambio de cola y el salto
manual en caché tardó 537 ms. La precarga usó dos descargas en total. La
primera pista registró 13.258.284 frames, cero underruns del buffer y un xrun
notificado por CPAL. También pasaron MediaSession, la notificación multimedia,
la reapertura con el mismo motor, la interrupción y recuperación de audio
focus, y la limpieza del servicio tras Stop. La primera ejecución de
instrumentación perdió su conexión de UiAutomation. La repetición emitió el
veredicto completo y conservó los hashes protegidos. En pausa con la pantalla
apagada se observaron 1.081 ms de CPU en 32 s.

Para la prueba de letras se usó la grabación de «I Feel Fantastic» de
Jonathan Coulton publicada en *JoCo Looks Back*. El Samsung conservó los
hashes del perfil habitual durante la prueba aislada. [Licencia de la colección](https://creativecommons.org/2008/11/17/campaign-exclusive-custon-usb-drives-unreleased-jonathan-coulton-album/).

| Hardware y modo | CPU de un núcleo | Memoria | Underruns / xruns |
| --- | ---: | ---: | ---: |
| Intel N5000/UHD 605 con XWayland, normal | 7,7 % | 109,5 MiB RSS | 0 / 0 |
| Intel N5000/UHD 605 con XWayland, EQ | 8,0 % | 109,6 MiB RSS | 0 / 0 |
| Intel N5000/UHD 605 con XWayland, EQ y espectro | 18,6 % | 109,6 MiB RSS | 0 / 0 |
| Samsung SM-J701M Android 9 ARMv7, normal | 25,1 % | 60,0 MiB PSS | 0 / 0 |
| Samsung SM-J701M Android 9 ARMv7, EQ | 25,5 % | 62,2 MiB PSS | 0 / 0 |
| Samsung SM-J701M Android 9 ARMv7, EQ y osciloscopio | 65,9 % | 63,3 MiB PSS | 0 / 0 |

Muestras de 6–7 s con los artefactos de desarrollo y la escala original,
con una banda de EQ modificada. Son métricas del proceso y del backend de
audio. El uso de GPU y los percentiles de tiempo de frame quedaron sin medir.
Xvfb con renderizado por software fue una prueba funcional aparte.

Se corrigieron el inventario de dependencias, la expansión del diagnóstico
JSON, dos conflictos de préstamo y los lints de Rust fijado. La QA nativa
encontró que repetir el evento de cierre podía salir de la aplicación o
volver a abrir la ventana equivocada. `a96d701` conserva la primera decisión
hasta que se destruye la ventana. Pasaron Clippy por crate y `check-local.py`
en ese commit. También pasaron 32 tests del reproductor y el probe nativo
que requiere un dispositivo de salida.

## CI y artefactos

Pasaron los nueve jobs de `fa523a4`, incluidos tests completos, Clippy, QA
gráfica, Windows, macOS y el paquete Android ARMv7/ARM64. [Ejecución de CI](https://github.com/Eirom16/spotifast/actions/runs/45714894173).

| Artefacto bajo `target/legacy-artifacts/fa523a4/` | SHA-256 |
| --- | --- |
| `linux/doremi` | `f4b1dd36ae0c888dde917e0bdaf4419072a16cf1366eafff28e8495ccf06e451` |
| `android/app-debug.apk` | `eafee5014e1dcb9dd2f5b30e0c9ce7f5fec803b2fa88312b534a012984513e0f` |

**Compilación de `a96d701` bloqueada.** Los intentos de lanzar los workflows
Linux y Android devolvieron HTTP 422 con el mensaje «Actions has been
disabled for this repository». La API de permisos devolvió `enabled: true`.
Los artefactos de la tabla pertenecen a `fa523a4` y preceden a la corrección
de ventana y al límite nuevo de visualización en Android.
Se mantuvo el circuito de compilación exclusivamente en CI.

La inspección local del APK comprobó checksum, commit incrustado y ambos ELF.
El checksum de la copia instalada en el Samsung coincidió con el artefacto.
El gate de `verify-android.py` con `aapt` del SDK 36.0.0 se ejecutó en Actions.
Las capturas y mediciones están fuera del árbol en
`~/.local/share/doremi/product-qa/2026-11-25-legacy/`.

- En Linux se guardaron `linux-eq-controls-isolated/preset-selected.png`,
  `linux-xwayland-gpu/spectrum.png`, `linux-x11-verified/scope.png`,
  `linux-lyrics/lyric-seek.png`, `mini-artwork/mini.png` y
  `linux-library/playlist-filter-duplicates.png`.
- En el Samsung se guardaron `android-ui/equalizer.png`,
  `android-ui/oscilloscope.png`, `android-lyrics/lyric-seek.png`, `android-lyrics/restored-profile.png` y
  `android-background-verified/qa-background-lockscreen.png`.
