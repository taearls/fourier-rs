//! Preset system for saving and loading complete engine configurations.
//!
//! A [`Preset`] captures a source specification, transform chain, and gain
//! level so that entire sound design configurations can be stored as JSON
//! files and recalled later.

use serde::{Deserialize, Serialize};

use crate::params::{SourceSpec, TransformSpec};
use fourier_core::transform::{BandType, EqBand};

/// A complete engine configuration that can be saved and loaded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Human-readable name for this preset.
    pub name: String,
    /// Audio source configuration.
    pub source: SourceSpec,
    /// Spectral transform chain.
    pub transform: TransformSpec,
    /// Master output gain (linear, 0.0–1.0+).
    pub gain: f32,
}

/// Metadata about a preset for listing purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    /// The preset name (also used as the filename stem).
    pub name: String,
    /// Whether this is a built-in factory preset.
    pub is_factory: bool,
}

/// Returns the built-in factory presets.
pub fn factory_presets() -> Vec<Preset> {
    vec![
        Preset {
            name: "Clean Sine".to_string(),
            source: SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 440.0,
                amplitude: 1.0,
            },
            transform: TransformSpec::Identity,
            gain: 0.75,
        },
        Preset {
            name: "Low-Pass Voice".to_string(),
            source: SourceSpec::LiveInput,
            transform: TransformSpec::LowPass { cutoff_hz: 2000.0 },
            gain: 0.8,
        },
        Preset {
            name: "Octave Up".to_string(),
            source: SourceSpec::LiveInput,
            transform: TransformSpec::PitchShift { semitones: 12.0 },
            gain: 0.75,
        },
        Preset {
            name: "Warm Pad".to_string(),
            source: SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sawtooth,
                frequency: 220.0,
                amplitude: 0.8,
            },
            transform: TransformSpec::Chain(vec![
                TransformSpec::LowPass { cutoff_hz: 3000.0 },
                TransformSpec::ParametricEq {
                    bands: vec![EqBand {
                        frequency: 800.0,
                        gain_db: 3.0,
                        q: 1.5,
                        band_type: BandType::Peak,
                    }],
                },
            ]),
            gain: 0.6,
        },
        Preset {
            name: "Pink Noise Ambience".to_string(),
            source: SourceSpec::Noise {
                noise_type: fourier_core::NoiseType::Pink,
                amplitude: 0.5,
            },
            transform: TransformSpec::LowPass { cutoff_hz: 4000.0 },
            gain: 0.4,
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

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

    #[test]
    fn preset_roundtrip_identity() {
        let preset = Preset {
            name: "Test".to_string(),
            source: SourceSpec::LiveInput,
            transform: TransformSpec::Identity,
            gain: 0.75,
        };
        let json = roundtrip_json(&preset);
        assert!(json.contains("\"name\": \"Test\""));
        assert!(json.contains("LiveInput"));
    }

    #[test]
    fn preset_roundtrip_oscillator_with_chain() {
        let preset = Preset {
            name: "Complex".to_string(),
            source: SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sawtooth,
                frequency: 220.0,
                amplitude: 0.8,
            },
            transform: TransformSpec::Chain(vec![
                TransformSpec::LowPass { cutoff_hz: 5000.0 },
                TransformSpec::PitchShift { semitones: 7.0 },
            ]),
            gain: 0.5,
        };
        roundtrip_json(&preset);
    }

    #[test]
    fn preset_roundtrip_noise() {
        let preset = Preset {
            name: "Noise".to_string(),
            source: SourceSpec::Noise {
                noise_type: fourier_core::NoiseType::Pink,
                amplitude: 0.3,
            },
            transform: TransformSpec::Gain { factor: 0.5 },
            gain: 1.0,
        };
        roundtrip_json(&preset);
    }

    #[test]
    fn factory_presets_are_valid() {
        let presets = factory_presets();
        assert_eq!(presets.len(), 5);
        for preset in &presets {
            roundtrip_json(preset);
        }
    }

    #[test]
    fn factory_preset_names_unique() {
        let presets = factory_presets();
        let mut names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
        let len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), len, "factory preset names must be unique");
    }

    #[test]
    fn preset_info_roundtrip() {
        let info = PresetInfo {
            name: "My Preset".to_string(),
            is_factory: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let recovered: PresetInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered.name, "My Preset");
        assert!(!recovered.is_factory);
    }

    #[test]
    fn preset_json_is_human_readable() {
        let preset = Preset {
            name: "Readable".to_string(),
            source: SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 440.0,
                amplitude: 1.0,
            },
            transform: TransformSpec::Identity,
            gain: 0.75,
        };
        let json = serde_json::to_string_pretty(&preset).unwrap();
        assert!(json.contains("\"name\": \"Readable\""));
        assert!(json.contains("\"gain\": 0.75"));
        assert!(json.contains("\"frequency\": 440.0"));
    }
}
