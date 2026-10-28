# Doremi Android development host

Independent Rust/egui host using `doremi-core`, GameActivity 4.4.0 and OpenGL.
No Spotify dependencies, network permission or personal storage access.
API 26 minimum, target/compile 36, NDK 28.2.13676358, JDK 17, Gradle 8.13,
AGP 8.13.0 and cargo-ndk 4.1.2. CI builds ARM64 and ARMv7 in one test APK.
The connected Samsung SM-J701M runs a 32-bit Android 9 OS and requires ARMv7.

The public `development.keystore` has password `android` and alias
`androiddebugkey`. It is exclusively a disposable development identity for
`dev.eirom.doremi.dev`, never a production credential. Keeping this key stable
lets development updates preserve preferences instead of uninstalling the app.
No release/store publication is configured.

Home, Unicode search, saved demo library, editable queue, player simulation,
purple/green and light/dark themes are offline. Preferences and saved library
are atomically persisted by a bounded worker in Android's private files directory.
The Rust owner survives Activity recreation in-process and pauses when dropped.
Process-death queue restoration and real audio remain separate gates.
`qa-state.json` contains this demo's state and control bounds for ADB testing.
No screenshots/logs belong in Git.

CI produces APK, instrumentation APK, manifest and SHA256SUMS. Run
`bash scripts/build-android.sh` in the pinned CI environment. The instrumentation
runner checks Unicode via Android InputConnection and Activity recreation;
ADB tests additionally navigate the actual rendered controls.

The optional Linux companion is `cargo run -p doremi-app`; it uses the same
host and core without modifying the approved `doremi-preview` executable.
Its data is isolated in `~/.local/share/doremi/native-dev` or `DOREMI_DATA_DIR`.
The original logo and its GPL asset license are packaged separately from MIT code.
