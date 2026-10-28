# Doremi session persistence

First library slice: readable `queue-v1.json` checkpoints, core validation,
paused restoration, and a bounded worker for atomic app-owned file writes.
No UI, Spotify, network, credentials or playback resources. The host preserves
unknown/corrupt files by omitting writes to them, and shows errors. It reads
preferences/checkpoints before drawing; durable writes always use `Writer`.
The writer acknowledges success/failure without blocking the UI, coalesces
pending snapshots and skips unchanged documents. Desktop joins it after its
event loop exits; Android retains the same owner across Activity recreation.

`cargo test --locked -p doremi-core -p doremi-library` is a lightweight daily
check. Host persistence migration and rendered UI tests run in CI.
