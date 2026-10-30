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

Saved metadata uses `collection::Collection` and `library-v1.json`: up to 256
qualified IDs with bounded title/duration metadata, never stream resources.
Immutable shared snapshots make UI submission constant-time; the same writer
encodes changed collections and acknowledges atomic durable writes.
The host seeds the file from legacy preference favorites only when it is
absent. That archived list remains intact for rollback and is not merged into
an existing neutral collection, so removed favorites cannot reappear.
Unknown or corrupt files stay read-only and are preserved.
