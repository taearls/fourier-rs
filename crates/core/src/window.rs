//! Window functions for spectral analysis.
//!
//! Windowing reduces spectral leakage when performing FFT on finite-length
//! signal frames. Pre-computed window tables avoid per-sample trig calls
//! in the processing loop.

use std::f32::consts::PI;

/// Supported window types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    Hann,
    Hamming,
    Blackman,
    Rectangular,
}

/// A pre-computed window function table.
#[derive(Debug, Clone)]
pub struct WindowFunction {
    /// The window coefficients, length == fft_size.
    table: Vec<f32>,
    window_type: WindowType,
}

impl WindowFunction {
    /// Create a new window function of the given type and size.
    pub fn new(window_type: WindowType, size: usize) -> Self {
        let table = match window_type {
            WindowType::Hann => Self::hann(size),
            WindowType::Hamming => Self::hamming(size),
            WindowType::Blackman => Self::blackman(size),
            WindowType::Rectangular => vec![1.0; size],
        };
        Self { table, window_type }
    }

    /// Apply the window to a buffer in-place.
    #[inline]
    pub fn apply(&self, buffer: &mut [f32]) {
        debug_assert_eq!(
            buffer.len(),
            self.table.len(),
            "buffer length must match window size"
        );
        for (sample, &coeff) in buffer.iter_mut().zip(self.table.iter()) {
            *sample *= coeff;
        }
    }

    /// Apply the window, writing into `output` from `input`.
    #[inline]
    pub fn apply_to(&self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.table.len());
        debug_assert_eq!(output.len(), self.table.len());
        for ((out, &inp), &coeff) in output.iter_mut().zip(input.iter()).zip(self.table.iter()) {
            *out = inp * coeff;
        }
    }

    pub fn size(&self) -> usize {
        self.table.len()
    }

    pub fn window_type(&self) -> WindowType {
        self.window_type
    }

    pub fn table(&self) -> &[f32] {
        &self.table
    }

    /// Compute the coherent gain (sum of window coefficients / N).
    /// Useful for normalizing spectral magnitudes.
    pub fn coherent_gain(&self) -> f32 {
        let sum: f32 = self.table.iter().sum();
        sum / self.table.len() as f32
    }

    // --- Private generators ---

    fn hann(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let phase = 2.0 * PI * i as f32 / size as f32;
                0.5 * (1.0 - phase.cos())
            })
            .collect()
    }

    fn hamming(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let phase = 2.0 * PI * i as f32 / size as f32;
                0.54 - 0.46 * phase.cos()
            })
            .collect()
    }

    fn blackman(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let phase = 2.0 * PI * i as f32 / size as f32;
                0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_endpoints_are_zero() {
        let w = WindowFunction::new(WindowType::Hann, 1024);
        assert!(w.table()[0].abs() < 1e-6);
        // Last sample of a periodic Hann window is near zero but not exact zero
        // for periodic windows. Check it's small.
        assert!(w.table()[1023] < 0.01);
    }

    #[test]
    fn hann_midpoint_is_one() {
        let w = WindowFunction::new(WindowType::Hann, 1024);
        assert!((w.table()[512] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn rectangular_is_all_ones() {
        let w = WindowFunction::new(WindowType::Rectangular, 256);
        for &v in w.table() {
            assert!((v - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn apply_scales_buffer() {
        let w = WindowFunction::new(WindowType::Hann, 4);
        let mut buf = vec![1.0; 4];
        w.apply(&mut buf);
        // Hann window at size 4: 0.5*(1-cos(2*pi*i/4))
        // i=0: 0.0, i=1: 0.5, i=2: 1.0, i=3: 0.5
        assert!(buf[0].abs() < 1e-6);
        assert!((buf[1] - 0.5).abs() < 0.01);
        assert!((buf[2] - 1.0).abs() < 0.01);
        assert!((buf[3] - 0.5).abs() < 0.01);
    }
}
