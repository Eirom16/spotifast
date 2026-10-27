# Doremi: infraestructura y evidencia CI

## Estado

La infraestructura está implementada y se ejecutó en GitHub. **El primer run
falló en Windows; la corrección y su verificación están pendientes.**
No se declara validada por pasar análisis YAML o solo los jobs Linux.
No hay releases públicas automáticas.

Equipo local: Pentium Silver N5000, 8 GB RAM. La compilación fría de F0
consumió varios límites de 20 min; Clippy default tardó 14 min 31 s y el
build standalone posterior, 10 min 51 s. Estos son tiempos locales observados,
no mediciones de GitHub ni estimaciones de aceleración.

## Trabajo por entorno

| Job | Runner estándar | Validación | Reutilización |
|---|---|---|---|
| `quick` | ubuntu-24.04 | Políticas, licencias MIT/assets, procedencia, formato, core, packaging y tests de tooling | Sin compilación de app ni caché de escritorio |
| `full-tests` | ubuntu-24.04 | Default, all-features/demo, doctests y CLI/integración | Dependencias compartidas dentro del job; exporta el binario default ya construido por integración antes de activar demo |
| `clippy` | ubuntu-24.04 | Clippy default/all-features y rustdoc `-D warnings` | Caché metadata distinta de codegen, sin enlazar tests |
| `desktop-build` | macos-15 y windows-latest | Ejecutable nativo default con MilkDrop y smoke `--version` | Solo descargas en caché, no duplicar varios GiB de objetos por SO |
| `android-core` | ubuntu-24.04 | Compilación `.rlib` arm64 neutral | Sin caché del árbol desktop, no APK ni NDK |
| `verified-artifacts` | ubuntu-24.04 | Resumen de todos los jobs y verificación del conjunto de payloads | No recompila nada |

Tests y Clippy compilan representaciones distintas (codegen frente a metadata):
separarlos exige algo de trabajo duplicado, no se promete eliminarlo. No hay
build Linux completo adicional, ni release/LTO, ni copias del núcleo por SO.
`max-parallel: 2` limita la matriz desktop. Un fallo no cancela la otra plataforma.
Los jobs pesados esperan que pase `quick`; superseder una rama cancela el run viejo.

Configuración uniforme solo en CI: `CARGO_INCREMENTAL=0`, debug info de dev/test
desactivada, dos trabajos Cargo y dos de CMake. No cambia Cargo.toml, el DSP,
features default ni la configuración cotidiana de la máquina local.
Los binarios son de validación dev, no de rendimiento/release.

## Cachés y almacenamiento

`Swatinem/rust-cache@v2`, después de activar el compilador fijado, con cuatro
espacios separados:

- `linux-dev-codegen`: objetos necesarios para tests, target con presupuesto
  de 4.096 MiB antes del guardado.
- `linux-dev-metadata`: Clippy/rustdoc, target con presupuesto de 2.048 MiB.
- `macos-native-dev-downloads` y `windows-native-dev-downloads`: solo Cargo
  registry/git, `cache-targets: false`. Nunca restaurar objetos Linux allí.
- Android/core y quick no duplican caches de un árbol desktop que no consumen.

La clave automática incorpora host/compilador, lock/manifests, config y variables
Rust/Cargo. Un perfil release o nuevo target necesitará otro namespace.
No se usa SHA de commit como clave: invalidaría la caché en cada edición.
El namespace `doremi-v3` corrige los presupuestos iniciales tras medir el primer
run, no cambia por commit. No se cachean binarios de herramientas ni crates workspace; no se guarda en
fallo ni desde PR. Durante F0 solo main/rama de trabajo admiten escrituras.

El job rápido consulta el consumo real del repositorio por API y permite
nuevos guardados solo si está por debajo de 2 GiB, reservando margen para
guardados paralelos y el límite estándar de 10 GiB. Es una política
conservadora de admisión, no una garantía de tamaño comprimido: GitHub reporta
el tamaño definitivo después del guardado. El reporte final vuelve a medirlo.
Una caché existente puede leerse cuando se bloquean guardados.

`ci-cache-budget.py` solo actúa en runners efímeros github-hosted y exclusivamente
en `target/ci`. Si excede el presupuesto descarta ese target **entero**, después
de tests y copia de payloads, y conserva las descargas. No borra librerías
OUT_DIR aisladas dejando fingerprints nativos incoherentes. El reporte registra
la caída a caché de descargas; no presentarla como reutilización de codegen.
No borra fuentes, outputs fuera del target, Cargo home ni caches remotas.

Antes de cada comando Cargo pesado se exigen 4 GiB libres; se registra disco
antes/después. Los logs/reportes y payloads están fuera del directorio cacheado.
Artifacts caducan a los 7 días, sin símbolos de debug ni targets completos.
No hay limpieza indiscriminada de software del runner ni `cargo clean`.

Si la presión bloquea nuevos guardados, revisar tamaños/key por API antes de
rotar caches con autorización. No cambiar versiones/prefixes por cada run ni
ampliar presupuestos sin medir, pues eso crea churn y evicciones.

## Evidencia que produce cada ejecución

- `report-<job>-<sha>-<attempt>`: JSON/log por comando, duración monotónica,
  exit code, categoría orientativa de fallo, espacio libre y resultado exacto
  de caché. `cache-hit=false` puede significar restauración parcial de otra
  versión del lock, no necesariamente caché vacía.
- `payload-<platform>-<sha>`: ejecutable o `.rlib`, MIT y manifest
  con commit, versión, target, tipo, hash y estado dirty.
  Se reemplaza solo dentro del mismo run al reejecutar ese job. Así se pueden
  repetir jobs fallidos sin recompilar plataformas que ya pasaron: el gate
  comprueba todos los hashes/SHA/versiones, no mezcla commits.
- `ci-report-<sha>-<attempt>`: reportes agrupados, JSON real de GitHub Jobs
  (incluye restore/post-save), consumo de caches y tabla de conclusiones.
  El agregador puede seguir en progreso al tomar su propio snapshot.

El gate verifica cuatro payloads: Linux, macOS nativo, Windows x64 y core Android,
todos limpios y de SHA/versión idénticos. No interpreta "skipped" o "cancelled"
como éxito. Los reportes se suben también en fallo; un setup/timeout anterior
al wrapper puede no tener JSON por comando y se diagnostica con GitHub Jobs/log.
Una ejecución interrumpida puede no alcanzar a subir su reporte.

Los hashes prueban correspondencia/integridad, no una firma de publisher ni
reproducibilidad bit a bit. Un binario compilado no acredita pruebas auditivas,
GUI nativa, instalación ni APK funcional. Descargar un ejecutable macOS/Linux
desde artifact puede requerir restaurar permiso ejecutable; no es un instalador.

## Seguridad y permisos

Se conservan checks de fuentes, evidencia, regresiones de políticas,
settings con aprobación en PR, MIT/assets, packaging y tests de branding/
actualizaciones. No se debilita un test para ahorrar runner.
Permisos mínimos `contents: read`, `actions: read`, sin secretos de firma,
publicación, Pages, `pull_request_target` ni ejecución de código upstream
account-wide. Actions externas conservan su responsabilidad de suministro;
F0 no sustituye una revisión de vulnerabilidades/licencias transitorias.

## Desarrollo local ligero y reproducción

```sh
python3 scripts/check-local.py
# Core sin desktop:
cargo test --locked -p doremi-core
# Validación de infraestructura:
python3 scripts/test-ci.py
```

El circuito ligero no instala Ruby/SDK/NDK ni compila MilkDrop. Los checks
completos y dependencias siguen documentados en `CONTRIBUTING.md` para
reproducción voluntaria, no como requisito por edición.

En una rama del fork ya publicada:

```sh
gh workflow run ci.yml --repo Eirom16/spotifast --ref chore/doremi-f0
gh run list --repo Eirom16/spotifast --workflow ci.yml --branch chore/doremi-f0 --limit 5
gh run view RUN_ID --repo Eirom16/spotifast
gh run view RUN_ID --repo Eirom16/spotifast --log-failed
gh run download RUN_ID --repo Eirom16/spotifast --dir target/downloaded-ci
gh api repos/Eirom16/spotifast/actions/cache/usage
```

Comparar un run frío y una reejecución del mismo SHA antes de atribuir mejoras
a caché. `gh run rerun RUN_ID --job JOB_ID` permite medir tests o Clippy sin
recompilar los otros sistemas; sus payloads del mismo SHA siguen verificándose.
No sumar dos veces tests subprocess ni atribuir conclusiones de un SHA a otro.

## Registro de validación

- Local: `scripts/check-local.py` pasó (9 tests F0, 6 tests de CI y 1 doctest
  core), sin compilar de nuevo el cliente.
- Local: PyYAML parseó el workflow; actionlint 1.7.12 pasó con ShellCheck
  desactivado. Esto no prueba runners, cache ni artefactos remotos.
- Primer run real: [40552655182](https://github.com/Eirom16/spotifast/actions/runs/40552655182),
  commit `2171a402f7995901529d7a09eabe48b3058498de`, conclusión **failure**.
  Ruby/launchers pasaron en el runner. No se trata el fallo Windows como éxito.

| Trabajo/comando | Segundos medidos por wrapper | Resultado |
|---|---:|---|
| Checks locales ligeros en runner | 43,416 | Pasó |
| Launchers con Ruby | 1,871 | Pasó |
| Tests default (compilación fría incluida) | 981,014 | Pasó |
| Tests all-features | 80,073 | Pasó |
| Clippy default | 272,055 | Pasó |
| Clippy all-features | 11,051 | Pasó |
| Rustdoc | 13,917 | Pasó |
| Build nativo macOS arm64 | 939,099 | Pasó y subió ejecutable |
| Core Android | 61,871 | Pasó y subió `.rlib` |
| Build Windows | 53,656 | Falló: tomó `Git/usr/bin/link.exe`, no MSVC |

Todos los caches eran fríos (`exact_hit=false`). Targets antes del guardado:
tests 3.147.304.333 bytes, Clippy 1.237.332.440 bytes. Los presupuestos
iniciales de 2.048/768 MiB los descartaron enteros: se guardaron descargas,
no codegen. Esto motivó `doremi-v3` y presupuestos 4.096/2.048 MiB, no una
afirmación falsa de aceleración. Tres caches v2 ocuparon 1.352.077.849 bytes.

Corrección Windows: ejecutar Cargo desde PowerShell tras configurar MSVC,
evitando la prioridad de PATH de Git Bash. `PYTHONUTF8=1` uniforma los logs.
El gate final rechazó correctamente el conjunto incompleto y subió el
reporte. Verificación de estas correcciones y aciertos de caché aún pendiente.
