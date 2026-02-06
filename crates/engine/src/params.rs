//! Engine parameter types and lock-free parameter messaging.

use fourier_core::WaveformType;

/// Run-time adjustable engine parameters.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseType {
    /// Uniform white noise (equal energy per sample).
    White,
    /// Pink noise (equal energy per octave, −3 dB/octave roll-off).
    Pink,
}

/// A single partial for additive synthesis.
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Default, Clone, PartialEq)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub enum TransformSpec {
    Identity,
    LowPass { cutoff_hz: f32 },
    HighPass { cutoff_hz: f32 },
    BandPass { low_hz: f32, high_hz: f32 },
    Gain { factor: f32 },
    Chain(Vec<Self>),
}
