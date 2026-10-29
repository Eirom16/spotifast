//! Short, bounded Ogg/Vorbis resources, prepared away from UI/audio callbacks.
//! Callers still own authorization/hash checking. This is not a streaming API.
use super::{DecodeError, MAX_SAMPLES, Pcm};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{CODEC_TYPE_VORBIS, DecoderOptions},
    errors::Error,
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};

pub const MAX_OGG_BYTES: usize = 256 * 1024;

// Bound the complete single-stream envelope before handing it to a decoder.
// Symphonia verifies page CRCs and codec contents. No chained/partial streams.
fn envelope(bytes: &[u8]) -> Result<(), DecodeError> {
    if bytes.len() > MAX_OGG_BYTES {
        return Err(DecodeError::TooLarge);
    }
    let mut offset = 0;
    let mut serial = None;
    let mut sequence = 0_u32;
    let mut ended = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 27)
            .ok_or(DecodeError::InvalidOgg)?;
        if ended || &header[..4] != b"OggS" || header[4] != 0 || header[5] & !7 != 0 {
            return Err(DecodeError::InvalidOgg);
        }
        let page_serial = u32::from_le_bytes(header[14..18].try_into().unwrap());
        let page_sequence = u32::from_le_bytes(header[18..22].try_into().unwrap());
        if serial.is_none() {
            if header[5] & 2 == 0 {
                return Err(DecodeError::InvalidOgg);
            }
            serial = Some(page_serial);
        } else if header[5] & 2 != 0 {
            return Err(DecodeError::InvalidOgg);
        }
        if serial != Some(page_serial) || page_sequence != sequence || sequence >= 4096 {
            return Err(DecodeError::InvalidOgg);
        }
        sequence += 1;
        let table_end = offset + 27 + usize::from(header[26]);
        let table = bytes
            .get(offset + 27..table_end)
            .ok_or(DecodeError::InvalidOgg)?;
        let end = table_end + table.iter().map(|n| usize::from(*n)).sum::<usize>();
        if end > bytes.len() {
            return Err(DecodeError::InvalidOgg);
        }
        ended = header[5] & 4 != 0;
        offset = end;
    }
    if ended {
        Ok(())
    } else {
        Err(DecodeError::InvalidOgg)
    }
}

pub fn decode_ogg(bytes: &[u8]) -> Result<Pcm, DecodeError> {
    decode_ogg_cancellable(bytes, || false)
}

pub fn decode_ogg_cancellable(
    bytes: &[u8],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Pcm, DecodeError> {
    use DecodeError::*;
    if cancelled() {
        return Err(Cancelled);
    }
    envelope(bytes)?;
    let source = MediaSourceStream::new(
        Box::new(std::io::Cursor::new(bytes.to_vec())),
        Default::default(),
    );
    let metadata = MetadataOptions {
        limit_metadata_bytes: Limit::Maximum(16 * 1024),
        limit_visual_bytes: Limit::Maximum(0),
    };
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let probed = symphonia::default::get_probe()
        .format(&hint, source, &options, &metadata)
        .map_err(|_| InvalidOgg)?;
    let mut format = probed.format;
    if format.tracks().len() != 1 {
        return Err(UnsupportedFormat);
    }
    let track = format.default_track().ok_or(InvalidOgg)?;
    if track.codec_params.codec != CODEC_TYPE_VORBIS {
        return Err(UnsupportedFormat);
    }
    let id = track.id;
    let rate = track.codec_params.sample_rate.ok_or(InvalidOgg)?;
    let channels = track.codec_params.channels.ok_or(InvalidOgg)?.count();
    if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
        return Err(UnsupportedFormat);
    }
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| UnsupportedFormat)?;
    // Exact capacity avoids geometric Vec growth exceeding the advertised cap.
    let mut samples = Vec::with_capacity(MAX_SAMPLES);
    for _ in 0..4096 {
        if cancelled() {
            return Err(Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                if samples.is_empty() {
                    return Err(InvalidOgg);
                }
                return Ok(Pcm {
                    samples,
                    rate,
                    channels,
                });
            }
            Err(_) => return Err(InvalidOgg),
        };
        if packet.track_id() != id {
            return Err(UnsupportedFormat);
        }
        let decoded = decoder.decode(&packet).map_err(|_| InvalidOgg)?;
        if decoded.spec().rate != rate || decoded.spec().channels.count() != channels {
            return Err(UnsupportedFormat);
        }
        if decoded.capacity() > 8192
            || decoded.frames() * channels > MAX_SAMPLES.saturating_sub(samples.len())
        {
            return Err(TooLarge);
        }
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        for sample in buffer.samples() {
            if !sample.is_finite() {
                return Err(InvalidOgg);
            }
            samples.push(sample.clamp(-1.0, 1.0));
        }
    }
    Err(TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ORIGINAL: &[u8] = include_bytes!("../tests/fixtures/scale.ogg");
    #[test]
    fn original_tone_decodes_and_can_be_cancelled_between_packets() {
        let pcm = decode_ogg(ORIGINAL).unwrap();
        assert_eq!(pcm.sample_rate(), 48000);
        assert_eq!(pcm.duration_ms(), 8000);
        assert!(pcm.byte_len() <= MAX_SAMPLES * 4);
        assert!(pcm.samples.iter().any(|value| value.abs() > 0.05));
        assert!(pcm.samples.iter().all(|value| value.is_finite()));
        let mut checks = 0;
        let result = decode_ogg_cancellable(ORIGINAL, || {
            checks += 1;
            checks > 4
        });
        assert!(matches!(result, Err(DecodeError::Cancelled)));
    }
    #[test]
    fn incomplete_chained_corrupt_and_oversized_input_is_rejected() {
        assert!(decode_ogg(&ORIGINAL[..ORIGINAL.len() - 1]).is_err());
        assert!(decode_ogg(&[ORIGINAL, ORIGINAL].concat()).is_err());
        let mut corrupt = ORIGINAL.to_vec();
        corrupt[30] ^= 1;
        assert!(decode_ogg(&corrupt).is_err());
        assert!(matches!(
            decode_ogg(&vec![0; MAX_OGG_BYTES + 1]),
            Err(DecodeError::TooLarge)
        ));
        // A small compressed stream can expand beyond the PCM budget.
        assert!(matches!(
            decode_ogg(include_bytes!("../tests/fixtures/scale-repeat24.ogg")),
            Err(DecodeError::TooLarge)
        ));
    }
}
