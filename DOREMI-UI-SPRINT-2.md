# Doremi UI 2.0 — Sprint 2

Inicio musical con estantes y carátulas, reproductor integrado y mini-player
coherente, implementados y comprobados en Linux y Android.

Rama: `feature/doremi-ui-sprint-2`.
Commit de implementación: `cac4786b5c5f1bf4c83235c525fda9fd0d49dd81`.
Base verificada: `f3722a1ae4ed8746d0308cb70aeaa9d01802e2f4`, informe posterior
al código probado `7d79bb2133d5d7fa51a1b85a603d32756ff3e3b5`.
Este informe se entrega en un commit documental posterior, sin cambios de código.
Se conservan `main`, la rama de integración y el trabajo ajeno del checkout original.

## Cambios implementados

- Inicio: encabezado editorial compacto, búsqueda principal, secciones del
  proveedor con estantes horizontales de carátulas, favoritos en tarjetas y
  playlists propias. Los estados sin contenido ofrecen acciones útiles.
- Reproductor de escritorio: carátula grande y transporte junto a la identidad
  musical; favoritos, cola, progreso, volumen y cancelación en la misma superficie.
  La barra inferior se reserva a la navegación, evitando controles duplicados.
- Reproductor móvil: carátula más amplia con sombra existente, encabezado,
  créditos disponibles del catálogo, jerarquía musical y acceso directo a la cola.
  Composición horizontal conservada para ventanas compactas y rotación.
- Mini-player: miniatura de 48 dp, superficie con acento moderado, título y
  créditos cuando existen; los estados de carga/error conservan prioridad.
- Microinteracciones: hover/foco animado en tarjetas y navegación mediante
  `egui::Context::animate_bool_with_time`, respetando `style.animation_time`.

Reutilización Spotifast/Doremi: `cards::card_sized`, `cards::shadow`,
`cover_placeholder`, `nav_row`, `mobile_nav`, `player_bar`, botones circulares,
iconos Lucide, Inter, sliders, paletas y workers de carátulas. No cambian el motor, la descarga completa, las dependencias ni los contratos
de cola y persistencia. Código modificado: `home_view`, `presentation`,
`player_view`, `listening`, `music_catalog` y las primitivas de `doremi-ui`.

## Evidencia y verificación

Evidencia fuera del árbol:
`/home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/`.

Se usan metadatos y miniaturas del catálogo público, historial CC0 persistido
de QA y perfiles privados. No se presenta demostración como contenido real.

**Checks locales:** 69 tests de aplicación y 6 de UI aprobados; 1 recorrido de
red largo ignorado; `check-local.py` aprobado. Se corrigieron el desbordamiento
de las acciones en anchos estrechos y la alineación vertical del transporte.
Los tests verifican el recorte del estante horizontal, todas sus ocurrencias,
la primera tarjeta visible y ausencia de red implícita. Los intentos previos
se conservan en la evidencia.

CI del mismo commit:
[CI completa](https://github.com/Eirom16/spotifast/actions/runs/45243063593) ·
[hosts nativos y APK](https://github.com/Eirom16/spotifast/actions/runs/45243070002).
Linux, Android y Windows del workflow nativo: **aprobados**. En la CI completa
pasaron quick, tests default/all-features/doctests, Clippy/Rustdoc, Linux gráfico,
Windows, Android core y APK. macOS quedó **cancelado sin obtener runner**: GitHub
registró “The job was not acquired by Runner of type hosted even after multiple
attempts” y avisos de capacidad. El gate `verified-artifacts` quedó **fallido**
por esa dependencia. **La CI global no está completamente verde.** No se
repitieron indefinidamente los runners de macOS.

## Resultados visuales y funcionales

[HTML con 35 comparaciones antes/después](</home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/comparacion-antes-despues.html>) ·
[ZIP con los PNG originales y resultados](</home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/capturas-sprint-2-cac4786.zip>).
El HTML se comprobó en el navegador: 70 imágenes cargadas, sin enlaces rotos.

| Recorrido sobre `cac4786` | Resultado |
| --- | --- |
| Linux 1240×800, `native-qa.py --playlist-music` | **Aprobado**: PCM nativo, búsqueda, cola, favoritos, playlists y restauración |
| Linux 640×480, recorrido funcional original | **Aprobado**: transporte, navegación y edición de cola/playlists |
| Linux, catálogo público en ambos tamaños y temas | **Aprobado**: carátulas, canciones y colecciones reales; reproductor y barra |
| Linux, historial persistido de QA CC0 en ambos temas | **Aprobado**, sin red ni audio nuevo |
| Samsung, recorrido musical público | **Aprobado**, 14 capturas; vertical/horizontal y claro/oscuro |
| Samsung, producto vacío, navegación e IME | **Aprobado**, 23 capturas y cuatro filtros de búsqueda |
| Samsung, controles con IDs musicales reales restaurados | **Aprobado en etapas**: PCM nativo, pausa/seek/resume, volumen, shuffle/repeat, favoritos, siguiente/anterior, cancelación, cola duplicada, playlists y restauración |
| Recorrido adicional Android Commons `--playlists` | **Fallido** esperando estado tras `save:26896924`; también falló en el baseline Sprint 1. Se conserva el fallo; no se declara aprobado |

El recorrido físico directo está documentado en `functional-android.py` y
`android-product-controls/`; usa solo IDs reales guardados por la prueba pública.
Se verificó en etapas, conservando los intentos del driver. Biblioteca, playlists
e historial normales del Samsung mantuvieron sus hashes.

## Aplicaciones disponibles

Artefactos de desarrollo, `release: false`, del mismo commit y versión, con
checksums verificados y licencias conservadas:

- [Linux ejecutable y auxiliares en TAR.GZ](</home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/deliverables/Doremi-Linux-Dev-cac4786.tar.gz>).
- [APK Android instalada y probada](</home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/deliverables/Doremi-Android-Dev-cac4786.apk>).
- [SHA256SUMS](</home/eirom/.local/share/doremi/product-qa/2026-11-23-ui-sprint-2/deliverables/SHA256SUMS>).

Manifiestos y checksums de CI comprobados. La APK instalada coincide por hash
y contiene ambos ABIs; la verificación SDK se ejecutó en CI, sin SDK local.
No se hicieron merges, force-push ni releases.


## Límites

- Commons en Android conserva el fallo del recorrido descrito. Requiere
  diagnóstico separado; los controles productivos se verificaron físicamente.
- Los resultados públicos y su orden pueden cambiar entre consultas. La QA
  distingue las canciones de las colecciones y guarda los IDs y estados usados.
- Los créditos se reutilizan de los snapshots y el historial de navegación
  disponibles. El modelo persistido mínimo no guarda todos los créditos del
  artista; no se inventan cuando faltan después de restaurar un proceso.
- Las imágenes ausentes conservan el placeholder heredado. Las carátulas de
  catálogo proceden del proveedor; no se introducen assets musicales inventados.
- No hay preferencia de movimiento reducido del SO expuesta por el host actual;
  las animaciones usan la duración de estilo de egui, sin añadir ajustes.
- La validación física se limita al Samsung SM-J701M, Android 9/ARMv7. Los
  tamaños 360/393/430 dp se prueban además con los tests de layout compartidos.
- No se afirma captura acústica ni GUI física de Windows/macOS. No se modifica
  la pausa al salir de Android ni el motor o estrategia de descarga completa.

