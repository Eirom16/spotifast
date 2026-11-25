# Doremi · catálogo y búsqueda

**CATALOG_WORKING**

Rama `feature/doremi-catalog-search`, creada desde el HEAD remoto
`022ff79d77c78b5f1c46107d9f8e9dc323d8462d`, que incluye la precarga verificada en
`7ddc52acad99feed9921955a79421d5625a16d65`. Código y artefactos del commit
`3366307db46fc190d66e4bf106c79e239aefd42e`. El informe se publica en un commit
documental posterior.

## Funciones entregadas

- Búsqueda de canciones, artistas, lanzamientos y playlists públicas. La
  deduplicación por ID cualificado conserva el orden del proveedor y las
  versiones con IDs distintos. Los lanzamientos incluyen singles y EP cuando
  aparecen en el catálogo.
- Sugerencias de YouTube Music con debounce de 220 ms, cancelación por
  generación y selección con teclado, ratón o toque. La caché conserva hasta
  12 entradas por worker durante 120 s y se invalida al cambiar de sesión.
- Páginas de artistas con canciones populares, discografía, singles y EP,
  y secciones relacionadas cuando existen. «Ver todo» usa el enlace del
  proveedor. En álbumes y playlists se conserva el orden y se cargan las
  continuaciones válidas. Las acciones sobre toda la colección recorren
  las páginas al solicitarlas. Se puede guardar una referencia remota o
  añadir canciones a una playlist propia.
- Se persisten créditos, álbum, duración, carátula y tipo junto al ID y
  proveedor. Biblioteca, historial y playlists leen esquemas v1 y v2.
  La cola incorpora un campo opcional. Para revertir una copia de un archivo
  v2 a v1, se quita `music` de las entradas y se restaura el esquema v1.
- Las carátulas reutilizan el worker y la caché de imágenes. Las filas
  visibles comparten descargas por URL. El parser prefiere miniaturas
  cercanas a 226 px y la interfaz conserva la proporción al recortarlas.

Se reutilizaron el cliente Rust y el parser derivado de Tubefast bajo MIT,
la interfaz egui, los workers y el escritor atómico existentes. Las consultas
de sugerencias, discografías y continuaciones se contrastaron con el código
de ytmusicapi. [Sugerencias](https://github.com/sigma67/ytmusicapi/blob/28e00ddc0fffb69d65656d18af35845d656b3096/ytmusicapi/mixins/search.py),
[discografías](https://github.com/sigma67/ytmusicapi/blob/28e00ddc0fffb69d65656d18af35845d656b3096/ytmusicapi/mixins/browsing.py),
[continuaciones](https://github.com/sigma67/ytmusicapi/blob/28e00ddc0fffb69d65656d18af35845d656b3096/ytmusicapi/continuations.py).

## Resultados reales

| Consulta o recorrido | Observación |
| --- | --- |
| `Linkin` | Sugerencias `linkin park`, `linkin park in the end` y `linkin park numb` |
| `Linkin Park`, `Michael Jackson`, `Daft Punk` | El artista buscado apareció primero en su filtro |
| `Numb`, `Linkin Park Numb` | El título aislado mezcla versiones. Al indicar el artista, la grabación de Linkin Park apareció segunda en la muestra de red |
| `Billie Jean` | Primero Michael Jackson en *Thriller*, seguido de otra edición. En Linux la lista pasó de 20 a 40 resultados |
| `Random Access Memories` | Daft Punk, 2013, 13 pistas en orden desde *Give Life Back to Music* |
| `Hasta la Raíz`, `Björk Jóga`, `東京事変` | Coincidencias relevantes con tildes y escritura japonesa |
| `qxmqxmq567842001` en Artistas | Cero resultados |
| Playlist pública larga | 100 pistas disponibles al abrir y 199 tras continuar. La acción de añadir la colección encoló 948 pistas en 8,4 s sin iniciar audio |

## Validación y tiempos

| Plataforma y consulta | Referencia anterior | Artefacto de este commit |
| --- | ---: | ---: |
| Linux, `Linkin Park` en Artistas | 1.997 ms | 1.780 ms |
| Linux, `Random Access Memories` en Álbumes | 2.118 ms | 2.704 ms |
| Linux, `Billie Jean` en Canciones | 2.091 ms | 2.109 ms |
| Linux, `Linkin Park` dentro de la app | — | 1.118 ms en frío, 388 ms en caché |
| Samsung, `Linkin Park` en Artistas | 6.261 ms | 5.824 ms |
| Samsung, `Random Access Memories` en Álbumes | 6.748 ms | 6.739 ms |
| Samsung, `Linkin Park` dentro de la app | — | 1.460 ms en frío, 363 ms en caché |

La medición externa incluye entrada de texto y observación del estado. Los
tiempos de la app incluyen el debounce y la espera del worker. Son muestras
puntuales con red real. En el ejecutable de CI, abrir el artista tardó 724 ms,
su discografía 424 ms, el álbum 653 ms y una playlist 7.519 ms. La segunda
página de la playlist larga tardó 1.511 ms. En el Samsung, el artista tardó
1.040 ms, el álbum 542 ms, la playlist 711 ms y la segunda página de `Billie
Jean` 1.386 ms dentro de la app.

- Linux N5000 con X11. Se verificaron sugerencias, selección con teclado,
  entrada rápida, carátulas, navegación, cola, playlist propia y restauración
  de créditos y álbum tras reiniciar. La prueba de interfaz a 384 px pasó en
  ambos temas. El binario de CI produjo PCM no nulo con los dos medios
  autorizados. Se midieron 2.476 ms al primer PCM y 547 ms al pasar a la pista
  precargada, con cero underruns del buffer en ambas.
- Rust local y CI. 78 tests activos de la app, 39 de YouTube, 23 de biblioteca y
  38 del núcleo. Pasaron Clippy con `-D warnings`, `check-local.py` y 40 casos
  del arnés HTTP sintético. La ejecución final de GitHub Actions pasó en Linux,
  Windows y el empaquetado Android de ambas ABI. [Ejecución de CI](https://github.com/Eirom16/spotifast/actions/runs/45667443183).
- Samsung SM-J701M con Android API 28 y ARMv7. El checksum instalado coincide
  con el APK de CI. Pasaron selección táctil de sugerencias con el teclado
  abierto, envío desde el teclado, 20 a 38 resultados en la búsqueda paginada,
  carátulas, álbum y playlist pública. Los estantes completos de Linkin Park
  mostraron 24 álbumes y 48 singles o EP. Tras reiniciar el proceso siguieron
  presentes el crédito Daft Punk, el álbum y las 56 pistas en cola. Se
  comprobaron los hashes del perfil ordinario. Los dos medios autorizados
  produjeron PCM no nulo. El primer PCM se observó a los 4.050 ms y el paso a
  la pista precargada a los 1.223 ms, con cero underruns del buffer en ambas.

## Entregables y límites

- Ejecutable Linux x86_64 en `target/catalog-3366307/linux/native-dist/doremi`.
  SHA-256 `8fa122d081fb75fae55d45984dff1b71b4f1f5b673856d17b89c7bd1a591a50c`.
- APK de desarrollo con ARMv7 y ARM64 en
  `target/catalog-3366307/android/app-debug.apk`.
  SHA-256 `01d3dbfb3cffab86415db7d431d02f612e47a2e1f265c9da083465b67942c973`.
- Capturas y mediciones en
  `~/.local/share/doremi/product-qa/2026-11-25-catalog/`. Linux está en
  `linux-final-ci/`. En Android, las sugerencias y la paginación están en
  `android-final/`, y el artista, el álbum, la playlist y la restauración en
  `android-final-details/`. La discografía y los singles están en
  `android-final-discography/`.

La disponibilidad, el orden y el idioma de los metadatos dependen del
proveedor. La prueba física Android se hizo en ARMv7. ARM64 se verificó en el
paquete de CI. La prueba de audio se limitó a `qQzdAsjWGPg` y
`_d3SaQI_Ag4`. Las listas públicas de la prueba larga se consultaron como
metadatos y se verificaron mediante la cola.
