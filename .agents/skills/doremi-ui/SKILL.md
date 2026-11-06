---
name: doremi-ui
description: "Implement, review, or visually audit the Doremi desktop GUI (egui/eframe + fastframe, Rust): src/ui, crates/doremi-ui, crates/doremi-app. Use when working on Doremi screens, theme, layout, accessibility, i18n, tray, or the legacy src/ui → Action → src/app.rs extraction. Never block the UI thread; no Qt/QML or web engines."
---

# Interfaz de Doremi (egui/eframe + fastframe)

## Stack real (no suponer otro)

- `eframe 0.36` con glow y AccessKit; `egui_extras` (image/svg). Sin wgpu en
  2D; sin Electron, Qt/QML ni WebView.
- `fastframe` v0.15.2 congela en `Cargo.lock` (text, fonts, icons, theme,
  emoji, i18n, log, tray, shell, graphics). No cambiar fuentes git sin
  aprobación explícita.
- Módulos clave: `crates/doremi-ui` (tema/tokens), `crates/doremi-app`
  (host, binario `doremi`), legado en `src/ui` → `Action` → `src/app.rs` →
  `src/backend.rs`.

## Reglas

- UI solo dibuja y emite intenciones; consume snapshots con generación.
  Cero bloqueos por búsqueda, red, descargas, decodificación, keyring o disco.
- AccessKit: todo control nuevo con nombre/estado/acciones.
- i18n: catálogos PO compilados por `build.rs` vía fastframe-i18n; ninguna
  traducción literal en código. Al cambiar traducciones:
  `.github/scripts/update-translations.sh --check`.
- Preferir defaults/shortcuts a ajustes nuevos; un ajuste nuevo requiere
  aprobación (etiqueta `setting-approved`, check `check-new-settings.py`).
- Visualizers: post-EQ/pre-volumen; cola per `docs/_reference/queue.md`;
  EQ y cola no se rediseñan desde una tarea visual.
- Evidencia visual: antes/después en claro y oscuro en el PR; nunca versionar
  capturas de revisión en el árbol.

## Verificación

```sh
python3 scripts/check-local.py                      # circuito ligero diario
cargo test --locked -p doremi-app --all-targets     # solo si el cambio lo pide
xvfb-run -a python3 scripts/native-qa.py <binario> target/native-qa  # QA gráfica real
```

CI es la validación completa (`.github/workflows/ci.yml`); no compilar el
cliente completo en local salvo necesidad justificada.
