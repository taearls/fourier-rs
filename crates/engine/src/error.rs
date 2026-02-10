//! Error types for the engine crate.

/// Errors that can occur in the audio engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Failed to spawn the processing thread.
    #[error("failed to spawn processing thread: {0}")]
    ThreadSpawnFailed(#[from] std::io::Error),
    /// Failed to send a parameter message because the channel is full.
    #[error("failed to send parameter: channel full")]
    ChannelFull,
    /// Failed to send a parameter message because the channel is disconnected.
    #[error("failed to send parameter: channel disconnected")]
    ChannelDisconnected,
    /// Core DSP error.
    #[error(transparent)]
    Core(#[from] fourier_core::CoreError),
    /// Audio I/O error.
    #[error(transparent)]
    AudioIo(#[from] fourier_audio_io::AudioIoError),
    /// File I/O error.
    #[error(transparent)]
    FileIo(#[from] fourier_file_io::FileIoError),
}
