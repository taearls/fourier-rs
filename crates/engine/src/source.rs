//! Audio source abstraction and implementations.
//!
//! An [`AudioSource`] generates sample buffers for the processing loop.
//! The engine owns a current source and calls [`AudioSource::generate`] each
//! frame to fill its input chunk instead of (or in addition to) reading
//! from the live-input ring buffer.

use std::f32::consts::TAU;
use std::sync::Arc;

use fourier_core::{NoiseGenerator, NoiseType, Oscillator};
use fourier_file_io::AudioBuffer;

use crate::params::{Partial, SourceSpec};

/// Trait for audio sources that fill buffers with samples.
///
/// Implementations must be `Send` so they can live on the processing thread.
pub trait AudioSource: Send {
    /// Fill `output` with generated samples.
    fn generate(&mut self, output: &mut [f32]);

    /// Seek to a normalized position (0.0 = start, 1.0 = end).
    ///
    /// The default implementation is a no-op for sources that don't support seeking.
    fn seek(&mut self, _position: f32) {}
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

/// Wraps `fourier_core::NoiseGenerator` as an `AudioSource`.
struct NoiseSource {
    generator: NoiseGenerator,
}

impl NoiseSource {
    fn new(noise_type: NoiseType, amplitude: f32, sample_rate: f32) -> Self {
        Self {
            generator: NoiseGenerator::new(noise_type, amplitude, sample_rate),
        }
    }
}

impl AudioSource for NoiseSource {
    fn generate(&mut self, output: &mut [f32]) {
        self.generator.generate(output);
    }
}

/// Additive synthesis source: sums phase-continuous sinusoidal partials.
struct AdditiveSource {
    /// Per-partial state: `(phase_increment, amplitude, current_phase)`.
    partials: Vec<(f32, f32, f32)>,
}

impl AdditiveSource {
    fn new(partials: &[Partial], sample_rate: f32) -> Self {
        let partials = partials
            .iter()
            .map(|p| (TAU * p.frequency / sample_rate, p.amplitude, p.phase))
            .collect();
        Self { partials }
    }
}

impl AudioSource for AdditiveSource {
    fn generate(&mut self, output: &mut [f32]) {
        // Zero the buffer first — we accumulate across partials.
        for s in output.iter_mut() {
            *s = 0.0;
        }

        for (phase_inc, amp, phase) in &mut self.partials {
            for s in output.iter_mut() {
                *s += *amp * phase.sin();
                *phase += *phase_inc;
                // Wrap per-sample to prevent precision loss at high frequencies.
                if *phase >= TAU {
                    *phase -= TAU;
                }
            }
        }
    }
}

/// Plays back samples from a loaded [`AudioBuffer`].
///
/// Reads mono samples from the buffer (mixing stereo to mono if needed)
/// and optionally loops when reaching the end.
///
/// **Note:** Samples are played back at the engine's sample rate regardless
/// of the buffer's native rate. No sample rate conversion is performed, so
/// a buffer recorded at a different rate will play back at the wrong pitch.
pub struct AudioBufferSource {
    /// Shared reference to the audio buffer data.
    buffer: Arc<AudioBuffer>,
    /// Current playback position in sample frames.
    position: usize,
    /// Whether playback loops back to the start.
    looping: bool,
}

impl AudioBufferSource {
    /// Create a new audio buffer source.
    pub const fn new(buffer: Arc<AudioBuffer>, looping: bool) -> Self {
        Self {
            buffer,
            position: 0,
            looping,
        }
    }

    /// Read a single mono sample at the given frame index.
    ///
    /// For multi-channel buffers, averages all channels into mono.
    #[inline]
    fn read_frame(&self, frame: usize) -> f32 {
        let channels = self.buffer.channels as usize;
        if channels <= 1 {
            self.buffer.samples[frame]
        } else {
            let base = frame * channels;
            let mut sum = 0.0_f32;
            for ch in 0..channels {
                sum += self.buffer.samples[base + ch];
            }
            sum / channels as f32
        }
    }
}

impl AudioSource for AudioBufferSource {
    fn generate(&mut self, output: &mut [f32]) {
        let num_frames = self.buffer.num_frames();
        if num_frames == 0 {
            output.fill(0.0);
            return;
        }

        for sample in output.iter_mut() {
            if self.position >= num_frames {
                if self.looping {
                    self.position = 0;
                } else {
                    // Past the end and not looping — output silence.
                    *sample = 0.0;
                    continue;
                }
            }
            *sample = self.read_frame(self.position);
            self.position += 1;
        }
    }

    fn seek(&mut self, position: f32) {
        let num_frames = self.buffer.num_frames();
        let clamped = position.clamp(0.0, 1.0);
        self.position = ((clamped * num_frames as f32) as usize).min(num_frames);
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
        } => Some(Box::new(NoiseSource::new(
            *noise_type,
            *amplitude,
            sample_rate,
        ))),
        SourceSpec::Additive { partials } => {
            Some(Box::new(AdditiveSource::new(partials, sample_rate)))
        }
        SourceSpec::AudioBuffer { buffer, looping } => {
            buffer.as_ref().map(|buf| -> Box<dyn AudioSource> {
                Box::new(AudioBufferSource::new(Arc::clone(buf), *looping))
            })
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
        let mut source = NoiseSource::new(NoiseType::White, 1.0, SAMPLE_RATE);
        let mut buf = vec![0.0_f32; BUFFER_SIZE];
        source.generate(&mut buf);

        let energy: f32 = buf.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "white noise should have energy");
    }

    #[test]
    fn white_noise_respects_amplitude() {
        let amplitude = 0.5;
        let mut source = NoiseSource::new(NoiseType::White, amplitude, SAMPLE_RATE);
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
        let mut source = NoiseSource::new(NoiseType::Pink, 1.0, SAMPLE_RATE);
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

    // --- AudioBufferSource tests ---

    /// Helper: create a mono `AudioBuffer` with a known ramp pattern.
    fn make_mono_buffer(num_frames: usize, sample_rate: u32) -> Arc<AudioBuffer> {
        let samples: Vec<f32> = (0..num_frames)
            .map(|i| i as f32 / num_frames as f32)
            .collect();
        Arc::new(AudioBuffer {
            samples,
            sample_rate,
            channels: 1,
        })
    }

    /// Helper: create a stereo `AudioBuffer` with known L/R pattern.
    fn make_stereo_buffer(num_frames: usize, sample_rate: u32) -> Arc<AudioBuffer> {
        let mut samples = Vec::with_capacity(num_frames * 2);
        for i in 0..num_frames {
            let val = i as f32 / num_frames as f32;
            samples.push(val); // Left channel
            samples.push(-val); // Right channel (inverted)
        }
        Arc::new(AudioBuffer {
            samples,
            sample_rate,
            channels: 2,
        })
    }

    #[test]
    fn audio_buffer_source_plays_mono_samples() {
        let buf = make_mono_buffer(100, 44100);
        let mut source = AudioBufferSource::new(buf, false);
        let mut output = vec![0.0_f32; 50];
        source.generate(&mut output);

        // Should read the first 50 frames of the ramp.
        for (i, &s) in output.iter().enumerate() {
            let expected = i as f32 / 100.0;
            assert!(
                (s - expected).abs() < 1e-6,
                "frame {i}: expected {expected}, got {s}"
            );
        }
    }

    #[test]
    fn audio_buffer_source_stereo_mixdown() {
        let buf = make_stereo_buffer(100, 44100);
        let mut source = AudioBufferSource::new(buf, false);
        let mut output = vec![0.0_f32; 50];
        source.generate(&mut output);

        // Stereo mixdown: (val + (-val)) / 2 = 0.0 for all frames.
        for (i, &s) in output.iter().enumerate() {
            assert!(
                s.abs() < 1e-6,
                "frame {i}: stereo mixdown should be 0.0, got {s}"
            );
        }
    }

    #[test]
    fn audio_buffer_source_no_loop_outputs_silence_past_end() {
        let buf = make_mono_buffer(10, 44100);
        let mut source = AudioBufferSource::new(buf, false);
        let mut output = vec![0.0_f32; 20];
        source.generate(&mut output);

        // First 10 samples should have data, next 10 should be silence.
        for &s in &output[10..] {
            assert!(s.abs() < 1e-6, "past-end samples should be silent, got {s}");
        }
    }

    #[test]
    fn audio_buffer_source_looping() {
        let buf = make_mono_buffer(10, 44100);
        let mut source = AudioBufferSource::new(buf, true);
        let mut output = vec![0.0_f32; 25];
        source.generate(&mut output);

        // Should play 10 frames, loop back, play 10 more, loop back, play 5 more.
        for (i, &s) in output.iter().enumerate() {
            let frame = i % 10;
            let expected = frame as f32 / 10.0;
            assert!(
                (s - expected).abs() < 1e-6,
                "looped frame {i} (pos {frame}): expected {expected}, got {s}"
            );
        }
    }

    #[test]
    fn audio_buffer_source_seek() {
        let buf = make_mono_buffer(100, 44100);
        let mut source = AudioBufferSource::new(buf, false);

        // Seek to 50%.
        source.seek(0.5);

        let mut output = vec![0.0_f32; 10];
        source.generate(&mut output);

        // Should start reading from frame 50.
        for (i, &s) in output.iter().enumerate() {
            let expected = (50 + i) as f32 / 100.0;
            assert!(
                (s - expected).abs() < 1e-6,
                "after seek to 50%, frame {i}: expected {expected}, got {s}"
            );
        }
    }

    #[test]
    fn audio_buffer_source_seek_clamps() {
        let buf = make_mono_buffer(100, 44100);
        let mut source = AudioBufferSource::new(buf, false);

        // Seek past end — should clamp to end.
        source.seek(2.0);
        let mut output = vec![0.0_f32; 5];
        source.generate(&mut output);
        for &s in &output {
            assert!(s.abs() < 1e-6, "seek past end should produce silence");
        }

        // Seek before start — should clamp to start.
        source.seek(-1.0);
        source.generate(&mut output);
        assert!(
            output[0].abs() < 1e-6,
            "seek before start should clamp to frame 0"
        );
    }

    #[test]
    fn audio_buffer_source_empty_buffer() {
        let buf = Arc::new(AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        });
        let mut source = AudioBufferSource::new(buf, true);
        let mut output = vec![1.0_f32; 10];
        source.generate(&mut output);

        // Empty buffer should produce silence.
        for &s in &output {
            assert!(s.abs() < 1e-6, "empty buffer should produce silence");
        }
    }

    #[test]
    fn build_source_returns_some_for_audio_buffer() {
        let buf = make_mono_buffer(100, 44100);
        let spec = SourceSpec::AudioBuffer {
            buffer: Some(buf),
            looping: false,
        };
        assert!(
            build_source(&spec, SAMPLE_RATE).is_some(),
            "AudioBuffer spec with buffer should produce a source"
        );
    }

    #[test]
    fn build_source_returns_none_for_audio_buffer_without_buffer() {
        let spec = SourceSpec::AudioBuffer {
            buffer: None,
            looping: false,
        };
        assert!(
            build_source(&spec, SAMPLE_RATE).is_none(),
            "AudioBuffer spec without buffer should return None"
        );
    }
}
