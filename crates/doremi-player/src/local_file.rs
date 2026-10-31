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
}
impl LocalFile {
    pub fn open(mut file: File) -> Result<Self, DecodeError> {
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
                file,
            ))?))),
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
        }
    }
    fn seek_frame(&mut self, frame: u64) -> Result<(), DecodeError> {
        match self {
            Self::Wav(source) => source.seek_frame(frame),
            #[cfg(feature = "local-flac")]
            Self::Flac(source) => source.seek_frame(frame),
        }
    }
    fn read_frame(&mut self) -> Result<Option<[f32; 2]>, DecodeError> {
        match self {
            Self::Wav(source) => source.read_frame(),
            #[cfg(feature = "local-flac")]
            Self::Flac(source) => source.read_frame(),
        }
    }
}
