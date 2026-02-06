//! FFT/IFFT wrapper using `realfft` for efficient real-valued transforms.
//!
//! `realfft` exploits the conjugate symmetry of real-valued FFTs to operate
//! on N/2+1 complex bins instead of N, giving roughly 2x speedup over a
//! full complex FFT.

use num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Wraps a forward (real→complex) and inverse (complex→real) FFT pair
/// for a fixed frame size.
pub struct FftProcessor {
    fft_size: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    /// Scratch buffer for forward FFT.
    scratch_fwd: Vec<Complex<f32>>,
    /// Scratch buffer for inverse FFT.
    scratch_inv: Vec<Complex<f32>>,
    /// Normalization factor: 1/N applied after IFFT.
    inv_norm: f32,
}

impl FftProcessor {
    /// Create a new processor for frames of `fft_size` samples.
    /// `fft_size` must be even (power of 2 recommended for performance).
    pub fn new(fft_size: usize) -> Self {
        assert!(fft_size >= 4, "fft_size must be >= 4");
        assert!(fft_size.is_multiple_of(2), "fft_size must be even");

        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);

        let scratch_fwd = forward.make_scratch_vec();
        let scratch_inv = inverse.make_scratch_vec();

        Self {
            fft_size,
            forward,
            inverse,
            scratch_fwd,
            scratch_inv,
            inv_norm: 1.0 / fft_size as f32,
        }
    }

    /// Number of complex frequency bins produced by the forward FFT.
    /// For a real-valued FFT of size N, this is N/2 + 1.
    #[inline]
    pub fn complex_bins(&self) -> usize {
        self.fft_size / 2 + 1
    }

    #[inline]
    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    /// Perform forward FFT: real time-domain `input` → complex frequency-domain `output`.
    ///
    /// - `input`: mutable slice of length `fft_size` (will be modified in-place by realfft).
    /// - `output`: slice of length `fft_size/2 + 1` to receive complex spectrum.
    pub fn forward(&mut self, input: &mut [f32], output: &mut [Complex<f32>]) {
        debug_assert_eq!(input.len(), self.fft_size);
        debug_assert_eq!(output.len(), self.complex_bins());

        self.forward
            .process_with_scratch(input, output, &mut self.scratch_fwd)
            .expect("forward FFT failed");
    }

    /// Perform inverse FFT: complex frequency-domain `input` → real time-domain `output`.
    ///
    /// Output is normalized by 1/N so that forward→inverse is identity.
    ///
    /// - `input`: mutable slice of length `fft_size/2 + 1` (modified in-place by realfft).
    /// - `output`: slice of length `fft_size` to receive time-domain samples.
    pub fn inverse(&mut self, input: &mut [Complex<f32>], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.complex_bins());
        debug_assert_eq!(output.len(), self.fft_size);

        self.inverse
            .process_with_scratch(input, output, &mut self.scratch_inv)
            .expect("inverse FFT failed");

        // realfft's inverse is unnormalized; apply 1/N.
        for sample in output.iter_mut() {
            *sample *= self.inv_norm;
        }
    }

    /// Convenience: allocate a correctly-sized complex spectrum buffer.
    pub fn alloc_spectrum(&self) -> Vec<Complex<f32>> {
        vec![Complex::new(0.0, 0.0); self.complex_bins()]
    }

    /// Convenience: allocate a correctly-sized time-domain buffer.
    pub fn alloc_time_buffer(&self) -> Vec<f32> {
        vec![0.0; self.fft_size]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_identity() {
        let fft_size = 1024;
        let mut proc = FftProcessor::new(fft_size);

        // Create a test signal: sum of two sinusoids.
        let mut input: Vec<f32> = (0..fft_size)
            .map(|i| {
                let t = i as f32 / fft_size as f32;
                (2.0 * std::f32::consts::PI * 100.0 * t).sin()
                    + 0.5 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect();

        let original = input.clone();

        let mut spectrum = proc.alloc_spectrum();
        proc.forward(&mut input, &mut spectrum);
        proc.inverse(&mut spectrum, &mut input);

        // Verify roundtrip matches within floating-point tolerance.
        for (i, (&orig, &recon)) in original.iter().zip(input.iter()).enumerate() {
            assert!(
                (orig - recon).abs() < 1e-4,
                "mismatch at sample {}: orig={}, recon={}",
                i,
                orig,
                recon
            );
        }
    }

    #[test]
    fn dc_component() {
        let fft_size = 256;
        let mut proc = FftProcessor::new(fft_size);
        let mut input = vec![1.0_f32; fft_size];
        let mut spectrum = proc.alloc_spectrum();

        proc.forward(&mut input, &mut spectrum);

        // DC bin should equal N (unnormalized DFT of constant signal).
        assert!((spectrum[0].re - fft_size as f32).abs() < 1e-3);
        assert!(spectrum[0].im.abs() < 1e-6);

        // All other bins should be ~0.
        for bin in &spectrum[1..] {
            assert!(bin.norm() < 1e-3);
        }
    }
}
