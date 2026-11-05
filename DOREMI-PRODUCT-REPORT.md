# Doremi — avance de producto

Rama `feature/doremi-product-ui`, creada desde `ea71b92` tras consultar el
remoto. El informe local anterior se conserva sin incorporarlo a los commits.
El objetivo completo sigue abierto; no hay release estable.

## Funciones verificadas

- `doremi-ui` extrae paleta, Inter, iconos, navegación, controles, slider,
  contenedor de player y placeholder de las fuentes originales. El legado
  los reutiliza; conserva sus acciones, credenciales, DSP y visualizadores.
- Cabeceras de colección extraídas de la vista original: el legado conserva
  artwork, ampliación, enlaces, bidi e i18n mediante slots. Doremi las usa para
  artista, álbum y grabación. Resultados en filas compactas con artista,
  duración y guardado. Abrir metadatos desde Biblioteca o sidebar conserva
  la reproducción; los contextos de audio omiten esos metadatos.
- El host neutral compone sidebar desktop, navegación táctil, Inicio,
  Biblioteca, Cola, Ajustes y barra persistente sobre Queue/Engine existentes.
  Inicio prioriza Buscar/Biblioteca; las muestras permanecen en un desplegable.
- MusicBrainz público: búsqueda real por grabación, artista y álbum/edición,
  grabaciones de artista, pistas de álbum, paginación y guardado por ID neutral.
  Sin Spotify, cuenta, cookies, extractor ni proceso musical externo.
- Linux: recorrido gráfico real PASS, con artista, álbum, Unicode, vacío,
  cancelación y guardado. También PASS navegación, teclado, cola con duplicados,
  temas/acento y ventana 384×658. Evidencias:
  `/home/eirom/.local/share/doremi/product-qa/2026-11-04/doremi-product-linux-6bbc/` y `/home/eirom/.local/share/doremi/product-qa/2026-11-04/doremi-product-linux-rate/`.
  La barra de `94d112b` pasó audio nativo real, pausa/reanudación, seek en
  pausa a 4006 ms, volumen por rueda/puntero, next/previous y cancelación:
  25.600 frames/51.194 muestras no nulas en la captura inicial. Muestras propias
  CC0, sin captura acústica; no es streaming ni benchmark de catálogo.
  `/home/eirom/.local/share/doremi/product-qa/2026-11-04/doremi-product-audio-94d112b/qa-seek/`; build local con cambios
  exclusivamente documentales fuera de sus entradas Rust, declarado como tal.
- Android `6bbcfe3`: APK verificada e instalada por ADB en SM-J701M,
  Android 9/API 28, ARMv7. GUI y consulta «Moonlight Sonata» con **20 resultados**.
  Favoritos preservados, sesión pausada, sin autoplay. Navegación completa
  dentro del viewport 384×658. Evidencia `/home/eirom/.local/share/doremi/product-qa/2026-11-04/doremi-product-android-6bbc/`;
  APK SHA-256 `1caab6cdb906d8b64715a09b518aa629cd4ab977f874dc60cb14a37c9dfc37c4`.
- Windows `6bbcfe3`: runner nativo pasó tests/build/CLI del nuevo `doremi.exe`.
  Artifact descargado: manifiesto/hash y PE x64 verificados, imports examinados.
  SHA-256 `c26d91187effe1d12868ad925bca88de554a75b1ec0e775631aa0bdae4653114`;
  evidencia `/home/eirom/.local/share/doremi/product-qa/2026-11-04/doremi-product-windows-6bbc/`. Sin prueba física Windows.

## Alcance y limitaciones

MusicBrainz entrega **metadatos, no audio ni carátulas**. No se convierten sus
IDs en búsquedas de otro sitio. Demo y los dos tonos Commons CC0 permanecen
como fuentes separadas y explícitas. Commons aún descarga el recurso antes
de decodificar: no es streaming progresivo. No se añaden formatos locales.

La recuperación visual completa sigue pendiente: tablas completas, shelves,
playlist, artwork, transiciones y demás capacidades del roadmap. Las cabeceras
de artista/álbum ya comparten el componente original.
Este corte no cierra P0 ni sustituye la Preview aprobada como evidencia final.
Playlists propias, historial completo, cuenta opcional, EQ/DSP/visualizadores,
integraciones multimedia y segundo plano Android siguen abiertos.

Proveedor: HTTP/TLS nativo compartido, respuestas de 1 MiB, páginas de 20,
ocho páginas de caché/5 minutos, generaciones y canales acotados. Tras observar
un 503/límite real se serializan peticiones y se espera 1,1 s **después** de
finalizar/cancelar, respetando Retry-After. Error visible y reintento manual;
sin retries automáticos ni cambios de identidad. El actor se conserva al
recrear Activity; cancelación por generación y un hilo de DNS como máximo.
Revisión actual de Tubefast, yt-dlp/EJS y RustyPipe en `TUBEFAST-ASSESSMENT.md`.

## Validación y correcciones en curso

CI [41959750650](https://github.com/Eirom16/spotifast/actions/runs/41959750650),
código 05e91b7, en curso: quick y núcleo Android aprobados. Incluye la
corrección del enlace Rustdoc. No se presenta el conjunto como aprobado.

28 tests del host, Clippy del host, circuito ligero y Rustdoc de UI/catálogo
pasaron localmente. La geometría incluye 360, 384, 393, 430 y 440 puntos,
paisaje y 384×332 con IME. Comprueba también etiquetas del timeline y título,
no solo hitboxes; se corrigió una altura mínima de 48 pt heredada por una fila
de progreso de 20 pt que recortaba las etiquetas en Android.

GUI real Linux/XTest del código de 05e91b7: PASS en oscuro a 1240×800 y claro
a 384×658. Grabación, Björk Unicode, grabaciones de artista, Kind of Blue/pistas,
vacío, cancelación, guardar y reabrir sin alterar playback. Capturas antes/después
y reportes con hash del ejecutable y driver en la evidencia persistente,
carpetas hero-final-dark/ y hero-final-light-phone/. Son builds locales;
no artifacts oficiales ni prueba física Android. El diseño móvil aún mantiene
un player demasiado grande en navegación: pendiente mini-player/vista dedicada.

CI anterior [41933320025](https://github.com/Eirom16/spotifast/actions/runs/41933320025),
código 10f80b5: tests completos, ambos lints Clippy, núcleo/APK Android y builds
nativos Windows/macOS pasaron. Rustdoc falló por un enlace sin formato (corregido
en 5ea6954). La revisión gráfica Linux quedó cancelada; el gate agregado falló.
No se rebajó ningún check.

Artifacts Linux/Windows 10f80b5 descargados: manifiestos y conjunto completo
de checksums verificados. Linux instalado con scripts/install-native-dev.py,
conservando versiones/datos; launcher «Doremi Native Dev» actualizado y GUI
abierta en CachyOS Wayland, viewport 1240×768, estado Stopped, sin autoplay.
Linux SHA-256 b8ce2153a0cd385846a07aafc6402173f1c2c350edfd6d103307885b6f498813.
Windows SHA-256 f3bda2af8ea4abf8da049a8cf5189655d2765b872f5e08cae7766d9272363c2c.
Smoke Windows ejecutado: **sin GUI ni audio**; el runner devuelve OpenGL <2.0
y NoDevice. Esto prueba build/tests/CLI, no playback ni hardware Windows.

La APK 10f80b5 se verificó e instaló mediante ADB antes del reinicio del
entorno: consulta Moonlight Sonata con 20 resultados, biblioteca conservada,
controles compactos sin intersección. SHA-256
f929b18869d0655317b9e269029f94265a3270023c932d18f5f60652649d35f9.
La captura mostró el recorte de etiquetas corregido después. Esa evidencia
temporal se perdió al reiniciar; no sustituye una nueva prueba física del código
05e91b7. ADB actualmente no detecta dispositivos.

Evidencia persistente, incluidos artifacts/licencias y fallos:
/home/eirom/.local/share/doremi/product-qa/2026-11-04/.
El informe local previo del usuario y sus skills se conservan fuera de commits.

Siguiente corte: mini-player/vista móvil dedicada y transporte progresivo
autorizado sobre el motor existente. Se investiga también Audius como
candidato público: una consulta real sin credenciales devolvió metadatos y
flags de disponibilidad diferentes por pista. No se ha integrado ni reproducido
su catálogo; revisión contractual/API pendiente. MusicBrainz continúa siendo
metadatos, y Commons sigue limitado a sus dos tonos.
