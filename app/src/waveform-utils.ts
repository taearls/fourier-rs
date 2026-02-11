/**
 * Utility functions for the waveform oscilloscope display.
 */

/**
 * Find the first rising zero-crossing in the samples array.
 *
 * A rising zero-crossing is where `samples[i] <= 0` and `samples[i+1] > 0`.
 * Returns the index of the crossing point, or 0 if none found.
 */
export function findRisingZeroCrossing(samples: number[]): number {
  for (let i = 0; i < samples.length - 1; i++) {
    if (samples[i] <= 0 && samples[i + 1] > 0) {
      return i;
    }
  }
  return 0;
}

/**
 * Build WebGL vertices for the waveform display.
 *
 * Takes a window of samples and maps them to clip space [-1, 1]:
 * - X axis: time (left to right)
 * - Y axis: amplitude (-1.0 to 1.0 maps to clip space)
 *
 * @param samples - Time-domain audio samples
 * @param windowSize - Number of samples to display
 * @param triggerOffset - Sample index offset from zero-crossing trigger
 */
export function buildWaveformVertices(
  samples: number[],
  windowSize: number,
  triggerOffset: number,
): Float32Array {
  const displayCount = Math.min(windowSize, samples.length - triggerOffset);
  if (displayCount <= 0) return new Float32Array(0);

  const vertices = new Float32Array(displayCount * 2);

  for (let i = 0; i < displayCount; i++) {
    const x = (i / (displayCount - 1)) * 2 - 1; // [0, 1] -> [-1, 1]
    const y = Math.max(-1, Math.min(1, samples[triggerOffset + i])); // clamp amplitude
    vertices[i * 2] = x;
    vertices[i * 2 + 1] = y;
  }

  return vertices;
}

/** Time-axis tick marks for the overlay (ms). */
export const TIME_TICKS_MS = [0, 5, 10, 15, 20, 25, 30, 35, 40, 45];

/** Amplitude-axis tick marks. */
export const AMP_TICKS = [-1.0, -0.5, 0, 0.5, 1.0];

/** Format a time value in ms for display. */
export function formatTimeMs(ms: number): string {
  if (ms === 0) return "0";
  return `${ms}`;
}
