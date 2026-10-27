# Doremi F1B-1: integración y arquitectura de proveedores

## Estado

**Provisional: implementación local comprobada; CI completa pendiente.**
No declarar el gate F1B-1 completado hasta registrar aquí la ejecución real
verde del SHA implementado. Base: `b9fe1a5`.

No se modificaron UI, DSP, credenciales, perfiles, actualizador, versión ni
identidades nativas. No hay releases ni dependencias productivas de
Tubefast, RustyPipe o yt-dlp. La fase informativa de artifacts es F1B1.

## Corte vertical real

La eliminación manual por ocurrencia y la confirmación del resync pasan por
`doremi-core::manual`. El vector `App.manual_queue` existente sigue siendo la
única autoridad y se presta mutable al núcleo. La operación devuelve índices
para la proyección y remapeo de adiciones pendientes; no guarda filas ni
crea otro Queue autoritativo.

El bridge mantiene el transporte Spirc: ClearQueue y AddToQueue en el orden
resultante. No reescribe App ni reemplaza contexto, autoplay, radio o player.
Confirmación core compara contra ese mismo vector vivo, no contra una
segunda copia del orden que podría quedar obsoleta tras otro write.

Se conservan ventana de seis segundos, seis reintentos y fallback legacy.
Las lecturas parciales o writes en vuelo no confirman prematuramente. Un
reply aceptado de esa edición se consume una vez. Reset de sesión invalida
router y avanza la secuencia wire, rechazando polls antiguos.

## Sesiones, contratos y tests

`requests::Router` introduce sesiones explícitas y tickets por lane/generación,
cancelación y rechazo de reemplazos que reutilizan sesión. Los RequestId del
reducer de cola también llevan sesión: restaurar en una nueva instancia con
otra sesión no acepta callbacks emitidos después del checkpoint antiguo.

Caracterizaciones nuevas contra App real offline:

- Selección de la segunda copia con contexto coincidente, remapeo de pending
  additions y resync Spirc en orden.
- Selecciones stale/context/resume-only no editan otra fila.
- Confirmaciones parciales, éxito y reply duplicado posterior.
- Intercalado de un write rechazado, una edición y errores de lectura:
  solo se revierte el write fallido; no aparece otro dueño de la cola.
- Reset/restauración de vista no acepta polls de otra sesión.
- Wrap resume-only y ambigüedad por URI duplicada permanecen caracterizados.

La UI legacy todavía envía índice + URI, no ID global de ocurrencia; no se
inventa identidad perdida. El core completo no dirige todavía toda la cola
Spirc. Estas diferencias se describen en `PROVIDER-CONTRACTS.md`.

## Catálogo/resolución y harness

Core sigue sin dependencias externas. El coordinador puro `Catalog` es
consumido de verdad por una herramienta de host, no añadido como API sin uso.
Tiene capacidades, resultados acotados, selección estable explícita, tickets,
deadlines inyectados, errores tipados y recursos temporales redactados.
No se persisten URLs/headers/cookies en Media o Snapshot.

`tools/provider-harness` solo depende de core y serde_json ya bloqueado.
Usa fixtures HTTP loopback y WAV autogenerado CC0. Produce JSONL/summary/
manifest con SHA, dirty, etapas y fallos. No hace tráfico a proveedores reales.
Warm/cold es caché del adaptador falso, no rendimiento musical.
PCM/audio audible/cancelación física/CPU/RSS permanecen null, no se inventan.

Comandos implementados:

```sh
python3 scripts/check-local.py
cargo clippy --locked -p doremi-core -p doremi-provider-harness --all-targets -- -D warnings
python3 scripts/test-provider-harness.py
python3 scripts/provider-bench-summary.py target/provider-fixtures/samples.jsonl
```

El circuito cotidiano solo ejecuta schema checks y core. La integración HTTP
compila una herramienta pequeña, no el reproductor completo. En CI sus
registros se adjuntan a los reportes del job quick.

## Verificación local real y fallos

- Core: **32 tests y 1 doctest pasados**.
- Harness: **2 tests de schema y 40 muestras sintéticas validadas**,
  incluidos fallos esperados 403/429/404, JSON, rango, truncado, tamaño y timeout.
- Clippy core/harness: pasó con `-D warnings`.
- Inventario de dependencias: pasó.
- Primer compile local falló por falta de Hash en SessionId; corregido sin
  quitar el contrato Hash de RequestId.
- Primer Clippy del harness falló por collapsible-if; corregido sin allow.
- Tests App/legado, matrices nativas, Rustdoc y Android: pendientes de CI.

No se mide ni se afirma latencia de reproducción real. Los tiempos de
compilación no son resultados de un proveedor musical.

## Cachés

La auditoría actualizada está en `CACHE-ROTATION.md`. Los doce IDs de F1A
siguen presentes: 7.841.152.854 bytes. Se propone eliminar selectivamente
ocho obsoletos, con potencial de ~4,74 GiB, conservando cuatro útiles.
**No se ejecutó ningún borrado ni se elevó el presupuesto.**

## Android F1B-2: preparación, no APK

Comprobado en fuentes de las revisiones fijadas:

- eframe ofrece android-game-activity/android-native-activity y NativeOptions
  recibe AndroidApp. La app legacy no tiene ese host.
- fastframe shell/now-playing/graphics declara integraciones desktop; sus
  stubs no constituyen Activity/IME/lifecycle Android.
- CPAL fijado declara ndk con audio/api-level-26, ndk-context y JNI para
  Android. Eso no implementa audio focus, MediaSession o foreground service.
- Localmente hay Java y adb, pero no Gradle, sdkmanager, cargo-ndk ni SDK/NDK
  configurados. No se instaló nada global ni se invocaron dispositivos.

Alcance mínimo F1B-2: APK offline con fake de memoria, mismo core, GameActivity
e IME, rotación y lifecycle; sin Spotify/login/backend/runtimes externos.
Un dueño de core fuera de la Activity evita duplicación al recrearla.

Conjunto inicial propuesto, no probado como APK: JDK 17, Gradle 8.13,
AGP 8.13.x fijado, compile/target SDK 36 y minSdk 26, NDK r28 o posterior
fijado y cargo-ndk. La documentación AGP 8.13 confirma Gradle 8.13/JDK 17
y soporte hasta API 36.1; no usar versiones dinámicas en el spike.
Fijar revisión exacta y checksums al crear el proyecto.

Gates: build/instalación APK en API mínima/actual, teclado Unicode, misma
traza tras rotación, destroy/recreate, process death y callbacks cancelados;
sin permisos de Internet en el offline mínimo. Luego audio PCM local, focus,
auriculares y permisos/servicio según API, con validación de páginas 16 KiB.
La v1 HTTP de recursos requiere extensión por handle/asset del host antes
de audio offline; no inventar un servidor hospedado para suplirla.

Fuentes de requisitos:
[AGP 8.13](https://developer.android.com/build/releases/agp-8-13-0-release-notes),
[NDK](https://developer.android.com/ndk/downloads/revision_history),
Cargo manifests y epi.rs de los forks ya fijados en Cargo.lock.

No ejecutados: APK/Gradle/NDK/dispositivo, GUI/audio reales, proveedores
online, benchmarks comparativos, tests nativos completos macOS/Windows,
Nix/Jekyll/Flatpak runtime/store nativo ignorado. La `.rlib` no es una APK.
