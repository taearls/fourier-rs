//! Tauri command handlers for the Fourier-RS desktop application.
//!
//! Exposes the `fourier-engine` API to the `SolidJS` frontend via Tauri IPC.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use fourier_audio_io::stream::StreamConfig;
use fourier_audio_io::{
    default_input_device, default_output_device, list_input_devices, list_output_devices,
};
use fourier_engine::{
    factory_presets, Engine, EngineError, NoiseType, Preset, PresetInfo, SourceSpec,
    SpectralSnapshot, TransformSpec, WaveformSnapshot, WaveformType,
};

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Holds a running engine and its associated audio streams.
///
/// The audio streams must be kept alive for the duration of the engine's
/// operation — dropping them stops the underlying cpal streams.
struct EngineState {
    engine: Engine,
    // Audio streams are held alive — dropping them stops audio I/O.
    // They are never accessed after creation, only kept alive.
    _input_stream: fourier_audio_io::AudioStream,
    _output_stream: fourier_audio_io::AudioStream,
}

// SAFETY: `EngineState` wraps `Engine` (which is `Send` — it only holds
// channel endpoints and a `JoinHandle`) and two `AudioStream` values.
// `AudioStream` contains `Option<cpal::Stream>` which is `!Send` due to
// a `PhantomData<*mut ()>` marker in cpal. However, the cpal `Stream` on
// macOS/CoreAudio is just an opaque handle to an AudioUnit that is safe
// to move between threads — the `!Send` marker is overly conservative.
// We only hold the streams alive (never call methods on them from other
// threads), and `Mutex` provides exclusive access for start/stop.
#[allow(unsafe_code, clippy::non_send_fields_in_send_ty)]
unsafe impl Send for EngineState {}

/// Tauri-managed state wrapper.
#[derive(Default)]
pub struct AppState {
    engine: Mutex<Option<EngineState>>,
}

// ---------------------------------------------------------------------------
// Types exposed to the frontend
// ---------------------------------------------------------------------------

/// Information about an available audio device, sent to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Start the audio engine with the given configuration.
///
/// Uses the system default input and output devices. The `sample_rate` is in
/// Hz and `fft_size` determines the spectral resolution.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn start_engine(
    state: State<'_, AppState>,
    sample_rate: u32,
    fft_size: usize,
) -> Result<(), String> {
    let mut guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    if guard.is_some() {
        return Err("Engine is already running".to_string());
    }

    if !fft_size.is_power_of_two() || fft_size < 4 {
        return Err(format!(
            "fft_size must be a power of 2 and at least 4, got {fft_size}"
        ));
    }

    // Default hop_size = fft_size / 4 (75% overlap, standard for OLA).
    let hop_size = fft_size / 4;

    let (engine, io) = Engine::new(sample_rate as f32, fft_size, hop_size)
        .map_err(|e| format!("Failed to start engine: {e}"))?;

    // Open audio streams using the default devices.
    let stream_config = StreamConfig {
        sample_rate,
        channels: 1,
        buffer_size: 512,
    };

    let input_device =
        default_input_device().ok_or_else(|| "No default input device found".to_string())?;
    let output_device =
        default_output_device().ok_or_else(|| "No default output device found".to_string())?;

    let input_stream =
        fourier_audio_io::AudioStream::open_input(&input_device, &stream_config, io.input_producer)
            .map_err(|e| format!("Failed to open input stream: {e}"))?;

    let output_stream = fourier_audio_io::AudioStream::open_output(
        &output_device,
        &stream_config,
        io.output_consumer,
    )
    .map_err(|e| format!("Failed to open output stream: {e}"))?;

    *guard = Some(EngineState {
        engine,
        _input_stream: input_stream,
        _output_stream: output_stream,
    });

    Ok(())
}

/// Stop the audio engine and release all audio resources.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn stop_engine(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .take()
        .ok_or_else(|| "Engine is not running".to_string())?;

    drop(guard);

    // Explicit shutdown joins the processing thread.
    engine_state.engine.shutdown();

    Ok(())
}

/// Helper: convert `EngineError` to a user-friendly string for Tauri IPC.
///
/// Takes ownership so it can be used directly as `.map_err(engine_err)`.
#[allow(clippy::needless_pass_by_value)]
fn engine_err(e: EngineError) -> String {
    e.to_string()
}

/// Set the spectral transform chain on the running engine.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_transform(state: State<'_, AppState>, spec: TransformSpec) -> Result<(), String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state.engine.set_transform(spec).map_err(engine_err)
}

/// Set the master output gain (linear, 0.0–1.0+).
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_gain(state: State<'_, AppState>, gain: f32) -> Result<(), String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state
        .engine
        .set_output_gain(gain)
        .map_err(engine_err)
}

/// Enable or disable bypass mode (pass audio through without processing).
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_bypass(state: State<'_, AppState>, bypass: bool) -> Result<(), String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state.engine.set_bypass(bypass).map_err(engine_err)
}

/// Get the latest spectral snapshot from the engine.
///
/// Drains all pending snapshots and returns only the most recent one,
/// preventing stale data from accumulating. Returns `null` when the engine
/// is not running or no snapshot is available yet.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn get_spectrum(state: State<'_, AppState>) -> Result<Option<SpectralSnapshot>, String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let Some(engine_state) = guard.as_ref() else {
        return Ok(None);
    };

    Ok(engine_state.engine.latest_snapshot())
}

/// Get the latest waveform snapshot from the engine.
///
/// Drains all pending waveform snapshots and returns only the most recent one.
/// Returns `null` when the engine is not running or no snapshot is available yet.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn get_waveform(state: State<'_, AppState>) -> Result<Option<WaveformSnapshot>, String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let Some(engine_state) = guard.as_ref() else {
        return Ok(None);
    };

    Ok(engine_state.engine.latest_waveform())
}

/// List available audio devices (both input and output).
#[tauri::command]
fn get_devices() -> Vec<DeviceInfo> {
    let to_info = |d: fourier_audio_io::AudioDevice| DeviceInfo {
        name: d.name,
        is_input: d.is_input,
        is_output: d.is_output,
    };

    let mut devices: Vec<DeviceInfo> = list_input_devices().into_iter().map(to_info).collect();
    devices.extend(list_output_devices().into_iter().map(to_info));
    devices
}

// ---------------------------------------------------------------------------
// Source selection commands
// ---------------------------------------------------------------------------

/// Parse a waveform name string into a `WaveformType` (case-insensitive).
fn parse_waveform(waveform: &str) -> Result<WaveformType, String> {
    if waveform.eq_ignore_ascii_case("sine") {
        Ok(WaveformType::Sine)
    } else if waveform.eq_ignore_ascii_case("square") {
        Ok(WaveformType::Square)
    } else if waveform.eq_ignore_ascii_case("sawtooth") {
        Ok(WaveformType::Sawtooth)
    } else if waveform.eq_ignore_ascii_case("triangle") {
        Ok(WaveformType::Triangle)
    } else {
        Err(format!(
            "Unknown waveform: \"{waveform}\". Expected one of: Sine, Square, Sawtooth, Triangle"
        ))
    }
}

/// Parse a noise type name string into a `NoiseType` (case-insensitive).
fn parse_noise_type(noise_type: &str) -> Result<NoiseType, String> {
    if noise_type.eq_ignore_ascii_case("white") {
        Ok(NoiseType::White)
    } else if noise_type.eq_ignore_ascii_case("pink") {
        Ok(NoiseType::Pink)
    } else {
        Err(format!(
            "Unknown noise type: \"{noise_type}\". Expected one of: White, Pink"
        ))
    }
}

/// Switch the engine audio source to live input (microphone / line-in).
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_source_live_input(state: State<'_, AppState>) -> Result<(), String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state
        .engine
        .set_source(SourceSpec::LiveInput)
        .map_err(engine_err)
}

/// Switch the engine audio source to a generated oscillator waveform.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_source_oscillator(
    state: State<'_, AppState>,
    waveform: String,
    frequency: f32,
) -> Result<(), String> {
    let waveform = parse_waveform(&waveform)?;

    if frequency <= 0.0 {
        return Err(format!("Frequency must be positive, got {frequency}"));
    }

    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state
        .engine
        .set_source(SourceSpec::Oscillator {
            waveform,
            frequency,
            amplitude: 1.0,
        })
        .map_err(engine_err)
}

/// Switch the engine audio source to a noise generator.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_source_noise(state: State<'_, AppState>, noise_type: String) -> Result<(), String> {
    let noise_type = parse_noise_type(&noise_type)?;

    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state
        .engine
        .set_source(SourceSpec::Noise {
            noise_type,
            amplitude: 1.0,
        })
        .map_err(engine_err)
}

/// Switch the engine audio source to a loaded WAV file.
///
/// Loads the WAV file at `path` via `fourier-file-io`, wraps it in an
/// `Arc<AudioBuffer>`, and sends it to the engine as the active source.
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn set_source_file(state: State<'_, AppState>, path: String, looping: bool) -> Result<(), String> {
    let buffer = fourier_file_io::load_wav(Path::new(&path))
        .map_err(|e| format!("Failed to load WAV file \"{path}\": {e}"))?;

    let buffer = Arc::new(buffer);

    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state
        .engine
        .set_source(SourceSpec::AudioBuffer {
            buffer: Some(buffer),
            looping,
        })
        .map_err(engine_err)
}

/// Seek to a normalized position in the current audio source.
///
/// `position` is normalized: 0.0 = start, 1.0 = end. This is only
/// meaningful for seekable sources (e.g. audio buffer playback).
#[tauri::command]
#[allow(clippy::needless_pass_by_value, clippy::significant_drop_tightening)]
fn seek_source(state: State<'_, AppState>, position: f32) -> Result<(), String> {
    let guard = state
        .engine
        .lock()
        .map_err(|e| format!("Lock poisoned: {e}"))?;

    let engine_state = guard
        .as_ref()
        .ok_or_else(|| "Engine is not running".to_string())?;

    engine_state.engine.seek(position).map_err(engine_err)
}

// ---------------------------------------------------------------------------
// Preset commands
// ---------------------------------------------------------------------------

/// Get the user presets directory, creating it if it doesn't exist.
fn presets_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_dir()
        .ok_or_else(|| "Could not determine user data directory".to_string())?
        .join("fourier-rs")
        .join("presets");

    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create presets directory: {e}"))?;
    }

    Ok(dir)
}

/// Sanitize a preset name into a safe filename stem.
fn preset_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Save the current engine configuration as a named preset.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn save_preset(
    name: String,
    source: SourceSpec,
    transform: TransformSpec,
    gain: f32,
) -> Result<(), String> {
    let stem = preset_filename(&name);
    if stem.is_empty() {
        return Err("Preset name cannot be empty".to_string());
    }

    // Prevent overwriting factory presets.
    let factory_names: Vec<String> = factory_presets().iter().map(|p| p.name.clone()).collect();
    if factory_names.iter().any(|n| n == &name) {
        return Err(format!("Cannot overwrite factory preset \"{name}\""));
    }

    let preset = Preset {
        name,
        source,
        transform,
        gain,
    };

    let dir = presets_dir()?;
    let path = dir.join(format!("{stem}.json"));
    let json = serde_json::to_string_pretty(&preset)
        .map_err(|e| format!("Failed to serialize preset: {e}"))?;

    std::fs::write(&path, json).map_err(|e| format!("Failed to write preset file: {e}"))?;

    Ok(())
}

/// Load a preset by name and return it.
///
/// Checks factory presets first, then user presets directory.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn load_preset(name: String) -> Result<Preset, String> {
    // Check factory presets first.
    if let Some(preset) = factory_presets().iter().find(|p| p.name == name) {
        return Ok(preset.clone());
    }

    // Load from user directory.
    let dir = presets_dir()?;
    let stem = preset_filename(&name);
    let path = dir.join(format!("{stem}.json"));

    if !path.exists() {
        return Err(format!("Preset \"{name}\" not found"));
    }

    let json =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read preset file: {e}"))?;

    serde_json::from_str(&json).map_err(|e| format!("Failed to parse preset file: {e}"))
}

/// List all available presets (factory + user).
#[tauri::command]
fn list_presets() -> Result<Vec<PresetInfo>, String> {
    let mut presets: Vec<PresetInfo> = factory_presets()
        .iter()
        .map(|p| PresetInfo {
            name: p.name.clone(),
            is_factory: true,
        })
        .collect();

    // Scan user presets directory.
    let dir = presets_dir()?;
    if dir.exists() {
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| format!("Failed to read presets directory: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match std::fs::read_to_string(&path) {
                    Ok(json) => match serde_json::from_str::<Preset>(&json) {
                        Ok(preset) => {
                            // Skip if a factory preset has the same name.
                            if !presets.iter().any(|p| p.name == preset.name) {
                                presets.push(PresetInfo {
                                    name: preset.name,
                                    is_factory: false,
                                });
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Skipping malformed preset file {}: {e}",
                                path.display()
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to read preset file {}: {e}", path.display());
                    }
                }
            }
        }
    }

    presets.sort_by(|a, b| {
        // Factory first, then alphabetical.
        b.is_factory.cmp(&a.is_factory).then(a.name.cmp(&b.name))
    });

    Ok(presets)
}

/// Delete a user preset by name. Factory presets cannot be deleted.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
fn delete_preset(name: String) -> Result<(), String> {
    let factory_names: Vec<String> = factory_presets().iter().map(|p| p.name.clone()).collect();
    if factory_names.iter().any(|n| n == &name) {
        return Err(format!("Cannot delete factory preset \"{name}\""));
    }

    let dir = presets_dir()?;
    let stem = preset_filename(&name);
    let path = dir.join(format!("{stem}.json"));

    if !path.exists() {
        return Err(format!("Preset \"{name}\" not found"));
    }

    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete preset file: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Public setup function for main.rs
// ---------------------------------------------------------------------------

/// Register all Tauri commands and managed state.
///
/// Called from `main.rs` to configure the Tauri application builder.
#[allow(clippy::expect_used)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_engine,
            stop_engine,
            set_transform,
            set_gain,
            set_bypass,
            get_spectrum,
            get_waveform,
            get_devices,
            set_source_live_input,
            set_source_oscillator,
            set_source_noise,
            set_source_file,
            seek_source,
            save_preset,
            load_preset,
            list_presets,
            delete_preset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
