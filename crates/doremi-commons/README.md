# Adaptador Commons experimental

Transporte async compartido, extraído del experimento ejecutable después de
verificar los dos tonos CC0 reales. Conserva la allowlist de corpus, IDs,
licencia, hashes, HTTPS/hosts, límites, agente transparente y errores tipados.
No depende de egui, audio, Spotify, credenciales, procesos o un backend propio.
El núcleo sigue sin dependencias y decide tickets, selección y leases.

Crear el cliente en un worker: cargar certificados del sistema puede acceder
a disco. Búsqueda/resolución/descarga son futuros; el host debe cancelarlos al
reemplazar una intención y descartar completions por ticket/generación.
`download` verifica formato, lease, ausencia de headers de autenticación,
identidad auditada, tamaño y hash antes de devolver bytes al decoder separado.
No declara autorización para descargar todos los resultados CC0 de búsqueda.

El CLI `tools/provider-live-probe` consume este mismo adaptador. El subproceso
yt-dlp permanece exclusivamente en ese laboratorio y no forma parte de la
biblioteca. Aún faltan worker/UI y pruebas de su cancelación/lifecycle reales;
extraer un futuro no acredita por sí solo esas pruebas.
