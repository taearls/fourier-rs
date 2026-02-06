//! Engine parameter types and lock-free parameter messaging.

use crossbeam_channel::{Receiver, Sender, bounded};

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
    Chain(Vec<TransformSpec>),
}

/// Create a parameter message channel (UI → engine).
pub fn param_channel() -> (Sender<ParamMessage>, Receiver<ParamMessage>) {
    bounded(64)
}
