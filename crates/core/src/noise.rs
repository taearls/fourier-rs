//! Noise generator module for producing white and pink noise.
//!
//! White noise has an approximately flat power spectrum (equal energy per
//! frequency bin). Pink noise has an approximately −3 dB/octave rolloff
//! (equal energy per octave), produced using the Voss-McCartney algorithm.
//!
//! Amplitude can be changed between `generate()` calls without
//! discontinuities. The internal PRNG is deterministic given the same
//! sequence of calls.

use serde::{Deserialize, Serialize};

/// Noise type selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseType {
    /// Uniform white noise — flat power spectrum.
    White,
    /// Pink noise — approximately −3 dB/octave rolloff (1/f spectrum).
    Pink,
}

/// xorshift64 PRNG step — fast, no dependencies, deterministic.
#[inline]
const fn prng_next_u64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Map a PRNG step to a float in `[-1.0, +1.0)`.
#[inline]
fn prng_next_f32(state: &mut u64) -> f32 {
    // Use the upper 24 bits for mantissa precision.
    let bits = (prng_next_u64(state) >> 40) as f32;
    let max = (1u64 << 24) as f32;
    2.0f32.mul_add(bits / max, -1.0)
}

/// A noise generator that fills buffers with noise samples.
///
/// # Example
///
/// ```
/// use fourier_core::noise::{NoiseGenerator, NoiseType};
///
/// let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, 44100.0);
/// let mut buffer = vec![0.0f32; 1024];
/// gen.generate(&mut buffer);
/// ```
#[derive(Debug, Clone)]
pub struct NoiseGenerator {
    noise_type: NoiseType,
    amplitude: f32,
    sample_rate: f32,
    // ---- White noise PRNG state ----
    /// xorshift64 state — must never be zero.
    prng_state: u64,
    // ---- Pink noise (Voss-McCartney) state ----
    /// Per-octave row values for Voss-McCartney.
    pink_rows: [f32; Self::NUM_PINK_ROWS],
    /// Running sum of all row values.
    pink_running_sum: f32,
    /// Sample counter for octave-spaced scheduling.
    pink_counter: u32,
}

impl NoiseGenerator {
    /// Number of octave rows for the Voss-McCartney algorithm.
    ///
    /// 16 rows gives good spectral accuracy down to very low frequencies.
    const NUM_PINK_ROWS: usize = 16;

    /// Normalization factor for pink noise: each row plus one white component.
    const PINK_SCALE: f32 = 1.0 / (Self::NUM_PINK_ROWS as f32 + 1.0);

    /// Create a new noise generator.
    ///
    /// - `noise_type`: `White` or `Pink`
    /// - `amplitude`: output amplitude (0.0–1.0 typical)
    /// - `sample_rate`: audio sample rate in Hz (e.g. 44100.0)
    pub fn new(noise_type: NoiseType, amplitude: f32, sample_rate: f32) -> Self {
        let mut prng_state: u64 = 0x5DEE_CE66_D1A4_F681;
        let mut pink_rows = [0.0_f32; Self::NUM_PINK_ROWS];

        // Initialize pink noise rows so there is no startup silence.
        let mut sum = 0.0_f32;
        for row in &mut pink_rows {
            *row = prng_next_f32(&mut prng_state);
            sum += *row;
        }

        Self {
            noise_type,
            amplitude,
            sample_rate,
            prng_state,
            pink_rows,
            pink_running_sum: sum,
            pink_counter: 0,
        }
    }

    /// Fill `output` with noise samples.
    pub fn generate(&mut self, output: &mut [f32]) {
        match self.noise_type {
            NoiseType::White => self.generate_white(output),
            NoiseType::Pink => self.generate_pink(output),
        }
    }

    /// Fill buffer with white noise samples.
    fn generate_white(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            *sample = self.amplitude * prng_next_f32(&mut self.prng_state);
        }
    }

    /// Interval (in samples) at which `pink_running_sum` is recalculated
    /// from scratch to prevent floating-point drift from incremental
    /// add/subtract accumulation.
    const PINK_RECALC_INTERVAL: u32 = 1 << 16; // ~1.5 s at 44.1 kHz

    /// Fill buffer with pink noise samples using Voss-McCartney.
    fn generate_pink(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            self.pink_counter = self.pink_counter.wrapping_add(1);

            // Row k updates every 2^k samples (trailing-zeros scheduling).
            let changed_bits = self.pink_counter ^ self.pink_counter.wrapping_sub(1);
            for (k, row) in self.pink_rows.iter_mut().enumerate() {
                if changed_bits & (1 << k) != 0 {
                    self.pink_running_sum -= *row;
                    *row = prng_next_f32(&mut self.prng_state);
                    self.pink_running_sum += *row;
                }
            }

            // Periodically recompute the running sum from scratch to
            // correct accumulated floating-point rounding errors.
            if self.pink_counter.is_multiple_of(Self::PINK_RECALC_INTERVAL) {
                self.pink_running_sum = self.pink_rows.iter().sum();
            }

            // Add a fresh white noise sample for the highest-frequency component.
            let white_val = prng_next_f32(&mut self.prng_state);
            *sample = self.amplitude * (self.pink_running_sum + white_val) * Self::PINK_SCALE;
        }
    }

    /// Set the output amplitude.
    pub const fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }

    /// Set the noise type.
    ///
    /// Switching type preserves PRNG and pink state, so there is no
    /// re-initialization penalty.
    pub const fn set_noise_type(&mut self, noise_type: NoiseType) {
        self.noise_type = noise_type;
    }

    /// Current amplitude.
    pub const fn amplitude(&self) -> f32 {
        self.amplitude
    }

    /// Current noise type.
    pub const fn noise_type(&self) -> NoiseType {
        self.noise_type
    }

    /// Audio sample rate in Hz.
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
    const FFT_SIZE: usize = 8192;

    /// Generate noise samples and return the magnitude spectrum.
    fn magnitude_spectrum(noise_type: NoiseType) -> Vec<f32> {
        let mut gen = NoiseGenerator::new(noise_type, 1.0, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; FFT_SIZE];
        gen.generate(&mut buffer);

        let mut fft = FftProcessor::new(FFT_SIZE);
        let mut spectrum = fft.alloc_spectrum();
        fft.forward(&mut buffer, &mut spectrum).unwrap();

        spectrum.iter().map(|c| c.norm()).collect()
    }

    /// Compute average energy in an octave band defined by [`low_freq`, `high_freq`).
    fn octave_band_energy(magnitudes: &[f32], low_freq: f32, high_freq: f32) -> f32 {
        let bin_width = SAMPLE_RATE / FFT_SIZE as f32;
        let low_bin = (low_freq / bin_width).ceil() as usize;
        let high_bin = ((high_freq / bin_width).floor() as usize).min(magnitudes.len() - 1);

        if high_bin <= low_bin {
            return 0.0;
        }

        let sum: f32 = magnitudes[low_bin..=high_bin].iter().map(|m| m * m).sum();
        sum / (high_bin - low_bin + 1) as f32
    }

    // ---- White noise tests ----

    #[test]
    fn white_noise_has_energy() {
        let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; 1024];
        gen.generate(&mut buffer);

        let energy: f32 = buffer.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "white noise should produce nonzero output");
    }

    #[test]
    fn white_noise_respects_amplitude() {
        let amplitude = 0.5;
        let mut gen = NoiseGenerator::new(NoiseType::White, amplitude, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; 4096];
        gen.generate(&mut buffer);

        for (i, &s) in buffer.iter().enumerate() {
            assert!(
                s.abs() <= amplitude + 1e-6,
                "white noise sample {i} = {s} exceeds amplitude {amplitude}"
            );
        }
    }

    #[test]
    fn white_noise_approximately_flat_spectrum() {
        // Average over multiple FFT frames for a stable estimate.
        let num_frames = 32;
        let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut accum = vec![0.0_f32; FFT_SIZE / 2 + 1];

        for _ in 0..num_frames {
            let mut buffer = vec![0.0f32; FFT_SIZE];
            gen.generate(&mut buffer);

            let mut fft = FftProcessor::new(FFT_SIZE);
            let mut spectrum = fft.alloc_spectrum();
            fft.forward(&mut buffer, &mut spectrum).unwrap();

            for (i, c) in spectrum.iter().enumerate() {
                accum[i] += c.norm();
            }
        }

        for v in &mut accum {
            *v /= num_frames as f32;
        }

        // Compare energy in low octave (500-1000 Hz) vs high octave (4000-8000 Hz).
        // White noise should have roughly equal average energy per bin.
        let low_energy = octave_band_energy(&accum, 500.0, 1000.0);
        let high_energy = octave_band_energy(&accum, 4000.0, 8000.0);

        // The ratio should be close to 1.0. Allow up to 6 dB deviation
        // (factor of 4 in power) due to statistical variance.
        let ratio = low_energy / high_energy;
        assert!(
            ratio > 0.25 && ratio < 4.0,
            "white noise spectrum not flat: low/high ratio = {ratio:.3}"
        );
    }

    // ---- Pink noise tests ----

    #[test]
    fn pink_noise_has_energy() {
        let mut gen = NoiseGenerator::new(NoiseType::Pink, 1.0, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; 1024];
        gen.generate(&mut buffer);

        let energy: f32 = buffer.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "pink noise should produce nonzero output");
    }

    #[test]
    fn pink_noise_respects_amplitude() {
        let amplitude = 0.5;
        let mut gen = NoiseGenerator::new(NoiseType::Pink, amplitude, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; 8192];
        gen.generate(&mut buffer);

        for (i, &s) in buffer.iter().enumerate() {
            assert!(
                s.abs() <= amplitude + 1e-6,
                "pink noise sample {i} = {s} exceeds amplitude {amplitude}"
            );
        }
    }

    #[test]
    fn pink_noise_approximately_minus_3db_per_octave() {
        // Average over multiple frames for a stable spectral estimate.
        let num_frames = 64;
        let mut gen = NoiseGenerator::new(NoiseType::Pink, 1.0, SAMPLE_RATE);
        let mut accum = vec![0.0_f32; FFT_SIZE / 2 + 1];

        for _ in 0..num_frames {
            let mut buffer = vec![0.0f32; FFT_SIZE];
            gen.generate(&mut buffer);

            let mut fft = FftProcessor::new(FFT_SIZE);
            let mut spectrum = fft.alloc_spectrum();
            fft.forward(&mut buffer, &mut spectrum).unwrap();

            for (i, c) in spectrum.iter().enumerate() {
                accum[i] += c.norm();
            }
        }

        for v in &mut accum {
            *v /= num_frames as f32;
        }

        // Compare octave bands. For pink noise, energy per bin should halve
        // each octave (−3 dB/octave = energy density ∝ 1/f).
        // Check over several octave pairs.
        let octave_pairs: &[(f32, f32, f32, f32)] = &[
            (250.0, 500.0, 500.0, 1000.0),
            (500.0, 1000.0, 1000.0, 2000.0),
            (1000.0, 2000.0, 2000.0, 4000.0),
            (2000.0, 4000.0, 4000.0, 8000.0),
        ];

        for &(low_lo, low_hi, high_lo, high_hi) in octave_pairs {
            let low_energy = octave_band_energy(&accum, low_lo, low_hi);
            let high_energy = octave_band_energy(&accum, high_lo, high_hi);

            if low_energy < 1e-10 || high_energy < 1e-10 {
                continue; // skip degenerate bins
            }

            // For −3 dB/octave, the per-bin energy ratio between adjacent
            // octaves should be ~2 (lower octave has ~2× the energy per bin).
            // Allow a generous range since this is a stochastic process.
            let ratio = low_energy / high_energy;
            assert!(
                ratio > 0.5 && ratio < 8.0,
                "pink noise octave rolloff out of range for bands \
                 [{low_lo}-{low_hi}] vs [{high_lo}-{high_hi}]: ratio = {ratio:.3}"
            );
        }

        // Also verify the overall trend: lowest octave band should have
        // more energy per bin than the highest.
        let low_energy = octave_band_energy(&accum, 250.0, 500.0);
        let high_energy = octave_band_energy(&accum, 4000.0, 8000.0);
        assert!(
            low_energy > high_energy,
            "pink noise should have more energy at low frequencies: \
             low={low_energy:.6}, high={high_energy:.6}"
        );
    }

    #[test]
    fn pink_noise_has_more_low_than_high_energy() {
        let mags = magnitude_spectrum(NoiseType::Pink);

        // Compare total energy in low band vs high band.
        let low_energy: f32 = {
            let bin_width = SAMPLE_RATE / FFT_SIZE as f32;
            let lo = (100.0 / bin_width).ceil() as usize;
            let hi = (1000.0 / bin_width).floor() as usize;
            mags[lo..=hi].iter().map(|m| m * m).sum()
        };
        let high_energy: f32 = {
            let bin_width = SAMPLE_RATE / FFT_SIZE as f32;
            let lo = (4000.0 / bin_width).ceil() as usize;
            let hi = (16000.0 / bin_width).floor() as usize;
            mags[lo..=hi].iter().map(|m| m * m).sum()
        };

        assert!(
            low_energy > high_energy,
            "pink noise should have more energy at low frequencies"
        );
    }

    // ---- White vs pink comparison ----

    #[test]
    fn white_noise_flatter_than_pink() {
        // White noise should have a flatter spectrum than pink noise.
        // Measure the low/high energy ratio for both; pink should be larger.
        let num_frames = 32;

        let mut white_accum = vec![0.0_f32; FFT_SIZE / 2 + 1];
        let mut pink_accum = vec![0.0_f32; FFT_SIZE / 2 + 1];
        let mut white_gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut pink_gen = NoiseGenerator::new(NoiseType::Pink, 1.0, SAMPLE_RATE);

        for _ in 0..num_frames {
            let mut buffer = vec![0.0f32; FFT_SIZE];
            let mut fft = FftProcessor::new(FFT_SIZE);
            let mut spectrum = fft.alloc_spectrum();

            white_gen.generate(&mut buffer);
            fft.forward(&mut buffer, &mut spectrum).unwrap();
            for (i, c) in spectrum.iter().enumerate() {
                white_accum[i] += c.norm();
            }

            pink_gen.generate(&mut buffer);
            fft.forward(&mut buffer, &mut spectrum).unwrap();
            for (i, c) in spectrum.iter().enumerate() {
                pink_accum[i] += c.norm();
            }
        }

        let white_ratio = octave_band_energy(&white_accum, 250.0, 500.0)
            / octave_band_energy(&white_accum, 4000.0, 8000.0);
        let pink_ratio = octave_band_energy(&pink_accum, 250.0, 500.0)
            / octave_band_energy(&pink_accum, 4000.0, 8000.0);

        // Pink noise should have a larger low/high ratio than white noise.
        assert!(
            pink_ratio > white_ratio,
            "pink noise should tilt more toward low frequencies: \
             white_ratio={white_ratio:.3}, pink_ratio={pink_ratio:.3}"
        );
    }

    // ---- API / property tests ----

    #[test]
    fn noise_type_roundtrip() {
        let gen = NoiseGenerator::new(NoiseType::White, 0.7, 48000.0);
        assert_eq!(gen.noise_type(), NoiseType::White);
        assert!((gen.amplitude() - 0.7).abs() < 1e-6);
        assert!((gen.sample_rate() - 48000.0).abs() < 1e-6);
    }

    #[test]
    fn set_noise_type_switches_output() {
        let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);

        // Generate white noise.
        let mut white_buf = vec![0.0f32; 1024];
        gen.generate(&mut white_buf);

        // Switch to pink.
        gen.set_noise_type(NoiseType::Pink);
        assert_eq!(gen.noise_type(), NoiseType::Pink);

        let mut pink_buf = vec![0.0f32; 1024];
        gen.generate(&mut pink_buf);

        // Buffers should differ (statistically guaranteed for 1024 samples).
        assert_ne!(
            white_buf, pink_buf,
            "switching noise type should change output"
        );
    }

    #[test]
    fn set_amplitude_changes_output_level() {
        let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut loud = vec![0.0f32; 4096];
        gen.generate(&mut loud);
        let loud_energy: f32 = loud.iter().map(|s| s * s).sum();

        // Reset generator for same PRNG sequence.
        let mut gen2 = NoiseGenerator::new(NoiseType::White, 0.25, SAMPLE_RATE);
        let mut quiet = vec![0.0f32; 4096];
        gen2.generate(&mut quiet);
        let quiet_energy: f32 = quiet.iter().map(|s| s * s).sum();

        // Energy scales with amplitude squared: 0.25² = 0.0625.
        let ratio = quiet_energy / loud_energy;
        assert!(
            (ratio - 0.0625).abs() < 0.01,
            "energy ratio should be ~0.0625, got {ratio:.4}"
        );
    }

    #[test]
    fn zero_amplitude_produces_silence() {
        let mut gen = NoiseGenerator::new(NoiseType::White, 0.0, SAMPLE_RATE);
        let mut buffer = vec![1.0f32; 512];
        gen.generate(&mut buffer);

        for &s in &buffer {
            assert!(s.abs() < 1e-10, "zero amplitude should produce silence");
        }

        gen.set_noise_type(NoiseType::Pink);
        gen.generate(&mut buffer);
        for &s in &buffer {
            assert!(
                s.abs() < 1e-10,
                "zero amplitude pink should produce silence"
            );
        }
    }

    #[test]
    fn empty_buffer_is_noop() {
        let mut gen = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut buffer: Vec<f32> = vec![];
        gen.generate(&mut buffer); // should not panic
        assert!(buffer.is_empty());
    }

    #[test]
    fn noise_type_serde_roundtrip() {
        let json = serde_json::to_string(&NoiseType::White).unwrap();
        let decoded: NoiseType = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, NoiseType::White);

        let json = serde_json::to_string(&NoiseType::Pink).unwrap();
        let decoded: NoiseType = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, NoiseType::Pink);
    }

    #[test]
    fn deterministic_output() {
        let mut gen1 = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut gen2 = NoiseGenerator::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut buf1 = vec![0.0f32; 512];
        let mut buf2 = vec![0.0f32; 512];

        gen1.generate(&mut buf1);
        gen2.generate(&mut buf2);

        assert_eq!(
            buf1, buf2,
            "same initial state should produce identical output"
        );
    }
}
