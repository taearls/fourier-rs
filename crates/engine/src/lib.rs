//! fourier-engine: Orchestrator that wires audio-io, core DSP, and MIDI together.
//!
//! Owns the processing thread that pulls samples from the input ring buffer,
//! runs them through the overlap-add FFT pipeline with user-defined transforms,
//! and pushes results to the output ring buffer.
//!
//! Also routes MIDI events and exposes a high-level API for the UI layer.

pub mod error;
pub mod export;
pub mod params;
pub mod preset;
pub mod processor;
pub mod source;

pub use error::EngineError;
pub use export::{compute_total_frames, render_offline, RenderConfig};
pub use fourier_core::{
    AdditiveSynth, BandType, EqBand, NoiseGenerator, PitchShift, SpectralDelay, SpectralFreeze,
    WaveformType,
};
pub use params::{EngineParams, NoiseType, ParamMessage, Partial, SourceSpec, TransformSpec};
pub use preset::{factory_presets, Preset, PresetInfo};
pub use processor::{Engine, SpectralSnapshot, WaveformSnapshot};
pub use source::{AudioBufferSource, AudioSource};
