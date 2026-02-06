//! Error types for audio file I/O operations.

use std::fmt;
use std::path::PathBuf;

/// Errors that can occur when loading or saving audio files.
#[derive(Debug)]
pub enum FileIoError {
    /// The specified file was not found.
    FileNotFound(PathBuf),
    /// The file exists but is not a valid audio format.
    InvalidFormat(String),
    /// The audio format is recognized but not supported (e.g. 8-bit PCM).
    UnsupportedFormat(String),
    /// An I/O error occurred while reading or writing.
    Io(std::io::Error),
}

impl fmt::Display for FileIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileNotFound(path) => write!(f, "file not found: {}", path.display()),
            Self::InvalidFormat(msg) => write!(f, "invalid audio format: {msg}"),
            Self::UnsupportedFormat(msg) => write!(f, "unsupported audio format: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for FileIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FileIoError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
