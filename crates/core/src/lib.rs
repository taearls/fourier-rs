//! fourier-core: Pure DSP math with no I/O or platform dependencies.
//!
//! Contains FFT/IFFT wrappers, window functions, overlap-add logic,
//! spectral peak detection, and the user-defined transform interface.

pub mod fft;
pub mod overlap_add;
pub mod spectral;
pub mod transform;
pub mod window;

// Re-export key types at crate root for convenience.
pub use fft::FftProcessor;
pub use overlap_add::OverlapAddProcessor;
pub use spectral::{SpectralPeak, detect_peaks};
pub use transform::{FrequencyBin, SpectralTransform};
pub use window::{WindowFunction, WindowType};
