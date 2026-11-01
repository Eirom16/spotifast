//! Worker-only MPEG Layer III reader. A bounded frame scan establishes duration
//! rather than trusting a bitrate estimate; Symphonia performs audio decoding.
//! Encoder delay/padding are validated and removed without unsigned subtraction
//! inside a demuxer configured for gapless playback on untrusted header values.
use crate::{
    DecodeError,
    wav_stream::{FrameReader, Info},
};
use std::io::{BufReader, Read, Seek, SeekFrom};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_MP3, Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

pub const ENCODED_BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;
pub const MAX_PACKET_FRAMES: usize = 1152;
#[derive(Clone, Copy)]
struct Header {
    rate: u32,
    channels: usize,
    bytes: usize,
    samples: u64,
    mpeg1: bool,
}
fn header(bytes: [u8; 4]) -> Result<Header, DecodeError> {
    let h = u32::from_be_bytes(bytes);
    let version = (h >> 19) & 3;
    if h >> 21 != 0x7ff || version == 1 || (h >> 17) & 3 != 1 {
        return Err(DecodeError::InvalidMp3);
    }
    let rate_index = (h >> 10) & 3;
    let bitrate_index = (h >> 12) & 15;
    if rate_index == 3 || bitrate_index == 0 || bitrate_index == 15 {
        return Err(DecodeError::UnsupportedFormat);
    }
    let mpeg1 = version == 3;
    let mut rate = [44100, 48000, 32000][rate_index as usize];
    if version == 2 {
        rate /= 2;
    } else if version == 0 {
        rate /= 4;
    }
    let bitrate = if mpeg1 {
        [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ][bitrate_index as usize]
    } else {
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160][bitrate_index as usize]
    };
    let size = (if mpeg1 { 144 } else { 72 }) * bitrate * 1000 / rate + ((h >> 9) & 1);
    if !(24..=4096).contains(&size) {
        return Err(DecodeError::InvalidMp3);
    }
    Ok(Header {
        rate,
        channels: if (h >> 6) & 3 == 3 { 1 } else { 2 },
        bytes: size as usize,
        samples: if mpeg1 { 1152 } else { 576 },
        mpeg1,
    })
}
// The first MPEG packet may contain an index instead of audio. Recognize the
// standard index markers and validate their duration against the scanned frames.
fn index_packet(bytes: &[u8], h: Header) -> Result<Option<Option<u64>>, DecodeError> {
    let side = if h.mpeg1 {
        if h.channels == 1 { 17 } else { 32 }
    } else if h.channels == 1 {
        9
    } else {
        17
    };
    for at in [4 + side, 6 + side] {
        if bytes.len() >= at + 8 && matches!(&bytes[at..at + 4], b"Xing" | b"Info") {
            let flags = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap());
            if flags & !15 != 0 {
                return Err(DecodeError::InvalidMp3);
            }
            let frames = if flags & 1 != 0 {
                let b = bytes.get(at + 8..at + 12).ok_or(DecodeError::InvalidMp3)?;
                Some(u64::from(u32::from_be_bytes(b.try_into().unwrap())))
            } else {
                None
            };
            return Ok(Some(frames));
        }
    }
    if bytes.len() >= 62 && &bytes[36..40] == b"VBRI" {
        if bytes[40..42] != [0, 1] {
            return Err(DecodeError::UnsupportedFormat);
        }
        return Ok(Some(Some(u64::from(u32::from_be_bytes(
            bytes[50..54].try_into().unwrap(),
        )))));
    }
    Ok(None)
}
fn scan(input: &mut dyn MediaSource) -> Result<Info, DecodeError> {
    let length = input.byte_len().ok_or(DecodeError::UnsupportedFormat)?;
    if length > MAX_FILE_BYTES {
        return Err(DecodeError::TooLarge);
    }
    let mut reader = BufReader::with_capacity(ENCODED_BUFFER_BYTES, &mut *input);
    let mut start = 0_u64;
    for _ in 0..4 {
        reader
            .seek(SeekFrom::Start(start))
            .map_err(|_| DecodeError::Input)?;
        let mut tag = [0; 10];
        reader
            .read_exact(&mut tag)
            .map_err(|_| DecodeError::InvalidMp3)?;
        if &tag[..3] != b"ID3" {
            break;
        }
        if !(2..=4).contains(&tag[3]) || tag[6..10].iter().any(|b| b & 0x80 != 0) {
            return Err(DecodeError::InvalidMp3);
        }
        let size = tag[6..10]
            .iter()
            .fold(0_u64, |size, b| (size << 7) | u64::from(*b));
        start = start
            .checked_add(
                10 + size
                    + if tag[3] == 4 && tag[5] & 0x10 != 0 {
                        10
                    } else {
                        0
                    },
            )
            .ok_or(DecodeError::TooLarge)?;
        if start > MAX_METADATA_BYTES {
            return Err(DecodeError::TooLarge);
        }
    }
    let mut end = length;
    if end >= 128 {
        reader
            .seek(SeekFrom::Start(end - 128))
            .map_err(|_| DecodeError::Input)?;
        let mut tag = [0; 3];
        reader
            .read_exact(&mut tag)
            .map_err(|_| DecodeError::Input)?;
        if &tag == b"TAG" {
            end -= 128;
        }
    }
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|_| DecodeError::Input)?;
    let mut at = start;
    let mut first = None;
    let mut frames = 0_u64;
    let mut declared = None;
    let mut bytes = [0; 4096];
    while at < end {
        if end - at < 4 {
            return Err(DecodeError::InvalidMp3);
        }
        reader
            .read_exact(&mut bytes[..4])
            .map_err(|_| DecodeError::InvalidMp3)?;
        let h = header(bytes[..4].try_into().unwrap())?;
        if end - at < h.bytes as u64 {
            return Err(DecodeError::InvalidMp3);
        }
        reader
            .read_exact(&mut bytes[4..h.bytes])
            .map_err(|_| DecodeError::InvalidMp3)?;
        if let Some(previous) = first {
            let previous: Header = previous;
            if previous.rate != h.rate
                || previous.channels != h.channels
                || previous.samples != h.samples
            {
                return Err(DecodeError::UnsupportedFormat);
            }
            if index_packet(&bytes[..h.bytes], h)?.is_some() {
                return Err(DecodeError::UnsupportedFormat);
            }
            frames += h.samples;
        } else {
            first = Some(h);
            if let Some(count) = index_packet(&bytes[..h.bytes], h)? {
                declared = count;
            } else {
                frames += h.samples;
            }
        }
        at += h.bytes as u64;
    }
    let h = first.ok_or(DecodeError::InvalidMp3)?;
    if frames == 0 || declared.is_some_and(|n| n.checked_mul(h.samples) != Some(frames)) {
        return Err(DecodeError::InvalidMp3);
    }
    drop(reader);
    input
        .seek(SeekFrom::Start(0))
        .map_err(|_| DecodeError::Input)?;
    Ok(Info {
        rate: h.rate,
        channels: h.channels,
        frames,
    })
}

pub struct Mp3 {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    info: Info,
    track: u32,
    delay: u64,
    raw_end: u64,
    raw_position: u64,
    position: u64,
    buffer: Vec<f32>,
    cursor: usize,
}
impl Mp3 {
    pub fn open(mut input: Box<dyn MediaSource>) -> Result<Self, DecodeError> {
        let scanned = scan(input.as_mut())?;
        let stream = MediaSourceStream::new(
            input,
            MediaSourceStreamOptions {
                buffer_len: ENCODED_BUFFER_BYTES,
            },
        );
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let metadata = MetadataOptions {
            limit_metadata_bytes: Limit::Maximum(16 * 1024),
            limit_visual_bytes: Limit::Maximum(0),
        };
        let format = symphonia::default::get_probe()
            .format(&hint, stream, &FormatOptions::default(), &metadata)
            .map_err(|_| DecodeError::InvalidMp3)?
            .format;
        if format.tracks().len() != 1 {
            return Err(DecodeError::UnsupportedFormat);
        }
        let track = format.default_track().ok_or(DecodeError::InvalidMp3)?;
        let params = &track.codec_params;
        if params.codec != CODEC_TYPE_MP3
            || params.sample_rate != Some(scanned.rate)
            || params
                .channels
                .is_none_or(|c| c.count() != scanned.channels)
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        let delay = u64::from(params.delay.unwrap_or(0));
        let padding = u64::from(params.padding.unwrap_or(0));
        let frames = scanned
            .frames
            .checked_sub(delay)
            .and_then(|n| n.checked_sub(padding))
            .filter(|n| *n > 0)
            .ok_or(DecodeError::InvalidMp3)?;
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|_| DecodeError::UnsupportedFormat)?;
        let id = track.id;
        Ok(Self {
            format,
            decoder,
            info: Info { frames, ..scanned },
            track: id,
            delay,
            raw_end: scanned.frames - padding,
            raw_position: 0,
            position: 0,
            buffer: Vec::new(),
            cursor: 0,
        })
    }
    fn raw_frame(&mut self) -> Result<[f32; 2], DecodeError> {
        if self.cursor == self.buffer.len() {
            let packet = self
                .format
                .next_packet()
                .map_err(|_| DecodeError::InvalidMp3)?;
            if packet.track_id() != self.track || packet.ts() != self.raw_position {
                return Err(DecodeError::InvalidMp3);
            }
            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|_| DecodeError::InvalidMp3)?;
            if decoded.spec().rate != self.info.rate
                || decoded.spec().channels.count() != self.info.channels
                || decoded.capacity() > MAX_PACKET_FRAMES
                || decoded.frames() == 0
            {
                return Err(DecodeError::InvalidMp3);
            }
            let mut converted =
                SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            converted.copy_interleaved_ref(decoded);
            self.buffer.clear();
            self.buffer.reserve(converted.samples().len());
            for sample in converted.samples() {
                if !sample.is_finite() {
                    return Err(DecodeError::InvalidMp3);
                }
                self.buffer.push(sample.clamp(-1.0, 1.0));
            }
            self.cursor = 0;
        }
        let left = self.buffer[self.cursor];
        let right = if self.info.channels == 1 {
            left
        } else {
            self.buffer[self.cursor + 1]
        };
        self.cursor += self.info.channels;
        self.raw_position += 1;
        Ok([left, right])
    }
}
impl FrameReader for Mp3 {
    fn info(&self) -> Info {
        self.info
    }
    fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        let frame = frame.min(self.info.frames);
        self.buffer.clear();
        self.cursor = 0;
        if frame == self.info.frames {
            self.position = frame;
            self.raw_position = self.raw_end;
            return Ok(());
        }
        let target = frame + self.delay;
        let seek = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    // Short low-bitrate VBR packets can need more reservoir
                    // history than the demuxer's reference-frame ring retains.
                    // Decode a bounded preroll on this worker before the target.
                    ts: target.saturating_sub(32 * MAX_PACKET_FRAMES as u64),
                    track_id: self.track,
                },
            )
            .map_err(|_| DecodeError::InvalidMp3)?;
        if seek.track_id != self.track || seek.actual_ts > target {
            return Err(DecodeError::InvalidMp3);
        }
        self.decoder.reset();
        self.raw_position = seek.actual_ts;
        while self.raw_position < target {
            self.raw_frame()?;
        }
        self.position = frame;
        Ok(())
    }
    fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        if self.position == self.info.frames {
            return Ok(None);
        }
        while self.raw_position < self.delay {
            self.raw_frame()?;
        }
        let frame = self.raw_frame()?;
        self.position += 1;
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    const VBR: &[u8] = include_bytes!("../tests/fixtures/scale.mp3");
    const CBR: &[u8] = include_bytes!("../tests/fixtures/scale-cbr.mp3");
    fn open(bytes: &[u8]) -> Result<Mp3, DecodeError> {
        Mp3::open(Box::new(Cursor::new(bytes.to_vec())))
    }
    #[test]
    fn original_vbr_gapless_duration_and_output() {
        let mut reader = open(VBR).unwrap();
        assert_eq!(
            (
                reader.info().rate,
                reader.info().channels,
                reader.info().frames
            ),
            (48000, 1, 384000)
        );
        let mut count = 0;
        let mut energy = 0.0;
        let original = crate::decode_wav(crate::Builtin::Scale.bytes()).unwrap();
        let mut error_energy = 0.0;
        while let Some([left, right]) = reader.read_frame().unwrap() {
            assert_eq!(left, right);
            assert!(left.is_finite() && left.abs() <= 1.0);
            energy += f64::from(left) * f64::from(left);
            let error = f64::from(left - original.frame(count as f64)[0]);
            error_energy += error * error;
            count += 1;
        }
        assert_eq!(count, 384000);
        assert!(energy > 100.0);
        assert!(
            error_energy / energy < 0.001,
            "gapless output is shifted or differs from the original signal"
        );
    }
    #[test]
    fn cbr_without_index_uses_scanned_frames() {
        let mut input = Cursor::new(CBR.to_vec());
        let expected = scan(&mut input).unwrap();
        let mut reader = open(CBR).unwrap();
        assert_eq!(reader.info().frames, expected.frames);
        assert!(expected.frames >= 384000 && expected.frames < 388000);
        let mut count = 0;
        while reader.read_frame().unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, expected.frames);
    }
    #[test]
    fn seek_matches_continuous_decode_with_bounded_preroll() {
        let mut sequential = open(VBR).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = sequential.read_frame().unwrap() {
            frames.push(frame);
        }
        let mut seeked = open(VBR).unwrap();
        for target in [96000, 24000, 370000, 0] {
            seeked.seek_frame(target).unwrap();
            for offset in 0..6000 {
                let actual = seeked.read_frame().unwrap().unwrap();
                let expected = frames[target as usize + offset];
                assert!(
                    (actual[0] - expected[0]).abs() < 0.0001,
                    "seek={target} offset={offset} actual={} expected={}",
                    actual[0],
                    expected[0]
                );
            }
        }
        seeked.seek_frame(u64::MAX).unwrap();
        assert_eq!(seeked.read_frame().unwrap(), None);
        seeked.seek_frame(0).unwrap();
        assert!(seeked.read_frame().unwrap().is_some());
    }
    #[test]
    fn rejects_truncation_and_oversized_metadata() {
        assert!(open(&VBR[..VBR.len() - 1]).is_err());
        let mut tag = vec![0; 32];
        tag[..10].copy_from_slice(&[b'I', b'D', b'3', 4, 0, 0, 0x7f, 0x7f, 0x7f, 0x7f]);
        assert!(matches!(open(&tag), Err(DecodeError::TooLarge)));
    }
    #[test]
    fn rejects_untrusted_index_frame_counts_before_decoding() {
        let mut corrupt = VBR.to_vec();
        let xing = corrupt.windows(4).position(|s| s == b"Xing").unwrap();
        corrupt[xing + 8..xing + 12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(open(&corrupt), Err(DecodeError::InvalidMp3)));
    }
}
