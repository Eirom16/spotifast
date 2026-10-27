# Doremi: propuesta de rotación selectiva de cachés

## Estado

**No se ha borrado ninguna caché remota. Esta propuesta requiere autorización.**
Auditoría GET por API de `Eirom16/spotifast`:
2026-10-27T23:04:31Z. Snapshot: 12 entradas, 7.841.152.854 bytes (~7,30 GiB).
Todos los IDs citados en F1A siguen presentes, con el mismo ref/key/tamaño.

La CI conserva la admisión de escrituras por debajo de 6 GiB y lectura de
caches existentes. No se elevó el límite ni se rotó el namespace por commit.
Una rama nueva no hereda automáticamente caches de otra feature branch;
sin entrada elegible, un run puede ser frío y seguir siendo correcto.

## Conservar

Estas cuatro entradas usan la toolchain aislada y fueron verificadas en F0.
Ref común: `refs/heads/chore/doremi-f0`.

| ID | Key | Bytes | Último acceso UTC |
|---|---|---:|---|
| 9601986172 | doremi-v3-linux-dev-codegen-Linux-x64-920b5be1-a2fc4591 | 986.373.588 | 2026-10-27T19:38:33Z |
| 9602812663 | doremi-v3-macos-native-dev-downloads-Darwin-arm64-3c606d02-a2fc4591 | 450.655.873 | 2026-10-27T19:34:54Z |
| 9601828365 | doremi-v3-windows-native-dev-downloads-Windows_NT-x64-f0b9e35e-059d0e08 | 560.379.666 | 2026-10-27T19:13:20Z |
| 9601047406 | doremi-v3-linux-dev-metadata-Linux-x64-920b5be1-a2fc4591 | 753.929.161 | 2026-10-27T18:55:44Z |

Total a conservar de este snapshot: 2.751.338.288 bytes (~2,56 GiB).
No se garantiza que una entrada F0 sea elegible desde F1B1 ni que su
dependencia completa sea idéntica; la restauración debe medirse.

## Candidatos obsoletos, no autorización

Ref común: `refs/heads/chore/doremi-f0`.

| ID | Key | Bytes | Último acceso UTC |
|---|---|---:|---|
| 9600597326 | doremi-v3-linux-dev-codegen-Linux-x64-5396bed8-a2fc4591 | 986.454.883 | 2026-10-27T18:46:03Z |
| 9599836994 | doremi-v3-windows-native-dev-downloads-Windows_NT-x64-229e8b8e-059d0e08 | 560.436.954 | 2026-10-27T18:30:03Z |
| 9599349209 | doremi-v3-macos-native-dev-downloads-Darwin-arm64-d903b2aa-a2fc4591 | 450.599.913 | 2026-10-27T18:19:51Z |
| 9599303131 | doremi-v3-linux-dev-codegen-Linux-x64-06add2ef-a2fc4591 | 986.335.684 | 2026-10-27T18:18:48Z |
| 9599231576 | doremi-v3-linux-dev-metadata-Linux-x64-5396bed8-a2fc4591 | 753.909.283 | 2026-10-27T18:17:17Z |
| 9598426317 | doremi-v2-linux-dev-codegen-Linux-x64-5396bed8-a2fc4591 | 450.676.392 | 2026-10-27T18:00:09Z |
| 9598317257 | doremi-v2-macos-native-dev-downloads-Darwin-arm64-d903b2aa-a2fc4591 | 450.649.450 | 2026-10-27T17:57:56Z |
| 9597878812 | doremi-v2-linux-dev-metadata-Linux-x64-5396bed8-a2fc4591 | 450.752.007 | 2026-10-27T17:48:15Z |

Motivo: v2 se reemplazó tras medir budgets; los v3 enumerados son anteriores
al aislamiento de RUSTUP_HOME y contienen claves ligadas a toolchains
preinstaladas variables. No seleccionar por prefix `doremi-v3` entero:
también contiene las entradas útiles.

Liberación estimada por suma de API: 5.089.814.566 bytes (~4,74 GiB).
Es una estimación del snapshot, no espacio ya liberado ni fecha de expiración.

## Procedimiento seguro si se autoriza

1. Volver a consultar GET de cada ID, key, ref, tamaño y último acceso.
   Si cambió el uso o hay un run activo que lo necesita, detenerse y revisar.
2. Confirmar por escrito los ocho IDs concretos. No usar borrado global ni
   patrones de prefix/ref que incluyan entradas nuevas.
3. Exportar antes el listado a un artifact de auditoría. Las caches no son
   artefactos de release ni una copia de fuentes.
4. Solo entonces eliminar los IDs aprobados. No se incluye ni ejecuta aquí
   un comando DELETE automático.
5. Consultar uso posterior y ejecutar una sola validación necesaria para
   calentar la rama que vaya a conservarse. Medir cache-hit real y tiempo;
   no asumir elegibilidad por ancestro Git.

Alternativa sin autorización: mantener lectura, presupuesto y expiración
normal del proveedor. La corrección funcional no debe depender de una caché.

Comandos de auditoría de solo lectura:

```sh
gh api repos/Eirom16/spotifast/actions/cache/usage
gh api 'repos/Eirom16/spotifast/actions/caches?per_page=100'
gh api repos/Eirom16/spotifast/actions/caches/ID
```
