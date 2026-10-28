# Doremi Preview 0.1 — Linux x86_64

Offline visual demo, not a music streaming release. No Spotify account or
Premium subscription needed. No Rust, Python, yt-dlp or compilation required.

1. Download the `doremi-preview-0.1.0-linux-x86_64-<SHA>` artifact from the
   **Doremi CI** run on `feature/doremi-preview-0-1`. GitHub requires sign-in.
   Extract its ZIP first. Artifacts expire after seven days.
2. Verify and extract the portable package:

   ```sh
   sha256sum -c SHA256SUMS
   tar -xzf doremi-preview-0.1.0-linux-x86_64.tar.gz
   cd DoremiPreview-0.1.0
   sha256sum -c SHA256SUMS
   ./run.sh
   ```

Extract somewhere writable. `run.sh` preserves executable permissions from
the tar file and stores only Preview appearance preferences in
`./doremi-preview-data/config/preview.json`. It never reads, imports or
changes the installed Spotifast profile/keyring. To choose another Preview
folder, set `DOREMI_PREVIEW_DATA` or pass `--data-dir /your/preview/folder`.
Direct binary launch defaults to `$XDG_CONFIG_HOME/doremi-preview`
(or `$HOME/.config/doremi-preview`), not Spotifast's folder.

## CachyOS

Requires a graphical X11 or Wayland session and OpenGL 2+ drivers.
Built on Ubuntu 24.04 (glibc 2.39); intended for up-to-date CachyOS x86_64,
not musl/Alpine or old glibc distributions. It is not a universal AppImage.
Check your system without compiling anything:

```sh
ldd ./doremi-preview
./doremi-preview --version
./run.sh
# Diagnostic fallback if native Wayland window creation fails:
env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET ./run.sh
```

On CachyOS the desktop normally provides these libraries. Install any
missing runtime packages, not Rust development packages:

```sh
sudo pacman -S --needed alsa-lib pipewire libxkbcommon libxkbcommon-x11 wayland libx11 mesa libglvnd
```

`runtime-libraries.txt` records the actual CI `ldd` output. Any `not found`
line on the user's machine is a missing runtime dependency. X11 also needs
`libxkbcommon-x11`, which winit loads dynamically and does not appear in
`ldd`. The graphical CI test validates that path. No audio device or Spotify
service is opened by this demo.

## Try it

- Navigate Home, Search, the library sidebar, playlists, artists, albums
  and queue. Ctrl+F focuses search; type `Rosewood` or `Bonobo` and Enter.
- The initial catalogue is simulated and search is local, capped at 20 tracks.
  The existing fixture's artist/album/playlist search sections are initially
  visible; a new query filters songs only.
- Open Settings (gear). Change Dark/Light and Purple/Green. Close and reopen:
  both selections persist. Green is Spotifast's original light/dark accent.
- Playback, account actions, writes to the catalogue and system integrations
  are intentionally disabled and display a demo notice. The paused player
  is a visual fixture; **no music is played**. Covers use placeholders offline.
- Existing keyboard navigation and scrolling remain; not every legacy
  context-menu operation is enabled. No automatic updater or public release.

The package carries commit, feature set, version, hashes and licenses.
Spotifast's MIT copyright remains in `LICENSE`; the original Doremi artwork
has its separate upstream GPL v3 license in `assets/doremi/LICENSE`.
Original image files are provided unchanged. No code from Doremi-Rust is used.
