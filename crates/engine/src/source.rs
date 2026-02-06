//! Audio source abstraction and implementations.
//!
//! An [`AudioSource`] generates sample buffers for the processing loop.
//! The engine owns a current source and calls [`AudioSource::generate`] each
//! frame to fill its input chunk instead of (or in addition to) reading
//! from the live-input ring buffer.

use std::f32::consts::TAU;

use fourier_core::Oscillator;

use crate::params::{NoiseType, Partial, SourceSpec};

/// Trait for audio sources that fill buffers with samples.
///
/// Implementations must be `Send` so they can live on the processing thread.
pub trait AudioSource: Send {
    /// Fill `output` with generated samples.
    fn generate(&mut self, output: &mut [f32]);
}

/// Wraps `fourier_core::Oscillator` as an `AudioSource`.
pub struct OscillatorSource {
    oscillator: Oscillator,
}

impl OscillatorSource {
    pub const fn new(oscillator: Oscillator) -> Self {
        Self { oscillator }
    }
}

impl AudioSource for OscillatorSource {
    fn generate(&mut self, output: &mut [f32]) {
        self.oscillator.generate(output);
    }
}

/// White noise generator using a simple xorshift PRNG.
///
/// Produces uniform random values in `[-amplitude, +amplitude]`.
struct WhiteNoiseSource {
    state: u64,
    amplitude: f32,
}

impl WhiteNoiseSource {
    const fn new(amplitude: f32) -> Self {
        // Non-zero seed for xorshift.
        Self {
            state: 0x5DEE_CE66_D1A4_F681,
            amplitude,
        }
    }

    /// `xorshift64` PRNG — fast, no dependencies, deterministic.
    #[inline]
    const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Map a u64 to a float in `[-1.0, +1.0)`.
    #[inline]
    fn next_f32(&mut self) -> f32 {
        // Use the upper 24 bits for mantissa precision.
        let bits = (self.next_u64() >> 40) as f32;
        let max = (1u64 << 24) as f32;
        2.0f32.mul_add(bits / max, -1.0)
    }
}

impl AudioSource for WhiteNoiseSource {
    fn generate(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            *sample = self.amplitude * self.next_f32();
        }
    }
}

/// Pink noise generator using the Voss-McCartney algorithm.
///
/// Sums several random values updated at octave-spaced intervals,
/// producing an approximate −3 dB/octave (1/f) spectrum.
struct PinkNoiseSource {
    white: WhiteNoiseSource,
    /// Per-octave row values.
    rows: [f32; Self::NUM_ROWS],
    /// Running sum of row values.
    running_sum: f32,
    /// Sample counter for octave scheduling.
    counter: u32,
    amplitude: f32,
}

impl PinkNoiseSource {
    const NUM_ROWS: usize = 12;
    /// Normalization factor: each row contributes ±1, plus the white component.
    const SCALE: f32 = 1.0 / (Self::NUM_ROWS as f32 + 1.0);

    fn new(amplitude: f32) -> Self {
        let mut white = WhiteNoiseSource::new(1.0);
        let mut rows = [0.0_f32; Self::NUM_ROWS];
        let mut running_sum = 0.0_f32;
        for row in &mut rows {
            *row = white.next_f32();
            running_sum += *row;
        }
        Self {
            white,
            rows,
            running_sum,
            counter: 0,
            amplitude,
        }
    }
}

impl AudioSource for PinkNoiseSource {
    fn generate(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            self.counter = self.counter.wrapping_add(1);

            // Determine which rows to update this sample.
            // Row k updates every 2^k samples (trailing-zeros scheduling).
            let changed_bits = self.counter ^ self.counter.wrapping_sub(1);
            for (k, row) in self.rows.iter_mut().enumerate() {
                if changed_bits & (1 << k) != 0 {
                    self.running_sum -= *row;
                    *row = self.white.next_f32();
                    self.running_sum += *row;
                }
            }

            // Add a fresh white noise sample for the highest-frequency component.
            let white_val = self.white.next_f32();
            *sample = self.amplitude * (self.running_sum + white_val) * Self::SCALE;
        }
    }
}

/// Additive synthesis source: sums phase-continuous sinusoidal partials.
struct AdditiveSource {
    /// Per-partial state: `(frequency, amplitude, current_phase)`.
    partials: Vec<(f32, f32, f32)>,
    sample_rate: f32,
}

impl AdditiveSource {
    fn new(partials: &[Partial], sample_rate: f32) -> Self {
        let partials = partials
            .iter()
            .map(|p| (p.frequency, p.amplitude, p.phase))
            .collect();
        Self {
            partials,
            sample_rate,
        }
    }
}

impl AudioSource for AdditiveSource {
    fn generate(&mut self, output: &mut [f32]) {
        // Zero the buffer first — we accumulate across partials.
        for s in output.iter_mut() {
            *s = 0.0;
        }

        for (freq, amp, phase) in &mut self.partials {
            let phase_inc = TAU * *freq / self.sample_rate;
            for s in output.iter_mut() {
                *s += *amp * phase.sin();
                *phase += phase_inc;
            }
            // Wrap phase to prevent precision loss.
            *phase %= TAU;
        }
    }
}

/// Build a concrete [`AudioSource`] from a [`SourceSpec`].
///
/// Returns `None` for `SourceSpec::LiveInput` since live input is handled
/// by reading from the ring buffer directly.
pub fn build_source(spec: &SourceSpec, sample_rate: f32) -> Option<Box<dyn AudioSource>> {
    match spec {
        SourceSpec::LiveInput => None,
        SourceSpec::Oscillator {
            waveform,
            frequency,
            amplitude,
        } => {
            let osc = Oscillator::new(*waveform, *frequency, *amplitude, sample_rate);
            Some(Box::new(OscillatorSource::new(osc)))
        }
        SourceSpec::Noise {
            noise_type,
            amplitude,
        } => match noise_type {
            NoiseType::White => Some(Box::new(WhiteNoiseSource::new(*amplitude))),
            NoiseType::Pink => Some(Box::new(PinkNoiseSource::new(*amplitude))),
        },
        SourceSpec::Additive { partials } => {
            Some(Box::new(AdditiveSource::new(partials, sample_rate)))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use fourier_core::WaveformType;

    const SAMPLE_RATE: f32 = 44100.0;
    const BUFFER_SIZE: usize = 1024;

    #[test]
    fn oscillator_source_generates_nonzero_output() {
        let osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, SAMPLE_RATE);
        let mut source = OscillatorSource::new(osc);
        let mut buf = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(
            energy > 0.0,
            "oscillator source should produce nonzero output"
        );
    }

    #[test]
    fn white_noise_has_energy() {
        let mut source = WhiteNoiseSource::new(1.0);
        let mut buf = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "white noise should have energy");
    }

    #[test]
    fn white_noise_respects_amplitude() {
        let amplitude = 0.5;
        let mut source = WhiteNoiseSource::new(amplitude);
        let mut buf = vec![0.0_f32; 4096];
        source.generate(&mut buf);

        for &s in &buf {
            assert!(
                s.abs() <= amplitude + 1e-6,
                "white noise sample {s} exceeds amplitude {amplitude}"
            );
        }
    }

    #[test]
    fn pink_noise_has_energy() {
        let mut source = PinkNoiseSource::new(1.0);
        let mut buf = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "pink noise should have energy");
    }

    #[test]
    fn additive_single_partial_matches_sine() {
        let partials = vec![Partial {
            frequency: 440.0,
            amplitude: 1.0,
            phase: 0.0,
        }];
        let mut source = AdditiveSource::new(&partials, SAMPLE_RATE);
        let mut osc = Oscillator::new(WaveformType::Sine, 440.0, 1.0, SAMPLE_RATE);

        let mut buf_additive = vec![0.0_f32; BUFFER_SIZE];
        let mut buf_osc = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf_additive);
        osc.generate(&mut buf_osc);

        for (a, o) in buf_additive.iter().zip(buf_osc.iter()) {
            assert!(
                (a - o).abs() < 1e-3,
                "additive single partial should match sine: additive={a}, osc={o}"
            );
        }
    }

    #[test]
    fn additive_multiple_partials_sum() {
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
            Partial {
                frequency: 300.0,
                amplitude: 0.25,
                phase: 0.0,
            },
        ];
        let mut source = AdditiveSource::new(&partials, SAMPLE_RATE);
        let mut buf = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(
            energy > 0.0,
            "additive source should produce nonzero output"
        );
    }

    #[test]
    fn build_source_returns_none_for_live_input() {
        assert!(
            build_source(&SourceSpec::LiveInput, SAMPLE_RATE).is_none(),
            "LiveInput should return None"
        );
    }

    #[test]
    fn build_source_returns_some_for_oscillator() {
        let spec = SourceSpec::Oscillator {
            waveform: WaveformType::Sine,
            frequency: 440.0,
            amplitude: 1.0,
        };
        let source = build_source(&spec, SAMPLE_RATE);
        assert!(source.is_some(), "Oscillator spec should produce a source");
    }

    #[test]
    fn build_source_returns_some_for_noise() {
        let spec = SourceSpec::Noise {
            noise_type: NoiseType::White,
            amplitude: 1.0,
        };
        assert!(
            build_source(&spec, SAMPLE_RATE).is_some(),
            "White noise spec should produce a source"
        );

        let spec = SourceSpec::Noise {
            noise_type: NoiseType::Pink,
            amplitude: 1.0,
        };
        assert!(
            build_source(&spec, SAMPLE_RATE).is_some(),
            "Pink noise spec should produce a source"
        );
    }

    #[test]
    fn build_source_returns_some_for_additive() {
        let spec = SourceSpec::Additive {
            partials: vec![Partial {
                frequency: 440.0,
                amplitude: 1.0,
                phase: 0.0,
            }],
        };
        assert!(
            build_source(&spec, SAMPLE_RATE).is_some(),
            "Additive spec should produce a source"
        );
    }
}
