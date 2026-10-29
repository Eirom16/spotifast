# Experimento acotado de proveedores

Herramienta CLI experimental, separada del host UI/audio. Usa los contratos
`Catalog`, tickets, IDs neutrales, errores, formatos y leases de `doremi-core`.
No añade acceso de red al APK ni anuncia un proveedor definitivo.

```sh
doremi-provider-live-probe sine 21923157
doremi-provider-live-probe --yt-dlp target/provider-tools/yt-dlp-2026.10.26/yt-dlp sine 21923157
```

La búsqueda real usa la API pública de Wikimedia Commons en ambas variantes.
yt-dlp no ofrece búsqueda de Commons: solo se comparan resolutores del mismo
original explícitamente seleccionado. El formato fuente Ogg/Vorbis se valida
por cabecera, tamaño y SHA-1 publicado antes de conservarlo bajo `target/`.
El corpus permite descargar únicamente dos tonos sintéticos declarados propios
y CC0 por Ubershmekel; `corpus.json` conserva las páginas primarias/licencias.
Encontrar una licencia en una búsqueda no amplía esta autorización de descarga.

El transporte HTTPS tiene allowlist de hosts/puerto, timeout de ocho segundos,
límites de respuesta de 1 MiB JSON y 256 KiB audio, sin cookies, credenciales,
reintentos ni cambio de identidad tras denegaciones. Un agente descriptivo
identifica el experimento según la política de Wikimedia. Las URIs de transporte
solo viven en memoria; los informes contienen IDs, títulos y hashes.

El resolutor alternativo requiere el zipapp oficial yt-dlp 2026.10.26
(Unlicense), con SHA-256 fijado y comprobado antes de ejecutarlo mediante Python
aislado. Desactiva configuración, plugins, caché, cookies del navegador, JS,
componentes remotos y X-Forwarded-For. Se limita al extractor Wikimedia y al
formato `source`; no usa perfiles YouTube, suplantación, cuentas ni evasión.
La salida del subproceso se drena con límites y se mata/espera tras ocho segundos.

Este proceso bloqueante es un laboratorio fuera de la UI, no un adaptador listo
para incorporarse a la app. La cancelación del experimento debe terminar su grupo
de procesos; la cancelación por tickets del futuro worker aún requiere integración.
El contrato descarta respuestas expiradas, pero eso no cancela por sí solo el IO.

Los tiempos separan búsqueda, resolución, primer byte y transferencia completa.
`first_pcm_ms` y audio audible permanecen nulos: esta herramienta no decodifica
ni reproduce. CPU/RAM, frío/caliente, cancelación y decodificación común se miden
externamente, sin comparar una resolución con el reproductor completo de otro.
No existe todavía evidencia equivalente sobre búsqueda o resolución de YouTube.

Referencias primarias:
[Imageinfo](https://www.mediawiki.org/wiki/API:Imageinfo),
[agente Wikimedia](https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy),
[yt-dlp 2026.10.26](https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26).
