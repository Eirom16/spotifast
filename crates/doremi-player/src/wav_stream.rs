//! Seekable PCM16 WAV reader. Header scanning and encoded scratch space are bounded.
use super::DecodeError;
use std::io::{Read, Seek, SeekFrom};

pub const SCRATCH_BYTES: usize = 8192;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub rate: u32,
    pub channels: usize,
    pub frames: u64,
}
impl Info {
    pub fn duration_ms(self) -> u64 {
        self.frames * 1000 / u64::from(self.rate)
    }
}
pub struct Wav<R> {
    input: R,
    info: Info,
    offset: u64,
    frame: u64,
    buffer: [u8; SCRATCH_BYTES],
    cursor: usize,
    buffered: usize,
}
fn io(error: std::io::Error) -> DecodeError {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        DecodeError::InvalidWav
    } else {
        DecodeError::Input
    }
}
impl<R: Read + Seek> Wav<R> {
    pub fn open(mut input: R) -> Result<Self, DecodeError> {
        let length = input.seek(SeekFrom::End(0)).map_err(io)?;
        input.seek(SeekFrom::Start(0)).map_err(io)?;
        let mut header = [0; 12];
        input.read_exact(&mut header).map_err(io)?;
        if &header[..4] != b"RIFF"
            || &header[8..] != b"WAVE"
            || u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8 != length
        {
            return Err(DecodeError::InvalidWav);
        }
        let mut at = 12u64;
        let mut format = None;
        let mut data = None;
        let mut chunks = 0;
        while at < length {
            chunks += 1;
            if chunks > 256 {
                return Err(DecodeError::TooLarge);
            }
            if length - at < 8 {
                return Err(DecodeError::InvalidWav);
            }
            input.seek(SeekFrom::Start(at)).map_err(io)?;
            let mut chunk = [0; 8];
            input.read_exact(&mut chunk).map_err(io)?;
            let size = u64::from(u32::from_le_bytes(chunk[4..].try_into().unwrap()));
            let start = at + 8;
            let end = start
                .checked_add(size)
                .filter(|end| *end <= length)
                .ok_or(DecodeError::InvalidWav)?;
            match &chunk[..4] {
                b"fmt " => {
                    if format.is_some() || size < 16 {
                        return Err(DecodeError::InvalidWav);
                    }
                    let mut f = [0; 16];
                    input.read_exact(&mut f).map_err(io)?;
                    let channels = usize::from(u16::from_le_bytes(f[2..4].try_into().unwrap()));
                    let rate = u32::from_le_bytes(f[4..8].try_into().unwrap());
                    if u16::from_le_bytes(f[..2].try_into().unwrap()) != 1
                        || u16::from_le_bytes(f[14..16].try_into().unwrap()) != 16
                        || !(1..=2).contains(&channels)
                    {
                        return Err(DecodeError::UnsupportedFormat);
                    }
                    if !(8000..=192000).contains(&rate)
                        || u16::from_le_bytes(f[12..14].try_into().unwrap()) as usize
                            != channels * 2
                        || u32::from_le_bytes(f[8..12].try_into().unwrap())
                            != rate * channels as u32 * 2
                    {
                        return Err(DecodeError::InvalidWav);
                    }
                    format = Some((rate, channels));
                }
                b"data" => {
                    if data.is_some() {
                        return Err(DecodeError::InvalidWav);
                    }
                    data = Some((start, size));
                }
                _ => {}
            }
            at = end
                .checked_add(size % 2)
                .filter(|end| *end <= length)
                .ok_or(DecodeError::InvalidWav)?;
        }
        let (rate, channels) = format.ok_or(DecodeError::InvalidWav)?;
        let (offset, bytes) = data.ok_or(DecodeError::InvalidWav)?;
        if bytes == 0 || bytes % (channels as u64 * 2) != 0 {
            return Err(DecodeError::InvalidWav);
        }
        let mut wav = Self {
            input,
            info: Info {
                rate,
                channels,
                frames: bytes / (channels as u64 * 2),
            },
            offset,
            frame: 0,
            buffer: [0; SCRATCH_BYTES],
            cursor: 0,
            buffered: 0,
        };
        wav.seek_frame(0)?;
        Ok(wav)
    }
    pub fn info(&self) -> Info {
        self.info
    }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        self.frame = frame.min(self.info.frames);
        self.input
            .seek(SeekFrom::Start(
                self.offset + self.frame * self.info.channels as u64 * 2,
            ))
            .map_err(io)?;
        self.cursor = 0;
        self.buffered = 0;
        Ok(())
    }
    pub fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        if self.frame == self.info.frames {
            return Ok(None);
        }
        let width = self.info.channels * 2;
        if self.cursor == self.buffered {
            let bytes =
                ((self.info.frames - self.frame) * width as u64).min(SCRATCH_BYTES as u64) as usize;
            self.input
                .read_exact(&mut self.buffer[..bytes])
                .map_err(io)?;
            self.cursor = 0;
            self.buffered = bytes;
        }
        let sample =
            |at| f32::from(i16::from_le_bytes([self.buffer[at], self.buffer[at + 1]])) / 32768.0;
        let left = sample(self.cursor);
        let right = if self.info.channels == 1 {
            left
        } else {
            sample(self.cursor + 2)
        };
        self.cursor += width;
        self.frame += 1;
        Ok(Some([left, right]))
    }
}
/// Linear interpolation stays on the decode worker, outside the output callback.
pub struct Resampler<R> {
    wav: Wav<R>,
    output_rate: u32,
    start: f64,
    output_frame: u64,
    source_frame: u64,
    left: [f32; 2],
    right: [f32; 2],
}
impl<R: Read + Seek> Resampler<R> {
    pub fn new(mut wav: Wav<R>, output_rate: u32, position_ms: u64) -> Result<Self, DecodeError> {
        if !(8000..=192000).contains(&output_rate) {
            return Err(DecodeError::InvalidOutputRate);
        }
        let start =
            (position_ms as f64 * wav.info.rate as f64 / 1000.0).min(wav.info.frames as f64);
        let source_frame = start.floor() as u64;
        wav.seek_frame(source_frame)?;
        let left = wav.read_frame()?.unwrap_or([0.0; 2]);
        let right = wav.read_frame()?.unwrap_or(left);
        Ok(Self {
            wav,
            output_rate,
            start,
            output_frame: 0,
            source_frame,
            left,
            right,
        })
    }
    pub fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        let position = self.start
            + self.output_frame as f64 * self.wav.info.rate as f64 / self.output_rate as f64;
        if position >= self.wav.info.frames as f64 {
            return Ok(None);
        }
        let index = position.floor() as u64;
        while self.source_frame < index {
            self.left = self.right;
            self.right = self.wav.read_frame()?.unwrap_or(self.left);
            self.source_frame += 1;
        }
        let fraction = (position - position.floor()) as f32;
        self.output_frame += 1;
        Ok(Some([
            self.left[0] + (self.right[0] - self.left[0]) * fraction,
            self.left[1] + (self.right[1] - self.left[1]) * fraction,
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn streaming_resampling_matches_prepared_pcm_at_seek_and_eof() {
        for builtin in [super::super::Builtin::Scale, super::super::Builtin::Chord] {
            let pcm = super::super::decode_wav(builtin.bytes()).unwrap();
            let mut expected = super::super::Cursor::new(&pcm, 44100, 2501).unwrap();
            let mut actual = Resampler::new(
                Wav::open(Cursor::new(builtin.bytes())).unwrap(),
                44100,
                2501,
            )
            .unwrap();
            while let Some(frame) = expected.next_frame(&pcm, 1.0) {
                assert_eq!(actual.read_frame().unwrap(), Some(frame));
            }
            assert_eq!(actual.read_frame().unwrap(), None);
        }
    }
    #[test]
    fn malformed_data_cannot_be_silently_treated_as_normal_eof() {
        let data = super::super::Builtin::Scale.bytes();
        assert!(Wav::open(Cursor::new(&data[..data.len() - 1])).is_err());
        let mut bad = data.to_vec();
        bad[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Wav::open(Cursor::new(bad)).is_err());
        let mut wav = Wav::open(Cursor::new(data.to_vec())).unwrap();
        wav.input.get_mut().truncate(100);
        assert_eq!(wav.read_frame(), Err(DecodeError::InvalidWav));
    }
}
