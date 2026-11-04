# Doremi — avance de producto

Rama `feature/doremi-product-ui`, creada desde `ea71b92` tras consultar el
remoto. El informe local anterior se conserva sin incorporarlo a los commits.
El objetivo completo sigue abierto; no hay release estable.

## Funciones verificadas

- `doremi-ui` extrae paleta, Inter, iconos, navegación, controles, slider,
  contenedor de player y placeholder de las fuentes originales. El legado
  los reutiliza; conserva sus acciones, credenciales, DSP y visualizadores.
- El host neutral compone sidebar desktop, navegación táctil, Inicio,
  Biblioteca, Cola, Ajustes y barra persistente sobre Queue/Engine existentes.
  Inicio prioriza Buscar/Biblioteca; las muestras permanecen en un desplegable.
- MusicBrainz público: búsqueda real por grabación, artista y álbum/edición,
  grabaciones de artista, pistas de álbum, paginación y guardado por ID neutral.
  Sin Spotify, cuenta, cookies, extractor ni proceso musical externo.
- Linux: recorrido gráfico real PASS, con artista, álbum, Unicode, vacío,
  cancelación y guardado. También PASS navegación, teclado, cola con duplicados,
  temas/acento y ventana 384×658. Evidencias:
  `/tmp/doremi-product-linux-6bbc/` y `/tmp/doremi-product-linux-rate/`.
  La barra de `94d112b` pasó audio nativo real, pausa/reanudación, seek en
  pausa a 4006 ms, volumen por rueda/puntero, next/previous y cancelación:
  25.600 frames/51.194 muestras no nulas en la captura inicial. Muestras propias
  CC0, sin captura acústica; no es streaming ni benchmark de catálogo.
  `/tmp/doremi-product-audio-94d112b/qa-seek/`; build local con cambios
  exclusivamente documentales fuera de sus entradas Rust, declarado como tal.
- Android `6bbcfe3`: APK verificada e instalada por ADB en SM-J701M,
  Android 9/API 28, ARMv7. GUI y consulta «Moonlight Sonata» con **20 resultados**.
  Favoritos preservados, sesión pausada, sin autoplay. Navegación completa
  dentro del viewport 384×658. Evidencia `/tmp/doremi-product-android-6bbc/`;
  APK SHA-256 `1caab6cdb906d8b64715a09b518aa629cd4ab977f874dc60cb14a37c9dfc37c4`.
- Windows `6bbcfe3`: runner nativo pasó tests/build/CLI del nuevo `doremi.exe`.
  Artifact descargado: manifiesto/hash y PE x64 verificados, imports examinados.
  SHA-256 `c26d91187effe1d12868ad925bca88de554a75b1ec0e775631aa0bdae4653114`;
  evidencia `/tmp/doremi-product-windows-6bbc/`. Sin prueba física Windows.

## Alcance y limitaciones

MusicBrainz entrega **metadatos, no audio ni carátulas**. No se convierten sus
IDs en búsquedas de otro sitio. Demo y los dos tonos Commons CC0 permanecen
como fuentes separadas y explícitas. Commons aún descarga el recurso antes
de decodificar: no es streaming progresivo. No se añaden formatos locales.

La recuperación visual completa sigue pendiente: tablas, shelves, headers
de álbum/artista/playlist, artwork, transiciones y demás capacidades del roadmap.
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

26 tests del host y 3 del parser pasaron; también Clippy del host, circuito
ligero, 7 tests de tooling y 27 de políticas. La geometría cubre ahora el tamaño
384×332 observado con IME y rechaza solapamientos en controles de transporte.
La compilación fría del test tomó 15 min 28 s; no es latencia de uso.

CI completa [`41923094028`](https://github.com/Eirom16/spotifast/actions/runs/41923094028),
SHA `6bbcfe3`: quick, tests completos, core/APK Android y build Windows pasaron.
Clippy detectó un import usado solo por tests en el legado: corregido con
`cfg(test)`, pendiente de repetir en CI. macOS fue cancelado; preview/QA Linux pasó. El gate agregado falló.
No se presenta el conjunto como verde.

APK `af95d8d` mostró overflow de Ajustes; la corrección ya está comprobada
físicamente en `6bbcfe3`. Con IME se detectó además intersección de volumen/play;
se prepara barra compacta con menú de extras, comprobada por tests, pendiente
verificar en la nueva APK. Los drivers conservan las aserciones y usan
ahora el rectángulo completo del slider heredado y el área visible de la página.

Se prepara smoke Windows de ventana/captura y audio nativo del runner, con
perfil aislado y resultados separados; aún no ejecutado. El empaquetado genera
checksums de ejecutable/licencias y el instalador acepta el nombre declarado
`doremi`, preservando el alias anterior. Pendiente artifact/instalación Linux.

Siguiente: CI del nuevo corte, descargar/verificar artifacts e instalar Linux
y APK; cerrar GUI/audio y compactación móvil del mismo código. Después,
seguir extrayendo las vistas reales y conectar transporte progresivo autorizado.
