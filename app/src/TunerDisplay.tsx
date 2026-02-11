/**
 * `<TunerDisplay />` – Peak frequency and musical note detection display.
 *
 * Features:
 * - Detects dominant pitch from spectral snapshot
 * - Maps frequency to nearest musical note (A4 = 440 Hz, equal temperament)
 * - Displays frequency in Hz, note name with octave, cents deviation
 * - Color coding: green (±5 cents), yellow (±5–20), red (>±20)
 * - Handles no-signal case gracefully (shows "--")
 */

import { createMemo, type Component } from "solid-js";
import type { SpectralSnapshot } from "./bindings";
import { getTunerReading, type TunerColor } from "./tuner-utils";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface TunerDisplayProps {
  /** Current spectral snapshot from the engine (null when not running). */
  snapshot: SpectralSnapshot | null;
}

/** Map tuner color names to CSS custom property values. */
const COLOR_MAP: Record<TunerColor, string> = {
  green: "#22c55e",
  yellow: "#eab308",
  red: "#ef4444",
};

const TunerDisplay: Component<TunerDisplayProps> = (props) => {
  const reading = createMemo(() => getTunerReading(props.snapshot));

  /** Resolved color hex for the current reading, or dim fallback. */
  const indicatorColor = () => {
    const r = reading();
    return r ? COLOR_MAP[r.color] : "rgba(255, 255, 255, 0.2)";
  };

  /** Format frequency for display (e.g. "440.0 Hz"). */
  const freqLabel = () => {
    const r = reading();
    if (!r) return "--";
    return `${r.frequency.toFixed(1)} Hz`;
  };

  /** Format cents deviation with sign (e.g. "+3.2", "-12.5"). */
  const centsLabel = () => {
    const r = reading();
    if (!r) return "--";
    const sign = r.cents >= 0 ? "+" : "";
    return `${sign}${r.cents.toFixed(1)}`;
  };

  /** Cents bar position: 0 at center, offset left/right by cents. */
  const centsBarOffset = () => {
    const r = reading();
    if (!r) return 0;
    // Clamp to ±50 and normalize to [-1, 1]
    return Math.max(-50, Math.min(50, r.cents)) / 50;
  };

  return (
    <div
      class="tuner-display"
      role="status"
      aria-label="Tuner display showing detected pitch and note"
    >
      <div class="tuner-note">
        {reading()?.noteLabel ?? "--"}
      </div>
      <div class="tuner-freq">
        {freqLabel()}
      </div>
      <div class="tuner-cents-bar">
        <div class="tuner-cents-track">
          <div class="tuner-cents-center" />
          <div
            class="tuner-cents-indicator"
            style={{
              left: `${50 + centsBarOffset() * 50}%`,
              "background-color": indicatorColor(),
            }}
          />
        </div>
        <div class="tuner-cents-labels">
          <span>-50</span>
          <span
            class="tuner-cents-value"
            style={{ color: indicatorColor() }}
          >
            {centsLabel()} ct
          </span>
          <span>+50</span>
        </div>
      </div>
    </div>
  );
};

export default TunerDisplay;
