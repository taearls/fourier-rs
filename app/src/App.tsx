import { createSignal, onCleanup, onMount, type Component } from "solid-js";
import { getSpectrum, type SpectralSnapshot } from "./bindings";
import ControlPanel from "./ControlPanel";
import SpectrumAnalyzer from "./SpectrumAnalyzer";
import type { RenderMode } from "./webgl-renderer";

const App: Component = () => {
  const [snapshot, setSnapshot] = createSignal<SpectralSnapshot | null>(null);
  const [renderMode, setRenderMode] = createSignal<RenderMode>("filled");

  let polling = true;

  async function pollSpectrum(): Promise<void> {
    while (polling) {
      try {
        const snap = await getSpectrum();
        setSnapshot(snap);
      } catch {
        // Engine not running — that's fine, keep polling
      }
      // Yield to next animation frame for ~60fps pacing
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    }
  }

  onMount(() => {
    pollSpectrum();
  });

  onCleanup(() => {
    polling = false;
  });

  return (
    <main class="app">
      <header class="app-header">
        <h1>Fourier-RS</h1>
        <div class="mode-selector">
          <button
            class={renderMode() === "line" ? "active" : ""}
            onClick={() => setRenderMode("line")}
          >
            Line
          </button>
          <button
            class={renderMode() === "filled" ? "active" : ""}
            onClick={() => setRenderMode("filled")}
          >
            Filled
          </button>
          <button
            class={renderMode() === "bars" ? "active" : ""}
            onClick={() => setRenderMode("bars")}
          >
            Bars
          </button>
        </div>
      </header>
      <div class="app-body">
        <SpectrumAnalyzer snapshot={snapshot()} mode={renderMode()} />
        <ControlPanel />
      </div>
    </main>
  );
};

export default App;
