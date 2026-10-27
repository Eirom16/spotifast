# Doremi: roadmap de migración

## Estado y objetivo

F0 prepara el fork Eirom16/spotifast. La aplicación preservada todavía es
Spotifast 0.14.2, con Spotify/librespot. No hay reproducción yt-dlp ni APK.
El resultado verificable, los checks fallidos/no ejecutados y los riesgos
están en `F0-REPORT.md`. Una fase no se acepta solo porque exista su código.

Objetivo: reproductor nativo Rust, egui/eframe, sin Spotify obligatorio,
búsqueda/resolución yt-dlp, escritorio y Android con el mismo núcleo.
Sin Electron, motores de navegador, backend hospedado obligatorio ni evasión
de DRM o controles de acceso. No hay sustitución de audio Spotify: los nuevos
medios tienen identidad y procedencia propias, elegidas por el usuario.

## Decisión de arquitectura

Se elige una migración por cortes verticales, no reescribir todo ni mover
130 mil líneas de una vez. El legado se ejecuta y prueba por separado hasta
que cada reemplazo cumpla su contrato. Conservar compilación no equivale a
haber conservado comportamiento.

### Workspace objetivo

| Paquete propuesto | Responsabilidad y límites | Dependencias internas |
|---|---|---|
| `doremi-core` | IDs neutrales con namespace de proveedor, metadatos, intenciones/eventos, errores, máquina de cola y estado. Sin I/O, UI, SO ni secretos. | Ninguna |
| `doremi-catalog` | Casos de uso de búsqueda y resolución, capabilities, caché temporal, cancelación y generaciones. URL de página estable separada del recurso reproducible efímero. | core |
| `doremi-player` | Transporte, buffering acotado, decodificación, reloj, seek, DSP, eventos y puertos de salida. Sin catálogo ni UI. | core |
| `doremi-library` | Biblioteca, playlists propias, historial, repositorio versionado y migraciones. No conserva URLs firmadas como IDs. | core |
| `doremi-ui` | Vistas egui, estado de presentación y emisión de intenciones. Sin procesos, descargas, disco ni decodificación. | core |
| `doremi-adapter-ytdlp` | Protocolo JSON, supervisor, límites, traducción de errores y selección de formatos soportados. | core, catalog |
| `doremi-platform-desktop` | Procesos, rutas, selección de archivos, keyring, audio, notificaciones, media keys, tray. Expone capacidades, no finge soporte móvil. | core, puertos player/library |
| `doremi-platform-android` | JNI/Activity, contexto Android, SAF/MediaStore, audio focus, MediaSession/servicio, lifecycle, almacenamiento y runtime empaquetado. | core, puertos player/library |
| `doremi-desktop` | Composición eframe y workers. Único entrypoint nuevo de escritorio. | UI y servicios/adaptadores |
| `doremi-android` | `cdylib` + proyecto Gradle mínimo, composición eframe/Activity, sin duplicar reglas del núcleo. | Misma UI y servicios, adaptador Android |
| `spotifast` (legado temporal) | Paquete raíz actual, congelado funcionalmente; no es dependencia de los nuevos paquetes. | Árbol existente |

**F0 solo crea `doremi-core`, con constantes de identidad y un doctest.**
La tabla es diseño, no lista de módulos implementados. Los puertos y crates
restantes nacen con su primer caso de uso/test, no como APIs especulativas.
En F1 se centralizará versión/MSRV una vez adaptados los lectores de manifiesto
de Nix, packaging y tests. F0 conserva la versión literal raíz para no romperlos
y un test rechaza divergencias con el núcleo.

Regla de dependencia: composición → servicios/adaptadores → core.
La UI recibe snapshots y envía comandos; no conoce el motor ni yt-dlp.
Las traits de puertos viven junto al consumidor; ningún adaptador inyecta
tipos Spotify o JNI al dominio. Tests con adaptadores falsos en ambos hosts.

### Concurrencia y contratos antes de implementar

- egui consume eventos sin espera y emite intenciones mediante `try_send`.
  Backpressure es visible/coalescible, no un `send().await` ni un
  `block_on` en UI. Canales de comandos/eventos con capacidad explícita.
- Runtime Tokio para red, subprocess pipes y timers. Workers separados
  acotados para lectura de tags, imágenes, disco y decodificación bloqueante.
  El audio callback solo consume PCM ya preparado de un buffer acotado.
- Cada búsqueda/resolución lleva ID de solicitud y generación. Cancelar,
  borrar búsqueda o cambiar pista invalida respuestas tardías. Un timeout
  cancela y recoge el proceso y sus hijos, no solo deja de esperar el future.
- Drenar stdout y stderr concurrentemente, con límites independientes.
  Errores tipados: binario/runtime ausente, formato incompatible,
  red/deadline, cancelación, medio no disponible, acceso denegado y fallo
  del extractor. No reintentar denegaciones indefinidamente.
- Una referencia estable al medio no es una URL de CDN. El recurso resuelto
  incluye protocolo/formato, headers permitidos, capacidad de seek y
  caducidad cuando se conoce. Resolver cerca del inicio de reproducción.
  No loguear URLs firmadas, cookies o respuestas enteras.
- Persistir IDs/metadatos y posición, no sesiones Spotify ni secretos en
  JSON. Cambios de esquema con backup, versión y tests de rollback.

### Preservación y sustituciones

| Capacidad heredada | Dependencias identificadas | Estrategia y prueba requerida antes de retirar |
|---|---|---|
| Cola, next/previous, shuffle/repeat, duplicados, orden y resume | app/model, tipos API, Spirc y session.json | Extraer máquina pura con IDs de ocurrencia, tests de contratos existentes y replay del mismo escenario en desktop/Android. |
| EQ, limiter, resample, espectro y osciloscopio | sink, rodio Source, librespot Sink, AudioTap | Conservar DSP, separar PCM de Spotify. Fixtures de impulsos/silencio/seek y señal post-EQ/pre-volumen. |
| Búsqueda, catálogo y artwork | API/gateway/session_reads, auth, images y modelos Spotify | Proveedor yt-dlp explícito; thumbnails y resultados parciales, cancelación y errores sin login Spotify. |
| Playlists, favoritos e historial | API, liked/history/account_cache/settings | Biblioteca propia versionada. Importación explícita de metadatos si se aprueba, nunca equivalencias automáticas por título. |
| Archivos locales, tags, carátulas y letras | lookup `spotify:local:` en librespot, symphonia/local_tags y lyrics | Fuente de archivo neutral. Desktop paths y Android content URIs/FD, sin exigir Spotify. |
| UI, navegación, teclas, i18n, bidi, skins y accesibilidad | app/model, fastframe, assets | Reutilizar vistas gradualmente con view models neutrales, headless/demo y evidencia visual. Gestos/touch aparte, no cambiar escritorio silenciosamente. |
| MilkDrop | projectm-sys/CMake/C++/GL, proceso hijo, memoria compartida | Mantener desktop opcional. Sin promesa Android; decidir tras medición de energía/GPU y lifecycle. |
| Tray, autostart, IPC, menús, notch y thumbbar | fastframe + APIs SO | Adaptador desktop; Android usa integración nativa distinta. Ausencia declarada como capability. |
| Letras | Spotify, LRCLIB, tags y caché | Parser/tag local reutilizable; acceso LRCLIB opcional, explícito y documentado, nunca backend Doremi obligatorio. |
| Spotify Connect, Cast y Sonos actuales | sesiones/tokens Spotify, zeroconf y protocolos de receptor | No son streaming genérico de yt-dlp. Conservar en legado; no prometer paridad. Gate de retirada con decisión documentada; no inventar reemplazo. |
| Radio, Made for you, amigos, releases, podcasts y guardados Spotify | APIs privadas/session_reads y perfil autenticado | Datos exclusivos sin equivalencia garantizada. Conservar legado, definir sustitución local o exclusión aprobada antes de quitar vistas/datos. |
| Actualización, packaging, protocolos y perfiles | IDs Spotifast, upstream releases, clave pública y recetas | Separación obligatoria antes del primer binario Doremi distribuible; fixtures de actualización y migración sin tocar perfiles originales. |

## Fases, dependencias y criterios medibles

### F0: preparar y demostrar el baseline

Sin dependencias previas.

- [x] Inventario estructural, fuentes de dependencias y mapa de acoplamientos.
- [x] Normas Doremi, MIT/atribuciones preservadas y separación documental.
- [x] Workspace mínimo, sin borrar ni modificar funciones del legado.
- [x] CI de validación Linux y core Android sin permisos de publicación.
- [ ] Todas las comprobaciones requeridas verdes y ejecución remota de CI.
  Consultar resultados reales en el informe, no asumir este gate cumplido.

Aprobación F0: comandos de `CONTRIBUTING.md` verdes o bloqueo claramente
identificado y resuelto, aplicación baseline compilada, fuente externa sin
cambios, core portable compilado, cero releases. No declarar terminado el
gate de baseline si no se llegó a ejecutar su suite.

### F1: extraer contratos y una cola neutral

Depende de cerrar el gate F0.

1. Caracterizar la cola actual con escenarios de duplicados, reordenación,
   clear manual, contexto, cambio de pista y restauración.
2. Extraer tipos neutrales y reducer de comandos/eventos en core, mantener
   bridge de legado. Separar view models del API Spotify.
3. Introducir composición de desktop propia, sin migrar perfiles por defecto,
   con modo de pruebas/fake y sin importar `spotifast` como dependencia.
4. Centralizar versiones y adaptar lectores de Nix/packaging conscientemente.

Aceptación: mismos vectores deterministas ejecutados en host y núcleo;
0 dependencias Spotify, egui, JNI o SO en core; fallos/respuestas obsoletas
no alteran la intención actual; builds legados siguen verdes. Cero bloqueos
de I/O en la UI nueva, canales con capacidad probada.

### F2: yt-dlp y prueba temprana de viabilidad Android

Depende de F1 para contratos. Desktop y spike Android pueden avanzar en
paralelo, pero ambos deben cerrar antes de comprometer un lanzamiento móvil.

- Implementar supervisor desktop sin shell: executable validado, argv fijo,
  `--ignore-config`, `--no-plugin-dirs`, `--no-remote-components`,
  `--skip-download`, JSON y límite de resultados. Separar búsqueda plana y
  resolución; metadatos de búsqueda pueden estar incompletos.
- Sin argumentos arbitrarios del usuario, `--exec`, configuración/plugins
  ambientales ni importación automática de cookies. Validar esquemas y
  hosts admitidos, rechazar `file:` y recursos de red local no autorizados.
  No asumir que flags estándar convierten todo extractor en uso permitido.
- Fakes ejecutables prueban timeout, cancelación, pipes llenos, JSON inválido,
  salida enorme, exit no cero y ruta ausente. Suites sin internet.
- Validación real separada, manual, con medio público propio/autorizado:
  búsqueda → resolución JSON → formato usable. Registrar versión yt-dlp,
  runtime JS, sistema y resultado, sin secretos.
- Spike Android: eframe de la revisión usada, GameActivity/IME, Gradle,
  cargo-ndk, un ABI arm64, Python/yt-dlp y runtime JS empaquetados. Comparar
  bridge Python embebido frente a ejecutable Android-compatible empaquetado.
  Elegir por prueba, licencias, ABI y restricciones de ejecución, no por
  suponer que un binario Linux aarch64 funciona en Bionic.

Aceptación propuesta para supervisor: ≤2 procesos activos, 20 resultados por
búsqueda, 30 s de deadline, stdout ≤4 MiB y stderr retenido ≤64 KiB,
cancelación con recogida del proceso ≤2 s. Valores iniciales, ajustar con
evidencia antes del merge. Tests verifican límites, no solo happy path.
Android: APK de prueba instalable con búsqueda/resolución real y teclado en
dispositivo o emulador, sin Termux, Python del sistema ni servicio externo
obligatorio. Si no es viable legal/técnicamente, se bloquea Android y se
documenta, no se crea un segundo núcleo ni un backend alojado para ocultarlo.

### F3a: reproductor independiente

Depende de F1; medios de yt-dlp dependen de F2.

Crear motor con fuente local y HTTP autorizada, symphonia/rodio/salida por
puertos, sin Spirc. Seleccionar formatos que realmente se pueden decodificar,
no descargar todo ni transcodificar por defecto. MP3/FLAC no cubren por sí
solos todos los contenedores AAC/Opus/WebM/MP4 que devuelve yt-dlp.

Aceptación: fixture de audio con play/pause/seek/next/fin/error, EQ/volumen y
resume; buffers acotados incluso con red lenta; limpieza tras cancelar;
prueba auditiva y medición en Linux y Android; focus loss, desconexión de
auriculares y cambio de salida no dejan audio huérfano. Reproducir con
Spotify ausente y sin sus credenciales.

### F3b: biblioteca independiente

Depende de F1; puede ir en paralelo con F2/F3a.

Playlists, favoritos, historial, metadatos y persistencia propios. Decidir
JSON versionado frente a SQLite con volumen de datos/consultas medidos,
no añadir la dependencia solo por anticipación. SAF/FD para Android.

Aceptación: CRUD y persistencia tras reinicio, duplicados preservados,
migración/rollback de fixtures, escrituras atómicas fuera de UI, biblioteca
de 10.000 entradas sin trabajo de disco en un frame. No modificar ni
importar automáticamente el perfil Spotifast.

### F4: integrar producto desktop/Android compartido

Depende de F2, F3a y F3b.

UI neutral con catálogo → cola → reproducción → favoritos/resume. Adaptar
shell y capacidades por plataforma; conservar vistas/DSP/skins viables.
MediaSession y foreground service Android para background playback:
destruir la Activity no destruye ni duplica la cola.

Aceptación: flujo completo sin login Spotify, misma secuencia de comandos
con mismo estado de dominio en ambos hosts; no congelación con proveedor
simulado bloqueado 10 s; objetivo p95 frame ≤16,7 ms en hardware documentado
y ninguna espera de red/proceso/disco en UI. Capturas claro/oscuro a tamaños
desktop estrecho/normal y teléfono, lector de pantalla, IME, rotación,
background/foreground y process death probados.

### F5: retirar el legado y separar identidades

Depende de F4 y decisión explícita de paridad/exclusiones de la tabla.

Quitar fuentes Spotify solo cuando ningún nuevo paquete ni test las importe.
Separar binario, app ID, keyring, paths, URL schemes, desktop/MPRIS IDs,
Info.plist/MSI/Flatpak, autostart, caché, updater y claves Doremi. El nuevo
producto no usa la clave ni releases de crmne como canal de actualización.
Conservar MIT, crédito e historial.

Aceptación: `cargo tree` del producto sin librespot/Spotify, fixture sin
credenciales, 0 peticiones a endpoints Spotify durante el flujo completo;
instalar al lado de Spotifast no altera sus archivos ni IPC; datos no
migrables permanecen recuperables; matriz desktop y Android verde.

### F6: empaquetado y distribución verificada

Depende de F5 y del gate Android si se ofrece Android.

Construir Linux y APK/AAB desde un checkout/SHA/versión únicos, toolchains y
dependencias externas fijados. SBOM/licencias de Rust, Python, yt-dlp,
runtime JS y FFmpeg si se distribuye. Checksums y firma por identidad nueva.
No confundir compartir versión con reproducibilidad bit a bit.

Aceptación: artefactos descargados e instalados en entornos limpios,
hashes/versión/commit verificados; pruebas offline tras instalación, smoke
online autorizado, actualización y desinstalación; ARM64/NDK/API/16 KiB de
página comprobados; permisos y políticas de tienda revisados; CI completa
en el commit de versión. Publicar requiere autorización específica aparte.

## CI: hoy y evolución

F0 activa solo `.github/workflows/ci.yml`. Linux ejecuta políticas, tests de
packaging, fmt/clippy/tests default/all-features y rustdoc, luego construye el
ejecutable legado de debug. Android prueba core en host y genera una `.rlib`
para `aarch64-linux-android`, sin requerir NDK porque no enlaza un binario.
Cada payload lleva JSON con SHA, versión, target, tipo, estado dirty y SHA-256.
Un job dependiente de ambos rechaza discrepancias o fuentes sucias antes de
agruparlos. Son artefactos de validación, no instaladores Doremi.

F1/F2 añade compilación real del shell Android y Gradle/NDK fijados; F3/F4,
tests instrumentados y dispositivo. Recuperar macOS/Windows/Nix de la CI
histórica sin sus destinos de publicación. No declarar soporte retirado por
haber reducido la matriz activa durante F0: queda como brecha de cobertura.
Reactivar Pages, triage, releases, AUR/Homebrew o firma exige revisión separada.
