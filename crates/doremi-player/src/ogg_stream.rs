//! Incremental, seekable Vorbis input. The host must authorize and authenticate
//! encoded bytes before exposing them here. One decoded packet is retained;
//! output/resampling use the existing native player's bounded PCM feed.
use crate::{
    DecodeError,
    wav_stream::{FrameReader, Info},
};
use std::io::{Read, Seek, SeekFrom};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_VORBIS, Decoder, DecoderOptions},
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions},
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

pub struct Vorbis {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    info: Info,
    track: u32,
    position: u64,
    buffer: Vec<f32>,
    cursor: usize,
}
impl Vorbis {
    pub fn from_read_seek<R: Read + Seek + Send + Sync + 'static>(
        input: R,
        length: u64,
        expected: Info,
    ) -> Result<Self, DecodeError> {
        Self::open(Box::new(Seekable { input, length }), expected)
    }
    pub fn open(input: Box<dyn MediaSource>, expected: Info) -> Result<Self, DecodeError> {
        if !(8000..=192000).contains(&expected.rate)
            || !(1..=2).contains(&expected.channels)
            || expected.frames == 0
            || expected.frames > u64::from(expected.rate) * 600
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        let stream = MediaSourceStream::new(
            input,
            MediaSourceStreamOptions {
                buffer_len: 64 * 1024,
            },
        );
        let mut hint = Hint::new();
        hint.with_extension("ogg");
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
            .map_err(|_| DecodeError::InvalidOgg)?
            .format;
        if format.tracks().len() != 1 {
            return Err(DecodeError::UnsupportedFormat);
        }
        let track = format.default_track().ok_or(DecodeError::InvalidOgg)?;
        let params = &track.codec_params;
        if params.codec != CODEC_TYPE_VORBIS
            || params.sample_rate != Some(expected.rate)
            || params
                .channels
                .is_none_or(|channels| channels.count() != expected.channels)
            || params
                .n_frames
                .is_some_and(|frames| frames != expected.frames)
        {
            return Err(DecodeError::UnsupportedFormat);
        }
        crate::vorbis_limits::check(
            params
                .extra_data
                .as_deref()
                .ok_or(DecodeError::InvalidOgg)?,
        )?;
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
        })
    }
    fn packet(&mut self) -> Result<(), DecodeError> {
        for _ in 0..32 {
            let packet = self
                .format
                .next_packet()
                .map_err(|_| DecodeError::InvalidOgg)?;
            if packet.track_id() != self.track {
                return Err(DecodeError::UnsupportedFormat);
            }
            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|_| DecodeError::InvalidOgg)?;
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
                || decoded.frames() as u64 > self.info.frames - self.position
            {
                return Err(DecodeError::InvalidOgg);
            }
            let mut converted =
                SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
            converted.copy_interleaved_ref(decoded);
            self.buffer.clear();
            if self.buffer.capacity() < converted.samples().len() {
                self.buffer.reserve_exact(converted.samples().len());
            }
            for sample in converted.samples() {
                if !sample.is_finite() {
                    return Err(DecodeError::InvalidOgg);
                }
                self.buffer.push(sample.clamp(-1.0, 1.0));
            }
            self.cursor = 0;
            return Ok(());
        }
        Err(DecodeError::InvalidOgg)
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
        let seek = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: frame.saturating_sub(8192),
                    track_id: self.track,
                },
            )
            .map_err(|_| DecodeError::InvalidOgg)?;
        if seek.track_id != self.track || seek.actual_ts > frame {
            return Err(DecodeError::InvalidOgg);
        }
        self.decoder.reset();
        self.position = seek.actual_ts;
        while self.position < frame {
            self.read_frame()?.ok_or(DecodeError::InvalidOgg)?;
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
