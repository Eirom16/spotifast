//! Seekable FLAC decoding on a worker, never in the native output callback.
//! Symphonia is used unchanged under MPL-2.0; encoded/decoded working sets and
//! metadata are bounded independently of the track duration.
use crate::{
    DecodeError,
    wav_stream::{FrameReader, Info},
};
use std::io::SeekFrom;
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_FLAC, Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

pub const ENCODED_BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;
pub const MAX_BLOCK_FRAMES: usize = 65535;

// Reject excessive seek tables/pictures before a demuxer can allocate them.
// This scanner only reads four-byte headers and seeks over bounded metadata.
fn envelope(input: &mut dyn MediaSource) -> Result<(), DecodeError> {
    input
        .seek(SeekFrom::Start(0))
        .map_err(|_| DecodeError::Input)?;
    let mut signature = [0; 4];
    input
        .read_exact(&mut signature)
        .map_err(|_| DecodeError::InvalidFlac)?;
    if &signature != b"fLaC" {
        return Err(DecodeError::InvalidFlac);
    }
    let mut total = 4_u64;
    for block in 0..128 {
        let mut header = [0; 4];
        input
            .read_exact(&mut header)
            .map_err(|_| DecodeError::InvalidFlac)?;
        let length = u64::from(u32::from_be_bytes([0, header[1], header[2], header[3]]));
        if block == 0 && (header[0] & 0x7f != 0 || length != 34) {
            return Err(DecodeError::InvalidFlac);
        }
        total = total.checked_add(4 + length).ok_or(DecodeError::TooLarge)?;
        if total > MAX_METADATA_BYTES {
            return Err(DecodeError::TooLarge);
        }
        if input.byte_len().is_some_and(|len| total >= len) {
            return Err(DecodeError::InvalidFlac);
        }
        input
            .seek(SeekFrom::Current(length as i64))
            .map_err(|_| DecodeError::Input)?;
        if header[0] & 0x80 != 0 {
            input
                .seek(SeekFrom::Start(0))
                .map_err(|_| DecodeError::Input)?;
            return Ok(());
        }
    }
    Err(DecodeError::TooLarge)
}

pub struct Flac {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    info: Info,
    track: u32,
    buffer: Vec<f32>,
    cursor: usize,
    position: u64,
}
impl Flac {
    pub fn open(mut input: Box<dyn MediaSource>) -> Result<Self, DecodeError> {
        envelope(input.as_mut())?;
        let stream = MediaSourceStream::new(
            input,
            MediaSourceStreamOptions {
                buffer_len: ENCODED_BUFFER_BYTES,
            },
        );
        let mut hint = Hint::new();
        hint.with_extension("flac");
        let metadata = MetadataOptions {
            limit_metadata_bytes: Limit::Maximum(16 * 1024),
            limit_visual_bytes: Limit::Maximum(0),
        };
        let probed = symphonia::default::get_probe()
            .format(&hint, stream, &FormatOptions::default(), &metadata)
            .map_err(|_| DecodeError::InvalidFlac)?;
        let format = probed.format;
        if format.tracks().len() != 1 {
            return Err(DecodeError::UnsupportedFormat);
        }
        let track = format.default_track().ok_or(DecodeError::InvalidFlac)?;
        let params = &track.codec_params;
        if params.codec != CODEC_TYPE_FLAC {
            return Err(DecodeError::UnsupportedFormat);
        }
        let rate = params.sample_rate.ok_or(DecodeError::InvalidFlac)?;
        let channels = params.channels.ok_or(DecodeError::InvalidFlac)?.count();
        let frames = params
            .n_frames
            .filter(|n| *n > 0 && *n < (1_u64 << 36))
            .ok_or(DecodeError::UnsupportedFormat)?;
        if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
            return Err(DecodeError::UnsupportedFormat);
        }
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|_| DecodeError::UnsupportedFormat)?;
        let id = track.id;
        Ok(Self {
            format,
            decoder,
            info: Info {
                rate,
                channels,
                frames,
            },
            track: id,
            buffer: Vec::new(),
            cursor: 0,
            position: 0,
        })
    }
    fn packet(&mut self) -> Result<(), DecodeError> {
        let packet = self
            .format
            .next_packet()
            .map_err(|_| DecodeError::InvalidFlac)?;
        if packet.track_id() != self.track || packet.ts() != self.position {
            return Err(DecodeError::InvalidFlac);
        }
        let decoded = self
            .decoder
            .decode(&packet)
            .map_err(|_| DecodeError::InvalidFlac)?;
        if decoded.spec().rate != self.info.rate
            || decoded.spec().channels.count() != self.info.channels
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        if decoded.frames() == 0
            || decoded.capacity() > MAX_BLOCK_FRAMES
            || decoded.frames() as u64 > self.info.frames - self.position
        {
            return Err(DecodeError::InvalidFlac);
        }
        let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        converted.copy_interleaved_ref(decoded);
        // Keep one packet only. Decoder and conversion allocations are worker
        // work, bounded to one FLAC block, with no copy of the complete track.
        self.buffer.clear();
        if self.buffer.capacity() < converted.samples().len() {
            self.buffer.reserve_exact(converted.samples().len());
        }
        for sample in converted.samples() {
            if !sample.is_finite() {
                return Err(DecodeError::InvalidFlac);
            }
            self.buffer.push(sample.clamp(-1.0, 1.0));
        }
        self.cursor = 0;
        Ok(())
    }
}
impl FrameReader for Flac {
    fn info(&self) -> Info {
        self.info
    }
    fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        let frame = frame.min(self.info.frames);
        self.buffer.clear();
        self.cursor = 0;
        if frame == self.info.frames {
            self.position = frame;
            return Ok(());
        }
        let seek = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: frame,
                    track_id: self.track,
                },
            )
            .map_err(|_| DecodeError::InvalidFlac)?;
        if seek.track_id != self.track || seek.actual_ts > frame {
            return Err(DecodeError::InvalidFlac);
        }
        self.decoder.reset();
        self.position = seek.actual_ts;
        while self.position < frame {
            self.read_frame()?.ok_or(DecodeError::InvalidFlac)?;
        }
        Ok(())
    }
    fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        if self.position == self.info.frames {
            return Ok(None);
        }
        if self.cursor == self.buffer.len() {
            self.packet()?;
        }
        let left = self.buffer[self.cursor];
        let right = if self.info.channels == 1 {
            left
        } else {
            self.buffer[self.cursor + 1]
        };
        self.cursor += self.info.channels;
        self.position += 1;
        Ok(Some([left, right]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav_stream::{LinearResampler, Resampler, Wav};
    use std::io::Cursor;
    const SOURCE: &[u8] = include_bytes!("../tests/fixtures/scale.flac");
    fn decoder() -> Flac {
        Flac::open(Box::new(Cursor::new(SOURCE))).unwrap()
    }
    #[test]
    fn original_lossless_samples_match_wav_and_seek_uses_the_same_resampler() {
        let mut actual = decoder();
        let mut expected = Wav::open(Cursor::new(crate::Builtin::Scale.bytes())).unwrap();
        assert_eq!(actual.info(), expected.info());
        while let Some(frame) = expected.read_frame().unwrap() {
            assert_eq!(actual.read_frame().unwrap(), Some(frame));
            assert!(actual.buffer.capacity() <= MAX_BLOCK_FRAMES * 2);
        }
        assert_eq!(actual.read_frame().unwrap(), None);
        let mut actual = LinearResampler::new(decoder(), 44100, 2501).unwrap();
        let mut expected = Resampler::new(
            Wav::open(Cursor::new(crate::Builtin::Scale.bytes())).unwrap(),
            44100,
            2501,
        )
        .unwrap();
        while let Some(frame) = expected.read_frame().unwrap() {
            assert_eq!(actual.read_frame().unwrap(), Some(frame));
        }
        assert_eq!(actual.read_frame().unwrap(), None);
    }
    #[test]
    fn seek_to_eof_and_back_and_truncated_data_have_explicit_results() {
        let mut source = decoder();
        source.seek_frame(u64::MAX).unwrap();
        assert_eq!(source.read_frame().unwrap(), None);
        source.seek_frame(0).unwrap();
        let mut original = decoder();
        for _ in 0..2000 {
            assert_eq!(source.read_frame().unwrap(), original.read_frame().unwrap());
        }
        let mut broken =
            Flac::open(Box::new(Cursor::new(SOURCE[..SOURCE.len() - 12].to_vec()))).unwrap();
        let mut failed = false;
        for _ in 0..broken.info.frames {
            match broken.read_frame() {
                Ok(Some(_)) => {}
                Err(_) => {
                    failed = true;
                    break;
                }
                Ok(None) => break,
            }
        }
        assert!(failed, "Truncation was silently accepted as complete audio");
    }
    #[test]
    fn excessive_or_invalid_metadata_is_rejected_before_demux_allocations() {
        let mut bytes = SOURCE.to_vec();
        // The second metadata block advertises over the complete 1 MiB budget.
        bytes[42..46].copy_from_slice(&[0x81, 0x10, 0, 0]);
        assert!(matches!(
            Flac::open(Box::new(Cursor::new(bytes))),
            Err(DecodeError::TooLarge)
        ));
        assert!(Flac::open(Box::new(Cursor::new(b"not FLAC".to_vec()))).is_err());
    }
}
