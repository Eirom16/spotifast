---
name: doremi-testing
description: "Choose and run the correct Doremi validation circuit for any change (daily light check-local, per-crate tests with -p, provider offline fixtures, under-x11 graphical QA, CI gates) and interpret results honestly (executed/failed/blocked/not run). Use before closing any change or when a test, clippy, or CI job fails. Never weaken tests."
---

# Pruebas de Doremi

## Circuito diario local (Pentium Silver N5000/8 GB)

```sh
python3 scripts/check-local.py          # políticas, estructura/procedencia, fmt, core sin deps
```

Rust fijado en `rust-toolchain.toml` (estable 1.98): no uses otro compilador
para declarar que el baseline pasa. `--locked` siempre.

## Circuito de integración (CI es la validación completa)

```sh
cargo test --locked -p doremi-core                     # contratos neutrales
cargo test --locked -p <crate> --all-targets           # crate tocado
cargo clippy --locked -p <crate> --all-targets -- -D warnings
cargo fmt --all --check
python3 scripts/test-provider-harness.py               # proveedor offline
python3 scripts/audit-dependencies.py --check
python3 .github/scripts/check-dependency-sources.py
python3 .github/scripts/test-policy-checks.py          # tras tocar workflows/checks
```

Los builds completos, tests --workspace, all-features, doctests y APK viven
en `.github/workflows/ci.yml` y `android.yml` con cachés `doremi-v3`; no los
repitas localmente salvo necesidad escrita y justificada.

## Reglas de honestidad

- Distingue ejecutado / fallado / bloqueado (entorno) / no ejecutado.
- Prohibido debilitar tests o lints. Un fallo externo (proveedor, red) es
  resultado visible.
- Nunca tocar bibliotecas/perfiles reales de usuario; perfiles sintéticos o
  corpus. Evidencia de producto fuera del árbol:
  `~/.local/share/doremi/product-qa/<fecha>/`.
- QA gráfica con binario: `xvfb-run -a python3 scripts/native-qa.py
  <binario> target/native-qa` (X11 real, navegación, teclado, cola).
- CI por commit: `gh run list --branch <rama> --limit 3`; veredicto global
  con `scripts/summarize-ci.py` sobre artifacts `report-*`.
