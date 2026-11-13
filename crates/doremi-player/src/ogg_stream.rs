//! Incremental, seekable Ogg/Vorbis and optional AAC/MP4 input. The host must authorize and authenticate
//! encoded bytes before exposing them here. One decoded packet is retained;
//! output/resampling use the existing native player's bounded PCM feed.
use crate::{
    DecodeError,
    wav_stream::{FrameReader, Info},
};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_AAC, CODEC_TYPE_VORBIS, CodecType, Decoder, DecoderOptions},
    errors::Error as FormatError,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::{Limit, MetadataOptions},
    probe::Hint,
};
fn input_or_format(error: FormatError, invalid: DecodeError, observed: &AtomicU8) -> DecodeError {
    match observed.load(Ordering::Acquire) {
        1 => return DecodeError::Input,
        2 => return DecodeError::Cancelled,
        3 => return DecodeError::AccessDenied,
        _ => {}
    }
    match error {
        FormatError::IoError(error) => match error.kind() {
            std::io::ErrorKind::ConnectionAborted => DecodeError::Cancelled,
            std::io::ErrorKind::PermissionDenied => DecodeError::AccessDenied,
            std::io::ErrorKind::UnexpectedEof => invalid,
            _ => DecodeError::Input,
        },
        _ => invalid,
    }
}

pub struct Vorbis {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    info: Info,
    track: u32,
    position: u64,
    buffer: Vec<f32>,
    cursor: usize,
    invalid: DecodeError,
    aac: bool,
    input_error: Arc<AtomicU8>,
}
impl Vorbis {
    pub fn from_read_seek<R: Read + Seek + Send + Sync + 'static>(
        input: R,
        length: u64,
        expected: Info,
    ) -> Result<Self, DecodeError> {
        Self::open(Box::new(Seekable { input, length }), expected)
    }
    #[cfg(feature = "aac-mp4")]
    pub fn aac_from_read_seek<R: Read + Seek + Send + Sync + 'static>(
        input: R,
        length: u64,
    ) -> Result<Self, DecodeError> {
        Self::open_format(Box::new(Seekable { input, length }), None, CODEC_TYPE_AAC)
    }
    pub fn open(input: Box<dyn MediaSource>, expected: Info) -> Result<Self, DecodeError> {
        Self::open_format(input, Some(expected), CODEC_TYPE_VORBIS)
    }
    fn open_format(
        input: Box<dyn MediaSource>,
        expected: Option<Info>,
        codec: CodecType,
    ) -> Result<Self, DecodeError> {
        let aac = codec == CODEC_TYPE_AAC;
        let invalid = if aac {
            DecodeError::InvalidMp4
        } else {
            DecodeError::InvalidOgg
        };
        if expected.is_some_and(|expected| {
            !(8000..=192000).contains(&expected.rate)
                || !(1..=2).contains(&expected.channels)
                || expected.frames == 0
                || expected.frames > u64::from(expected.rate) * 600
        }) {
            return Err(DecodeError::UnsupportedFormat);
        }
        let input_error = Arc::new(AtomicU8::new(0));
        let stream = MediaSourceStream::new(
            Box::new(ObservedInput {
                input,
                error: input_error.clone(),
            }),
            MediaSourceStreamOptions {
                buffer_len: 64 * 1024,
            },
        );
        let mut hint = Hint::new();
        hint.with_extension(if aac { "m4a" } else { "ogg" });
        let format = symphonia::default::get_probe()
            .format(
                &hint,
                stream,
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions {
                    limit_metadata_bytes: Limit::Maximum(16 * 1024),
                    limit_visual_bytes: Limit::Maximum(0),
                },
            )
            .map_err(|error| input_or_format(error, invalid, &input_error))?
            .format;
        if format.tracks().len() != 1 {
            return Err(DecodeError::UnsupportedFormat);
        }
        let track = format.default_track().ok_or(invalid)?;
        let params = &track.codec_params;
        let expected = match expected {
            Some(expected) => expected,
            None => Info {
                rate: params.sample_rate.ok_or(invalid)?,
                channels: params
                    .channels
                    .map(|channels| channels.count())
                    .or_else(|| {
                        if aac {
                            aac_channels(params.extra_data.as_deref()?, params.sample_rate?)
                        } else {
                            None
                        }
                    })
                    .ok_or(invalid)?,
                frames: params.n_frames.ok_or(invalid)?,
            },
        };
        if !(8000..=192000).contains(&expected.rate)
            || !(1..=2).contains(&expected.channels)
            || expected.frames == 0
            || expected.frames > u64::from(expected.rate) * 21600
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        if aac && params.time_base != Some(symphonia::core::units::TimeBase::new(1, expected.rate))
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        if params.codec != codec
            || params.sample_rate != Some(expected.rate)
            || (!aac && params.channels.is_none())
            || params
                .channels
                .is_some_and(|channels| channels.count() != expected.channels)
            || params
                .n_frames
                .is_some_and(|frames| frames != expected.frames)
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        if !aac {
            crate::vorbis_limits::check(params.extra_data.as_deref().ok_or(invalid)?)?;
        }
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|_| DecodeError::UnsupportedFormat)?;
        let id = track.id;
        Ok(Self {
            format,
            decoder,
            info: expected,
            track: id,
            position: 0,
            buffer: Vec::new(),
            cursor: 0,
            invalid,
            aac,
            input_error,
        })
    }
    fn packet(&mut self) -> Result<(), DecodeError> {
        for _ in 0..32 {
            self.input_error.store(0, Ordering::Release);
            let packet = self
                .format
                .next_packet()
                .map_err(|error| input_or_format(error, self.invalid, &self.input_error))?;
            if packet.track_id() != self.track {
                return Err(DecodeError::UnsupportedFormat);
            }
            let decoded = self.decoder.decode(&packet).map_err(|_| self.invalid)?;
            if decoded.spec().rate != self.info.rate
                || decoded.spec().channels.count() != self.info.channels
            {
                return Err(DecodeError::UnsupportedFormat);
            }
            if decoded.capacity() > 8192 {
                return Err(DecodeError::TooLarge);
            }
            if decoded.frames() == 0 {
                continue;
            }
            // Vorbis needs the preceding packet to rebuild overlap after a
            // seek. Its first packet may decode no frames; use the timestamp
            // of the packet that actually produced this buffer.
            self.position = packet.ts();
            if self.position > self.info.frames
                || (!self.aac && decoded.frames() as u64 > self.info.frames - self.position)
            {
                return Err(self.invalid);
            }
            let mut converted =
                SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            converted.copy_interleaved_ref(decoded);
            self.buffer.clear();
            if self.buffer.capacity() < converted.samples().len() {
                self.buffer.reserve_exact(converted.samples().len());
            }
            let count = if self.aac {
                converted
                    .samples()
                    .len()
                    .min((self.info.frames - self.position) as usize * self.info.channels)
            } else {
                converted.samples().len()
            };
            for sample in &converted.samples()[..count] {
                if !sample.is_finite() {
                    return Err(self.invalid);
                }
                self.buffer.push(sample.clamp(-1.0, 1.0));
            }
            self.cursor = 0;
            return Ok(());
        }
        Err(self.invalid)
    }
}
// Probe::next treats an initial read error as EOF. Keep only its typed cause,
// never an IO message/URL, across that layer so a denied stream isn't called
// corrupt media. This wrapper belongs exclusively to the decoder worker.
struct ObservedInput {
    input: Box<dyn MediaSource>,
    error: Arc<AtomicU8>,
}
impl ObservedInput {
    fn observe<T>(&self, result: std::io::Result<T>) -> std::io::Result<T> {
        if let Err(error) = &result {
            let code = match error.kind() {
                std::io::ErrorKind::UnexpectedEof => 0,
                std::io::ErrorKind::ConnectionAborted => 2,
                std::io::ErrorKind::PermissionDenied => 3,
                _ => 1,
            };
            if code != 0 {
                self.error.store(code, Ordering::Release);
            }
        }
        result
    }
}
impl Read for ObservedInput {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let result = self.input.read(bytes);
        self.observe(result)
    }
}
impl Seek for ObservedInput {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let result = self.input.seek(from);
        self.observe(result)
    }
}
impl MediaSource for ObservedInput {
    fn is_seekable(&self) -> bool {
        self.input.is_seekable()
    }
    fn byte_len(&self) -> Option<u64> {
        self.input.byte_len()
    }
}
struct Seekable<R> {
    input: R,
    length: u64,
}
impl<R: Read> Read for Seekable<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(bytes)
    }
}
impl<R: Seek> Seek for Seekable<R> {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.input.seek(from)
    }
}
impl<R: Read + Seek + Send + Sync> MediaSource for Seekable<R> {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}
impl FrameReader for Vorbis {
    fn info(&self) -> Info {
        self.info
    }
    fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        let frame = frame.min(self.info.frames);
        if frame == self.position {
            return Ok(());
        }
        self.buffer.clear();
        self.cursor = 0;
        if frame == self.info.frames {
            self.position = frame;
            return Ok(());
        }
        // Opening begins at the first audio packet; avoid an unnecessary seek.
        if frame == 0 && self.position == 0 {
            return Ok(());
        }
        self.input_error.store(0, Ordering::Release);
        let seek = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: frame.saturating_sub(8192),
                    track_id: self.track,
                },
            )
            .map_err(|error| input_or_format(error, self.invalid, &self.input_error))?;
        if seek.track_id != self.track || seek.actual_ts > frame {
            return Err(self.invalid);
        }
        self.decoder.reset();
        self.position = seek.actual_ts;
        while self.position < frame {
            self.read_frame()?.ok_or(self.invalid)?;
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

// Read only the fixed AAC-LC AudioSpecificConfig fields needed for bounded PCM
// allocation. The codec remains Symphonia; PCE/multichannel/HE-AAC are rejected.
fn aac_channels(bytes: &[u8], rate: u32) -> Option<usize> {
    let first = *bytes.first()?;
    let second = *bytes.get(1)?;
    let object = first >> 3;
    let index = ((first & 7) << 1) | (second >> 7);
    let channels = (second >> 3) & 15;
    let rates = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    (object == 2 && rates.get(index as usize) == Some(&rate) && (1..=2).contains(&channels))
        .then_some(channels as usize)
}

#[cfg(all(test, feature = "ogg-vorbis"))]
mod tests {
    use super::*;
    #[test]
    fn incremental_decode_matches_prepared_pcm_and_seek() {
        let bytes = include_bytes!("../tests/fixtures/scale.ogg");
        let expected = crate::decode_ogg(bytes).unwrap();
        let info = Info {
            rate: expected.sample_rate(),
            channels: expected.channels,
            frames: expected.frames() as u64,
        };
        let mut source =
            Vorbis::open(Box::new(std::io::Cursor::new(bytes.to_vec())), info).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = source.read_frame().unwrap() {
            frames.push(frame);
        }
        assert_eq!(frames.len(), expected.frames());
        for (index, frame) in frames.iter().enumerate() {
            for (channel, sample) in frame.iter().enumerate() {
                assert!(
                    (sample
                        - expected.samples
                            [index * expected.channels + channel.min(expected.channels - 1)])
                    .abs()
                        < 1e-5
                );
            }
        }
        source.seek_frame(48000 * 3).unwrap();
        for index in 0..4800 {
            let frame = source.read_frame().unwrap().unwrap();
            for (channel, sample) in frame.iter().enumerate() {
                assert!(
                    (sample
                        - expected.samples[(48000 * 3 + index) * expected.channels
                            + channel.min(expected.channels - 1)])
                    .abs()
                        < 1e-5
                );
            }
        }
    }
}

#[cfg(all(test, feature = "aac-mp4"))]
mod aac_tests {
    use super::*;
    struct Unreadable(std::io::ErrorKind);
    impl Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(self.0.into())
        }
    }
    impl Seek for Unreadable {
        fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
            Ok(0)
        }
    }
    #[test]
    fn input_refusals_and_cancellation_are_not_misreported_as_invalid_mp4() {
        for (kind, expected) in [
            (
                std::io::ErrorKind::PermissionDenied,
                DecodeError::AccessDenied,
            ),
            (std::io::ErrorKind::ConnectionReset, DecodeError::Input),
            (
                std::io::ErrorKind::ConnectionAborted,
                DecodeError::Cancelled,
            ),
        ] {
            assert!(
                matches!(Vorbis::aac_from_read_seek(Unreadable(kind),1024),Err(error) if error==expected)
            );
        }
        assert!(matches!(
            Vorbis::aac_from_read_seek(std::io::Cursor::new(vec![0; 12]), 12),
            Err(DecodeError::InvalidMp4)
        ));
    }
    #[test]
    fn incremental_aac_decode_and_seek_use_the_shared_frame_reader() {
        let bytes = include_bytes!("../tests/fixtures/scale.m4a");
        let mut source =
            Vorbis::aac_from_read_seek(std::io::Cursor::new(bytes.to_vec()), bytes.len() as u64)
                .unwrap();
        let info = source.info();
        assert!((7900..=8100).contains(&(info.frames * 1000 / u64::from(info.rate))));
        let mut frames = Vec::new();
        while let Some(frame) = source.read_frame().unwrap() {
            frames.push(frame);
        }
        assert_eq!(frames.len() as u64, info.frames);
        assert!(frames.iter().flatten().all(|sample| sample.is_finite()));
        assert!(frames.iter().any(|frame| frame[0].abs() > 0.01));
        let target = u64::from(info.rate) * 4;
        source.seek_frame(target).unwrap();
        for expected in frames.iter().skip(target as usize).take(32) {
            let actual = source.read_frame().unwrap().unwrap();
            for channel in 0..2 {
                assert!((actual[channel] - expected[channel]).abs() < 0.001);
            }
        }
    }
}
