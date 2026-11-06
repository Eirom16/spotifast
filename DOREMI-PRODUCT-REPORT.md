# Doremi — avance de producto

Rama feature/doremi-product-ui. Objetivo completo abierto; sin release, merge a
main ni cambios a upstream. Informe previo y skills locales del usuario conservados.

## Funcionalidad disponible

- UI neutral que reutiliza paleta, Inter, iconos, sidebar, filas, sliders, player y
  cabeceras de colección del legado. Inicio, Buscar, Biblioteca, Cola y Ajustes
  comparten Queue/Engine. La migración visual completa sigue abierta.
- MusicBrainz público: grabaciones, artistas, ediciones, canciones de álbum y
  grabaciones de artista, paginación/caché acotadas y guardado por ID. Solo
  metadatos, sin audio/artwork y sin sustituir grabaciones por otra búsqueda.
- **Commons ampliado en 5aa9fbf**: búsqueda real de hasta diez resultados CC0 con
  fuente Own work, un stream Vorbis, 1–2 canales/8–192 kHz, hasta 10 min/32 MiB.
  Títulos de tags cuando existen, nombre de archivo como fallback; duración real.
  No inventa intérprete ni portada. Sin paginación Commons todavía.
- Cada selección o reapertura resuelve otra vez el ID. Grant opaco de diez minutos
  ligado a URL exacta, tamaño y SHA-1; no se guarda en biblioteca. Rangos HTTPS
  exigen 206/longitud/identidad de objeto y ETag coherente. Esta identidad sobre TLS
  no equivale a hashes por bloque; la selección auditada de Chopin por Membeth
  **conserva sus hashes por bloque**. Los dos tonos auditados conservan su ruta.
- Primer PCM sin esperar al archivo completo, Vorbis incremental y Feed/CPAL
  existentes. Pausa, seek, siguiente/anterior, volumen, buffering, cancelación y
  caducidad. Setup Vorbis limitado antes de construir el decoder. UI/callback
  libres de red/decodificación; errores visibles, sin retry ni método alternativo
  ante una negativa. Demo continúa etiquetada y sin audio.
- Biblioteca y playlists propias: guardar/quitar, CRUD, duplicados ordenados,
  selección de la aparición concreta, mover/quitar y confirmación de eliminación.
  Editar no cambia favoritos ni la cola activa. Snapshots y JSON v1 privado/atómico
  fuera de UI; documentos futuros conservados. Límite inicial 32 listas/256
  entradas totales; no cierra el gate de 10.000 ni el historial completo de F3b.
- Móvil: navegación por iconos, mini-player condicional, player dedicado y
  composición horizontal, filas compactas/menús, Atrás al origen y editor Android
  con destino/generación. Temas claro/oscuro y morado/verde. Android pausa al salir;
  MediaSession y reproducción en segundo plano siguen pendientes.

## Plataformas y artefactos verificados

[CI 42131761479](https://github.com/Eirom16/spotifast/actions/runs/42131761479),
**5aa9fbf: los tres jobs aprobados**. Tests y Clippy de los paquetes afectados,
Rustdoc, GUI Linux, nuevo ejecutable Windows nativo y ambas ABI/APK Android.
La regresión completa del legado más reciente es 42086375132/3201c4b, verde;
no se atribuye esa ejecución completa a este corte. El legado no cambió aquí.

| Plataforma | Evidencia de 5aa9fbf | Límite |
| --- | --- | --- |
| Linux/CachyOS | Artifact instalado; manifiesto/checksums y versión verificados. Recorrido GUI X11 con Mendelssohn 17687586, fuera de la lista fija, y callback nativo positivo. | Sin captura acústica. La ventana normal Wayland abierta antes sigue en d56ac45; launcher actualizado a 5aa9fbf. |
| Android SM-J701M/API 28/ARMv7 | APK instalada y hash del paquete instalado comprobado. Búsqueda/IME, claro/oscuro, guardar, pausa/seek/reanudar, Atrás, recreación y landscape; playlists y reapertura nativa sin buscar otra vez: PASS físico. | ARM64 verificado en paquete/CI, no en hardware; sin captura acústica. |
| Windows x64/MSVC | Nuevo doremi.exe compilado/testeado en runner; checksums, PE64, versión y manifiesto descargados/verificados. | GUI smoke: OpenGL insuficiente; audio: NoDevice. Sin Windows físico; no se declara playback probado. |

SHA256 Linux: 43a09932a2acc98afd4a9a058f9809b61903c48cc2bcc59a6113b5dcef3d408e.
APK: e6218b204c76c9475ced4d8ac32ffffc7df3fd89972440789ffd698a00f189ad.
Windows: 31bcbab948160b877566a554ede5f76776fcb3c98c344e023e2e0bd078852ac7.

Linux y Android observaron PCM con **196.490/3.342.218 B** recibidos de Mendelssohn.
Linux también pasó siguiente a otro recurso CC0, anterior, cancelación, proceso
nuevo y fin Vorbis natural tras seek cerca del final: Stopped, sin current/audio/
red activos. Cierre por WM_DELETE_WINDOW: exit 0; dos procesos observados y ninguno
huérfano. Máximo RSS muestreado 136.468 KiB, 19 threads/40 FD; es una observación,
no una cota de memoria. PCM 65.536 B; buffers comprimidos propios 262.146 B,
además de HTTP/TLS/decoder. La biblioteca normal Android mantuvo su hash exacto
b5d5ead52096157402aa54f217bf93672acabd395bea418352135657b29f7918.

Se conservan los fallos previos: timeouts/429 y geometría; CI 42095842479 falló
por sync de directorio Unix aplicado en Windows y timeout APT previo a tests.
Corregidos en e0d1744/d56ac45. La prueba de cierre inicial usó una petición al
window manager ausente en Xvfb; otra expectativa del driver confundía EOF con
Paused. Se corrigieron los controladores conforme al protocolo y al contrato
Stopped existente, sin cambiar el núcleo ni borrar los fallos.

## Siguiente corte en validación

Cola con las mismas filas de 56 pt: actual separada, futuras por aparición y
menús para subir/bajar/quitar; vaciar conserva current/context/favoritos. Cuerpo
estático para las entradas futuras; abrir el player desde la actual. Targets de
menú 48 pt. Player vacío compartido con acceso real a Buscar. Aviso de pausa en
Android horizontal reservado bajo la carátula, con límites comprobados en tests.

Tests/Clippy/Rustdoc/build locales y Java aprobados. GUI de cola/duplicados y
playlists aprobada en escritorio y 360 px; el build final pasó a 393 px con Chopin
real, incluida la ruta auditada y restauración sin búsqueda. Capturas antes/después
360/393/430 px y claro/oscuro revisadas. La nueva Cola y la corrección del aviso
horizontal necesitan CI/APK y prueba física; **no son el recorrido de 5aa9fbf**.
Se conservaron dos fallos de QA local que compartieron accidentalmente el display
X11; las repeticiones aisladas pasaron y el driver ahora comprueba el foco.

Evidencia fuera del árbol: /home/eirom/.local/share/doremi/product-qa/2026-11-05/.
Incluye ci-5aa9fbf/{linux-mendelssohn-final,android-mendelssohn,windows}/,
queue-{before,after}-layouts/, queue-ui-{phone360,desktop}-isolated/,
queue-player-phone393/ y queue-player-final-checks/. No se repitieron suites
físicas extensas de formatos locales ni se añadieron formatos/importadores.

## Pendiente del objetivo completo

Cerrar el corte de Cola/player en las tres plataformas. Ampliar navegación y
artwork, historial, cuenta opcional compatible, EQ/DSP/visualizadores, integración
multimedia y background Android. Audius sigue investigado, sin integrar; sus
condiciones de caché/sesión y acceso requieren adaptación. Sin integración YouTube
ni promesa de acceso universal. El inventario del legado y los gates completos
permanecen en el roadmap; no hay release estable ni objetivo completado.
