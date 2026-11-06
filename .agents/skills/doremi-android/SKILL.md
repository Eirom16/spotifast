---
name: doremi-android
description: "Build, install, inspect, and verify the Doremi Android APK on the connected physical device via adb: android/ gradle project, JNI in crates/doremi-app via cargo-ndk, android-*-qa.py scripts. CI-first: local machine has adb but no SDK; download CI artifacts with gh. No Android APIs in doremi-core."
---

# Android de Doremi

## Entorno de esta máquina (verificado)

- adb 1.0.41 en PATH; dispositivo Samsung ARMv7 usualmente conectado
  (`adb devices` para confirmar el serial).
- Sin SDK/NDK local (ANDROID_HOME vacío): NO compiles APK en local.
  El build real es `bash scripts/build-android.sh` en
  `.github/workflows/android.yml` (platform android-36, build-tools 36.0.0,
  NDK 28.2.13676358, JDK 17, gradle 8.13, cargo-ndk 4.1.2, API 26,
  ABIs arm64-v8a + armeabi-v7a).
- Iterar con el APK de CI:
  ```sh
  gh run list --workflow=android.yml --limit 5
  gh run download <run-id> --name "doremi-android-dev-<sha>" --dir target/android-dist
  adb install -r target/android-dist/app-*.apk
  ```
- Solo núcleo neutral en local:
  `cargo build --locked -p doremi-core --target aarch64-linux-android`.

## Reglas

- `doremi-core` puro; JNI en `crates/doremi-app`; gradle solo empaqueta.
  Nada de lógica de negocio en Kotlin.
- Hilos: el hilo principal nunca espera red, decodificación, disco ni
  locks; errores visibles, buffers acotados.
- Evidencia: scripts QA reales antes de afirmar que funciona:
  `scripts/android-music-ui-qa.py`, `scripts/android-instrumentation-qa.py`,
  `scripts/android-audio-thread-qa.py`, `scripts/android-history-qa.py`,
  `scripts/android-library-restore-qa.py`; verificación de paquete con
  `python3 scripts/verify-android.py target/android-dist --verify`.
  Una rlib aarch64 no es un APK.
- Nunca tocar bibliotecas/perfiles reales del dispositivo; QA con datos
  sintéticos o corpus. Estado actual y última corrida validada:
  `DOREMI-STATUS.md`.
