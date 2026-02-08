/**
 * Utility functions for spectrum analyzer frequency/magnitude mapping.
 */

/** Minimum displayed frequency in Hz. */
export const FREQ_MIN = 20;
/** Maximum displayed frequency in Hz. */
export const FREQ_MAX = 20_000;
/** Minimum displayed magnitude in dB. */
export const DB_MIN = -90;
/** Maximum displayed magnitude in dB. */
export const DB_MAX = 0;

/**
 * Map a frequency (Hz) to a normalized position [0, 1] on a logarithmic scale.
 * 20Hz -> 0, 20kHz -> 1.
 */
export function freqToNorm(freq: number): number {
  if (freq <= FREQ_MIN) return 0;
  if (freq >= FREQ_MAX) return 1;
  return (Math.log10(freq) - Math.log10(FREQ_MIN)) /
    (Math.log10(FREQ_MAX) - Math.log10(FREQ_MIN));
}

/**
 * Map a normalized position [0, 1] back to frequency (Hz) on a logarithmic scale.
 */
export function normToFreq(norm: number): number {
  const logMin = Math.log10(FREQ_MIN);
  const logMax = Math.log10(FREQ_MAX);
  return Math.pow(10, logMin + norm * (logMax - logMin));
}

/**
 * Map a dB magnitude to a normalized position [0, 1].
 * -90dB -> 0, 0dB -> 1.
 */
export function dbToNorm(db: number): number {
  return Math.max(0, Math.min(1, (db - DB_MIN) / (DB_MAX - DB_MIN)));
}

/**
 * Convert FFT bin index to frequency in Hz.
 */
export function binToFreq(binIndex: number, sampleRate: number, fftSize: number): number {
  return binIndex * sampleRate / fftSize;
}

/**
 * Build an array of log-spaced vertex positions from linear FFT magnitude_db data.
 * Returns Float32Array of [x, y] pairs in clip space [-1, 1].
 *
 * @param magnitudeDb - dB magnitude per FFT bin
 * @param sampleRate - Engine sample rate
 * @param fftSize - FFT size (number of bins = fftSize / 2 + 1)
 */
export function buildSpectrumVertices(
  magnitudeDb: number[],
  sampleRate: number,
  fftSize: number,
): Float32Array {
  const numBins = magnitudeDb.length;
  // 2 floats per vertex (x, y)
  const vertices = new Float32Array(numBins * 2);

  for (let i = 0; i < numBins; i++) {
    const freq = binToFreq(i, sampleRate, fftSize);
    const x = freqToNorm(freq) * 2 - 1; // map [0,1] to [-1,1] clip space
    const y = dbToNorm(magnitudeDb[i]) * 2 - 1; // map [0,1] to [-1,1] clip space
    vertices[i * 2] = x;
    vertices[i * 2 + 1] = y;
  }

  return vertices;
}

/**
 * Build vertices for the filled area under the spectrum curve.
 * Returns Float32Array of triangle strip vertices [x, y] where each
 * spectrum point has a top vertex and a bottom vertex at y = -1.
 */
export function buildFilledVertices(
  magnitudeDb: number[],
  sampleRate: number,
  fftSize: number,
): Float32Array {
  const numBins = magnitudeDb.length;
  // Triangle strip: 2 vertices per bin (top + bottom), 2 floats each
  const vertices = new Float32Array(numBins * 4);

  for (let i = 0; i < numBins; i++) {
    const freq = binToFreq(i, sampleRate, fftSize);
    const x = freqToNorm(freq) * 2 - 1;
    const y = dbToNorm(magnitudeDb[i]) * 2 - 1;
    // Bottom vertex
    vertices[i * 4] = x;
    vertices[i * 4 + 1] = -1;
    // Top vertex
    vertices[i * 4 + 2] = x;
    vertices[i * 4 + 3] = y;
  }

  return vertices;
}

/**
 * Build vertices for bar-style rendering.
 * Each bin becomes a vertical quad (2 triangles = 6 vertices).
 */
export function buildBarVertices(
  magnitudeDb: number[],
  sampleRate: number,
  fftSize: number,
): Float32Array {
  const numBins = magnitudeDb.length;
  // Each bar = 6 vertices * 2 floats
  const vertices = new Float32Array(numBins * 12);
  const barWidthClip = 2.0 / numBins * 0.8; // 80% fill

  let idx = 0;
  for (let i = 0; i < numBins; i++) {
    const freq = binToFreq(i, sampleRate, fftSize);
    const xCenter = freqToNorm(freq) * 2 - 1;
    const y = dbToNorm(magnitudeDb[i]) * 2 - 1;
    const halfW = barWidthClip / 2;

    const left = xCenter - halfW;
    const right = xCenter + halfW;
    const bottom = -1;

    // Triangle 1
    vertices[idx++] = left;  vertices[idx++] = bottom;
    vertices[idx++] = right; vertices[idx++] = bottom;
    vertices[idx++] = right; vertices[idx++] = y;
    // Triangle 2
    vertices[idx++] = left;  vertices[idx++] = bottom;
    vertices[idx++] = right; vertices[idx++] = y;
    vertices[idx++] = left;  vertices[idx++] = y;
  }

  return vertices;
}

/** Frequency tick marks for the log axis. */
export const FREQ_TICKS = [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000];

/** dB tick marks for the magnitude axis. */
export const DB_TICKS = [-90, -80, -70, -60, -50, -40, -30, -20, -10, 0];

/** Format a frequency for display. */
export function formatFreq(hz: number): string {
  if (hz >= 1000) return `${hz / 1000}k`;
  return `${hz}`;
}
