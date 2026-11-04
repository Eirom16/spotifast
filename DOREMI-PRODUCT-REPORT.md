# Doremi — avance de producto

Rama de producto: `feature/doremi-product-ui`, creada desde `ea71b92` tras
consultar el remoto. El informe local anterior se conserva sin incorporarlo
a los commits de implementación.

## Corte actual: presentación compartida (en validación)

- `doremi-ui` extrae de `src/theme.rs` la paleta, pesos Inter, iconos,
  controles y estilos. El legado los reexporta; no hay una segunda copia.
  El slider de seek/volumen también se extrae de los widgets originales,
  conservando teclado, rueda y semántica AccessKit.
- Navegación extraída de `src/ui/sidebar.rs`, contenedor de barra inferior
  de `src/ui/player_bar.rs` y placeholder de `src/ui/widgets.rs`, usados por
  ambos hosts. El legado conserva sus acciones, datos y visualizadores.
- `doremi-app` compone sidebar, navegación, biblioteca y barra persistente
  sobre el mismo Queue/Engine existente. Sin Spotify, cabecera adicional ni
  motor nuevo. Ventana desktop 1240×800; navegación táctil en ventanas pequeñas.
  Inicio prioriza Buscar y Biblioteca; las muestras se conservan en un
  desplegable. El QA desplaza controles hasta el área visible de la página.
- Alias ejecutable `doremi`/`doremi.exe`; se conserva `doremi-app` para los
  consumidores anteriores. Perfil Linux conservado y ruta propia Windows.
- CI enfocada incorpora Windows nativo del nuevo host, tests, lint, smoke
  CLI y artifact con manifiesto/hash. No equivale a reproducción Windows real.

No está terminada la migración de las vistas completas: faltan home con
secciones musicales reales, filas/tablas completas, álbum, artista y playlist,
transiciones y otras capacidades inventariadas en el roadmap. Este corte
no cierra P0 y no sustituye la Preview aprobada como evidencia visual.

## Datos, reproducción y plataformas

Corte de catálogo implementado, pendiente de GUI/CI: MusicBrainz público
para grabaciones, artistas y álbumes/ediciones, paginación, detalles de
pistas de álbum y grabaciones de artista, y guardado de grabaciones por ID.
Tipos/créditos neutrales en core, sin ampliar el schema persistido ni añadir
Spotify al host. La fuente identifica disponibilidad: metadatos sin audio
ni carátulas. Demo y Commons siguen siendo fuentes explícitas separadas.
API real consultada desde CachyOS: «Moonlight Sonata» devolvió registros,
IDs, créditos y duración. Esto no prueba todavía el recorrido en GUI.

El adaptador usa 1.1 s entre solicitudes por proceso, timeout total 15 s,
respuestas de 1 MiB, páginas de 20, caché de ocho páginas/5 min y generaciones.
Tres tests del parser pasaron (Unicode, vacío, IDs inválidos, tamaño,
créditos/duración ausentes y pistas de álbum). Clippy del host pasó;
las pruebas nuevas de cancelación/cambio de fuente están compilándose.
Revisión actual de Tubefast/yt-dlp/RustyPipe en `TUBEFAST-ASSESSMENT.md`.

 La reproducción Commons
existente descarga el recurso antes de decodificar; **no es streaming
progresivo online**. No se añaden formatos/importadores locales.

Linux: capturas anteriores reales guardadas en `/tmp/doremi-product-before`;
compilación, capturas nuevas y audio de este corte pendientes. Windows:
el job `41919750405` compiló los tests nativos; fallaron por la misma
aserción de Pausa heredada que Linux. No hay todavía un artifact Windows
aprobado de este corte. Android: mismo código de presentación;
APK `af95d8d` pasó el job Android; firma/alineación/manifest/checksum
verificados y actualización instalada por ADB en SM-J701M, API 28/ARMv7.
Abrió la GUI, preservó favoritos/ID de sesión y permaneció pausada. La
captura confirmó overflow de Ajustes (x final 405 frente a viewport 384).
Se corrigen padding y etiqueta móvil, y se usa el slider original sin la
etiqueta visible que ensanchaba la barra. Pendiente repetir con el nuevo APK.
Evidencia: `/tmp/doremi-product-android-af95/`; SHA-256 APK
`554a8534887b53766a8abc75d8b2c46d8f4cfa0a72687eb7f1a7ac3733cafaf9`.
No se modificaron datos personales ni configuración global del dispositivo.

Checks: `cargo check --offline -p doremi-ui` pasó antes de componer el host.
Circuito ligero y Clippy del host/all-targets pasaron, también tras añadir el catálogo. El primer check del
host detectó el nombre de API de teclado cambiado en egui fijado; corregido
y repetido con éxito. CI enfocada iniciada en `41919424431` para `8223b0d`;
ese SHA anterior no contiene todavía la corrección de teclado. Capturas,
logs y evidencias quedan fuera del árbol versionado. No hay release.

CI `41919424431` cancelada al sustituirla por `41919750405` (`af95d8d`).
En este último, Linux detectó que un test de publicación de geometría aún
buscaba el antiguo botón Pausa permanente. La barra ahora alterna Play/Pausa;
se adapta el test para comparar el rectángulo publicado con el widget actual,
verificar viewport/página y rechazar controles Home obsoletos. Las tres
pruebas posteriores fallaron por el mutex envenenado; sus geometrías todavía
no se dan por aprobadas. No se han ocultado ni rebajado aserciones.

Siguiente corte: terminar QA de geometría/navegación, ejecutar la búsqueda
real desde la GUI, descargar artifacts del mismo SHA e instalar Android;
continuar con artwork y transporte progresivo de fuentes autorizadas.
