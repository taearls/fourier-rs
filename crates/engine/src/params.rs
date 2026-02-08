//! Engine parameter types and lock-free parameter messaging.

use std::sync::Arc;

use fourier_core::WaveformType;
use fourier_file_io::AudioBuffer;
use serde::{Deserialize, Serialize};

/// Run-time adjustable engine parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineParams {
    /// Master output gain (linear, 0.0–1.0+).
    pub output_gain: f32,
    /// Whether audio processing is bypassed (pass-through).
    pub bypass: bool,
    /// Spectral peak detection threshold in dBFS.
    pub peak_threshold_db: f32,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            output_gain: 1.0,
            bypass: false,
            peak_threshold_db: -60.0,
        }
    }
}

/// Type of noise to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseType {
    /// Uniform white noise (equal energy per sample).
    White,
    /// Pink noise (equal energy per octave, −3 dB/octave roll-off).
    Pink,
}

/// A single partial for additive synthesis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Partial {
    /// Frequency in Hz.
    pub frequency: f32,
    /// Linear amplitude (0.0–1.0 typical).
    pub amplitude: f32,
    /// Starting phase in radians.
    pub phase: f32,
}

/// Specification for the audio source feeding the engine.
///
/// This is a serializable description; the engine constructs the actual
/// source objects from this spec.
///
/// The `AudioBuffer` variant contains an `Arc<AudioBuffer>` that is not
/// serializable — it is loaded at runtime and referenced by handle.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SourceSpec {
    /// Live audio input (microphone / line-in via ring buffer).
    #[default]
    LiveInput,
    /// Generated oscillator waveform.
    Oscillator {
        waveform: WaveformType,
        frequency: f32,
        amplitude: f32,
    },
    /// Generated noise signal.
    Noise {
        noise_type: NoiseType,
        amplitude: f32,
    },
    /// Additive synthesis from a set of partials.
    Additive { partials: Vec<Partial> },
    /// Play back a loaded audio buffer through the pipeline.
    AudioBuffer {
        /// The audio buffer to play. Skipped during serialization.
        #[serde(skip)]
        buffer: Option<Arc<AudioBuffer>>,
        /// Whether to loop playback when reaching the end.
        looping: bool,
    },
}

impl PartialEq for SourceSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::LiveInput, Self::LiveInput) => true,
            (
                Self::Oscillator {
                    waveform: w1,
                    frequency: f1,
                    amplitude: a1,
                },
                Self::Oscillator {
                    waveform: w2,
                    frequency: f2,
                    amplitude: a2,
                },
            ) => w1 == w2 && f1 == f2 && a1 == a2,
            (
                Self::Noise {
                    noise_type: n1,
                    amplitude: a1,
                },
                Self::Noise {
                    noise_type: n2,
                    amplitude: a2,
                },
            ) => n1 == n2 && a1 == a2,
            (Self::Additive { partials: p1 }, Self::Additive { partials: p2 }) => p1 == p2,
            (
                Self::AudioBuffer {
                    buffer: b1,
                    looping: l1,
                },
                Self::AudioBuffer {
                    buffer: b2,
                    looping: l2,
                },
            ) => {
                let bufs_eq = match (b1, b2) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                };
                bufs_eq && l1 == l2
            }
            _ => false,
        }
    }
}

/// Messages sent from UI → processing thread to update parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ParamMessage {
    /// Set the output gain.
    SetOutputGain(f32),
    /// Enable/disable bypass.
    SetBypass(bool),
    /// Set peak detection threshold.
    SetPeakThreshold(f32),
    /// Replace the active transform chain with a new one.
    SetTransform(TransformSpec),
    /// Replace the audio source.
    SetSource(SourceSpec),
    /// Seek to a position in the audio buffer (0.0 = start, 1.0 = end).
    Seek(f32),
    /// Stop the engine.
    Shutdown,
}

/// Specification for a transform to be applied.
/// This is a serializable description; the engine constructs the actual
/// transform objects from this spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum TransformSpec {
    Identity,
    LowPass { cutoff_hz: f32 },
    HighPass { cutoff_hz: f32 },
    BandPass { low_hz: f32, high_hz: f32 },
    Gain { factor: f32 },
    Chain(Vec<Self>),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper: serialize to JSON, then deserialize back and assert equality.
    fn roundtrip_json<T>(value: &T) -> String
    where
        T: Serialize + serde::de::DeserializeOwned + std::fmt::Debug + PartialEq,
    {
        let json = serde_json::to_string_pretty(value).unwrap();
        let recovered: T = serde_json::from_str(&json).unwrap();
        assert_eq!(
            *value, recovered,
            "roundtrip failed for {value:?}\nJSON: {json}"
        );
        json
    }

    // --- SourceSpec roundtrips ---

    #[test]
    fn source_spec_live_input_roundtrip() {
        let spec = SourceSpec::LiveInput;
        let json = roundtrip_json(&spec);
        assert!(
            json.contains("\"type\""),
            "should use tagged representation"
        );
        assert!(json.contains("LiveInput"));
    }

    #[test]
    fn source_spec_oscillator_roundtrip() {
        let spec = SourceSpec::Oscillator {
            waveform: WaveformType::Sawtooth,
            frequency: 440.0,
            amplitude: 0.8,
        };
        let json = roundtrip_json(&spec);
        assert!(json.contains("Oscillator"));
        assert!(json.contains("Sawtooth"));
    }

    #[test]
    fn source_spec_noise_roundtrip() {
        let spec = SourceSpec::Noise {
            noise_type: NoiseType::Pink,
            amplitude: 0.5,
        };
        roundtrip_json(&spec);
    }

    #[test]
    fn source_spec_additive_roundtrip() {
        let spec = SourceSpec::Additive {
            partials: vec![
                Partial {
                    frequency: 220.0,
                    amplitude: 1.0,
                    phase: 0.0,
                },
                Partial {
                    frequency: 440.0,
                    amplitude: 0.5,
                    phase: std::f32::consts::PI,
                },
            ],
        };
        roundtrip_json(&spec);
    }

    // --- TransformSpec roundtrips ---

    #[test]
    fn transform_spec_identity_roundtrip() {
        roundtrip_json(&TransformSpec::Identity);
    }

    #[test]
    fn transform_spec_low_pass_roundtrip() {
        roundtrip_json(&TransformSpec::LowPass { cutoff_hz: 1000.0 });
    }

    #[test]
    fn transform_spec_high_pass_roundtrip() {
        roundtrip_json(&TransformSpec::HighPass { cutoff_hz: 200.0 });
    }

    #[test]
    fn transform_spec_band_pass_roundtrip() {
        roundtrip_json(&TransformSpec::BandPass {
            low_hz: 300.0,
            high_hz: 3000.0,
        });
    }

    #[test]
    fn transform_spec_gain_roundtrip() {
        roundtrip_json(&TransformSpec::Gain { factor: 0.5 });
    }

    #[test]
    fn transform_spec_chain_roundtrip() {
        let spec = TransformSpec::Chain(vec![
            TransformSpec::LowPass { cutoff_hz: 5000.0 },
            TransformSpec::Gain { factor: 0.8 },
            TransformSpec::HighPass { cutoff_hz: 100.0 },
        ]);
        roundtrip_json(&spec);
    }

    #[test]
    fn transform_spec_nested_chain_roundtrip() {
        let spec = TransformSpec::Chain(vec![
            TransformSpec::Chain(vec![
                TransformSpec::LowPass { cutoff_hz: 2000.0 },
                TransformSpec::HighPass { cutoff_hz: 200.0 },
            ]),
            TransformSpec::Gain { factor: 0.5 },
        ]);
        roundtrip_json(&spec);
    }

    // --- NoiseType and Partial roundtrips ---

    #[test]
    fn noise_type_roundtrip() {
        roundtrip_json(&NoiseType::White);
        roundtrip_json(&NoiseType::Pink);
    }

    #[test]
    fn partial_roundtrip() {
        let partial = Partial {
            frequency: 880.0,
            amplitude: 0.75,
            phase: 1.5,
        };
        roundtrip_json(&partial);
    }

    // --- EngineParams roundtrip ---

    #[test]
    fn engine_params_roundtrip() {
        let params = EngineParams {
            output_gain: 0.7,
            bypass: true,
            peak_threshold_db: -40.0,
        };
        roundtrip_json(&params);
    }

    // --- ParamMessage roundtrips ---

    #[test]
    fn param_message_set_gain_roundtrip() {
        let msg = ParamMessage::SetOutputGain(0.5);
        let json = serde_json::to_string_pretty(&msg).unwrap();
        assert!(json.contains("SetOutputGain"));
        let recovered: ParamMessage = serde_json::from_str(&json).unwrap();
        match recovered {
            ParamMessage::SetOutputGain(g) => assert!((g - 0.5).abs() < f32::EPSILON),
            other => panic!("expected SetOutputGain, got {other:?}"),
        }
    }

    #[test]
    fn param_message_set_transform_roundtrip() {
        let msg = ParamMessage::SetTransform(TransformSpec::BandPass {
            low_hz: 200.0,
            high_hz: 4000.0,
        });
        let json = serde_json::to_string_pretty(&msg).unwrap();
        let recovered: ParamMessage = serde_json::from_str(&json).unwrap();
        match recovered {
            ParamMessage::SetTransform(spec) => {
                assert_eq!(
                    spec,
                    TransformSpec::BandPass {
                        low_hz: 200.0,
                        high_hz: 4000.0,
                    }
                );
            }
            other => panic!("expected SetTransform, got {other:?}"),
        }
    }

    #[test]
    fn param_message_shutdown_roundtrip() {
        let msg = ParamMessage::Shutdown;
        let json = serde_json::to_string_pretty(&msg).unwrap();
        let recovered: ParamMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(recovered, ParamMessage::Shutdown));
    }

    #[test]
    fn param_message_seek_roundtrip() {
        let msg = ParamMessage::Seek(0.5);
        let json = serde_json::to_string_pretty(&msg).unwrap();
        assert!(json.contains("Seek"));
        let recovered: ParamMessage = serde_json::from_str(&json).unwrap();
        match recovered {
            ParamMessage::Seek(pos) => assert!((pos - 0.5).abs() < f32::EPSILON),
            other => panic!("expected Seek, got {other:?}"),
        }
    }

    #[test]
    fn source_spec_audio_buffer_roundtrip() {
        // AudioBuffer serializes with buffer skipped (becomes None on deserialize).
        let spec = SourceSpec::AudioBuffer {
            buffer: None,
            looping: true,
        };
        let json = roundtrip_json(&spec);
        assert!(json.contains("AudioBuffer"));
        assert!(json.contains("\"looping\": true"));
    }

    #[test]
    fn source_spec_audio_buffer_with_buffer_serializes_without_buffer() {
        let buf = Arc::new(AudioBuffer {
            samples: vec![0.0; 100],
            sample_rate: 44100,
            channels: 1,
        });
        let spec = SourceSpec::AudioBuffer {
            buffer: Some(buf),
            looping: false,
        };
        let json = serde_json::to_string_pretty(&spec).unwrap();
        // Buffer field is skipped in serialization.
        assert!(!json.contains("samples"));
        // Deserialize — buffer comes back as None.
        let recovered: SourceSpec = serde_json::from_str(&json).unwrap();
        match recovered {
            SourceSpec::AudioBuffer { buffer, looping } => {
                assert!(
                    buffer.is_none(),
                    "buffer should be None after deserialization"
                );
                assert!(!looping);
            }
            other => panic!("expected AudioBuffer, got {other:?}"),
        }
    }

    // --- JSON readability ---

    #[test]
    fn json_output_is_human_readable() {
        let spec = SourceSpec::Oscillator {
            waveform: WaveformType::Sine,
            frequency: 440.0,
            amplitude: 1.0,
        };
        let json = serde_json::to_string_pretty(&spec).unwrap();

        // Verify the JSON uses readable field names, not indices.
        assert!(json.contains("\"type\": \"Oscillator\""));
        assert!(json.contains("\"waveform\": \"Sine\""));
        assert!(json.contains("\"frequency\""));
        assert!(json.contains("\"amplitude\""));
    }

    #[test]
    fn waveform_type_all_variants_roundtrip() {
        for waveform in [
            WaveformType::Sine,
            WaveformType::Square,
            WaveformType::Sawtooth,
            WaveformType::Triangle,
        ] {
            roundtrip_json(&waveform);
        }
    }
}
