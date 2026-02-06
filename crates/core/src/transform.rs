//! User-defined frequency-domain transforms.
//!
//! The [`SpectralTransform`] trait defines the interface for any operation
//! applied in the frequency domain between the FFT and IFFT stages.

use num_complex::Complex;

/// A single frequency bin with its metadata.
#[derive(Debug, Clone, Copy)]
pub struct FrequencyBin {
    /// Bin index (0 = DC, N/2 = Nyquist).
    pub index: usize,
    /// Frequency in Hz that this bin represents.
    pub frequency_hz: f32,
    /// Complex value (magnitude + phase).
    pub value: Complex<f32>,
}

impl FrequencyBin {
    #[inline]
    pub fn magnitude(&self) -> f32 {
        self.value.norm()
    }

    #[inline]
    pub fn phase(&self) -> f32 {
        self.value.arg()
    }

    /// Reconstruct the complex value from polar form.
    #[inline]
    pub fn from_polar(magnitude: f32, phase: f32) -> Complex<f32> {
        Complex::new(magnitude * phase.cos(), magnitude * phase.sin())
    }
}

/// Trait for frequency-domain transforms applied between FFT and IFFT.
///
/// Implementations receive the full complex spectrum and may modify it
/// in any way. The spectrum has `fft_size/2 + 1` bins due to real-valued
/// FFT conjugate symmetry.
pub trait SpectralTransform: Send {
    /// Apply the transform to the spectrum in-place.
    ///
    /// - `spectrum`: mutable slice of `fft_size/2 + 1` complex bins.
    /// - `sample_rate`: current audio sample rate in Hz.
    /// - `fft_size`: the FFT frame size (number of time-domain samples).
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize);

    /// Human-readable name for this transform.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Built-in transforms
// ---------------------------------------------------------------------------

/// Identity transform — passes audio through unmodified. Useful for testing.
pub struct IdentityTransform;

impl SpectralTransform for IdentityTransform {
    fn process(&mut self, _spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        // No-op.
    }

    fn name(&self) -> &str {
        "Identity"
    }
}

/// Brick-wall low-pass filter: zero all bins above the cutoff frequency.
pub struct LowPassFilter {
    pub cutoff_hz: f32,
}

impl SpectralTransform for LowPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq > self.cutoff_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &str {
        "Low-Pass Filter"
    }
}

/// Brick-wall high-pass filter: zero all bins below the cutoff frequency.
pub struct HighPassFilter {
    pub cutoff_hz: f32,
}

impl SpectralTransform for HighPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq < self.cutoff_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &str {
        "High-Pass Filter"
    }
}

/// Band-pass filter: keep only bins within [low_hz, high_hz].
pub struct BandPassFilter {
    pub low_hz: f32,
    pub high_hz: f32,
}

impl SpectralTransform for BandPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq < self.low_hz || freq > self.high_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &str {
        "Band-Pass Filter"
    }
}

/// Spectral gain: multiply all bins by a constant factor.
pub struct SpectralGain {
    pub gain: f32,
}

impl SpectralTransform for SpectralGain {
    fn process(&mut self, spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        for bin in spectrum.iter_mut() {
            *bin *= self.gain;
        }
    }

    fn name(&self) -> &str {
        "Spectral Gain"
    }
}

/// Chain of transforms applied in sequence.
pub struct TransformChain {
    transforms: Vec<Box<dyn SpectralTransform>>,
}

impl TransformChain {
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    pub fn push(&mut self, transform: Box<dyn SpectralTransform>) {
        self.transforms.push(transform);
    }

    pub fn len(&self) -> usize {
        self.transforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }
}

impl Default for TransformChain {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectralTransform for TransformChain {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        for t in &mut self.transforms {
            t.process(spectrum, sample_rate, fft_size);
        }
    }

    fn name(&self) -> &str {
        "Transform Chain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_pass_zeros_high_bins() {
        let mut spectrum: Vec<Complex<f32>> = (0..513).map(|_| Complex::new(1.0, 0.0)).collect();

        let sample_rate = 44100.0;
        let fft_size = 1024;
        let mut lp = LowPassFilter { cutoff_hz: 1000.0 };

        lp.process(&mut spectrum, sample_rate, fft_size);

        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter().enumerate() {
            let freq = i as f32 * bin_width;
            if freq > 1000.0 {
                assert!(
                    bin.norm() < 1e-6,
                    "bin {} at {:.1} Hz should be zero",
                    i,
                    freq
                );
            } else {
                assert!(
                    (bin.norm() - 1.0).abs() < 1e-6,
                    "bin {} should be untouched",
                    i
                );
            }
        }
    }

    #[test]
    fn chain_applies_in_order() {
        let mut chain = TransformChain::new();
        chain.push(Box::new(SpectralGain { gain: 2.0 }));
        chain.push(Box::new(SpectralGain { gain: 0.5 }));

        let mut spectrum = vec![Complex::new(1.0, 0.0); 513];
        chain.process(&mut spectrum, 44100.0, 1024);

        // 2.0 * 0.5 = 1.0, should be unchanged.
        for bin in &spectrum {
            assert!((bin.re - 1.0).abs() < 1e-6);
        }
    }
}
