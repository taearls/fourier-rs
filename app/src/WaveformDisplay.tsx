/**
 * `<WaveformDisplay />` – WebGL-based real-time oscilloscope visualization.
 *
 * Features:
 * - Time-domain waveform display with amplitude axis (-1 to +1)
 * - Zero-crossing trigger for stable display
 * - Configurable time window
 * - Responsive to container/window resize
 * - 60 fps via requestAnimationFrame
 * - Graceful fallback when WebGL is unavailable
 */

import { createSignal, onCleanup, onMount, type Component } from "solid-js";
import type { WaveformSnapshot } from "./bindings";
import {
  buildWaveformVertices,
  findRisingZeroCrossing,
  AMP_TICKS,
} from "./waveform-utils";
import { WebGLRenderer } from "./webgl-renderer";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MARGIN_LEFT = 35;
const MARGIN_TOP = 10;
const MARGIN_RIGHT = 10;
const MARGIN_BOTTOM = 28;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface WaveformDisplayProps {
  /** Current waveform snapshot from the engine (null when not running). */
  waveform: WaveformSnapshot | null;
}

const WaveformDisplay: Component<WaveformDisplayProps> = (props) => {
  let canvasRef: HTMLCanvasElement | undefined;
  let overlayCanvasRef: HTMLCanvasElement | undefined;
  let containerRef: HTMLDivElement | undefined;
  let renderer: WebGLRenderer | null = null;
  let animFrameId = 0;

  const [webglAvailable, setWebglAvailable] = createSignal(true);

  // --- Overlay (2D Canvas) for axis labels ---
  function drawOverlay(waveform: WaveformSnapshot | null): void {
    if (!overlayCanvasRef) return;
    const ctx = overlayCanvasRef.getContext("2d");
    if (!ctx) return;

    const w = overlayCanvasRef.width;
    const h = overlayCanvasRef.height;

    const dpr = window.devicePixelRatio || 1;
    const marginLeft = Math.round(MARGIN_LEFT * dpr);
    const marginBottom = Math.round(MARGIN_BOTTOM * dpr);
    const marginTop = Math.round(MARGIN_TOP * dpr);
    const marginRight = Math.round(MARGIN_RIGHT * dpr);
    const plotW = w - marginLeft - marginRight;
    const plotH = h - marginTop - marginBottom;

    ctx.clearRect(0, 0, w, h);

    const fontSize = Math.round(11 * dpr);
    ctx.font = `${fontSize}px -apple-system, BlinkMacSystemFont, sans-serif`;
    ctx.textBaseline = "middle";

    // --- Time axis (bottom) ---
    // Calculate display window duration
    const sampleRate = waveform?.sample_rate ?? 44100;
    const displaySamples = Math.min(
      waveform?.samples.length ?? 2048,
      sampleRate * 0.05, // max 50ms window
    );
    const durationMs = (displaySamples / sampleRate) * 1000;

    // Generate time ticks based on the actual duration
    const tickInterval = durationMs <= 10 ? 1 : durationMs <= 25 ? 5 : 10;
    ctx.textAlign = "center";
    for (let ms = 0; ms <= durationMs; ms += tickInterval) {
      const norm = ms / durationMs;
      const x = marginLeft + norm * plotW;

      // Grid line
      ctx.strokeStyle = "rgba(255, 255, 255, 0.06)";
      ctx.beginPath();
      ctx.moveTo(x, marginTop);
      ctx.lineTo(x, marginTop + plotH);
      ctx.stroke();

      // Label
      ctx.fillStyle = "rgba(255, 255, 255, 0.5)";
      ctx.fillText(`${ms}`, x, marginTop + plotH + Math.round(14 * dpr));
    }

    // --- Amplitude axis (left) ---
    ctx.textAlign = "right";
    for (const amp of AMP_TICKS) {
      const norm = (amp + 1) / 2; // [-1, 1] -> [0, 1]
      const y = marginTop + plotH - norm * plotH;

      // Grid line (center line slightly brighter)
      ctx.strokeStyle =
        amp === 0
          ? "rgba(255, 255, 255, 0.15)"
          : "rgba(255, 255, 255, 0.06)";
      ctx.beginPath();
      ctx.moveTo(marginLeft, y);
      ctx.lineTo(marginLeft + plotW, y);
      ctx.stroke();

      // Label
      ctx.fillStyle = "rgba(255, 255, 255, 0.5)";
      const label = amp === 0 ? "0" : amp > 0 ? `+${amp}` : `${amp}`;
      ctx.fillText(label, marginLeft - Math.round(6 * dpr), y);
    }

    // Axis unit labels
    ctx.fillStyle = "rgba(255, 255, 255, 0.35)";
    ctx.textAlign = "center";
    ctx.fillText("ms", marginLeft + plotW / 2, h - Math.round(2 * dpr));
    ctx.save();
    ctx.translate(Math.round(10 * dpr), marginTop + plotH / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText("Amp", 0, 0);
    ctx.restore();
  }

  // --- Resize handler ---
  function handleResize(): void {
    if (!containerRef || !canvasRef || !overlayCanvasRef) return;

    const rect = containerRef.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(rect.width * dpr);
    const h = Math.round(rect.height * dpr);

    canvasRef.width = w;
    canvasRef.height = h;
    renderer?.resize(
      w,
      h,
      Math.round(MARGIN_LEFT * dpr),
      Math.round(MARGIN_TOP * dpr),
      Math.round(MARGIN_RIGHT * dpr),
      Math.round(MARGIN_BOTTOM * dpr),
    );

    overlayCanvasRef.width = w;
    overlayCanvasRef.height = h;

    drawOverlay(props.waveform);
  }

  // --- Main render loop ---
  function renderFrame(): void {
    if (!renderer || !canvasRef) {
      animFrameId = requestAnimationFrame(renderFrame);
      return;
    }

    const waveform = props.waveform;

    renderer.clear();

    if (waveform && waveform.samples.length > 0) {
      const { samples, sample_rate } = waveform;

      // Display window: up to 50ms of audio (~2205 samples at 44.1kHz)
      const maxDisplaySamples = Math.round(sample_rate * 0.05);
      const windowSize = Math.min(maxDisplaySamples, samples.length);

      // Use only the most recent samples
      const recentStart = Math.max(0, samples.length - windowSize);
      const recentSamples = samples.slice(recentStart);

      // Zero-crossing trigger: find first rising zero-crossing
      const triggerOffset = findRisingZeroCrossing(recentSamples);

      const vertices = buildWaveformVertices(
        recentSamples,
        windowSize - triggerOffset,
        triggerOffset,
      );

      if (vertices.length > 0) {
        // Draw waveform as a line (green accent for oscilloscope look)
        renderer.drawWaveform(vertices);
      }

      // Redraw overlay when waveform data changes (for accurate time ticks)
      drawOverlay(waveform);
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

    const resizeObserver = new ResizeObserver(handleResize);
    if (containerRef) resizeObserver.observe(containerRef);

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
      class="waveform-display"
      role="img"
      aria-label="Waveform oscilloscope visualization"
    >
      {webglAvailable() ? (
        <>
          <canvas ref={canvasRef} class="waveform-canvas" />
          <canvas ref={overlayCanvasRef} class="waveform-overlay" />
        </>
      ) : (
        <div class="waveform-fallback">
          <p>WebGL is not available in this environment.</p>
          <p>Waveform visualization requires WebGL support.</p>
        </div>
      )}
    </div>
  );
};

export default WaveformDisplay;
