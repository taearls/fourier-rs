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

/** EQ band filter shape. */
export type BandType = "Peak" | "LowShelf" | "HighShelf";

/** A single parametric EQ band. */
export interface EqBand {
  frequency: number;
  gain_db: number;
  q: number;
  band_type: BandType;
}

/** Serde-tagged union: `{ type: "Identity" }`, `{ type: "LowPass", value: { cutoff_hz: 1000 } }`, etc. */
export type TransformSpec =
  | { type: "Identity" }
  | { type: "LowPass"; value: { cutoff_hz: number } }
  | { type: "HighPass"; value: { cutoff_hz: number } }
  | { type: "BandPass"; value: { low_hz: number; high_hz: number } }
  | { type: "Gain"; value: { factor: number } }
  | { type: "ParametricEq"; value: { bands: EqBand[] } }
  | { type: "PitchShift"; value: { semitones: number } }
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
  | { type: "Additive"; partials: Partial[] }
  | { type: "AudioBuffer"; looping: boolean };

// ---------------------------------------------------------------------------
// Spectral types (mirrors `SpectralSnapshot` / `SpectralPeak` in fourier-engine)
// ---------------------------------------------------------------------------

/** A detected peak in the frequency spectrum. */
export interface SpectralPeak {
  bin_index: number;
  frequency_hz: number;
  magnitude: number;
  magnitude_db: number;
}

/** A point-in-time snapshot of the frequency spectrum from the engine. */
export interface SpectralSnapshot {
  magnitude_db: number[];
  peaks: SpectralPeak[];
  sample_rate: number;
  fft_size: number;
  timestamp_ms: number;
}

/** A point-in-time snapshot of time-domain audio samples from the engine. */
export interface WaveformSnapshot {
  samples: number[];
  sample_rate: number;
  fft_size: number;
  timestamp_ms: number;
}

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

/**
 * Get the latest spectral snapshot from the engine.
 *
 * Returns `null` when the engine is not running or no snapshot is available.
 * Drains all pending snapshots and returns only the most recent one.
 * Call this from a `requestAnimationFrame` loop for 60fps polling.
 */
export function getSpectrum(): Promise<SpectralSnapshot | null> {
  return invoke("get_spectrum");
}

/**
 * Get the latest waveform snapshot from the engine.
 *
 * Returns `null` when the engine is not running or no snapshot is available.
 * Drains all pending snapshots and returns only the most recent one.
 * Call this from a `requestAnimationFrame` loop for 60fps polling.
 */
export function getWaveform(): Promise<WaveformSnapshot | null> {
  return invoke("get_waveform");
}

/** List available audio devices (both input and output). */
export function getDevices(): Promise<DeviceInfo[]> {
  return invoke("get_devices");
}

// ---------------------------------------------------------------------------
// Source selection commands
// ---------------------------------------------------------------------------

/** Switch the engine audio source to live input (microphone / line-in). */
export function setSourceLiveInput(): Promise<void> {
  return invoke("set_source_live_input");
}

/**
 * Switch the engine audio source to a generated oscillator waveform.
 *
 * @param waveform - Waveform type: "Sine", "Square", "Sawtooth", or "Triangle".
 * @param frequency - Frequency in Hz (must be positive).
 */
export function setSourceOscillator(
  waveform: WaveformType,
  frequency: number,
): Promise<void> {
  return invoke("set_source_oscillator", { waveform, frequency });
}

/**
 * Switch the engine audio source to a noise generator.
 *
 * @param noiseType - Noise type: "White" or "Pink".
 */
export function setSourceNoise(noiseType: NoiseType): Promise<void> {
  return invoke("set_source_noise", { noiseType });
}

/**
 * Switch the engine audio source to a loaded WAV file.
 *
 * @param path - Absolute path to a WAV file on disk.
 * @param looping - Whether to loop playback when reaching the end.
 */
export function setSourceFile(
  path: string,
  looping: boolean,
): Promise<void> {
  return invoke("set_source_file", { path, looping });
}

/**
 * Seek to a normalized position in the current audio source.
 *
 * Only meaningful for seekable sources (e.g. audio buffer playback).
 *
 * @param position - Normalized position (0.0 = start, 1.0 = end).
 */
export function seekSource(position: number): Promise<void> {
  return invoke("seek_source", { position });
}
