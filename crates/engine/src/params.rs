//! Engine parameter types and lock-free parameter messaging.

use fourier_core::WaveformType;
use serde::{Deserialize, Serialize};

/// Run-time adjustable engine parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
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
        assert!(json.contains("\"type\""), "should use tagged representation");
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
        let json = serde_json::to_string_pretty(&params).unwrap();
        let recovered: EngineParams = serde_json::from_str(&json).unwrap();
        assert!((params.output_gain - recovered.output_gain).abs() < f32::EPSILON);
        assert_eq!(params.bypass, recovered.bypass);
        assert!((params.peak_threshold_db - recovered.peak_threshold_db).abs() < f32::EPSILON);
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
