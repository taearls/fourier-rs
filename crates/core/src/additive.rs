//! Additive synthesis module for generating sound by summing sine wave partials.
//!
//! Each [`Partial`] has an independent frequency, amplitude, and phase.
//! [`AdditiveSynth`] sums all partials into an output buffer, tracking phase
//! continuously across `generate()` calls so there are no discontinuities.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

/// A single sinusoidal partial for additive synthesis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Partial {
    /// Frequency in Hz.
    pub frequency: f32,
    /// Linear amplitude (0.0–1.0 typical).
    pub amplitude: f32,
    /// Starting phase in radians.
    pub phase: f32,
}

/// Per-partial runtime state: phase increment, amplitude, and current phase.
#[derive(Debug, Clone)]
struct PartialState {
    phase_inc: f32,
    amplitude: f32,
    phase: f32,
}

/// Additive synthesizer that generates sound by summing sine wave partials.
///
/// # Example
///
/// ```
/// use fourier_core::additive::{AdditiveSynth, Partial};
///
/// let partials = vec![
///     Partial { frequency: 440.0, amplitude: 1.0, phase: 0.0 },
///     Partial { frequency: 880.0, amplitude: 0.5, phase: 0.0 },
/// ];
/// let mut synth = AdditiveSynth::new(&partials, 44100.0);
/// let mut buffer = vec![0.0f32; 1024];
/// synth.generate(&mut buffer);
/// ```
#[derive(Debug, Clone)]
pub struct AdditiveSynth {
    partials: Vec<PartialState>,
    sample_rate: f32,
}

impl AdditiveSynth {
    /// Create a new additive synthesizer.
    ///
    /// - `partials`: the set of sine partials to sum
    /// - `sample_rate`: audio sample rate in Hz (e.g. 44100.0)
    pub fn new(partials: &[Partial], sample_rate: f32) -> Self {
        let partials = partials
            .iter()
            .map(|p| PartialState {
                phase_inc: TAU * p.frequency / sample_rate,
                amplitude: p.amplitude,
                phase: p.phase,
            })
            .collect();
        Self {
            partials,
            sample_rate,
        }
    }

    /// Fill `output` with the sum of all partials.
    ///
    /// Phase wraps at 2π per partial to prevent floating-point drift.
    /// Phase is continuous across calls.
    pub fn generate(&mut self, output: &mut [f32]) {
        output.fill(0.0);

        for partial in &mut self.partials {
            for sample in output.iter_mut() {
                *sample += partial.amplitude * partial.phase.sin();
                partial.phase += partial.phase_inc;
                if partial.phase >= TAU {
                    partial.phase -= TAU;
                }
            }
        }
    }

    /// Return the number of partials.
    pub const fn num_partials(&self) -> usize {
        self.partials.len()
    }

    /// Return the audio sample rate.
    pub const fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

/// Create a harmonic series of partials starting from a fundamental frequency.
///
/// Generates `num_harmonics` partials at frequencies `f`, `2f`, `3f`, …
/// with amplitudes following a `1/n` rolloff (common for sawtooth-like timbres).
/// All partials start with zero phase.
///
/// # Example
///
/// ```
/// use fourier_core::additive::harmonic_series;
///
/// let partials = harmonic_series(440.0, 5);
/// assert_eq!(partials.len(), 5);
/// assert!((partials[0].frequency - 440.0).abs() < f32::EPSILON);
/// assert!((partials[1].frequency - 880.0).abs() < f32::EPSILON);
/// ```
pub fn harmonic_series(fundamental: f32, num_harmonics: usize) -> Vec<Partial> {
    (1..=num_harmonics)
        .map(|n| Partial {
            frequency: fundamental * n as f32,
            amplitude: 1.0 / n as f32,
            phase: 0.0,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::FftProcessor;

    const SAMPLE_RATE: f32 = 44100.0;
    const FFT_SIZE: usize = 4096;

    /// Generate one FFT frame from an additive synth and return the magnitude spectrum.
    fn magnitude_spectrum(partials: &[Partial]) -> Vec<f32> {
        let mut synth = AdditiveSynth::new(partials, SAMPLE_RATE);
        let mut buffer = vec![0.0f32; FFT_SIZE];
        synth.generate(&mut buffer);

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
            .skip(1)
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
            .0
    }

    fn bin_to_freq(bin: usize) -> f32 {
        bin as f32 * SAMPLE_RATE / FFT_SIZE as f32
    }

    fn bin_width() -> f32 {
        SAMPLE_RATE / FFT_SIZE as f32
    }

    // --- Single partial tests ---

    #[test]
    fn single_partial_peak_at_correct_frequency() {
        let freq = 440.0;
        let partials = vec![Partial {
            frequency: freq,
            amplitude: 1.0,
            phase: 0.0,
        }];
        let mags = magnitude_spectrum(&partials);
        let peak_freq = bin_to_freq(peak_bin(&mags));

        assert!(
            (peak_freq - freq).abs() <= bin_width(),
            "single partial peak at {peak_freq} Hz, expected ~{freq} Hz"
        );
    }

    #[test]
    fn single_partial_matches_sine_oscillator() {
        let freq = 440.0;
        let partials = vec![Partial {
            frequency: freq,
            amplitude: 1.0,
            phase: 0.0,
        }];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        let mut osc = crate::Oscillator::new(crate::WaveformType::Sine, freq, 1.0, SAMPLE_RATE);

        let mut buf_synth = vec![0.0f32; 1024];
        let mut buf_osc = vec![0.0f32; 1024];
        synth.generate(&mut buf_synth);
        osc.generate(&mut buf_osc);

        for (i, (s, o)) in buf_synth.iter().zip(buf_osc.iter()).enumerate() {
            assert!(
                (s - o).abs() < 1e-3,
                "sample {i}: synth={s}, osc={o}, diff={}",
                (s - o).abs()
            );
        }
    }

    // --- Multiple partials tests ---

    #[test]
    fn multiple_partials_have_expected_peaks() {
        let partials = vec![
            Partial {
                frequency: 200.0,
                amplitude: 1.0,
                phase: 0.0,
            },
            Partial {
                frequency: 400.0,
                amplitude: 0.5,
                phase: 0.0,
            },
            Partial {
                frequency: 600.0,
                amplitude: 0.25,
                phase: 0.0,
            },
        ];
        let mags = magnitude_spectrum(&partials);
        let bw = bin_width();

        // Each partial should have energy at its frequency.
        for &(freq, expected_amp) in &[(200.0, 1.0), (400.0, 0.5), (600.0, 0.25)] {
            let bin = (freq / bw).round() as usize;
            // Magnitude should be roughly proportional to amplitude * FFT_SIZE/2.
            assert!(
                mags[bin] > expected_amp * 0.1 * FFT_SIZE as f32,
                "missing energy at {freq} Hz (bin {bin}): mag={}",
                mags[bin]
            );
        }
    }

    #[test]
    fn summing_n_partials_produces_correct_output() {
        // Manually verify that the synth output equals the sum of individual sines.
        let partials = vec![
            Partial {
                frequency: 100.0,
                amplitude: 1.0,
                phase: 0.0,
            },
            Partial {
                frequency: 200.0,
                amplitude: 0.5,
                phase: 0.0,
            },
        ];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        let mut buf = vec![0.0f32; 512];
        synth.generate(&mut buf);

        // Compute expected output manually.
        let phase_inc_1 = TAU * 100.0 / SAMPLE_RATE;
        let phase_inc_2 = TAU * 200.0 / SAMPLE_RATE;
        for (i, &sample) in buf.iter().enumerate() {
            let expected = 0.5f32.mul_add(
                (phase_inc_2 * i as f32).sin(),
                (phase_inc_1 * i as f32).sin(),
            );
            assert!(
                (sample - expected).abs() < 1e-3,
                "sample {i}: got {sample}, expected {expected}"
            );
        }
    }

    // --- harmonic_series tests ---

    #[test]
    fn harmonic_series_correct_frequencies() {
        let fundamental = 220.0;
        let partials = harmonic_series(fundamental, 5);
        assert_eq!(partials.len(), 5);

        for (i, p) in partials.iter().enumerate() {
            let n = (i + 1) as f32;
            let expected = fundamental * n;
            assert!(
                (p.frequency - expected).abs() < f32::EPSILON,
                "harmonic {}: expected {expected}, got {}",
                i + 1,
                p.frequency
            );
        }
    }

    #[test]
    fn harmonic_series_correct_amplitudes() {
        let partials = harmonic_series(100.0, 4);

        for (i, p) in partials.iter().enumerate() {
            let n = (i + 1) as f32;
            let expected = 1.0 / n;
            assert!(
                (p.amplitude - expected).abs() < f32::EPSILON,
                "harmonic {} amplitude: expected {expected}, got {}",
                i + 1,
                p.amplitude
            );
        }
    }

    #[test]
    fn harmonic_series_zero_harmonics() {
        let partials = harmonic_series(440.0, 0);
        assert!(partials.is_empty());
    }

    #[test]
    fn harmonic_series_zero_phase() {
        let partials = harmonic_series(440.0, 3);
        for p in &partials {
            assert!(
                p.phase.abs() < f32::EPSILON,
                "harmonic_series partials should start at phase 0"
            );
        }
    }

    #[test]
    fn harmonic_series_spectral_peaks() {
        let fundamental = 200.0;
        let partials = harmonic_series(fundamental, 4);
        let mags = magnitude_spectrum(&partials);
        let bw = bin_width();

        // The spectrum should have peaks at 200, 400, 600, 800 Hz.
        for n in 1..=4 {
            let freq = fundamental * n as f32;
            let bin = (freq / bw).round() as usize;
            assert!(
                mags[bin] > 0.01 * FFT_SIZE as f32,
                "missing harmonic {n} at {freq} Hz"
            );
        }

        // The fundamental should be the strongest peak.
        let fund_bin = (fundamental / bw).round() as usize;
        assert_eq!(
            peak_bin(&mags),
            fund_bin,
            "fundamental should be the strongest peak"
        );
    }

    // --- Phase continuity tests ---

    #[test]
    fn phase_continuous_across_generate_calls() {
        let partials = vec![Partial {
            frequency: 440.0,
            amplitude: 1.0,
            phase: 0.0,
        }];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);

        let mut buf1 = vec![0.0f32; 256];
        let mut buf2 = vec![0.0f32; 256];
        synth.generate(&mut buf1);
        synth.generate(&mut buf2);

        // The transition between buf1's last sample and buf2's first sample
        // should be smooth (within one sample step).
        let last = buf1[255];
        let first = buf2[0];
        let max_step = TAU * 440.0 / SAMPLE_RATE;
        assert!(
            (first - last).abs() < max_step + 0.01,
            "phase discontinuity: last={last}, first={first}"
        );
    }

    #[test]
    fn phase_continuous_for_multiple_partials() {
        let partials = vec![
            Partial {
                frequency: 100.0,
                amplitude: 1.0,
                phase: 0.0,
            },
            Partial {
                frequency: 300.0,
                amplitude: 0.5,
                phase: 0.0,
            },
        ];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);

        // Generate a long buffer in two halves and compare to a single generation.
        let mut synth2 = synth.clone();
        let mut full = vec![0.0f32; 512];
        synth2.generate(&mut full);

        let mut half1 = vec![0.0f32; 256];
        let mut half2 = vec![0.0f32; 256];
        synth.generate(&mut half1);
        synth.generate(&mut half2);

        // The concatenated halves should match the full buffer.
        for (i, (&f, &h)) in full[..256].iter().zip(half1.iter()).enumerate() {
            assert!(
                (f - h).abs() < 1e-5,
                "first half sample {i} differs: full={f}, half={h}"
            );
        }
        for (i, (&f, &h)) in full[256..].iter().zip(half2.iter()).enumerate() {
            assert!(
                (f - h).abs() < 1e-5,
                "second half sample {i} differs: full={f}, half={h}"
            );
        }
    }

    // --- Edge cases ---

    #[test]
    fn empty_partials_produces_silence() {
        let mut synth = AdditiveSynth::new(&[], SAMPLE_RATE);
        let mut buf = vec![1.0f32; 256];
        synth.generate(&mut buf);

        for &s in &buf {
            assert!(s.abs() < 1e-10, "no partials should produce silence");
        }
    }

    #[test]
    fn empty_buffer_is_noop() {
        let partials = vec![Partial {
            frequency: 440.0,
            amplitude: 1.0,
            phase: 0.0,
        }];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        let mut buf: Vec<f32> = vec![];
        synth.generate(&mut buf);
        assert!(buf.is_empty());
    }

    #[test]
    fn zero_amplitude_partials_produce_silence() {
        let partials = vec![
            Partial {
                frequency: 440.0,
                amplitude: 0.0,
                phase: 0.0,
            },
            Partial {
                frequency: 880.0,
                amplitude: 0.0,
                phase: 0.0,
            },
        ];
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        let mut buf = vec![0.0f32; 256];
        synth.generate(&mut buf);

        for &s in &buf {
            assert!(s.abs() < 1e-10, "zero-amplitude partials should be silent");
        }
    }

    #[test]
    fn num_partials_correct() {
        let partials = harmonic_series(440.0, 7);
        let synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        assert_eq!(synth.num_partials(), 7);
    }

    #[test]
    fn sample_rate_correct() {
        let synth = AdditiveSynth::new(&[], 48000.0);
        assert!((synth.sample_rate() - 48000.0).abs() < f32::EPSILON);
    }

    // --- Serde tests ---

    #[test]
    fn partial_serde_roundtrip() {
        let partial = Partial {
            frequency: 440.0,
            amplitude: 0.75,
            phase: 1.5,
        };
        let json = serde_json::to_string(&partial).unwrap();
        let recovered: Partial = serde_json::from_str(&json).unwrap();
        assert_eq!(partial, recovered);
    }

    #[test]
    fn partial_vec_serde_roundtrip() {
        let partials = harmonic_series(220.0, 4);
        let json = serde_json::to_string(&partials).unwrap();
        let recovered: Vec<Partial> = serde_json::from_str(&json).unwrap();
        assert_eq!(partials, recovered);
    }
}
