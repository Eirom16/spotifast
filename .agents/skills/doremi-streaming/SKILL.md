---
name: doremi-streaming
description: "Work on Doremi's neutral catalog/resolution/playback ports and provider experiments (crates/doremi-core, doremi-player, doremi-commons, tools/provider-harness, tools/provider-live-probe), including the current YouTube Music/yt-dlp integration phase. Use when adding or changing search, resolution, streaming, or audio-queue behavior. Respect provider refusals; no DRM circumvention; yt-dlp is not a productive dependency yet."
---

# Streaming y proveedores de Doremi

## Estado y prioridad de fase

Prioridad actual según `DOREMI-ROADMAP.md`: integración real de YouTube
Music/yt-dlp primero, después reproducción (contenedores AAC/Opus/WebM/MP4
que entrega yt-dlp), UI y compatibilidad Android. El desarrollo de formatos
locales NO es la fase en curso salvo dependencia imprescindible.

- Elección Rust nativo vs yt-dlp es experimental. Los contratos neutrales
  viven en `crates/doremi-core` (`catalog.rs`, `requests.rs`, `media.rs`).
- yt-dlp y ffmpeg están instalados localmente (verificar `yt-dlp --version`).
  Supervisor sin shell si se evalúa yt-dlp: binario validado, argv fijo,
  input del catálogo jamás interpolado. Registra la versión.
- Referencia de como NO hacerlo: `src/doremi/api/youtube_music.py` (legado).
- Una denegación del proveedor es un error visible: nada de sortear
  DRM/paywall/bloqueo geográfico ni sustituir pistas silenciosamente.

## Flujo de trabajo

1. Protocolo offline primero:
   `python3 scripts/test-provider-harness.py` (fixtures HTTP sintéticos,
   sin servicios externos) y `cargo test --locked -p doremi-core`.
2. Experimento fuera de línea acotado solo bajo demanda:
   `cargo run --locked -p doremi-provider-live-probe` (corpus auditado en
   `tools/provider-live-probe/corpus.json`); su workflow CI
   `provider-probe.yml` es el único contexto de red en CI, por paths.
3. Reproducción: `crates/doremi-player` (symphonia). Features de prueba:
   `--features native-output,ogg-vorbis`; fixtures en
   `crates/doremi-player/tests/fixtures`. El callback de audio no hace red
   ni espera locks; buffers acotados; eventos con generación.
4. Terminado: `cargo fmt --all --check`, clippy del crate con `-D warnings`,
   tests del crate tocado. CI completa para integ (no compiles el workspace
   completo en local sin necesidad).
