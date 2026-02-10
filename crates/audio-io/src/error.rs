//! Error types for audio I/O operations.

/// Errors that can occur during audio device and stream operations.
#[derive(Debug, thiserror::Error)]
pub enum AudioIoError {
    /// Failed to query supported stream configurations.
    #[error("failed to query {device_type} configs: {source}")]
    ConfigQueryFailed {
        device_type: &'static str,
        source: cpal::SupportedStreamConfigsError,
    },
    /// Device does not support f32 sample format.
    #[error("no f32 {device_type} format supported")]
    UnsupportedSampleFormat { device_type: &'static str },
    /// Failed to build an audio stream.
    #[error("failed to build {stream_type} stream: {source}")]
    StreamBuildFailed {
        stream_type: &'static str,
        source: cpal::BuildStreamError,
    },
    /// Failed to start (play) an audio stream.
    #[error("failed to start {stream_type} stream: {source}")]
    StreamPlayFailed {
        stream_type: &'static str,
        source: cpal::PlayStreamError,
    },
}
