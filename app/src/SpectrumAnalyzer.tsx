/**
 * `<SpectrumAnalyzer />` – WebGL-based real-time spectrum visualization.
 *
 * Features:
 * - Log frequency axis (20 Hz – 20 kHz) with labeled ticks
 * - dB magnitude axis (-90 dB to 0 dB) with labeled ticks
 * - Configurable render modes: line, filled, bars
 * - Peak markers with hold and decay
 * - Responsive to container/window resize
 * - 60 fps via requestAnimationFrame
 * - Graceful fallback when WebGL is unavailable
 */

import { createSignal, onCleanup, onMount, type Component } from "solid-js";
import type { SpectralSnapshot } from "./bindings";
import {
  buildSpectrumVertices,
  buildFilledVertices,
  buildBarVertices,
  freqToNorm,
  dbToNorm,
  binToFreq,
  FREQ_TICKS,
  DB_TICKS,
  formatFreq,
} from "./spectrum-utils";
import { WebGLRenderer, type RenderMode } from "./webgl-renderer";

// ---------------------------------------------------------------------------
// Peak hold / decay state
// ---------------------------------------------------------------------------

/** Per-bin peak magnitude in dB with hold time and decay. */
interface PeakState {
  /** Held peak dB values per bin. */
  values: Float32Array;
  /** Timestamp (ms) when each bin's peak was last set. */
  timestamps: Float64Array;
}

const PEAK_HOLD_MS = 1000;
const PEAK_DECAY_DB_PER_SEC = 40;

function updatePeaks(
  peaks: PeakState,
  magnitudeDb: number[],
  now: number,
): void {
  const numBins = magnitudeDb.length;

  // Resize if needed (first frame or FFT size changed)
  if (peaks.values.length !== numBins) {
    peaks.values = new Float32Array(numBins).fill(-Infinity);
    peaks.timestamps = new Float64Array(numBins);
  }

  for (let i = 0; i < numBins; i++) {
    const current = magnitudeDb[i];
    if (current >= peaks.values[i]) {
      // New peak
      peaks.values[i] = current;
      peaks.timestamps[i] = now;
    } else {
      const elapsed = now - peaks.timestamps[i];
      if (elapsed > PEAK_HOLD_MS) {
        // Decay
        const decayAmount = PEAK_DECAY_DB_PER_SEC * (elapsed - PEAK_HOLD_MS) / 1000;
        peaks.values[i] = Math.max(peaks.values[i] - decayAmount, current);
      }
    }
  }
}

function buildPeakVertices(
  peaks: PeakState,
  sampleRate: number,
  fftSize: number,
): Float32Array {
  const numBins = peaks.values.length;
  const vertices = new Float32Array(numBins * 2);

  for (let i = 0; i < numBins; i++) {
    const freq = binToFreq(i, sampleRate, fftSize);
    vertices[i * 2] = freqToNorm(freq) * 2 - 1;
    vertices[i * 2 + 1] = dbToNorm(peaks.values[i]) * 2 - 1;
  }

  return vertices;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface SpectrumAnalyzerProps {
  /** Current spectral snapshot from the engine (null when not running). */
  snapshot: SpectralSnapshot | null;
  /** Render mode. Default: "filled". */
  mode?: RenderMode;
}

const SpectrumAnalyzer: Component<SpectrumAnalyzerProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let overlayCanvasRef: HTMLCanvasElement | undefined;
  let containerRef: HTMLDivElement | undefined;
  let renderer: WebGLRenderer | null = null;
  let animFrameId = 0;

  const [webglAvailable, setWebglAvailable] = createSignal(true);

  // Peak state persists across frames (mutated in-place, not reactive)
  const peakState: PeakState = {
    values: new Float32Array(0),
    timestamps: new Float64Array(0),
  };

  // --- Overlay (2D Canvas) for axis labels ---
  function drawOverlay(): void {
    if (!overlayCanvasRef) return;
    const ctx = overlayCanvasRef.getContext("2d");
    if (!ctx) return;

    const w = overlayCanvasRef.width;
    const h = overlayCanvasRef.height;

    // Margins for axis labels
    const marginLeft = 45;
    const marginBottom = 28;
    const marginTop = 10;
    const marginRight = 10;
    const plotW = w - marginLeft - marginRight;
    const plotH = h - marginTop - marginBottom;

    ctx.clearRect(0, 0, w, h);

    // Style
    ctx.font = "11px -apple-system, BlinkMacSystemFont, sans-serif";
    ctx.textBaseline = "middle";

    // --- Frequency axis (bottom) ---
    ctx.textAlign = "center";
    for (const freq of FREQ_TICKS) {
      const norm = freqToNorm(freq);
      const x = marginLeft + norm * plotW;

      // Grid line
      ctx.strokeStyle = "rgba(255, 255, 255, 0.06)";
      ctx.beginPath();
      ctx.moveTo(x, marginTop);
      ctx.lineTo(x, marginTop + plotH);
      ctx.stroke();

      // Label
      ctx.fillStyle = "rgba(255, 255, 255, 0.5)";
      ctx.fillText(formatFreq(freq), x, marginTop + plotH + 14);
    }

    // --- dB axis (left) ---
    ctx.textAlign = "right";
    for (const db of DB_TICKS) {
      const norm = dbToNorm(db);
      const y = marginTop + plotH - norm * plotH;

      // Grid line
      ctx.strokeStyle = "rgba(255, 255, 255, 0.06)";
      ctx.beginPath();
      ctx.moveTo(marginLeft, y);
      ctx.lineTo(marginLeft + plotW, y);
      ctx.stroke();

      // Label
      ctx.fillStyle = "rgba(255, 255, 255, 0.5)";
      ctx.fillText(`${db}`, marginLeft - 6, y);
    }

    // Axis unit labels
    ctx.fillStyle = "rgba(255, 255, 255, 0.35)";
    ctx.textAlign = "center";
    ctx.fillText("Hz", marginLeft + plotW / 2, h - 2);
    ctx.save();
    ctx.translate(10, marginTop + plotH / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText("dB", 0, 0);
    ctx.restore();
  }

  // --- Resize handler ---
  function handleResize(): void {
    if (!containerRef || !canvasRef || !overlayCanvasRef) return;

    const rect = containerRef.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(rect.width * dpr);
    const h = Math.round(rect.height * dpr);

    // Resize WebGL canvas
    canvasRef.width = w;
    canvasRef.height = h;
    renderer?.resize(w, h);

    // Resize overlay canvas
    overlayCanvasRef.width = w;
    overlayCanvasRef.height = h;

    drawOverlay();
  }

  // --- Main render loop ---
  function renderFrame(): void {
    if (!renderer || !canvasRef) {
      animFrameId = requestAnimationFrame(renderFrame);
      return;
    }

    const snapshot = props.snapshot;
    const mode = props.mode ?? "filled";

    renderer.clear();

    if (snapshot && snapshot.magnitude_db.length > 0) {
      const { magnitude_db, sample_rate, fft_size } = snapshot;

      switch (mode) {
        case "line": {
          const verts = buildSpectrumVertices(magnitude_db, sample_rate, fft_size);
          renderer.drawSpectrum(verts, "line");
          break;
        }
        case "filled": {
          const fillVerts = buildFilledVertices(magnitude_db, sample_rate, fft_size);
          renderer.drawSpectrum(fillVerts, "filled");
          // Draw line on top
          const lineVerts = buildSpectrumVertices(magnitude_db, sample_rate, fft_size);
          renderer.drawSpectrumLine(lineVerts);
          break;
        }
        case "bars": {
          const barVerts = buildBarVertices(magnitude_db, sample_rate, fft_size);
          renderer.drawSpectrum(barVerts, "bars");
          break;
        }
      }

      // Peak hold + decay
      const now = performance.now();
      updatePeaks(peakState, magnitude_db, now);
      const peakVerts = buildPeakVertices(peakState, sample_rate, fft_size);
      renderer.drawPeaks(peakVerts);
    }

    animFrameId = requestAnimationFrame(renderFrame);
  }

  // --- Lifecycle ---
  onMount(() => {
    if (!canvasRef) return;

    try {
      renderer = new WebGLRenderer(canvasRef);
    } catch {
      setWebglAvailable(false);
      return;
    }

    handleResize();

    // Observe container resize
    const resizeObserver = new ResizeObserver(handleResize);
    if (containerRef) resizeObserver.observe(containerRef);

    // Start render loop
    animFrameId = requestAnimationFrame(renderFrame);

    onCleanup(() => {
      cancelAnimationFrame(animFrameId);
      resizeObserver.disconnect();
      renderer?.dispose();
      renderer = null;
    });
  });

  return (
    <div
      ref={containerRef}
      class="spectrum-analyzer"
      role="img"
      aria-label="Spectrum analyzer visualization"
    >
      {webglAvailable() ? (
        <>
          <canvas ref={canvasRef} class="spectrum-canvas" />
          <canvas ref={overlayCanvasRef} class="spectrum-overlay" />
        </>
      ) : (
        <div class="spectrum-fallback">
          <p>WebGL is not available in this environment.</p>
          <p>Spectrum visualization requires WebGL support.</p>
        </div>
      )}
    </div>
  );
};

export default SpectrumAnalyzer;
