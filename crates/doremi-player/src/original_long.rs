//! A two-minute file fixture made by repeating Doremi's original CC0 scale.
//! Generated/validated only on an IO worker. Existing unknown files are never replaced.
use crate::{Builtin, DecodeError, media_id};
use doremi_core::media::{Media, MediaId};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const DURATION_MS: u64 = 120_000;
const REPEATS: usize = 15;
pub fn media() -> Media {
    Media {
        id: media_id("tone-scale-long-v1"),
        title: "Doremi: escala extendida (2 min)".into(),
        duration_ms: Some(DURATION_MS),
    }
}
pub fn matches(id: &MediaId) -> bool {
    id.provider.as_str() == "demo" && id.local() == "tone-scale-long-v1"
}
pub fn path(directory: &Path) -> PathBuf {
    directory.join("samples/original-scale-120s-v1.wav")
}
fn template() -> Result<([u8; 44], &'static [u8]), DecodeError> {
    let bytes = Builtin::Scale.bytes();
    if bytes.len() < 44
        || &bytes[12..16] != b"fmt "
        || bytes[16..20] != 16u32.to_le_bytes()
        || &bytes[36..40] != b"data"
    {
        return Err(DecodeError::InvalidWav);
    }
    let data = &bytes[44..];
    let length = u32::try_from(data.len() * REPEATS).map_err(|_| DecodeError::TooLarge)?;
    let mut header: [u8; 44] = bytes[..44].try_into().unwrap();
    header[4..8].copy_from_slice(&(length + 36).to_le_bytes());
    header[40..44].copy_from_slice(&length.to_le_bytes());
    Ok((header, data))
}
fn validate(path: &Path, cancelled: &mut impl FnMut() -> bool) -> Result<File, DecodeError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| DecodeError::Input)?;
    let (expected, data) = template()?;
    if !metadata.is_file() || metadata.len() != (44 + data.len() * REPEATS) as u64 {
        return Err(DecodeError::InvalidWav);
    }
    let mut file = File::open(path).map_err(|_| DecodeError::Input)?;
    let mut header = [0; 44];
    file.read_exact(&mut header)
        .map_err(|_| DecodeError::Input)?;
    if header != expected {
        return Err(DecodeError::InvalidWav);
    }
    let mut block = [0; 8192];
    for _ in 0..REPEATS {
        for chunk in data.chunks(block.len()) {
            if cancelled() {
                return Err(DecodeError::Cancelled);
            }
            file.read_exact(&mut block[..chunk.len()])
                .map_err(|_| DecodeError::Input)?;
            if block[..chunk.len()] != *chunk {
                return Err(DecodeError::InvalidWav);
            }
        }
    }
    Ok(file)
}
fn write_fixture(file: &mut File, cancelled: &mut impl FnMut() -> bool) -> Result<(), DecodeError> {
    let (header, data) = template()?;
    file.write_all(&header).map_err(|_| DecodeError::Input)?;
    for _ in 0..REPEATS {
        for chunk in data.chunks(8192) {
            if cancelled() {
                return Err(DecodeError::Cancelled);
            }
            file.write_all(chunk).map_err(|_| DecodeError::Input)?;
        }
    }
    file.sync_all().map_err(|_| DecodeError::Input)
}
// Android may deny hard links even within an application's private directory.
// Use an exclusively created, immediately unlinked regular file. Its descriptor
// remains seekable; cancellation/drop releases it without a partial named file.
#[cfg(any(target_os = "android", all(test, unix)))]
fn anonymous_fixture(
    path: &Path,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<File, DecodeError> {
    let parent = path.parent().ok_or(DecodeError::Input)?;
    fs::create_dir_all(parent).map_err(|_| DecodeError::Input)?;
    let temporary = path.with_extension(format!("{}.partial", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| DecodeError::Input)?;
    fs::remove_file(&temporary).map_err(|_| DecodeError::Input)?;
    write_fixture(&mut file, cancelled)?;
    Ok(file)
}
/// Return the same validated descriptor that will be streamed, not a reopened path.
pub fn ensure(path: &Path, mut cancelled: impl FnMut() -> bool) -> Result<File, DecodeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => return validate(path, &mut cancelled),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(DecodeError::Input),
    }
    if cancelled() {
        return Err(DecodeError::Cancelled);
    }
    #[cfg(target_os = "android")]
    return anonymous_fixture(path, &mut cancelled);
    #[cfg(not(target_os = "android"))]
    {
        let parent = path.parent().ok_or(DecodeError::Input)?;
        fs::create_dir_all(parent).map_err(|_| DecodeError::Input)?;
        let temporary = path.with_extension(format!("{}.partial", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| DecodeError::Input)?;
        let result = (|| {
            write_fixture(&mut file, &mut cancelled)?;
            drop(file);
            if cancelled() {
                return Err(DecodeError::Cancelled);
            }
            // link fails if another file appeared: never clobber existing data.
            fs::hard_link(&temporary, path).map_err(|_| DecodeError::Input)?;
            #[cfg(unix)]
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| DecodeError::Input)?;
            Ok(())
        })();
        // This temporary was created exclusively by this invocation.
        let _ = fs::remove_file(&temporary);
        result?;
        validate(path, &mut cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_original_file_streams_past_the_prepared_limit_and_unknown_data_is_preserved() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("original-long-{}", std::process::id()));
        let path = path(&directory);
        let file = ensure(&path, || false).unwrap();
        let wav = crate::wav_stream::Wav::open(file).unwrap();
        assert_eq!(wav.info().duration_ms(), DURATION_MS);
        assert!(wav.info().frames > crate::MAX_SAMPLES as u64);
        let mut source = crate::wav_stream::Resampler::new(wav, 44100, 90_000).unwrap();
        let mut nonzero = 0;
        for _ in 0..44100 {
            let frame = source.read_frame().unwrap().unwrap();
            nonzero += usize::from(frame[0] != 0.0);
        }
        assert!(nonzero > 40000);
        drop(source);
        fs::write(&path, b"preserve unknown user data").unwrap();
        assert!(ensure(&path, || false).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserve unknown user data");
        fs::remove_dir_all(directory).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn anonymous_android_fixture_is_seekable_and_cancelled_without_named_residue() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("original-anonymous-{}", std::process::id()));
        let path = path(&directory);
        let file = anonymous_fixture(&path, &mut || false).unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 0);
        let mut wav = crate::wav_stream::Wav::open(file).unwrap();
        assert_eq!(wav.info().duration_ms(), DURATION_MS);
        wav.seek_frame(90 * 48000).unwrap();
        assert!(wav.read_frame().unwrap().is_some());
        drop(wav);
        let mut calls = 0;
        assert!(matches!(
            anonymous_fixture(&path, &mut || {
                calls += 1;
                calls > 4
            }),
            Err(DecodeError::Cancelled)
        ));
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn cancelled_creation_does_not_leave_a_broken_final_fixture() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("original-cancel-{}", std::process::id()));
        let path = path(&directory);
        let mut calls = 0;
        assert!(matches!(
            ensure(&path, || {
                calls += 1;
                calls > 4
            }),
            Err(DecodeError::Cancelled)
        ));
        assert!(!path.exists());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }
}
