# Dependencias del workspace Doremi

Generado con `python3 scripts/audit-dependencies.py`. No editar a mano.
Clasificación y decisiones Android: `../../F0-REPORT.md`.
Se conservan las fuentes git heredadas; los nuevos paquetes usan paths y crates.io.

## Dependencias directas del workspace

| Ámbito | Crate | Especificación |
|---|---|---|
| `dependencies` | `aes` | `0.8` |
| `dependencies` | `anyhow` | `1` |
| `dependencies` | `base64` | `0.22` |
| `dependencies` | `clap` | `features=['derive']; version=4.5` |
| `dependencies` | `ctr` | `0.9` |
| `dependencies` | `doremi-core` | `path=crates/doremi-core` |
| `dependencies` | `eframe` | `default-features=False; features=['accesskit', 'glow', 'default_fonts', 'links', 'wayland', 'x11', 'persistence']; version=0.36.2` |
| `dependencies` | `egui` | `0.36.2` |
| `dependencies` | `egui_extras` | `features=['image', 'svg']; version=0.36.2` |
| `dependencies` | `fastframe-audio` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-autostart` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-dirs` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-emoji` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-file` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-fonts` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-graphics` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-i18n` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-icons` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-instance` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-log` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-now-playing` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-power` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-scroll` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-shell` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-text` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-theme` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-titlebar` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-tray` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `fastframe-update` | `features=['reqwest']; git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dependencies` | `flate2` | `1` |
| `dependencies` | `glutin` | `optional=True; version=0.32` |
| `dependencies` | `glutin-winit` | `optional=True; version=0.5` |
| `dependencies` | `hmac` | `0.12` |
| `dependencies` | `http` | `1` |
| `dependencies` | `hyper-util` | `default-features=False; features=['client-proxy', 'client-proxy-system']; version=0.1` |
| `dependencies` | `image` | `default-features=False; features=['jpeg', 'png', 'bmp', 'webp']; version=0.25` |
| `dependencies` | `jiff` | `0.2` |
| `dependencies` | `keyring-core` | `1` |
| `dependencies` | `librespot-connect` | `default-features=False; version=0.8` |
| `dependencies` | `librespot-core` | `default-features=False; features=['rustls-tls-native-roots']; version=0.8` |
| `dependencies` | `librespot-metadata` | `default-features=False; version=0.8` |
| `dependencies` | `librespot-playback` | `default-features=False; features=['rustls-tls-native-roots']; version=0.8` |
| `dependencies` | `librespot-protocol` | `default-features=False; version=0.8` |
| `dependencies` | `log` | `0.4` |
| `dependencies` | `md-5` | `0.10` |
| `dependencies` | `mdns-sd` | `0.21` |
| `dependencies` | `memmap2` | `0.9` |
| `dependencies` | `open` | `5` |
| `dependencies` | `pbkdf2` | `0.12` |
| `dependencies` | `percent-encoding` | `2` |
| `dependencies` | `projectm-sys` | `default-features=False; features=['static']; optional=True; version=1.2.2` |
| `dependencies` | `protobuf` | `3` |
| `dependencies` | `rand` | `0.9` |
| `dependencies` | `raw-window-handle` | `0.6` |
| `dependencies` | `reqwest` | `default-features=False; features=['rustls-tls-native-roots', 'json', 'gzip', 'http2', 'blocking', 'socks', 'system-proxy']; version=0.12` |
| `dependencies` | `rfd` | `0.17.2` |
| `dependencies` | `ring` | `0.17` |
| `dependencies` | `rodio` | `default-features=False; version=0.21` |
| `dependencies` | `rustls` | `default-features=False; features=['ring', 'std', 'tls12']; version=0.23` |
| `dependencies` | `rustls-webpki` | `default-features=False; features=['ring', 'std']; version=0.103` |
| `dependencies` | `serde` | `features=['derive']; version=1` |
| `dependencies` | `serde_json` | `1` |
| `dependencies` | `sha1` | `0.10` |
| `dependencies` | `sha2` | `0.10` |
| `dependencies` | `skrifa` | `0.44` |
| `dependencies` | `symphonia` | `default-features=False; features=['mp3', 'flac']; version=0.5` |
| `dependencies` | `thiserror` | `2` |
| `dependencies` | `tiny-skia` | `0.11` |
| `dependencies` | `tokio` | `features=['rt-multi-thread', 'sync', 'time', 'macros', 'fs', 'net', 'io-util']; version=1` |
| `dependencies` | `unicode-bidi` | `0.3` |
| `dependencies` | `urlencoding` | `2` |
| `dependencies` | `winit` | `optional=True; version=0.30` |
| `build-dependencies` | `fastframe-i18n` | `features=['build']; git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `dev-dependencies` | `directories` | `6` |
| `dev-dependencies` | `polib` | `0.3` |
| `dev-dependencies` | `tokio` | `features=['test-util']; version=1` |
| `cfg(target_os = "linux"): dependencies` | `x11rb` | `default-features=False; version=0.13` |
| `cfg(target_os = "linux"): dependencies` | `zbus` | `default-features=False; features=['tokio']; version=5` |
| `cfg(target_os = "linux"): dependencies` | `zbus-secret-service-keyring-store` | `features=['rt-tokio-crypto-rust']; version=1.0.1` |
| `cfg(target_os = "macos"): dependencies` | `apple-native-keyring-store` | `features=['keychain']; version=1.0.2` |
| `cfg(target_os = "macos"): dependencies` | `fastframe-dock` | `git=https://github.com/crmne/fastframe; tag=v0.15.2` |
| `cfg(target_os = "macos"): dependencies` | `objc2` | `features=['exception']; version=0.6` |
| `cfg(target_os = "macos"): dependencies` | `objc2-app-kit` | `default-features=False; features=['std', 'NSAccessibility', 'NSApplication', 'NSEvent', 'NSMenu', 'NSMenuItem', 'NSResponder', 'NSRunningApplication', 'NSWindow', 'NSView', 'NSScreen', 'NSColor', 'NSImage', 'NSImageView', 'NSButton', 'NSControl', 'NSTrackingArea', 'NSVisualEffectView', 'NSTextField', 'NSBezierPath', 'NSFont', 'NSAnimation', 'NSAnimationContext', 'NSWorkspace']; version=0.3` |
| `cfg(target_os = "macos"): dependencies` | `objc2-foundation` | `default-features=False; features=['std', 'NSAppleEventDescriptor', 'NSAppleEventManager', 'NSDate', 'NSObjCRuntime', 'NSRunLoop', 'NSString', 'NSThread', 'NSDictionary', 'NSAttributedString', 'NSData']; version=0.3` |
| `cfg(windows): build-dependencies` | `winresource` | `0.1` |
| `cfg(windows): dependencies` | `windows-native-keyring-store` | `default-features=False; version=1.1` |
| `cfg(windows): dependencies` | `windows-sys` | `features=['Win32_Foundation', 'Win32_Globalization', 'Win32_Graphics_Gdi', 'Win32_Security_Cryptography', 'Win32_System_Com', 'Win32_System_LibraryLoader', 'Win32_System_Registry', 'Win32_System_Threading', 'Win32_UI_Accessibility', 'Win32_UI_HiDpi', 'Win32_UI_Shell', 'Win32_UI_WindowsAndMessaging']; version=0.60` |
| `tools/provider-harness: dependencies` | `doremi-core` | `path=../../crates/doremi-core` |
| `tools/provider-harness: dependencies` | `serde_json` | `1` |
| `crates/doremi-app: dependencies` | `doremi-core` | `path=../doremi-core` |
| `crates/doremi-app: dependencies` | `doremi-library` | `path=../doremi-library` |
| `crates/doremi-app: dependencies` | `doremi-player` | `features=['native-output']; path=../doremi-player` |
| `crates/doremi-app: dependencies` | `egui` | `default-features=False; features=['default_fonts']; version=0.36.2` |
| `crates/doremi-app: dependencies` | `image` | `default-features=False; features=['png']; version=0.25` |
| `crates/doremi-app: dependencies` | `rand` | `0.9` |
| `crates/doremi-app: dependencies` | `serde` | `features=['derive']; version=1` |
| `crates/doremi-app: dependencies` | `serde_json` | `1` |
| `crates/doremi-app/cfg(not(target_os = "android")): dependencies` | `eframe` | `default-features=False; features=['glow', 'default_fonts', 'x11', 'wayland']; version=0.36.2` |
| `crates/doremi-app/cfg(target_os = "android"): dependencies` | `android_logger` | `0.15` |
| `crates/doremi-app/cfg(target_os = "android"): dependencies` | `egui_glow` | `default-features=False; version=0.36.2` |
| `crates/doremi-app/cfg(target_os = "android"): dependencies` | `jni` | `0.21` |
| `crates/doremi-app/cfg(target_os = "android"): dependencies` | `log` | `0.4` |
| `crates/doremi-app/cfg(target_os = "android"): dependencies` | `ndk-context` | `0.1` |
| `crates/doremi-app/cfg(target_os = "linux"): dependencies` | `doremi-commons` | `path=../doremi-commons` |
| `crates/doremi-app/cfg(target_os = "linux"): dependencies` | `doremi-player` | `features=['native-output', 'ogg-vorbis']; path=../doremi-player` |
| `crates/doremi-app/cfg(target_os = "linux"): dependencies` | `tokio` | `features=['rt', 'time', 'net', 'sync', 'macros']; version=1` |
| `crates/doremi-library: dependencies` | `doremi-core` | `path=../doremi-core` |
| `crates/doremi-library: dependencies` | `serde` | `features=['derive']; version=1` |
| `crates/doremi-library: dependencies` | `serde_json` | `1` |
| `crates/doremi-player: dependencies` | `doremi-core` | `path=../doremi-core` |
| `crates/doremi-player: dependencies` | `symphonia` | `default-features=False; features=['ogg', 'vorbis']; optional=True; version=0.5.5` |
| `crates/doremi-player/cfg(not(target_os = "linux")): dependencies` | `cpal` | `default-features=False; optional=True; version=0.18.2` |
| `crates/doremi-player/cfg(target_os = "linux"): dependencies` | `cpal` | `default-features=False; features=['pipewire', 'pulseaudio']; optional=True; version=0.18.2` |
| `tools/provider-live-probe: dependencies` | `doremi-commons` | `path=../../crates/doremi-commons` |
| `tools/provider-live-probe: dependencies` | `doremi-core` | `path=../../crates/doremi-core` |
| `tools/provider-live-probe: dependencies` | `doremi-player` | `features=['ogg-vorbis']; path=../../crates/doremi-player` |
| `tools/provider-live-probe: dependencies` | `serde_json` | `1` |
| `tools/provider-live-probe: dependencies` | `sha2` | `0.10` |
| `tools/provider-live-probe: dependencies` | `tokio` | `features=['rt', 'time', 'net']; version=1` |
| `crates/doremi-commons: dependencies` | `doremi-core` | `path=../doremi-core` |
| `crates/doremi-commons: dependencies` | `reqwest` | `default-features=False; features=['rustls-tls-native-roots']; version=0.12` |
| `crates/doremi-commons: dependencies` | `serde_json` | `1` |
| `crates/doremi-commons: dependencies` | `sha1` | `0.10` |

## Fuentes transitivas bloqueadas

`Cargo.lock`: 804 entradas (incluye miembros locales).

| Paquetes | Fuente exacta |
|---|---|
| 1 | `git+https://github.com/crmne/cpal?rev=fcd8b6ebee9d822426fa8d6839cdd89af6e0d491#fcd8b6ebee9d822426fa8d6839cdd89af6e0d491` |
| 10 | `git+https://github.com/crmne/egui?rev=6ce633ba5f7f64d19ef6d204f9377da560bb546b#6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| 22 | `git+https://github.com/crmne/fastframe?tag=v0.15.2#4d115b5e5838c207c0879205e6b1bc16fc1e9a9c` |
| 1 | `git+https://github.com/crmne/hyper-proxy2?rev=efca4811371ddae81c06be168de54194938d70b1#efca4811371ddae81c06be168de54194938d70b1` |
| 7 | `git+https://github.com/crmne/librespot?rev=3ba977f1f918582edce566cfc16617f8eccdfb1e#3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| 1 | `git+https://github.com/crmne/projectm-rs?rev=93852ff7299e5a63ed1315437593fc70915c0863#93852ff7299e5a63ed1315437593fc70915c0863` |
| 2 | `git+https://github.com/crmne/winit?rev=0bed9dc0a7db02590a279fb8bac795fa39ee6948#0bed9dc0a7db02590a279fb8bac795fa39ee6948` |
| 752 | `registry+https://github.com/rust-lang/crates.io-index` |
| 8 | `workspace` |

## Patches

| Crate | Fuente |
|---|---|
| `crates-io/cpal` | `https://github.com/crmne/cpal @ fcd8b6ebee9d822426fa8d6839cdd89af6e0d491` |
| `crates-io/ecolor` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/eframe` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/egui` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/egui-wgpu` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/egui-winit` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/egui_extras` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/egui_glow` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/emath` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/epaint` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/epaint_default_fonts` | `https://github.com/crmne/egui @ 6ce633ba5f7f64d19ef6d204f9377da560bb546b` |
| `crates-io/hyper-proxy2` | `https://github.com/crmne/hyper-proxy2 @ efca4811371ddae81c06be168de54194938d70b1` |
| `crates-io/librespot-audio` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-connect` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-core` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-metadata` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-oauth` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-playback` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/librespot-protocol` | `https://github.com/crmne/librespot @ 3ba977f1f918582edce566cfc16617f8eccdfb1e` |
| `crates-io/projectm-sys` | `https://github.com/crmne/projectm-rs @ 93852ff7299e5a63ed1315437593fc70915c0863` |
| `crates-io/winit` | `https://github.com/crmne/winit @ 0bed9dc0a7db02590a279fb8bac795fa39ee6948` |

## Núcleo nuevo

`crates/doremi-core` no tiene dependencias, build script ni código de SO.
La resolución global del workspace aún incluye los patches del legado;
compilar solo el núcleo no demuestra que el resto sea portable.
