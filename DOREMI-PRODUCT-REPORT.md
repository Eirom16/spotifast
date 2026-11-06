# Doremi — avance de producto

Rama feature/doremi-product-ui. Objetivo completo abierto; sin release, merge a
main ni cambios a upstream. DOREMI-GOAL-REPORT.md y skills del usuario conservados.

## Disponible y verificado

- UI neutral que reutiliza paleta, Inter, iconos, sidebar, filas, sliders, player y
  cabeceras del legado. Inicio, Buscar, Biblioteca, Cola, playlists y Ajustes
  comparten Queue/Engine; la migración visual completa continúa.
- MusicBrainz: grabaciones, artistas, ediciones, canciones de álbum y grabaciones
  de artista, paginación/caché acotadas y guardado por ID. Solo metadatos.
- Portada frontal CAA por MBID exacto de edición: 250 px, worker cancelable,
  decode/escalado extraídos del legado y caché en memoria de ocho imágenes/ocho
  texturas, hasta diez minutos de reutilización. no-store/no-cache se respetan.
  HTTPS/hosts permitidos, saltos/bytes/tiempos/cooldown acotados. CPU retenida y
  texturas nuevas tras recrear Activity. No habilita audio ni inventa carátulas
  para una pista Commons. Artistas/grabaciones sin imagen mantienen placeholder.
- Commons: hasta diez resultados CC0 declarados por su autor/Own work, Vorbis
  hasta 10 min/32 MiB, 1–2 canales/8–192 kHz. Resolución fresca por ID al tocar o
  reabrir; grant opaco de diez minutos ligado a URL/tamaño/SHA-1, nunca persistido.
  Rangos 206/longitud/identidad de objeto/ETag coherentes sobre TLS; no son hashes
  criptográficos por bloque. Chopin/Membeth y dos tonos auditados conservan sus
  comprobaciones especiales. Sin sustitución Spotify→búsqueda de otro servicio.
- Feed/CPAL/Symphonia existentes: PCM antes del archivo completo, pausa, seek,
  siguiente/anterior, volumen, buffering, cancelación, caducidad y EOF. UI/callback
  sin red/decode pesado. Setup Vorbis limitado antes de construir el decoder.
  Demo sigue etiquetada y sin audio. Commons aún no tiene paginación.
- Biblioteca/playlists propias: guardar/quitar, CRUD, duplicados ordenados,
  selección de aparición, mover/quitar, confirmación de borrado, JSON v1 privado
  y atómico fuera de UI. Editar conserva cola/favoritos. Límites iniciales:
  32 listas/256 entradas; el gate de biblioteca de 10.000 sigue abierto.
- Cola en filas de 56 pt, actual separada y futuras por aparición, menús de 48 pt,
  vaciado sin alterar current/context/favoritos. Mini-player condicional, player
  dedicado, Atrás al origen, claro/oscuro y morado/verde. Aviso horizontal respeta
  escala física 1,875. Editor nativo mantiene nombre/Guardar visibles al abrir IME.
  Android pausa al salir; MediaSession/background pendientes.

## Evidencia por plataforma

[CI enfocada 42228990808](https://github.com/Eirom16/spotifast/actions/runs/42228990808),
562a5ba: Linux, Windows y APK aprobados. La
[CI completa 42142254641](https://github.com/Eirom16/spotifast/actions/runs/42142254641)
de f641177 terminó verde en todos los jobs: workspace/default/all-features,
Clippy/Rustdoc, legado, Preview y plataformas. No se atribuye esa CI completa a
los cambios nuevos de tarjetas/historial de este worktree.

| Plataforma | Comprobado | Límite |
| --- | --- | --- |
| Linux/CachyOS | f641177 instalado; SHA/manifest/version, GUI X11 de portada real, caché y cierre normal exit 0. Arranque Wayland normal, PID 2091 al verificar; biblioteca/playlists con hashes idénticos. Artifacts 562a5ba descargados. | La ventana normal arranca en f641177; sin captura de esa ventana Wayland. Sin captura acústica. |
| Android SM-J701M/API 28/ARMv7 | f641177: portada The Slip, claro/oscuro, caché y Activity recreada: PASS físico. 562a5ba: música CC0/PCM, IME, guardar, pausa/seek, mini-player, Cola/duplicados, landscape, playlists/renombrado y proceso nuevo/reproducción sin buscar: PASS físico. APK instalada/hash comprobado. | ARM64 comprobado en CI/paquete, sin hardware. Sin captura acústica. |
| Windows x64/MSVC | Nuevo doremi.exe 562a5ba compilado/testeado nativamente, descargado; manifest, SHA y PE64 comprobados. | Smoke GUI: OpenGL insuficiente; audio: NoDevice. Sin Windows físico ni playback declarado. |

Linux f641177: a8a6f6bcb22e5bf81e9eacb007c781ba9b63b50504c5ca196451ecae21fd2c76.
APK 562a5ba: 3136cd79d177842cf0a5ac396394ede44da3e3a134f144d023f021caa0ef2e52.
La biblioteca habitual Android conservó b5d5ead52096157402aa54f217bf93672acabd395bea418352135657b29f7918.
Chopin dio PCM con 197.167/4.260.399 B recibidos. La prueba anterior 5aa9fbf con
Mendelssohn 17687586, fuera de la lista fija, dio PCM con 196.490/3.342.218 B;
Linux también pasó EOF natural, cancelación, siguiente/anterior y reapertura.
PCM 65.536 B; buffers comprimidos propios 262.146 B más HTTP/TLS/decoder.
RSS Linux muestreado en aquella prueba: 136.468 KiB; observación, no cota global.

Se conservan fallos y correcciones: sync de directorio Windows/APT, cierre X11
con controlador equivocado, displays compartidos, aviso horizontal 0,28125 pt
fuera de página y Guardar con IME. 562a5ba corrigió este último y pasó la suite
física completa. Una regresión del runner generalizado de carátulas recibió
Timeout de MusicBrainz el 6/11; error visible y biblioteca preservada, sin PASS
fabricado ni ampliar límites. La portada ya tiene PASS físico previo en f641177.

## Corte de historial en preparación

Escuchas reales → tarjetas recientes en Inicio → Historial → reproducir otra vez
→ restaurar tras proceso nuevo. Comparte la regla heredada de 30 s o media pista
corta (mínimo 1 s); cuenta frames de audio entregados, excluyendo pausas,
buffering y desplazamientos de seek. Cada escucha separada permanece; Inicio
muestra hasta 16 canciones únicas por ID cualificado. 500 escuchas/JSON v1 hasta
2 MiB, snapshots O(1), writer existente, privado/atómico y formatos futuros en
solo lectura. Confirmar vaciado conserva favoritos/playlists/cola y suprime la
reinserción de la escucha actual. No se importan perfiles Spotify ni URLs efímeras.

Tarjetas, sombras y filas virtuales extraídas de los widgets reales; el legado
conserva covers, RTL, traducciones, menús y autoscroll. Historial solo mide filas
visibles. Prueba local con 500 registros, 360/393/430 pt/escritorio, escala 1,875 y
ambos temas. Tests lib: app 36/core 3/library 22/MusicBrainz 5/bitmap 1, aprobados;
Clippy estricto, Rustdoc, build, Java y check-local aprobados.

GUI Linux del corte: música Chopin CC0, 30,0755625 s de salida nativa sumando los
dos requests alrededor del seek; guardar historial sin favorito, cancelar vaciado,
reiniciar sin IO automático, reproducir desde reciente sin buscar y vaciar
conservando cola/favoritos: PASS. Dos cierres WM_DELETE_WINDOW, exit 0. Capturas
360/393/430/1180 y ambos temas revisadas; los tamaños adicionales usan el snapshot
real del recorrido, sin volver a reproducir. Android UI/restore preparados, pero
este historial necesita CI, regresión del legado y prueba física de su propia APK.

## Evidencia y trabajo pendiente

Evidencia fuera del árbol: /home/eirom/.local/share/doremi/product-qa/2026-11-05/
y 2026-11-06/: ci-f641177/, ci-562a5ba/, history-{final393,layout-*}/,
history-before-legacy-preview/. La imagen The Slip de QA tiene atribución Rob
Sheridan/Trent Reznor y CC BY-NC-SA 3.0 US junto a las capturas; no se incluye en assets.

Cerrar historial en las tres plataformas; continuar navegación, cuenta opcional
oficial compatible, EQ/DSP/visualizadores, letras, integración multimedia, tray y
atajos, background Android y matriz de releases. Audius investigado, sin integrar;
sus condiciones requieren adaptación. Sin integración YouTube ni promesa de
acceso universal. El inventario/gates completos siguen en DOREMI-ROADMAP.md.
