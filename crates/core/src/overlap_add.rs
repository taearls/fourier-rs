//! Overlap-add (OLA) processor for streaming FFT-based audio processing.
//!
//! Implements the standard overlap-add method:
//! 1. Segment the input into overlapping frames.
//! 2. Window each frame.
//! 3. FFT → apply spectral transform → IFFT.
//! 4. Overlap-add the output frames to reconstruct the continuous signal.
//!
//! With a Hann window and 50% overlap, the analysis-synthesis window product
//! sums to a constant, ensuring perfect reconstruction for the identity
//! transform.

use num_complex::Complex;

use crate::fft::FftProcessor;
use crate::transform::SpectralTransform;
use crate::window::{WindowFunction, WindowType};

/// Configuration for the overlap-add processor.
#[derive(Debug, Clone)]
pub struct OlaConfig {
    /// FFT frame size in samples (must be power of 2).
    pub fft_size: usize,
    /// Hop size in samples. For 50% overlap with `fft_size=2048`, use `hop_size=1024`.
    pub hop_size: usize,
    /// Window type for analysis.
    pub window_type: WindowType,
    /// Audio sample rate in Hz.
    pub sample_rate: f32,
}

impl Default for OlaConfig {
    fn default() -> Self {
        Self {
            fft_size: 2048,
            hop_size: 1024, // 50% overlap
            window_type: WindowType::Hann,
            sample_rate: 44100.0,
        }
    }
}

/// Streaming overlap-add FFT processor.
///
/// Feed samples in via [`push_samples`], retrieve processed output via
/// [`pull_samples`]. Internally buffers input, processes complete frames
/// through FFT→transform→IFFT, and overlap-adds the results.
pub struct OverlapAddProcessor {
    config: OlaConfig,
    fft: FftProcessor,
    window: WindowFunction,

    /// Circular input buffer. Samples are appended; when we have a full
    /// frame's worth, we process it.
    input_buf: Vec<f32>,
    /// Write position into `input_buf`.
    input_write_pos: usize,
    /// Number of new samples received since last frame was processed.
    samples_since_last_frame: usize,

    /// Output accumulation buffer for overlap-add.
    output_buf: Vec<f32>,
    /// Read position in the output buffer (samples already consumed).
    output_read_pos: usize,
    /// Write position in the output buffer (where the next OLA frame starts).
    output_write_pos: usize,

    /// Scratch buffers to avoid allocation in the processing loop.
    windowed_frame: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    ifft_output: Vec<f32>,

    /// The latest spectrum snapshot for visualization (copied after transform).
    latest_spectrum: Vec<Complex<f32>>,
}

impl OverlapAddProcessor {
    pub fn new(config: OlaConfig) -> Self {
        assert!(config.fft_size >= 4);
        assert!(config.hop_size > 0);
        assert!(config.hop_size <= config.fft_size);

        let fft = FftProcessor::new(config.fft_size);
        let window = WindowFunction::new(config.window_type, config.fft_size);
        let n_bins = fft.complex_bins();

        // Size the output buffer large enough for smooth streaming.
        // Use 4x fft_size to accommodate overlap and output latency.
        let output_buf_size = config.fft_size * 4;

        Self {
            windowed_frame: vec![0.0; config.fft_size],
            spectrum: vec![Complex::new(0.0, 0.0); n_bins],
            ifft_output: vec![0.0; config.fft_size],
            latest_spectrum: vec![Complex::new(0.0, 0.0); n_bins],
            input_buf: vec![0.0; config.fft_size],
            input_write_pos: 0,
            samples_since_last_frame: 0,
            output_buf: vec![0.0; output_buf_size],
            output_read_pos: 0,
            output_write_pos: 0,
            fft,
            window,
            config,
        }
    }

    /// Push input samples into the processor. When enough samples accumulate
    /// to form a complete hop, an FFT frame is processed.
    ///
    /// `transform` is applied to the spectrum of each frame.
    pub fn push_samples(&mut self, samples: &[f32], transform: &mut dyn SpectralTransform) {
        for &sample in samples {
            // Write into the circular input buffer.
            self.input_buf[self.input_write_pos % self.config.fft_size] = sample;
            self.input_write_pos += 1;
            self.samples_since_last_frame += 1;

            // Process a frame every hop_size samples, once we have enough input.
            if self.samples_since_last_frame >= self.config.hop_size
                && self.input_write_pos >= self.config.fft_size
            {
                self.process_frame(transform);
                self.samples_since_last_frame = 0;
            }
        }
    }

    /// Pull processed output samples. Returns the number of samples actually written.
    pub fn pull_samples(&mut self, output: &mut [f32]) -> usize {
        let available = self.available_samples();
        let to_read = output.len().min(available);
        let buf_len = self.output_buf.len();

        for (i, out) in output.iter_mut().enumerate().take(to_read) {
            *out = self.output_buf[(self.output_read_pos + i) % buf_len];
            // Zero out consumed samples so they don't affect future OLA.
            self.output_buf[(self.output_read_pos + i) % buf_len] = 0.0;
        }
        self.output_read_pos = (self.output_read_pos + to_read) % buf_len;

        to_read
    }

    /// Number of output samples available for reading.
    pub fn available_samples(&self) -> usize {
        if self.output_write_pos >= self.output_read_pos {
            self.output_write_pos - self.output_read_pos
        } else {
            self.output_buf.len() - self.output_read_pos + self.output_write_pos
        }
    }

    /// Get the latest spectrum snapshot (for visualization).
    pub fn latest_spectrum(&self) -> &[Complex<f32>] {
        &self.latest_spectrum
    }

    pub fn config(&self) -> &OlaConfig {
        &self.config
    }

    // --- Internal ---

    fn process_frame(&mut self, transform: &mut dyn SpectralTransform) {
        let fft_size = self.config.fft_size;
        let buf_len = self.input_buf.len();

        // Extract the most recent fft_size samples from the circular input buffer.
        let start = if self.input_write_pos >= fft_size {
            self.input_write_pos - fft_size
        } else {
            0
        };

        for i in 0..fft_size {
            self.windowed_frame[i] = self.input_buf[(start + i) % buf_len];
        }

        // Apply analysis window.
        self.window.apply(&mut self.windowed_frame);

        // Forward FFT.
        self.fft
            .forward(&mut self.windowed_frame, &mut self.spectrum);

        // Apply user-defined spectral transform.
        transform.process(
            &mut self.spectrum,
            self.config.sample_rate,
            self.config.fft_size,
        );

        // Save spectrum snapshot for visualization.
        self.latest_spectrum.copy_from_slice(&self.spectrum);

        // Inverse FFT.
        self.fft
            .inverse(&mut self.spectrum, &mut self.ifft_output);

        // Overlap-add into output buffer.
        let out_len = self.output_buf.len();
        for i in 0..fft_size {
            let pos = (self.output_write_pos + i) % out_len;
            self.output_buf[pos] += self.ifft_output[i];
        }

        // Advance write position by hop_size.
        self.output_write_pos = (self.output_write_pos + self.config.hop_size) % out_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::IdentityTransform;

    #[test]
    fn identity_roundtrip_produces_output() {
        let config = OlaConfig {
            fft_size: 256,
            hop_size: 128,
            window_type: WindowType::Hann,
            sample_rate: 44100.0,
        };

        let mut ola = OverlapAddProcessor::new(config);
        let mut identity = IdentityTransform;

        // Feed in a sine wave.
        let input: Vec<f32> = (0..2048)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44100.0).sin())
            .collect();

        ola.push_samples(&input, &mut identity);

        // Pull output.
        let mut output = vec![0.0; 2048];
        let n = ola.pull_samples(&mut output);

        // We should get some output (accounting for initial latency).
        assert!(n > 0, "expected some output samples, got 0");

        // The output should not be all zeros.
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "output energy should be nonzero");
    }

    #[test]
    fn low_pass_removes_high_frequencies() {
        use crate::transform::LowPassFilter;

        let config = OlaConfig {
            fft_size: 1024,
            hop_size: 512,
            window_type: WindowType::Hann,
            sample_rate: 44100.0,
        };

        let mut ola = OverlapAddProcessor::new(config);
        let mut lp = LowPassFilter { cutoff_hz: 500.0 };

        // Feed in a high-frequency sine (5000 Hz) — should be eliminated.
        let input: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 5000.0 * i as f32 / 44100.0).sin())
            .collect();

        ola.push_samples(&input, &mut lp);

        let mut output = vec![0.0; 4096];
        let n = ola.pull_samples(&mut output);

        // Output energy should be much lower than input energy.
        let input_energy: f32 = input.iter().map(|s| s * s).sum();
        let output_energy: f32 = output[..n].iter().map(|s| s * s).sum();

        if n > 0 {
            let ratio = output_energy / input_energy;
            assert!(
                ratio < 0.01,
                "low-pass should remove 5kHz signal, energy ratio: {}",
                ratio
            );
        }
    }
}
