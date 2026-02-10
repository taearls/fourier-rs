//! fourier-core: Pure DSP math with no I/O or platform dependencies.
//!
//! Contains FFT/IFFT wrappers, window functions, overlap-add logic,
//! spectral peak detection, and the user-defined transform interface.

pub mod fft;
pub mod oscillator;
pub mod overlap_add;
pub mod spectral;
pub mod transform;
pub mod window;

// Re-export key types at crate root for convenience.
pub use fft::FftProcessor;
pub use oscillator::{Oscillator, WaveformType};
pub use overlap_add::OverlapAddProcessor;
pub use spectral::{detect_peaks, SpectralPeak};
pub use transform::{BandType, EqBand, FrequencyBin, ParametricEq, SpectralTransform};
pub use window::{WindowFunction, WindowType};
