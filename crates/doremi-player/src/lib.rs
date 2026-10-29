//! Small local-audio slice. PCM is fully prepared on a worker and hard-capped;
//! the native callback only reads immutable samples and atomics. No streaming
//! or arbitrary local-library support is claimed by these test fixtures.
#[cfg(feature = "ogg-vorbis")]
mod ogg;
#[cfg(feature = "native-output")]
mod output;
use doremi_core::media::{Media, MediaId, ProviderId};
#[cfg(feature = "ogg-vorbis")]
pub use ogg::{MAX_OGG_BYTES, decode_ogg, decode_ogg_cancellable};
#[cfg(feature = "native-output")]
pub use output::{Engine, EngineError, EngineEvent, Metrics};

pub const MAX_SAMPLES: usize = 1_000_000;
pub const DURATION_MS: u64 = 8000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Scale,
    Chord,
}
impl Builtin {
    pub fn id(self) -> &'static str {
        match self {
            Self::Scale => "tone-scale",
            Self::Chord => "tone-chord",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Scale => "Doremi: escala original",
            Self::Chord => "Doremi: arpegio original",
        }
    }
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Scale => include_bytes!("../../../assets/audio/scale.wav"),
            Self::Chord => include_bytes!("../../../assets/audio/chord.wav"),
        }
    }
    pub fn media(self) -> Media {
        Media {
            id: MediaId::new(ProviderId::new("demo").unwrap(), self.id()).unwrap(),
            title: self.title().into(),
            duration_ms: Some(DURATION_MS),
        }
    }
    pub fn from_media(id: &MediaId) -> Option<Self> {
        if id.provider.as_str() != "demo" {
            return None;
        }
        match id.local() {
            "tone-scale" => Some(Self::Scale),
            "tone-chord" => Some(Self::Chord),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    TooLarge,
    InvalidWav,
    InvalidOgg,
    Cancelled,
    UnsupportedFormat,
    InvalidOutputRate,
}
pub struct Pcm {
    samples: Vec<f32>,
    rate: u32,
    channels: usize,
}
impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
    pub fn duration_ms(&self) -> u64 {
        self.frames() as u64 * 1000 / u64::from(self.rate)
    }
    pub fn byte_len(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }
    fn frame(&self, position: f64) -> [f32; 2] {
        let index = (position as usize).min(self.frames() - 1);
        let next = (index + 1).min(self.frames() - 1);
        let fraction = (position - position.floor()) as f32;
        let sample = |channel: usize| {
            let a = self.samples[index * self.channels + channel];
            let b = self.samples[next * self.channels + channel];
            a + (b - a) * fraction
        };
        if self.channels == 1 {
            let mono = sample(0);
            [mono, mono]
        } else {
            [sample(0), sample(1)]
        }
    }
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, DecodeError> {
    let data = bytes.get(at..at + 2).ok_or(DecodeError::InvalidWav)?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, DecodeError> {
    let data = bytes.get(at..at + 4).ok_or(DecodeError::InvalidWav)?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}
pub fn decode_wav(bytes: &[u8]) -> Result<Pcm, DecodeError> {
    use DecodeError::*;
    if bytes.len() > MAX_SAMPLES * 2 + 4096 {
        return Err(TooLarge);
    }
    if bytes.get(0..4) != Some(b"RIFF")
        || bytes.get(8..12) != Some(b"WAVE")
        || u64::from(u32_at(bytes, 4)?) + 8 != bytes.len() as u64
    {
        return Err(InvalidWav);
    }
    let mut offset = 12_usize;
    let mut format = None;
    let mut data = None;
    let mut chunks = 0;
    while offset < bytes.len() {
        chunks += 1;
        if chunks > 256 {
            return Err(TooLarge);
        }
        let length = u32_at(bytes, offset.checked_add(4).ok_or(InvalidWav)?)? as usize;
        let start = offset.checked_add(8).ok_or(InvalidWav)?;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or(InvalidWav)?;
        match bytes.get(offset..offset + 4) {
            Some(b"fmt ") => {
                if format.is_some() || length < 16 {
                    return Err(InvalidWav);
                }
                let channels = usize::from(u16_at(bytes, start + 2)?);
                let rate = u32_at(bytes, start + 4)?;
                if u16_at(bytes, start)? != 1
                    || u16_at(bytes, start + 14)? != 16
                    || !(1..=2).contains(&channels)
                {
                    return Err(UnsupportedFormat);
                }
                if !(8000..=192000).contains(&rate)
                    || usize::from(u16_at(bytes, start + 12)?) != channels * 2
                    || u32_at(bytes, start + 8)? != rate * (channels as u32) * 2
                {
                    return Err(InvalidWav);
                }
                format = Some((channels, rate));
            }
            Some(b"data") => {
                if data.is_some() {
                    return Err(InvalidWav);
                }
                data = Some(&bytes[start..end]);
            }
            _ => {}
        }
        offset = end
            .checked_add(length % 2)
            .filter(|end| *end <= bytes.len())
            .ok_or(InvalidWav)?;
    }
    let (channels, rate) = format.ok_or(InvalidWav)?;
    let data = data.ok_or(InvalidWav)?;
    if data.is_empty() || data.len() % (channels * 2) != 0 {
        return Err(InvalidWav);
    }
    if data.len() / 2 > MAX_SAMPLES {
        return Err(TooLarge);
    }
    let samples = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|s| f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0)
        .collect();
    Ok(Pcm {
        samples,
        rate,
        channels,
    })
}
/// Decoder clock, not a second queue or playback-intent owner.
pub struct Cursor {
    frame: u64,
    start: f64,
    output_rate: u32,
}
impl Cursor {
    pub fn new(pcm: &Pcm, output_rate: u32, position_ms: u64) -> Result<Self, DecodeError> {
        if !(8000..=192000).contains(&output_rate) {
            return Err(DecodeError::InvalidOutputRate);
        }
        Ok(Self {
            frame: 0,
            start: (position_ms.min(pcm.duration_ms()) as f64 * pcm.rate as f64 / 1000.0)
                .min(pcm.frames() as f64),
            output_rate,
        })
    }
    pub fn position_ms(&self, pcm: &Pcm) -> u64 {
        ((self.start * 1000.0 / pcm.rate as f64) as u64
            + self.frame * 1000 / u64::from(self.output_rate))
        .min(pcm.duration_ms())
    }
    pub fn next_frame(&mut self, pcm: &Pcm, gain: f32) -> Option<[f32; 2]> {
        let position = self.start + self.frame as f64 * pcm.rate as f64 / self.output_rate as f64;
        if position >= pcm.frames() as f64 {
            return None;
        }
        self.frame += 1;
        let frame = pcm.frame(position);
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some([frame[0] * gain, frame[1] * gain])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_fixtures_decode_with_hard_memory_bounds_and_nonzero_pcm() {
        for fixture in [Builtin::Scale, Builtin::Chord] {
            let pcm = decode_wav(fixture.bytes()).unwrap();
            assert_eq!(pcm.duration_ms(), DURATION_MS);
            assert_eq!(pcm.sample_rate(), 48000);
            assert!(pcm.byte_len() <= MAX_SAMPLES * 4);
            assert!(pcm.samples.iter().any(|value| value.abs() > 0.05));
            assert!(pcm.samples.iter().all(|value| value.abs() <= 1.0));
        }
    }
    #[test]
    fn malformed_and_truncated_wav_is_rejected_before_playback() {
        let valid = Builtin::Scale.bytes();
        assert!(decode_wav(&valid[..valid.len() - 1]).is_err());
        let mut bad = valid.to_vec();
        bad[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_wav(&bad).is_err());
        let mut bad = valid.to_vec();
        bad[22..24].copy_from_slice(&0_u16.to_le_bytes());
        assert!(decode_wav(&bad).is_err());
        let mut bad = valid.to_vec();
        bad[34..36].copy_from_slice(&24_u16.to_le_bytes());
        assert!(matches!(
            decode_wav(&bad),
            Err(DecodeError::UnsupportedFormat)
        ));
    }
    #[test]
    fn seek_resampling_and_zero_volume_advance_the_pcm_clock() {
        let pcm = decode_wav(Builtin::Scale.bytes()).unwrap();
        let mut cursor = Cursor::new(&pcm, 44100, 2000).unwrap();
        for _ in 0..44100 {
            assert_eq!(cursor.next_frame(&pcm, 0.0), Some([0.0, 0.0]));
        }
        assert_eq!(cursor.position_ms(&pcm), 3000);
        let mut cursor = Cursor::new(&pcm, 44100, 0).unwrap();
        let mut frames = 0;
        while cursor.next_frame(&pcm, 1.0).is_some() {
            frames += 1;
        }
        assert_eq!(frames, 44100 * 8);
        assert_eq!(cursor.position_ms(&pcm), 8000);
    }
}
