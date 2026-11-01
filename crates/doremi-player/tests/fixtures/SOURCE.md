# Original Ogg test fixture

`scale.ogg` is the original Doremi mathematical scale from
`assets/audio/scale.wav`, encoded as Vorbis with local FFmpeg n9.0.2/libvorbis.
No recording or third-party music. The original CC0 dedication in
`assets/audio/LICENSE.txt` also applies to this encoding.

Generation executed from the repository root:

```sh
ffmpeg -nostdin -v error -fflags +bitexact -i assets/audio/scale.wav -map_metadata -1 -c:a libvorbis -q:a 3 -flags:a +bitexact -fflags +bitexact crates/doremi-player/tests/fixtures/scale.ogg
```

Encoder versions can change encoded bytes; tests validate semantics and bounds.
Fixture SHA-256: `486487f4a09b569084e988c7a2dbc9df515c4509778ea58c76a6c96a7ee75257`.

Symphonia 0.5.5 is an unmodified crates.io dependency under MPL-2.0, separately
licensed from this MIT crate. Source: https://crates.io/crates/symphonia/0.5.5
and https://github.com/pdeljanov/Symphonia/tree/v0.5.5 . No dependency source was
copied into Doremi. MPL text: https://www.mozilla.org/MPL/2.0/ .

`scale-repeat24.ogg` repeats that original three times (24 seconds), to test
compressed-to-PCM expansion limits. The same command adds `-stream_loop 2`
before `-i`, with this output name. SHA-256: `af2e37d2a6d1bbbcc588cfc90507e06e411872f8e403784a8cbc5dc1d69eaccb`.

## Original scale, FLAC

Derived exclusively from Doremi’s original CC0-1.0 `assets/audio/scale.wav`.
No external music or decoder source was copied. Generated with the local
FFmpeg CLI (not linked into or shipped with Doremi):

```sh
ffmpeg -nostdin -v error -n -i assets/audio/scale.wav -map_metadata -1 -c:a flac -compression_level 5 -fflags +bitexact -flags:a +bitexact scale.flac
```

Generator: ffmpeg version n9.0.2 Copyright (c) 2000-2026 the FFmpeg developers

Source SHA-256: `63aa41e98d3ff67982e2b56bb782df1f8ce151ae0f46666b32e761a81c24994d`.

FLAC SHA-256: `9a6af9a0825f6af8f461cbd483ae2d80e458e20d983b64bb57d2951d2a87ee6b`, 125316 bytes, 8 s / 48 kHz / mono / 16-bit.

## Original scale, MP3

`scale.mp3` (VBR with Xing/LAME gapless metadata) and `scale-cbr.mp3`
(CBR without an index) derive only from the same original CC0 scale.
Generated locally with FFmpeg n9.0.2/libmp3lame; this CLI is not linked into
or distributed with Doremi. The unchanged Symphonia 0.5.5 MP3 decoder
uses the same MPL-2.0 attribution above. No decoder source was copied.

```sh
ffmpeg -nostdin -v error -n -i assets/audio/scale.wav -map_metadata -1 -c:a libmp3lame -q:a 3 -fflags +bitexact -flags:a +bitexact crates/doremi-player/tests/fixtures/scale.mp3
ffmpeg -nostdin -v error -n -i assets/audio/scale.wav -map_metadata -1 -c:a libmp3lame -b:a 128k -write_xing 0 -fflags +bitexact -flags:a +bitexact crates/doremi-player/tests/fixtures/scale-cbr.mp3
```

SHA-256:
- VBR: `7652a88f8c73f63b3531edc3de5d6424e3fa2aa8e1334cb93d98a3a891aaa6a0`.
- CBR: `8871f1475ff1425dbd59b5981405ec04e3034453ae71e8d85ac29a99217aa906`.

Tests compare signal error against the original, exact gapless duration,
CBR frame-scan duration, every sample after seeks with bounded decoder
preroll, EOF/backward seek, truncation, metadata bounds and corrupt index counts.
