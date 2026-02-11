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

    /// Replace all partials with a new set.
    ///
    /// Resets phase tracking for every partial.
    pub fn set_partials(&mut self, partials: &[Partial]) {
        self.partials = partials
            .iter()
            .map(|p| PartialState {
                phase_inc: TAU * p.frequency / self.sample_rate,
                amplitude: p.amplitude,
                phase: p.phase,
            })
            .collect();
    }

    /// Append a partial. Returns the index of the new partial.
    pub fn add_partial(&mut self, partial: Partial) -> usize {
        self.partials.push(PartialState {
            phase_inc: TAU * partial.frequency / self.sample_rate,
            amplitude: partial.amplitude,
            phase: partial.phase,
        });
        self.partials.len() - 1
    }

    /// Remove the partial at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn remove_partial(&mut self, index: usize) {
        self.partials.remove(index);
    }

    /// Set the frequency of the partial at `index` (in Hz).
    ///
    /// Phase is preserved so the transition is continuous.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn set_partial_frequency(&mut self, index: usize, frequency: f32) {
        self.partials[index].phase_inc = TAU * frequency / self.sample_rate;
    }

    /// Set the amplitude of the partial at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    pub fn set_partial_amplitude(&mut self, index: usize, amplitude: f32) {
        self.partials[index].amplitude = amplitude;
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
    harmonic_series_with(fundamental, num_harmonics, |n| 1.0 / n as f32)
}

/// Create a harmonic series with a custom amplitude function.
///
/// Like [`harmonic_series`], but the amplitude of each partial is determined
/// by `amplitude_fn(n)` where `n` is the 1-based harmonic number.
///
/// # Common amplitude functions
///
/// | Timbre | Function |
/// |--------|----------|
/// | Sawtooth-like | `\|n\| 1.0 / n as f32` (default in [`harmonic_series`]) |
/// | Triangle-like | `\|n\| if n % 2 == 0 { 0.0 } else { 1.0 / (n as f32 * n as f32) }` |
/// | Organ-like | `\|_\| 1.0` (equal amplitude) |
///
/// # Example
///
/// ```
/// use fourier_core::additive::harmonic_series_with;
///
/// // Triangle-like: odd harmonics only, 1/n² rolloff.
/// let partials = harmonic_series_with(440.0, 5, |n| {
///     if n % 2 == 0 { 0.0 } else { 1.0 / (n * n) as f32 }
/// });
/// assert_eq!(partials.len(), 5);
/// assert!(partials[1].amplitude.abs() < f32::EPSILON); // 2nd harmonic silent
/// ```
pub fn harmonic_series_with(
    fundamental: f32,
    num_harmonics: usize,
    amplitude_fn: impl Fn(usize) -> f32,
) -> Vec<Partial> {
    (1..=num_harmonics)
        .map(|n| Partial {
            frequency: fundamental * n as f32,
            amplitude: amplitude_fn(n),
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

    // --- Runtime mutator tests ---

    #[test]
    fn set_partials_replaces_all() {
        let mut synth = AdditiveSynth::new(
            &[Partial {
                frequency: 100.0,
                amplitude: 1.0,
                phase: 0.0,
            }],
            SAMPLE_RATE,
        );
        assert_eq!(synth.num_partials(), 1);

        let new_partials = harmonic_series(440.0, 3);
        synth.set_partials(&new_partials);
        assert_eq!(synth.num_partials(), 3);

        // Verify it generates the new partials, not the old one.
        let mut buf = vec![0.0f32; FFT_SIZE];
        synth.generate(&mut buf);
        let mut fft = FftProcessor::new(FFT_SIZE);
        let mut spectrum = fft.alloc_spectrum();
        fft.forward(&mut buf, &mut spectrum).unwrap();
        let mags: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();

        let bw = bin_width();
        let fund_bin = (440.0 / bw).round() as usize;
        assert_eq!(peak_bin(&mags), fund_bin, "peak should be at 440 Hz");
    }

    #[test]
    fn add_partial_appends() {
        let mut synth = AdditiveSynth::new(&[], SAMPLE_RATE);
        assert_eq!(synth.num_partials(), 0);

        let idx = synth.add_partial(Partial {
            frequency: 440.0,
            amplitude: 1.0,
            phase: 0.0,
        });
        assert_eq!(idx, 0);
        assert_eq!(synth.num_partials(), 1);

        let idx2 = synth.add_partial(Partial {
            frequency: 880.0,
            amplitude: 0.5,
            phase: 0.0,
        });
        assert_eq!(idx2, 1);
        assert_eq!(synth.num_partials(), 2);

        // Verify it generates sound.
        let mut buf = vec![0.0f32; 256];
        synth.generate(&mut buf);
        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "added partials should produce output");
    }

    #[test]
    fn remove_partial_removes() {
        let partials = harmonic_series(440.0, 3);
        let mut synth = AdditiveSynth::new(&partials, SAMPLE_RATE);
        assert_eq!(synth.num_partials(), 3);

        synth.remove_partial(1); // remove the 2nd harmonic
        assert_eq!(synth.num_partials(), 2);

        synth.remove_partial(0); // remove the fundamental
        assert_eq!(synth.num_partials(), 1);
    }

    #[test]
    fn set_partial_frequency_changes_pitch() {
        let mut synth = AdditiveSynth::new(
            &[Partial {
                frequency: 440.0,
                amplitude: 1.0,
                phase: 0.0,
            }],
            SAMPLE_RATE,
        );

        // Change frequency to 1000 Hz.
        synth.set_partial_frequency(0, 1000.0);

        let mut buf = vec![0.0f32; FFT_SIZE];
        synth.generate(&mut buf);
        let mut fft = FftProcessor::new(FFT_SIZE);
        let mut spectrum = fft.alloc_spectrum();
        fft.forward(&mut buf, &mut spectrum).unwrap();
        let mags: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();

        let bw = bin_width();
        let expected_bin = (1000.0 / bw).round() as usize;
        assert_eq!(
            peak_bin(&mags),
            expected_bin,
            "peak should be at 1000 Hz after frequency change"
        );
    }

    #[test]
    fn set_partial_amplitude_changes_level() {
        let mut synth = AdditiveSynth::new(
            &[Partial {
                frequency: 440.0,
                amplitude: 1.0,
                phase: 0.0,
            }],
            SAMPLE_RATE,
        );

        // Generate at full amplitude.
        let mut buf_loud = vec![0.0f32; 1024];
        synth.generate(&mut buf_loud);
        let energy_loud: f32 = buf_loud.iter().map(|s| s * s).sum();

        // Reset phase and set amplitude to 0.5.
        synth.set_partials(&[Partial {
            frequency: 440.0,
            amplitude: 1.0,
            phase: 0.0,
        }]);
        synth.set_partial_amplitude(0, 0.5);

        let mut buf_quiet = vec![0.0f32; 1024];
        synth.generate(&mut buf_quiet);
        let energy_quiet: f32 = buf_quiet.iter().map(|s| s * s).sum();

        // Energy scales with amplitude squared: 0.5² = 0.25.
        let ratio = energy_quiet / energy_loud;
        assert!(
            (ratio - 0.25).abs() < 0.01,
            "energy ratio should be ~0.25, got {ratio:.4}"
        );
    }

    #[test]
    fn set_partial_frequency_preserves_phase_continuity() {
        let mut synth = AdditiveSynth::new(
            &[Partial {
                frequency: 440.0,
                amplitude: 1.0,
                phase: 0.0,
            }],
            SAMPLE_RATE,
        );

        let mut buf1 = vec![0.0f32; 256];
        synth.generate(&mut buf1);
        let last = buf1[255];

        // Change frequency mid-stream.
        synth.set_partial_frequency(0, 880.0);

        let mut buf2 = vec![0.0f32; 256];
        synth.generate(&mut buf2);
        let first = buf2[0];

        // Transition should be smooth — within one sample step of the new frequency.
        let max_step = TAU * 880.0 / SAMPLE_RATE;
        assert!(
            (first - last).abs() < max_step + 0.01,
            "frequency change should be phase-continuous: last={last}, first={first}"
        );
    }

    // --- harmonic_series_with tests ---

    #[test]
    fn harmonic_series_with_equal_amplitude() {
        let partials = harmonic_series_with(100.0, 4, |_| 1.0);
        assert_eq!(partials.len(), 4);

        for p in &partials {
            assert!(
                (p.amplitude - 1.0).abs() < f32::EPSILON,
                "equal amplitude: expected 1.0, got {}",
                p.amplitude
            );
        }
    }

    #[test]
    fn harmonic_series_with_triangle_rolloff() {
        // Triangle-like: odd harmonics only, 1/n² rolloff.
        let partials = harmonic_series_with(200.0, 5, |n| {
            if n % 2 == 0 {
                0.0
            } else {
                1.0 / (n * n) as f32
            }
        });

        assert_eq!(partials.len(), 5);
        // Fundamental (n=1): 1/1 = 1.0
        assert!((partials[0].amplitude - 1.0).abs() < f32::EPSILON);
        // 2nd harmonic (n=2): 0.0
        assert!(partials[1].amplitude.abs() < f32::EPSILON);
        // 3rd harmonic (n=3): 1/9
        assert!((partials[2].amplitude - 1.0 / 9.0).abs() < f32::EPSILON);
        // 4th harmonic (n=4): 0.0
        assert!(partials[3].amplitude.abs() < f32::EPSILON);
        // 5th harmonic (n=5): 1/25
        assert!((partials[4].amplitude - 1.0 / 25.0).abs() < f32::EPSILON);
    }

    #[test]
    fn harmonic_series_with_matches_default() {
        // harmonic_series should be equivalent to harmonic_series_with 1/n.
        let default = harmonic_series(440.0, 6);
        let custom = harmonic_series_with(440.0, 6, |n| 1.0 / n as f32);

        assert_eq!(default, custom);
    }

    #[test]
    fn harmonic_series_with_spectral_verification() {
        // Equal-amplitude harmonics: all 4 peaks should have similar magnitude.
        let partials = harmonic_series_with(200.0, 4, |_| 1.0);
        let mags = magnitude_spectrum(&partials);
        let bw = bin_width();

        let mut peak_mags = Vec::new();
        for n in 1..=4 {
            let freq = 200.0 * n as f32;
            let bin = (freq / bw).round() as usize;
            peak_mags.push(mags[bin]);
        }

        // All peaks should be within a generous range (unwindowed FFT has
        // spectral leakage that slightly affects peak magnitudes).
        let max_peak = peak_mags.iter().copied().reduce(f32::max).unwrap();
        let min_peak = peak_mags.iter().copied().reduce(f32::min).unwrap();
        let ratio = min_peak / max_peak;
        assert!(
            ratio > 0.5,
            "equal-amplitude harmonics should have similar peak magnitudes, ratio={ratio:.3}"
        );
    }
}
