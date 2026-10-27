# Doremi F1A: dominio de cola y evaluación de proveedores

## Estado y alcance

**Entrega F1A validada en el alcance limitado descrito**, con CI verde:
[run 40580787610](https://github.com/Eirom16/spotifast/actions/runs/40580787610),
SHA de código `bb81fc33e2ce56a30ac04723979bc3c579b435c2`, intento 1.
No equivale a migrar toda la cola viva ni a tener proveedor/APK funcional.
Base: `8661cbd`, F0 validado. Sin cambios visibles de UI. No se integró
Tubefast/RustyPipe/yt-dlp, ni se cambiaron IDs de aplicación, updater, claves,
versión del ejecutable o perfiles Spotify.

## Implementación

- `doremi-core::media`: namespace de proveedor validado, ID local opaco y
  metadatos mínimos. No URLs firmadas, cookies, APIs, UI ni entidades Spotify.
- `doremi-core::queue`: reducer determinista de comandos/eventos que devuelve
  efectos a interpretar fuera del dominio. Solo std; ningún runtime, RNG,
  reloj, JNI, red, disco, salida de audio o dependencia nueva.
- IDs propios por ocurrencia, no por canción. Duplicados conservados en
  append/insert/remove/reorder/next y snapshots.
- Cola manual separada de contexto, orden de shuffle explícito/permutación,
  repeat, previous/next, posición, estados de carga/pausa/error y restauración.
- Solicitudes identificadas para load y lectura de álbum, invalidación por
  cambio/cancelación/restore; respuestas antiguas/duplicadas no mutan estado.
- Snapshot tipado en memoria, restaurado pausado y sin reusar IDs anteriores
  dentro del Queue. No añade un formato persistido ni toca JSON/keyrings.
- Historial oído acotado a 256 entradas. Los efectos de audio y trabajo
  asíncrono no se ejecutan en este crate ni se llaman desde el callback.
- Inputs inválidos se rechazan antes de mutar; overflow de contadores se
  reporta como error, no wrapping. `Restore` usa Box para no inflar todos los
  mensajes del enum; no se silenció el lint.

### Bridge controlado

`src/queue_bridge.rs` proyecta URIs del legado a reglas puras sin convertirlas
en otro catálogo ni mantener una segunda copia del estado:

1. Límite de prefijo manual con multiplicidad, no set de URIs.
2. Plan de movimiento en gaps y remapeo de índices de adiciones pendientes.

`src/app.rs` solo delega esas dos reglas y añade un módulo de caracterización.
El resto de acciones, UI, auth, Spirc, backend, DSP y persistencia continúa
intacto. La nueva máquina **no dirige aún la cola viva completa**. Cambiar
la autoridad de transporte ahora rompería contratos remotos que F1A todavía
no puede representar; F1B debe hacerlo por cortes y con nuevos tests.

El root añade una dependencia local de path a core. Ninguna fuente/version
externa cambia; el lock solo añade esa arista al paquete raíz. Metadata de
artefactos marca F1A desde Cargo, sin renombrar identidades nativas ni versión
0.14.2 heredada. El inventario se regenera/comprueba.

## Caracterización y equivalencia

Una sola fixture `crates/doremi-core/tests/support/traces.rs` se reproduce
con el reducer y con las transiciones privadas reales de App en modo offline:
**4 trazas y 15 pasos**, comparación con expectativas independientes después
de cada paso:

| Escenario preservado | Prueba |
|---|---|
| Duplicados y consumo de una sola ocurrencia por next | append B,B y tres saltos; IDs separados en core |
| Reordenación/insert/remove/clear manual sin perder contexto | Movimiento al gap final, eliminación de copia, insert X,X y clear |
| Shuffle sin alterar manual/current; volver al orden normal | Permutación explícita y confirmación de cola del proveedor en App |
| Cambio de contexto conserva manual | Contexto A,C,D → X,Y con B,B por delante de Y |
| Límite manual | Regla original congelada frente a bridge en 465 combinaciones de duplicados/desplazamientos |
| Remapeo de pending rows | Enumeración de planes en core + tests existentes de pending additions |
| Cancelación y llegada tardía de álbum | Begin/clear/resultado tardío en core y ApiResponse real de App |
| Poll viejo después de reorder | Secuencia real de App invalidada, observación igual al core |
| Repeat One y Previous | Comparación del umbral 3.000/3.001 ms y normalización de repeat del legado actual |
| Bookmark restaurado | Pausa, posición, duplicados y Previous de reinicio sin empezar reproducción |

Las confirmaciones de transporte de las trazas usan snapshots simulados
offline, no Spotify real ni un segundo modelo de App. No se afirma haber
ejecutado un binario baseline anterior con la fixture nueva. Los tests
originales permanecen con sus assertions y cobertura; son otro gate de CI.

Tests de core adicionales cubren: namespace, errores transaccionales,
permutaciones inválidas, solicitudes supersedidas, ready/error/end/position
tardíos, pausa mientras carga, seek que invalida carga anterior, cancelación,
álbum atómico, repeat natural frente a skip explícito, historial/forward,
fin de lista y snapshot/restauración sin solicitudes vivas.

### Regla heredada descubierta

El test actual `changing_songs_by_hand_ends_repeat_one` y
`changes_song_by_hand`/`leave_repeat_one` contradicen texto histórico anterior:
**Next, Previous que retrocede y un inicio explícito pasan Repeat Track a
Context**. Previous después de 3 s solo reinicia y conserva Track. Fin natural
con Repeat Track repite. El core sigue código/tests actuales, no la guía vieja.

### Diferencias y límites explícitos

- **No equivalencia total Spirc/Connect.** Autoridad remota, transferencias,
  contexto paginado, radio/autoplay y recuperación de sesión siguen en legado.
  Los efectos de core no sustituyen ni confirman audio real.
- App rechaza un poll anterior por secuencia y respuestas incoherentes durante
  un presupuesto temporal, con fallback tras seis reintentos. Core, dueño
  de su cola, ignora tokens cancelados sin ese fallback. F1B necesita un
  adaptador de confirmación, no aceptar snapshots Spotify como comandos.
- La cola legacy tiene URIs sin IDs de ocurrencia globales: en algunos casos
  elige la primera copia por URI cuando los índices cambiaron. Core dirige
  edits por EntryId. El bridge no promete reconstruir identidad perdida.
- La previsualización `resume_only` legacy puede envolver extremos incluso
  sin repeat y elegir aleatoriamente cuando shuffle está activo. El core
  usa orden explícito y Next envuelve según Repeat Context. Previous sin
  historia/cursor anterior reinicia, no promete wrap en esa frontera.
  No sustituye esa navegación legacy.
- Historial core acotado a 256 y replay de lo oído no garantizan la historia
  completa de Spotify, que no se ha extraído ni modelado por equivalencia.
- Doble clic/debounce, selección de filas no disponibles y rechazo remoto
  parcial siguen en App/adaptador. Append core representa intenciones ya
  admitidas; no deduplica títulos ni usa reloj.
- Load es preparación: el host solo publica audio tras Ready aceptado y
  SetPlaying con intención actual. Esto evita que una pausa durante carga
  vuelva a autoplay por el flag antiguo. El host futuro debe respetar ese
  protocolo y el orden de efectos; F1A no sustituye el decoder legado.
- Snapshot no es schema de perfil: sin codec/migración de archivos en F1A.
  RequestId se enruta al Queue de esa sesión. Reemplazar instancias con
  callbacks vivos exige epoch del host; no intercambiar tokens entre Queues.
- Complejidad: edición Vec O(n), prefijo BTreeMap O(n log n), snapshots
  clonados y metadata duplicada por ocurrencia. No llamar el reducer desde
  audio realtime ni prometer límites de frame/memoria sin benchmark.

Estas diferencias no eliminan funciones del baseline: la autoridad que las
mantiene no se ha retirado. Son gates de F1B, no capacidades “equivalentes”
que se den por probadas.

## Investigación y Android

`TUBEFAST-ASSESSMENT.md` fija los SHAs de Tubefast, RustyPipe y yt-dlp,
detalla búsqueda/resolución/buffering/errores, licencias y riesgos de
endpoints internos, y prepara un benchmark y spike Android acotados.

Conclusión: no importar Tubefast, no tratar RustyPipe GPL-3.0-only como MIT,
no copiar perfiles remotos/client impersonation ni caminos de bypass.
Conservar frontera ID/solicitud/efectos, medir antes de elegir proveedor.
No hay métricas verificadas de latencia/RAM/audio de esos proveedores.
El spike APK y harness online autorizada están pendientes de F1B/F2;
F1A solo ejecuta el core neutral y comprueba sus tests para target Android.

## CI y presupuesto de caché

GitHub Actions sigue siendo el entorno pesado: gates previos intactos,
misma matriz desktop, tests default/all-features, Clippy/Rustdoc y artifacts
verificados. Se habilita la rama F1A y se añade check de contratos Android,
sin declarar que compilarlos sea ejecutarlos en dispositivo.
Los comandos locales siguen siendo core/check-local; no se compila el cliente
completo en N5000 para sustituir la CI.

Snapshot de caché al iniciar: **7.841.152.854 bytes, 12 entradas**, sobre el
umbral de admisión de 6 GiB, bajo el límite de 10 GiB. No se borró ninguna.
La rama F1A no debe asumir acceso a caches de otra feature branch (F0);
medir hit/restore real, no inferirlo por tener ancestro común.

Propuesta de rotación segura, **requiere autorización**:

1. Exportar listado de IDs/key/ref/último acceso/tamaño por API como evidencia.
2. Conservar namespaces actuales `doremi-v3` con toolchain aislada: Linux
   codegen/metadata `920b5be1`, macOS `3c606d02`, Windows `f0b9e35e`.
3. Revisar entradas `doremi-v2` y v3 anteriores a aislamiento
   (`5396bed8`, `06add2ef`, `d903b2aa`, `229e8b8e`) del ref F0.
   No borrar por prefix v3 entero: mezcla entradas útiles y obsoletas.
4. Borrar solo IDs concretos aprobados, nunca todos los caches del repositorio;
   recuperar espacio bajo admisión y hacer una única ejecución para calentar
   F1A cuando su rama vaya a mantenerse. Alternativa: esperar expiración
   automática sin prometer fecha o aceleración.

Listado revisado por API, todos del ref `refs/heads/chore/doremi-f0`.
**Solo propuesta, no ejecutada**:

| ID candidato obsoleto | Namespace/entorno | Bytes |
|---|---|---:|
| 9600597326 | v3 codegen sin aislamiento, 5396bed8 | 986.454.883 |
| 9599836994 | v3 Windows sin aislamiento, 229e8b8e | 560.436.954 |
| 9599349209 | v3 macOS sin aislamiento, d903b2aa | 450.599.913 |
| 9599303131 | v3 codegen sin aislamiento, 06add2ef | 986.335.684 |
| 9599231576 | v3 metadata sin aislamiento, 5396bed8 | 753.909.283 |
| 9598426317 | v2 codegen | 450.676.392 |
| 9598317257 | v2 macOS downloads | 450.649.450 |
| 9597878812 | v2 metadata | 450.752.007 |

Liberación estimada por suma de tamaños API: 5.089.814.566 bytes (~4,74 GiB).
Conservar IDs 9601986172, 9602812663, 9601828365 y 9601047406: las cuatro
caches F0 con toolchain aislada (2.751.338.288 bytes, ~2,56 GiB).
Antes de aprobar/ejecutar, volver a comprobar key/ref y último acceso por ID:
el listado es un snapshot, no una autorización ni una garantía de vigencia.

No ampliar cuotas, no subir budgets por intuición, no duplicar objetos por
SO ni guardar targets locales completos. Ninguna release, updater o
automatización upstream se activa.

## Validación y fallos

Comprobaciones locales:

- `cargo test --locked -p doremi-core`: 2 unitarios de invariantes/historial,
  15 contratos y 1 doctest pasaron. Tiempo observado de la compilación
  incremental local: 2,40 s; no es una medición del cliente ni de audio.
- `cargo clippy --locked -p doremi-core --all-targets -- -D warnings`:
  primer intento falló por `large_enum_variant` de Restore; corregido con
  `Box<Snapshot>`, después pasó, sin `allow`.
- `python3 scripts/check-local.py`, inventario, formato y actionlint sin
  ShellCheck: pasaron en comprobación local anterior a cierre.
- Se compiló/testeó solo core localmente; la aplicación pesada y las
  caracterizaciones de App se validaron en GitHub, no en el N5000.

### Evidencia CI final

Run `40580787610`, SHA `bb81fc33e2ce56a30ac04723979bc3c579b435c2`:
**todos los jobs y el gate de cuatro artefactos pasaron**. El run anterior
`40580329190` del SHA `bfdb5ca` quedó cancelado al supersederlo el cambio de
protocolo Ready/SetPlaying; no se cuenta como un pase ni fallo de regresión.
No hubo fallos de tests ni compilación en la ejecución final.

| Gate/job | Resultado real |
|---|---|
| quick | Pasó: 9 checks F0, 6 de CI, core, fuentes, evidencia, políticas y packaging con Ruby |
| core (default y all-features) | 2 unitarios + 15 contratos, 0 fallidos; doctest pasó |
| App default | 1.401 pasados, 0 fallidos, 3 ignorados |
| App all-features | 1.441 pasados, 0 fallidos, 3 ignorados |
| Caracterización nueva App | Los 6 tests `app::tests::f1a_characterization::*` pasaron en ambas suites |
| CLI/integración | CLI 14/16 según features, todas las suites de integración aplicables verdes |
| Clippy default/all-features y Rustdoc | Pasaron con `-D warnings` |
| Linux/macOS arm64/Windows x64 | Builds default + smoke `--version` y payloads subidos |
| Android core | `.rlib` compilada y contratos cross-checked para aarch64; no tests ejecutados en Android |
| verified-artifacts | Pasó: cuatro payloads limpios, SHA/versión/fase/target/hash coincidentes |

Los tres ignorados siguen siendo portal desktop, keyring nativo y ventana
X11 con servicios reales. Los summaries adicionales de subprocess tests de
IPC/tray no se suman otra vez como cobertura nueva.

Métricas del wrapper de CI, no benchmarks de reproducción:

| Comando | Segundos | Exit |
|---|---:|---:|
| Local-checks ligero en runner | 22,292 | 0 |
| Tests default, compilación fría incluida | 946,523 | 0 |
| Tests all-features | 72,221 | 0 |
| Doctests | 1,501 | 0 |
| Clippy default | 432,767 | 0 |
| Clippy all-features | 17,618 | 0 |
| Rustdoc | 23,236 | 0 |
| Build macOS | 898,856 | 0 |
| Build Windows | 1.236,516 | 0 |
| Android core build | 47,662 | 0 |
| Android contract check | 0,350 | 0 |

Tiempos totales de jobs: quick 42 s, full-tests 17 min 55 s, Clippy 8 min 33 s,
macOS 15 min 44 s, Windows 22 min 26 s, Android 1 min 14 s y gate 15 s.
GitHub anotó posible cola por capacidad macOS, pero el job pasó; no se
atribuye tiempo de cola/setup a la máquina de estados.

Los cuatro jobs con rust-cache informaron `exact_hit=false`. Esta rama no
tenía cachés propias y el presupuesto bloqueó nuevas escrituras; no se
atribuye a F1A el hit medido en F0. Snapshot final **sin crecimiento**:
12 entradas, 7.841.152.854 bytes. No se eliminaron caches remotas.
Target tests ~2,933 GiB y ~80,18 GiB libres en ese runner; metadata
~1,153 GiB y ~81,74 GiB libres. Los guards no borraron outputs de los targets.

Artifacts reales del run, de versión 0.14.2/fase F1A y SHA `bb81fc3`:

| Payload | Tamaño comprimido, bytes |
|---|---:|
| Linux | 36.262.831 |
| macOS | 28.566.388 |
| Windows | 25.351.320 |
| Android core `.rlib` | 137.734 |

El artifact `ci-report-bb81fc33e2ce56a30ac04723979bc3c579b435c2-1` conserva
logs/JSON por comando y resultados de GitHub Jobs. Retención siete días.
La evidencia corresponde al SHA de código, no al commit documental posterior.

Se comprobó además por diff contra `8661cbd` que tests de integración previos,
componentes visuales, player/sink/DSP, ajustes/credenciales/rutas/updater,
assets y MIT no cambian. Los paquetes externos del lock son idénticos.

Comandos ligeros:

```sh
python3 scripts/check-local.py
cargo clippy --locked -p doremi-core --all-targets -- -D warnings
```

Reproducción pesada en CI:

```sh
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features --doc
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
cargo check --locked -p doremi-core --tests --target aarch64-linux-android
```

No ejecutados: GUI/audio real, proveedores online, benchmark de reproducción,
APK/NDK/dispositivo, tests nativos completos macOS/Windows, Nix, Jekyll,
runtime Flatpak y store nativo ignorado. Mantener estas ausencias aunque
la CI de compilación pase.
