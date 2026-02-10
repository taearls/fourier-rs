//! Error types for audio file I/O operations.

use std::path::PathBuf;

/// Errors that can occur when loading or saving audio files.
#[derive(Debug, thiserror::Error)]
pub enum FileIoError {
    /// The specified file was not found.
    #[error("file not found: {}", .0.display())]
    FileNotFound(PathBuf),
    /// The file exists but is not a valid audio format.
    #[error("invalid audio format: {0}")]
    InvalidFormat(String),
    /// The audio format is recognized but not supported (e.g. 8-bit PCM).
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    /// An I/O error occurred while reading or writing.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
