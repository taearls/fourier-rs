//! Error types for the core DSP crate.

/// Errors that can occur during DSP operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// FFT forward transform failed.
    #[error("FFT forward transform failed: {0}")]
    FftForwardFailed(String),
    /// FFT inverse transform failed.
    #[error("FFT inverse transform failed: {0}")]
    FftInverseFailed(String),
}
