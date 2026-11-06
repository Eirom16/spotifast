---
name: doremi-release
description: "Prepare verifiable Doremi deliverables (manifests, SHA256SUMS, license payloads, version bumps) and apply the current release rules: CI artifacts are validation-only, no public releases in F0, no reactivating legacy workflows, never touch crmne/spotifast identity. Use when preparing artifacts, bumping versions, or drafting a release plan."
---

# Releases y entregables de Doremi

## Reglas de la fase actual

- F0/F1 y la rama actual NO publican releases. Los artifacts de CI son solo
  de validación (`verified-artifacts` verifica el payload); todo sube como
  artifact de run con retención de 7 días.
- Prohibido reactivar `.github/legacy-workflows/` sin revisar destinos,
  permisos, secretos y gates.
- Nunca publicar en `crmne/spotifast` ni reutilizar su identidad de firma o
  servicios de publicación; el repo del fork es Eirom16/spotifast.
- Una release futura exige: un mismo commit y versión en todos los
  artefactos, checksums, licencias completas y prueba nativa por plataforma
  (un runner sin audio/OpenGL no prueba hardware real).

## Preparar entregables verificables

```sh
python3 scripts/build-manifest.py --target <triple> --kind <kind> \
  --payload <binario> --output manifest.json      # binarios de payload
python3 scripts/native-manifest.py <binario> <manifest.json>   # host doremi + licencias
python3 scripts/verify-artifacts.py target/payloads --multiplatform
python3 scripts/verify-android.py target/android-dist --verify
python3 packaging/test-release-names.py
python3 packaging/test-launchers.py
```

Las licencias del payload (MIT, assets Doremi GPL-3, Inter/icons, corpus CC0,
player) se copian exactamente como en los workflows; no inventes archivos.

## Versiones

- La versión del workspace se lleva en dos sitios sincronizados: root
  `Cargo.toml` (0.14.2) y los crates `doremi-*`/tools; `Cargo.lock` se
  actualiza con el build. Mantener el campo `package.metadata.doremi`
  (`phase`) coherente con `DOREMI-ROADMAP.md`.
- Renombrar binarios o nombres de release solo con
  `packaging/test-release-names.py` y `packaging/flatpak/test-metainfo.py`
  en verde.
