//! Oscillator module for generating standard audio waveforms.
//!
//! Produces phase-continuous sample output suitable for real-time audio
//! synthesis. Frequency and amplitude can be changed between `generate()`
//! calls without discontinuities.

use std::f32::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

/// Standard oscillator waveform shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveformType {
    Sine,
    Square,
    Sawtooth,
    Triangle,
}

/// A phase-continuous oscillator that fills buffers with waveform samples.
///
/// # Example
///
/// ```
/// use fourier_core::oscillator::{Oscillator, WaveformType};
///
/// let mut osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, 44100.0);
/// let mut buffer = vec![0.0f32; 1024];
/// osc.generate(&mut buffer);
/// ```
#[derive(Debug, Clone)]
pub struct Oscillator {
    waveform: WaveformType,
    frequency: f32,
    amplitude: f32,
    phase: f32,
    sample_rate: f32,
}

impl Oscillator {
    /// Create a new oscillator.
    ///
    /// - `waveform`: the waveform shape to generate
    /// - `frequency`: oscillator frequency in Hz
    /// - `amplitude`: output amplitude (0.0–1.0 typical)
    /// - `sample_rate`: audio sample rate in Hz (e.g. 44100.0)
    pub const fn new(
        waveform: WaveformType,
        frequency: f32,
        amplitude: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            waveform,
            frequency,
            amplitude,
            phase: 0.0,
            sample_rate,
        }
    }

    /// Fill `output` with waveform samples, advancing phase continuously.
    ///
    /// Phase wraps at 2π to prevent floating-point drift over long runs.
    pub fn generate(&mut self, output: &mut [f32]) {
        let phase_inc = TAU * self.frequency / self.sample_rate;

        for sample in output.iter_mut() {
            *sample = self.amplitude * self.sample_at_phase(self.phase);
            self.phase += phase_inc;
            // Wrap phase to [0, TAU) to prevent precision loss.
            if self.phase >= TAU {
                self.phase %= TAU;
            }
        }
    }

    /// Compute the raw waveform value at the given phase (0 to TAU).
    #[inline]
    fn sample_at_phase(&self, phase: f32) -> f32 {
        match self.waveform {
            WaveformType::Sine => phase.sin(),
            WaveformType::Square => {
                if phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            WaveformType::Sawtooth => 2.0f32.mul_add(phase / TAU, -1.0),
            WaveformType::Triangle => {
                // Map phase to [0, 1), then triangle: rises 0→1 over first half, falls 1→0 over second.
                let t = phase / TAU;
                if t < 0.5 {
                    4.0f32.mul_add(t, -1.0)
                } else {
                    (-4.0f32).mul_add(t, 3.0)
                }
            }
        }
    }

    /// Set the oscillator frequency in Hz.
    ///
    /// Changing frequency between `generate()` calls is phase-continuous.
    pub const fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the output amplitude.
    pub const fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }

    /// Set the waveform type.
    pub const fn set_waveform(&mut self, waveform: WaveformType) {
        self.waveform = waveform;
    }

    pub const fn frequency(&self) -> f32 {
        self.frequency
    }

    pub const fn amplitude(&self) -> f32 {
        self.amplitude
    }

    pub const fn waveform(&self) -> WaveformType {
        self.waveform
    }

    pub const fn phase(&self) -> f32 {
        self.phase
    }

    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::FftProcessor;

    const SAMPLE_RATE: f32 = 44100.0;
    // Use a power-of-two FFT size for clean bin alignment.
    const FFT_SIZE: usize = 4096;

    /// Helper: generate one FFT frame of samples and return the magnitude spectrum.
    fn magnitude_spectrum(waveform: WaveformType, frequency: f32) -> Vec<f32> {
        let mut osc = Oscillator::new(waveform, frequency, 1.0, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; FFT_SIZE];
        osc.generate(&mut buffer);

        let mut fft = FftProcessor::new(FFT_SIZE);
        let mut spectrum = fft.alloc_spectrum();
        fft.forward(&mut buffer, &mut spectrum).unwrap();

        spectrum.iter().map(|c| c.norm()).collect()
    }

    /// Return the index of the bin with maximum magnitude (excluding DC).
    fn peak_bin(magnitudes: &[f32]) -> usize {
        magnitudes
            .iter()
            .enumerate()
            .skip(1) // skip DC
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
            .0
    }

    /// Convert a bin index to frequency.
    fn bin_to_freq(bin: usize) -> f32 {
        bin as f32 * SAMPLE_RATE / FFT_SIZE as f32
    }

    // --- Sine waveform tests ---

    #[test]
    fn sine_peak_at_correct_frequency() {
        let freq = 440.0;
        let mags = magnitude_spectrum(WaveformType::Sine, freq);
        let peak_freq = bin_to_freq(peak_bin(&mags));

        // Should be within one bin width of the target frequency.
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;
        assert!(
            (peak_freq - freq).abs() <= bin_width,
            "sine peak at {peak_freq} Hz, expected ~{freq} Hz"
        );
    }

    #[test]
    fn sine_single_peak() {
        let freq = 440.0;
        let mags = magnitude_spectrum(WaveformType::Sine, freq);
        let max_mag = mags.iter().skip(1).copied().reduce(f32::max).unwrap();

        // Count bins with significant energy (> 5% of peak).
        // An unwindowed FFT has spectral leakage, so we use a generous threshold.
        let threshold = max_mag * 0.05;
        let significant_bins = mags.iter().skip(1).filter(|&&m| m > threshold).count();

        // A pure sine should concentrate energy near one bin, with leakage spreading
        // to neighboring bins. Even without windowing, 30 bins is generous.
        assert!(
            significant_bins < 30,
            "sine has {significant_bins} significant bins, expected < 30"
        );
    }

    // --- Square waveform tests ---

    #[test]
    fn square_peak_at_fundamental() {
        let freq = 440.0;
        let mags = magnitude_spectrum(WaveformType::Square, freq);
        let peak_freq = bin_to_freq(peak_bin(&mags));
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        assert!(
            (peak_freq - freq).abs() <= bin_width,
            "square peak at {peak_freq} Hz, expected ~{freq} Hz"
        );
    }

    #[test]
    fn square_has_odd_harmonics() {
        let freq = 100.0;
        let mags = magnitude_spectrum(WaveformType::Square, freq);
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        // Check that odd harmonics (3rd, 5th, 7th) have energy.
        for &harmonic in &[3, 5, 7] {
            let expected_freq = freq * harmonic as f32;
            let bin = (expected_freq / bin_width).round() as usize;
            assert!(
                mags[bin] > mags[1], // stronger than noise floor around DC neighbors
                "square wave missing {harmonic}th harmonic at ~{expected_freq} Hz"
            );
        }
    }

    // --- Sawtooth waveform tests ---

    #[test]
    fn sawtooth_peak_at_fundamental() {
        let freq = 440.0;
        let mags = magnitude_spectrum(WaveformType::Sawtooth, freq);
        let peak_freq = bin_to_freq(peak_bin(&mags));
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        assert!(
            (peak_freq - freq).abs() <= bin_width,
            "sawtooth peak at {peak_freq} Hz, expected ~{freq} Hz"
        );
    }

    #[test]
    fn sawtooth_has_all_harmonics() {
        let freq = 100.0;
        let mags = magnitude_spectrum(WaveformType::Sawtooth, freq);
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        // Sawtooth should have both odd and even harmonics.
        for &harmonic in &[2, 3, 4, 5] {
            let expected_freq = freq * harmonic as f32;
            let bin = (expected_freq / bin_width).round() as usize;
            assert!(
                mags[bin] > mags[1],
                "sawtooth missing {harmonic}th harmonic at ~{expected_freq} Hz"
            );
        }
    }

    // --- Triangle waveform tests ---

    #[test]
    fn triangle_peak_at_fundamental() {
        let freq = 440.0;
        let mags = magnitude_spectrum(WaveformType::Triangle, freq);
        let peak_freq = bin_to_freq(peak_bin(&mags));
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        assert!(
            (peak_freq - freq).abs() <= bin_width,
            "triangle peak at {peak_freq} Hz, expected ~{freq} Hz"
        );
    }

    #[test]
    fn triangle_has_odd_harmonics_falling_off() {
        let freq = 100.0;
        let mags = magnitude_spectrum(WaveformType::Triangle, freq);
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;

        let fund_bin = (freq / bin_width).round() as usize;
        let third_bin = (3.0 * freq / bin_width).round() as usize;
        let fifth_bin = (5.0 * freq / bin_width).round() as usize;

        // Triangle wave: odd harmonics only, falling off as 1/n².
        // So 3rd harmonic should be ~1/9 of fundamental, 5th ~1/25.
        assert!(
            mags[fund_bin] > mags[third_bin],
            "triangle fundamental should be stronger than 3rd harmonic"
        );
        assert!(
            mags[third_bin] > mags[fifth_bin],
            "triangle 3rd harmonic should be stronger than 5th"
        );
    }

    // --- Phase continuity tests ---

    #[test]
    fn phase_continuous_across_calls() {
        let mut osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, SAMPLE_RATE);

        // Generate two consecutive buffers.
        let mut buf1 = vec![0.0f32; 256];
        let mut buf2 = vec![0.0f32; 256];
        osc.generate(&mut buf1);
        let phase_after_first = osc.phase();
        osc.generate(&mut buf2);

        // The first sample of buf2 should continue smoothly from the last of buf1.
        // Check that the difference is within one sample step.
        let last = buf1[255];
        let first = buf2[0];
        let expected_next = (phase_after_first).sin();

        assert!(
            (first - expected_next).abs() < 1e-4,
            "phase discontinuity: last={last}, first_next={first}, expected={expected_next}"
        );
    }

    #[test]
    fn phase_wraps_within_bounds() {
        let mut osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; 44100]; // one second
        osc.generate(&mut buffer);

        assert!(
            osc.phase() < TAU,
            "phase should stay < TAU, got {}",
            osc.phase()
        );
        assert!(
            osc.phase() >= 0.0,
            "phase should be >= 0, got {}",
            osc.phase()
        );
    }

    // --- Range / amplitude tests ---

    #[test]
    fn output_within_amplitude_range() {
        for waveform in [
            WaveformType::Sine,
            WaveformType::Square,
            WaveformType::Sawtooth,
            WaveformType::Triangle,
        ] {
            let amplitude = 0.8;
            let mut osc = Oscillator::new(waveform, 440.0, amplitude, SAMPLE_RATE);
            let mut buffer = vec![0.0f32; 4096];
            osc.generate(&mut buffer);

            for (i, &sample) in buffer.iter().enumerate() {
                assert!(
                    sample.abs() <= amplitude + 1e-6,
                    "{waveform:?} sample {i} = {sample} exceeds amplitude {amplitude}"
                );
            }
        }
    }

    // --- Frequency change test ---

    #[test]
    fn frequency_change_is_phase_continuous() {
        let mut osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, SAMPLE_RATE);
        let mut buf = vec![0.0f32; 256];
        osc.generate(&mut buf);
        let last_before = buf[255];

        // Change frequency and continue generating.
        osc.set_frequency(880.0);
        osc.generate(&mut buf);
        let first_after = buf[0];

        // The transition should be smooth — no sudden jump.
        // With sine waves, adjacent samples at audio rates differ by small amounts.
        let max_step = TAU * 880.0 / SAMPLE_RATE; // max possible single-sample delta
        assert!(
            (first_after - last_before).abs() < max_step + 0.01,
            "discontinuity on freq change: {last_before} -> {first_after}"
        );
    }
}
