/**
 * Pitch detection and musical note mapping utilities for the tuner display.
 *
 * Uses A4 = 440 Hz equal temperament tuning with 12 semitones per octave.
 */

import type { SpectralSnapshot } from "./bindings";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Concert pitch reference: A4 = 440 Hz. */
const A4_HZ = 440;

/** MIDI note number for A4. */
const A4_MIDI = 69;

/** Note names in chromatic order (sharps). */
const NOTE_NAMES = [
  "C",
  "C#",
  "D",
  "D#",
  "E",
  "F",
  "F#",
  "G",
  "G#",
  "A",
  "A#",
  "B",
] as const;

/** Minimum frequency we consider valid for pitch detection (Hz). */
const MIN_FREQ_HZ = 20;

/** Maximum frequency we consider valid for pitch detection (Hz). */
const MAX_FREQ_HZ = 10_000;

/** Minimum magnitude in dB to consider a peak as a real signal. */
const SIGNAL_THRESHOLD_DB = -60;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Result of pitch detection from a spectral snapshot. */
export interface TunerReading {
  /** Detected frequency in Hz. */
  frequency: number;
  /** Musical note name (e.g. "A", "C#"). */
  noteName: string;
  /** Octave number (e.g. 4 for A4). */
  octave: number;
  /** Full note label (e.g. "A4", "C#5"). */
  noteLabel: string;
  /** Cents deviation from the nearest note (-50 to +50). */
  cents: number;
  /** Color for the cents indicator: green, yellow, or red. */
  color: TunerColor;
}

/** Color coding for tuning accuracy. */
export type TunerColor = "green" | "yellow" | "red";

// ---------------------------------------------------------------------------
// Pitch / Note Mapping
// ---------------------------------------------------------------------------

/**
 * Convert a frequency in Hz to the nearest MIDI note number (fractional).
 *
 * Uses the formula: midi = 69 + 12 * log2(freq / 440).
 */
function freqToMidi(freq: number): number {
  return A4_MIDI + 12 * Math.log2(freq / A4_HZ);
}

/**
 * Get the note name for a given MIDI note number.
 */
function midiToNoteName(midi: number): string {
  const index = ((midi % 12) + 12) % 12;
  return NOTE_NAMES[index];
}

/**
 * Get the octave for a given MIDI note number.
 *
 * MIDI note 0 = C-1, so octave = floor(midi / 12) - 1.
 */
function midiToOctave(midi: number): number {
  return Math.floor(midi / 12) - 1;
}

/**
 * Calculate cents deviation from the nearest semitone.
 *
 * Returns a value in the range [-50, +50).
 */
function centsDeviation(fractionalMidi: number): number {
  const nearest = Math.round(fractionalMidi);
  return (fractionalMidi - nearest) * 100;
}

/**
 * Map a cents value to a color based on tuning accuracy.
 *
 * - Green: within ±5 cents (in tune)
 * - Yellow: ±5 to ±20 cents (close)
 * - Red: beyond ±20 cents (out of tune)
 */
function centsToColor(cents: number): TunerColor {
  const absCents = Math.abs(cents);
  if (absCents <= 5) return "green";
  if (absCents <= 20) return "yellow";
  return "red";
}

// ---------------------------------------------------------------------------
// Peak Frequency Extraction
// ---------------------------------------------------------------------------

/**
 * Extract a tuner reading from a spectral snapshot.
 *
 * Uses the highest-magnitude peak that falls within the valid frequency range
 * and exceeds the signal threshold. Returns `null` if no valid peak is found.
 */
export function getTunerReading(
  snapshot: SpectralSnapshot | null,
): TunerReading | null {
  if (!snapshot || snapshot.peaks.length === 0) return null;

  // Find the strongest peak within the valid frequency range
  const validPeak = snapshot.peaks.find(
    (p) =>
      p.frequency_hz >= MIN_FREQ_HZ &&
      p.frequency_hz <= MAX_FREQ_HZ &&
      p.magnitude_db >= SIGNAL_THRESHOLD_DB,
  );

  if (!validPeak) return null;

  const freq = validPeak.frequency_hz;
  const fractionalMidi = freqToMidi(freq);
  const nearestMidi = Math.round(fractionalMidi);
  const noteName = midiToNoteName(nearestMidi);
  const octave = midiToOctave(nearestMidi);
  const cents = centsDeviation(fractionalMidi);
  const color = centsToColor(cents);

  return {
    frequency: freq,
    noteName,
    octave,
    noteLabel: `${noteName}${octave}`,
    cents,
    color,
  };
}
