//! Content detection for explicitly selected local files. Extensions and titles
//! are not trusted to choose a decoder. This adapter owns no playback state.
use crate::{
    DecodeError,
    wav_stream::{FrameReader, Info, Wav},
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub enum LocalFile {
    Wav(Box<Wav<File>>),
    #[cfg(feature = "local-flac")]
    Flac(Box<crate::flac::Flac>),
    #[cfg(feature = "local-mp3")]
    Mp3(Box<crate::mp3::Mp3>),
}
impl LocalFile {
    pub fn open(file: File) -> Result<Self, DecodeError> {
        Self::open_cancellable(file, || false)
    }
    pub fn open_cancellable(
        mut file: File,
        cancelled: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Result<Self, DecodeError> {
        if cancelled() {
            return Err(DecodeError::Cancelled);
        }
        if !file.metadata().map_err(|_| DecodeError::Input)?.is_file() {
            return Err(DecodeError::Input);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| DecodeError::Input)?;
        let mut signature = [0; 4];
        file.read_exact(&mut signature)
            .map_err(|_| DecodeError::Input)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| DecodeError::Input)?;
        match &signature {
            b"RIFF" => Ok(Self::Wav(Box::new(Wav::open(file)?))),
            #[cfg(feature = "local-flac")]
            b"fLaC" => Ok(Self::Flac(Box::new(crate::flac::Flac::open(Box::new(
                CheckedFile::new(file, cancelled)?,
            ))?))),
            #[cfg(feature = "local-mp3")]
            _ if &signature[..3] == b"ID3"
                || signature[0] == 0xff && signature[1] & 0xe0 == 0xe0 =>
            {
                Ok(Self::Mp3(Box::new(crate::mp3::Mp3::open(Box::new(
                    CheckedFile::new(file, cancelled)?,
                ))?)))
            }
            _ => Err(DecodeError::UnsupportedFormat),
        }
    }
}
impl FrameReader for LocalFile {
    fn info(&self) -> Info {
        match self {
            Self::Wav(source) => source.info(),
            #[cfg(feature = "local-flac")]
            Self::Flac(source) => source.info(),
            #[cfg(feature = "local-mp3")]
            Self::Mp3(source) => source.info(),
        }
    }
    fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        match self {
            Self::Wav(source) => source.seek_frame(frame),
            #[cfg(feature = "local-flac")]
            Self::Flac(source) => source.seek_frame(frame),
            #[cfg(feature = "local-mp3")]
            Self::Mp3(source) => source.seek_frame(frame),
        }
    }
    fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        match self {
            Self::Wav(source) => source.read_frame(),
            #[cfg(feature = "local-flac")]
            Self::Flac(source) => source.read_frame(),
            #[cfg(feature = "local-mp3")]
            Self::Mp3(source) => source.read_frame(),
        }
    }
}

#[cfg(any(feature = "local-flac", feature = "local-mp3"))]
struct CheckedFile<F> {
    file: File,
    length: u64,
    cancelled: F,
}
#[cfg(any(feature = "local-flac", feature = "local-mp3"))]
impl<F: Fn() -> bool> CheckedFile<F> {
    fn new(file: File, cancelled: F) -> Result<Self, DecodeError> {
        let length = file.metadata().map_err(|_| DecodeError::Input)?.len();
        Ok(Self {
            file,
            length,
            cancelled,
        })
    }
    fn check(&self) -> std::io::Result<()> {
        if (self.cancelled)() {
            Err(std::io::Error::other("Local decode cancelled"))
        } else {
            Ok(())
        }
    }
}
#[cfg(any(feature = "local-flac", feature = "local-mp3"))]
impl<F: Fn() -> bool> Read for CheckedFile<F> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.file.read(bytes)
    }
}
#[cfg(any(feature = "local-flac", feature = "local-mp3"))]
impl<F: Fn() -> bool> Seek for CheckedFile<F> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.check()?;
        self.file.seek(position)
    }
}
#[cfg(any(feature = "local-flac", feature = "local-mp3"))]
impl<F: Fn() -> bool + Send + Sync> symphonia::core::io::MediaSource for CheckedFile<F> {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}

#[cfg(all(test, feature = "local-mp3"))]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[test]
    fn cancellation_interrupts_the_compressed_probe_during_io() {
        let file = File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/scale-cbr.mp3"
        ))
        .unwrap();
        let checks = Arc::new(AtomicUsize::new(0));
        let token = checks.clone();
        // Cancellation arrives after the public initial check. It must also
        // reach reads/seeks inside the metadata/frame scan, before playback.
        let result =
            LocalFile::open_cancellable(file, move || token.fetch_add(1, Ordering::Relaxed) >= 4);
        assert!(result.is_err());
        assert!(checks.load(Ordering::Relaxed) >= 5);
    }
}
