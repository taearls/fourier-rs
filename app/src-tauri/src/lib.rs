//! Tauri command handlers for the Fourier-RS desktop application.
//!
//! Exposes the `fourier-engine` API to the `SolidJS` frontend via Tauri IPC.

use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use fourier_audio_io::stream::StreamConfig;
use fourier_audio_io::{
    default_input_device, default_output_device, list_input_devices, list_output_devices,
};
use fourier_engine::{Engine, TransformSpec};

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

    let (engine, io) = Engine::new(sample_rate as f32, fft_size, hop_size);

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

    engine_state.engine.set_transform(spec)
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

    engine_state.engine.set_output_gain(gain)
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

    engine_state.engine.set_bypass(bypass)
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
// Public setup function for main.rs
// ---------------------------------------------------------------------------

/// Register all Tauri commands and managed state.
///
/// Called from `main.rs` to configure the Tauri application builder.
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_engine,
            stop_engine,
            set_transform,
            set_gain,
            set_bypass,
            get_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
