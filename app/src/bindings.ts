/**
 * TypeScript type bindings for Fourier-RS Tauri commands.
 *
 * These types mirror the Rust types exposed via Tauri IPC and should be
 * kept in sync with `app/src-tauri/src/lib.rs` and `crates/engine/src/params.rs`.
 */

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// Device types
// ---------------------------------------------------------------------------

/** Information about an available audio device. */
export interface DeviceInfo {
  name: string;
  is_input: boolean;
  is_output: boolean;
}

// ---------------------------------------------------------------------------
// Transform types (mirrors `TransformSpec` in fourier-engine)
// ---------------------------------------------------------------------------

/** Serde-tagged union: `{ type: "Identity" }`, `{ type: "LowPass", value: { cutoff_hz: 1000 } }`, etc. */
export type TransformSpec =
  | { type: "Identity" }
  | { type: "LowPass"; value: { cutoff_hz: number } }
  | { type: "HighPass"; value: { cutoff_hz: number } }
  | { type: "BandPass"; value: { low_hz: number; high_hz: number } }
  | { type: "Gain"; value: { factor: number } }
  | { type: "Chain"; value: TransformSpec[] };

// ---------------------------------------------------------------------------
// Source types (mirrors `SourceSpec` in fourier-engine)
// ---------------------------------------------------------------------------

export type WaveformType = "Sine" | "Square" | "Sawtooth" | "Triangle";

export type NoiseType = "White" | "Pink";

export interface Partial {
  frequency: number;
  amplitude: number;
  phase: number;
}

/** Serde internally-tagged union: `{ type: "LiveInput" }`, `{ type: "Oscillator", waveform: "Sine", ... }`, etc. */
export type SourceSpec =
  | { type: "LiveInput" }
  | {
      type: "Oscillator";
      waveform: WaveformType;
      frequency: number;
      amplitude: number;
    }
  | { type: "Noise"; noise_type: NoiseType; amplitude: number }
  | { type: "Additive"; partials: Partial[] };

// ---------------------------------------------------------------------------
// Command wrappers
// ---------------------------------------------------------------------------

/**
 * Start the audio engine with the given configuration.
 *
 * @param sampleRate - Sample rate in Hz (e.g. 44100)
 * @param fftSize - FFT size for spectral processing (e.g. 2048)
 */
export function startEngine(
  sampleRate: number,
  fftSize: number,
): Promise<void> {
  return invoke("start_engine", {
    sampleRate,
    fftSize,
  });
}

/** Stop the audio engine and release all audio resources. */
export function stopEngine(): Promise<void> {
  return invoke("stop_engine");
}

/**
 * Set the spectral transform chain on the running engine.
 *
 * @param spec - The transform specification to apply.
 */
export function setTransform(spec: TransformSpec): Promise<void> {
  return invoke("set_transform", { spec });
}

/**
 * Set the master output gain.
 *
 * @param gain - Linear gain value (0.0 = silence, 1.0 = unity).
 */
export function setGain(gain: number): Promise<void> {
  return invoke("set_gain", { gain });
}

/**
 * Enable or disable bypass mode.
 *
 * @param bypass - `true` to pass audio through without processing.
 */
export function setBypass(bypass: boolean): Promise<void> {
  return invoke("set_bypass", { bypass });
}

/** List available audio devices (both input and output). */
export function getDevices(): Promise<DeviceInfo[]> {
  return invoke("get_devices");
}
