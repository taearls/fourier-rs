import { describe, expect, it } from "vitest";
import type { SpectralSnapshot } from "./bindings";
import { getTunerReading } from "./tuner-utils";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a minimal snapshot with a single peak at the given frequency and dB. */
function snap(
  frequencyHz: number,
  magnitudeDb: number = -20,
): SpectralSnapshot {
  return {
    magnitude_db: [],
    peaks: [
      {
        bin_index: 0,
        frequency_hz: frequencyHz,
        magnitude: 0,
        magnitude_db: magnitudeDb,
      },
    ],
    sample_rate: 44100,
    fft_size: 2048,
    timestamp_ms: 0,
  };
}

// ---------------------------------------------------------------------------
// Null / no-signal cases
// ---------------------------------------------------------------------------

describe("getTunerReading — null/no-signal", () => {
  it("returns null for null snapshot", () => {
    expect(getTunerReading(null)).toBeNull();
  });

  it("returns null for snapshot with no peaks", () => {
    const s: SpectralSnapshot = {
      magnitude_db: [],
      peaks: [],
      sample_rate: 44100,
      fft_size: 2048,
      timestamp_ms: 0,
    };
    expect(getTunerReading(s)).toBeNull();
  });

  it("returns null when all peaks are below signal threshold", () => {
    expect(getTunerReading(snap(440, -70))).toBeNull();
  });

  it("returns null when peak is below minimum frequency", () => {
    expect(getTunerReading(snap(10))).toBeNull();
  });

  it("returns null when peak is above maximum frequency", () => {
    expect(getTunerReading(snap(15000))).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Note detection — exact frequencies
// ---------------------------------------------------------------------------

describe("getTunerReading — exact note frequencies", () => {
  it("detects A4 = 440 Hz with ~0 cents", () => {
    const r = getTunerReading(snap(440));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("A");
    expect(r!.octave).toBe(4);
    expect(r!.noteLabel).toBe("A4");
    expect(Math.abs(r!.cents)).toBeLessThan(0.1);
    expect(r!.color).toBe("green");
  });

  it("detects C4 (middle C) ≈ 261.63 Hz", () => {
    const r = getTunerReading(snap(261.63));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("C");
    expect(r!.octave).toBe(4);
    expect(r!.noteLabel).toBe("C4");
    expect(Math.abs(r!.cents)).toBeLessThan(1);
  });

  it("detects E2 ≈ 82.41 Hz", () => {
    const r = getTunerReading(snap(82.41));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("E");
    expect(r!.octave).toBe(2);
    expect(r!.noteLabel).toBe("E2");
  });

  it("detects C8 ≈ 4186 Hz (high piano note)", () => {
    const r = getTunerReading(snap(4186));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("C");
    expect(r!.octave).toBe(8);
  });

  it("detects B7 ≈ 3951 Hz (just below C8 boundary)", () => {
    const r = getTunerReading(snap(3951.07));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("B");
    expect(r!.octave).toBe(7);
  });
});

// ---------------------------------------------------------------------------
// Cents deviation and color coding
// ---------------------------------------------------------------------------

describe("getTunerReading — cents deviation & color", () => {
  it("reports green for ≤5 cents deviation", () => {
    // A4 + ~3 cents ≈ 440.76 Hz
    const r = getTunerReading(snap(440.76));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("A");
    expect(Math.abs(r!.cents)).toBeLessThanOrEqual(5);
    expect(r!.color).toBe("green");
  });

  it("reports yellow for 5–20 cents deviation", () => {
    // A4 + ~15 cents ≈ 443.8 Hz
    const r = getTunerReading(snap(443.8));
    expect(r).not.toBeNull();
    expect(r!.noteName).toBe("A");
    expect(Math.abs(r!.cents)).toBeGreaterThan(5);
    expect(Math.abs(r!.cents)).toBeLessThanOrEqual(20);
    expect(r!.color).toBe("yellow");
  });

  it("reports red for >20 cents deviation", () => {
    // A4 + ~30 cents ≈ 447.8 Hz
    const r = getTunerReading(snap(447.8));
    expect(r).not.toBeNull();
    expect(Math.abs(r!.cents)).toBeGreaterThan(20);
    expect(r!.color).toBe("red");
  });

  it("reports negative cents when flat", () => {
    // A4 - ~15 cents ≈ 436.2 Hz
    const r = getTunerReading(snap(436.2));
    expect(r).not.toBeNull();
    expect(r!.cents).toBeLessThan(0);
    expect(r!.color).toBe("yellow");
  });

  it("cents deviation is always in [-50, +50) range", () => {
    // Test a range of frequencies across several octaves
    for (const freq of [55, 110, 220, 440, 880, 1760, 3520]) {
      // Offset by varying amounts
      for (const offset of [0.97, 0.99, 1.0, 1.01, 1.03]) {
        const r = getTunerReading(snap(freq * offset));
        if (r) {
          expect(r.cents).toBeGreaterThanOrEqual(-50);
          expect(r.cents).toBeLessThan(50);
        }
      }
    }
  });
});

// ---------------------------------------------------------------------------
// Peak selection
// ---------------------------------------------------------------------------

describe("getTunerReading — peak selection", () => {
  it("uses the first valid peak (highest magnitude, pre-sorted by engine)", () => {
    const s: SpectralSnapshot = {
      magnitude_db: [],
      peaks: [
        // First peak: high frequency, out of range
        {
          bin_index: 500,
          frequency_hz: 15000,
          magnitude: 1,
          magnitude_db: -10,
        },
        // Second peak: valid
        {
          bin_index: 20,
          frequency_hz: 440,
          magnitude: 0.5,
          magnitude_db: -20,
        },
      ],
      sample_rate: 44100,
      fft_size: 2048,
      timestamp_ms: 0,
    };
    const r = getTunerReading(s);
    expect(r).not.toBeNull();
    expect(r!.noteLabel).toBe("A4");
  });

  it("skips peaks below signal threshold and uses next valid", () => {
    const s: SpectralSnapshot = {
      magnitude_db: [],
      peaks: [
        // First peak: too quiet
        {
          bin_index: 10,
          frequency_hz: 440,
          magnitude: 0.001,
          magnitude_db: -65,
        },
        // Second peak: valid
        {
          bin_index: 20,
          frequency_hz: 880,
          magnitude: 0.1,
          magnitude_db: -30,
        },
      ],
      sample_rate: 44100,
      fft_size: 2048,
      timestamp_ms: 0,
    };
    const r = getTunerReading(s);
    expect(r).not.toBeNull();
    expect(r!.noteLabel).toBe("A5");
  });
});

// ---------------------------------------------------------------------------
// Chromatic scale coverage
// ---------------------------------------------------------------------------

describe("getTunerReading — all 12 chromatic notes", () => {
  // A4 = 440, each semitone is 440 * 2^(n/12) where n is offset from A
  const expected: [number, string][] = [
    [261.63, "C4"],
    [277.18, "C#4"],
    [293.66, "D4"],
    [311.13, "D#4"],
    [329.63, "E4"],
    [349.23, "F4"],
    [369.99, "F#4"],
    [392.0, "G4"],
    [415.3, "G#4"],
    [440.0, "A4"],
    [466.16, "A#4"],
    [493.88, "B4"],
  ];

  for (const [freq, label] of expected) {
    it(`detects ${label} at ${freq} Hz`, () => {
      const r = getTunerReading(snap(freq));
      expect(r).not.toBeNull();
      expect(r!.noteLabel).toBe(label);
      expect(Math.abs(r!.cents)).toBeLessThan(2);
    });
  }
});
