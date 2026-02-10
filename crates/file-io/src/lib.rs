//! fourier-file-io: Audio file loading and saving.
//!
//! Supports WAV format via the `hound` crate, with 16-bit integer,
//! 24-bit integer, and 32-bit float sample formats in mono or stereo.

mod error;
mod wav;

pub use error::FileIoError;
pub use wav::{load_wav, save_wav, WavFormat};

/// A buffer of audio samples with associated metadata.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    /// Interleaved sample data normalized to the `[-1.0, 1.0]` range.
    pub samples: Vec<f32>,
    /// Sample rate in Hz (e.g. 44100, 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
}

impl AudioBuffer {
    /// Returns the total number of sample frames (samples per channel).
    pub const fn num_frames(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    /// Returns the duration in seconds.
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.num_frames() as f64 / f64::from(self.sample_rate)
    }
}
