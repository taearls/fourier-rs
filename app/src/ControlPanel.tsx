import { createSignal, Show, type Component } from "solid-js";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  startEngine,
  stopEngine,
  setGain,
  setSourceLiveInput,
  setSourceOscillator,
  setSourceNoise,
  setSourceFile,
  seekSource,
  setTransform,
  exportAudio,
  onExportProgress,
  type WaveformType,
  type NoiseType,
  type SourceSpec,
  type TransformSpec,
  type Preset,
} from "./bindings";
import TransformPanel from "./TransformPanel";
import PresetPanel from "./PresetPanel";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type SourceType = "live" | "oscillator" | "noise" | "file";

// Default engine configuration.
// TODO: query the system default output device's native sample rate (#19 follow-up)
const DEFAULT_SAMPLE_RATE = 44_100;
const DEFAULT_FFT_SIZE = 2048;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

const ControlPanel: Component = () => {
  // Engine state
  const [running, setRunning] = createSignal(false);
  const [gain, setGainValue] = createSignal(0.75);
  const [error, setError] = createSignal<string | null>(null);

  // Source selection
  const [source, setSource] = createSignal<SourceType>("live");

  // Oscillator controls
  const [waveform, setWaveform] = createSignal<WaveformType>("Sine");
  const [frequency, setFrequency] = createSignal(440);

  // Noise controls
  const [noiseType, setNoiseType] = createSignal<NoiseType>("White");

  // File controls
  const [filePath, setFilePath] = createSignal<string | null>(null);
  const [looping, setLooping] = createSignal(true);
  const [seekPos, setSeekPos] = createSignal(0);

  // Transform tracking (for preset save)
  const [currentTransform, setCurrentTransform] = createSignal<TransformSpec>({ type: "Identity" });
  const [presetTransform, setPresetTransform] = createSignal<TransformSpec | null>(null);

  // Export state
  const [exporting, setExporting] = createSignal(false);
  const [exportProgress, setExportProgress] = createSignal(0);
  const [exportDuration, setExportDuration] = createSignal(5);

  // -----------------------------------------------------------------------
  // Helpers
  // -----------------------------------------------------------------------

  function clearError(): void {
    setError(null);
  }

  async function withError(fn: () => Promise<void>): Promise<void> {
    try {
      clearError();
      await fn();
    } catch (e) {
      setError(String(e));
    }
  }

  function fileName(): string {
    const p = filePath();
    if (!p) return "No file selected";
    return p.replace(/.*[/\\]/, "") || p;
  }

  // -----------------------------------------------------------------------
  // Engine lifecycle
  // -----------------------------------------------------------------------

  async function handleStartStop(): Promise<void> {
    await withError(async () => {
      if (running()) {
        await stopEngine();
        setRunning(false);
      } else {
        await startEngine(DEFAULT_SAMPLE_RATE, DEFAULT_FFT_SIZE);
        setRunning(true);
        // Apply current gain after starting
        await setGain(gain());
        // Apply current source after starting
        await applySource();
      }
    });
  }

  // -----------------------------------------------------------------------
  // Gain
  // -----------------------------------------------------------------------

  async function handleGainChange(value: number): Promise<void> {
    setGainValue(value);
    if (running()) {
      await withError(async () => {
        await setGain(value);
      });
    }
  }

  // -----------------------------------------------------------------------
  // Source switching
  // -----------------------------------------------------------------------

  async function applySource(): Promise<void> {
    if (!running()) return;
    const s = source();
    if (s === "live") {
      await setSourceLiveInput();
    } else if (s === "oscillator") {
      await setSourceOscillator(waveform(), frequency());
    } else if (s === "noise") {
      await setSourceNoise(noiseType());
    } else if (s === "file") {
      const path = filePath();
      if (path) {
        await setSourceFile(path, looping());
      }
    }
  }

  async function handleSourceChange(newSource: SourceType): Promise<void> {
    setSource(newSource);
    await withError(applySource);
  }

  // -----------------------------------------------------------------------
  // Oscillator handlers
  // -----------------------------------------------------------------------

  async function handleWaveformChange(w: WaveformType): Promise<void> {
    setWaveform(w);
    if (running() && source() === "oscillator") {
      await withError(async () => {
        await setSourceOscillator(w, frequency());
      });
    }
  }

  async function handleFrequencyChange(f: number): Promise<void> {
    setFrequency(f);
    if (running() && source() === "oscillator") {
      await withError(async () => {
        await setSourceOscillator(waveform(), f);
      });
    }
  }

  // -----------------------------------------------------------------------
  // Noise handlers
  // -----------------------------------------------------------------------

  async function handleNoiseTypeChange(nt: NoiseType): Promise<void> {
    setNoiseType(nt);
    if (running() && source() === "noise") {
      await withError(async () => {
        await setSourceNoise(nt);
      });
    }
  }

  // -----------------------------------------------------------------------
  // File handlers
  // -----------------------------------------------------------------------

  async function handleFilePick(): Promise<void> {
    const result = await open({
      multiple: false,
      filters: [{ name: "WAV Audio", extensions: ["wav"] }],
    });
    if (result) {
      setFilePath(result);
      setSeekPos(0);
      if (running() && source() === "file") {
        await withError(async () => {
          await setSourceFile(result, looping());
        });
      }
    }
  }

  async function handleLoopingChange(newLooping: boolean): Promise<void> {
    setLooping(newLooping);
    const path = filePath();
    if (running() && source() === "file" && path) {
      // TODO: add a dedicated set_looping command to avoid reloading the WAV
      await withError(async () => {
        await setSourceFile(path, newLooping);
      });
    }
  }

  async function handleSeek(pos: number): Promise<void> {
    setSeekPos(pos);
    if (running() && source() === "file") {
      await withError(async () => {
        await seekSource(pos);
      });
    }
  }

  // -----------------------------------------------------------------------
  // Preset helpers
  // -----------------------------------------------------------------------

  function buildCurrentSource(): SourceSpec {
    const s = source();
    switch (s) {
      case "live":
        return { type: "LiveInput" };
      case "oscillator":
        return { type: "Oscillator", waveform: waveform(), frequency: frequency(), amplitude: 1.0 };
      case "noise":
        return { type: "Noise", noise_type: noiseType(), amplitude: 1.0 };
      case "file":
        return { type: "AudioBuffer", looping: looping() };
      default:
        return { type: "LiveInput" };
    }
  }

  async function handlePresetLoad(preset: Preset): Promise<void> {
    await withError(async () => {
      // Apply gain.
      setGainValue(preset.gain);
      if (running()) {
        await setGain(preset.gain);
      }

      // Apply source.
      const src = preset.source;
      switch (src.type) {
        case "LiveInput":
          setSource("live");
          if (running()) await setSourceLiveInput();
          break;
        case "Oscillator":
          setSource("oscillator");
          setWaveform(src.waveform as WaveformType);
          setFrequency(src.frequency);
          if (running()) await setSourceOscillator(src.waveform as WaveformType, src.frequency);
          break;
        case "Noise":
          setSource("noise");
          setNoiseType(src.noise_type as NoiseType);
          if (running()) await setSourceNoise(src.noise_type as NoiseType);
          break;
        case "AudioBuffer":
          // AudioBuffer presets only restore looping flag; file must be re-selected.
          setSource("file");
          setLooping(src.looping);
          break;
        case "Additive":
          // Additive source has no UI controls yet; fall back to live input.
          setSource("live");
          if (running()) await setSourceLiveInput();
          break;
      }

      // Apply transform via the preset signal (TransformPanel will pick this up).
      setPresetTransform(null); // Reset first to ensure reactivity.
      setPresetTransform(preset.transform);
      if (running()) {
        await setTransform(preset.transform);
      }
    });
  }

  // -----------------------------------------------------------------------
  // Audio export
  // -----------------------------------------------------------------------

  async function handleExport(): Promise<void> {
    if (exporting()) return;

    const path = await save({
      defaultPath: "export.wav",
      filters: [{ name: "WAV Audio", extensions: ["wav"] }],
    });
    if (!path) return;

    setExporting(true);
    setExportProgress(0);

    const unlisten = await onExportProgress((pct) => {
      setExportProgress(pct);
    });

    await withError(async () => {
      try {
        await exportAudio(
          path,
          buildCurrentSource(),
          currentTransform(),
          gain(),
          exportDuration(),
          DEFAULT_SAMPLE_RATE,
          DEFAULT_FFT_SIZE,
          filePath(),
        );
      } finally {
        unlisten();
        setExporting(false);
        setExportProgress(0);
      }
    });
  }

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <div class="control-panel">
      {/* Error display */}
      <Show when={error()}>
        <div class="cp-error" onClick={clearError}>
          {error()}
        </div>
      </Show>

      {/* Engine controls */}
      <section class="cp-section cp-engine">
        <button
          class={`cp-startstop ${running() ? "running" : ""}`}
          onClick={handleStartStop}
        >
          {running() ? "Stop" : "Start"}
        </button>
        <div class="cp-gain">
          <label>Gain</label>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={gain()}
            onInput={(e) => handleGainChange(parseFloat(e.currentTarget.value))}
          />
          <span class="cp-gain-value">{Math.round(gain() * 100)}%</span>
        </div>
      </section>

      {/* Source selector */}
      <section class="cp-section cp-source-selector">
        <label class="cp-section-label">Source</label>
        <div class="cp-radio-group">
          {(
            [
              ["live", "Live Input"],
              ["oscillator", "Oscillator"],
              ["noise", "Noise"],
              ["file", "File"],
            ] as const
          ).map(([value, label]) => (
            <button
              class={`cp-radio ${source() === value ? "active" : ""}`}
              onClick={() => handleSourceChange(value)}
            >
              {label}
            </button>
          ))}
        </div>
      </section>

      {/* Oscillator controls */}
      <Show when={source() === "oscillator"}>
        <section class="cp-section cp-oscillator">
          <label class="cp-section-label">Oscillator</label>
          <div class="cp-field">
            <label>Waveform</label>
            <select
              value={waveform()}
              onChange={(e) =>
                handleWaveformChange(e.currentTarget.value as WaveformType)
              }
            >
              <option value="Sine">Sine</option>
              <option value="Square">Square</option>
              <option value="Sawtooth">Sawtooth</option>
              <option value="Triangle">Triangle</option>
            </select>
          </div>
          <div class="cp-field">
            <label>Frequency</label>
            <input
              type="range"
              min="20"
              max="20000"
              step="1"
              value={frequency()}
              onInput={(e) =>
                handleFrequencyChange(parseFloat(e.currentTarget.value))
              }
            />
            <span class="cp-freq-value">{frequency()} Hz</span>
          </div>
        </section>
      </Show>

      {/* Noise controls */}
      <Show when={source() === "noise"}>
        <section class="cp-section cp-noise">
          <label class="cp-section-label">Noise</label>
          <div class="cp-field">
            <label>Type</label>
            <select
              value={noiseType()}
              onChange={(e) =>
                handleNoiseTypeChange(e.currentTarget.value as NoiseType)
              }
            >
              <option value="White">White</option>
              <option value="Pink">Pink</option>
            </select>
          </div>
        </section>
      </Show>

      {/* File controls */}
      <Show when={source() === "file"}>
        <section class="cp-section cp-file">
          <label class="cp-section-label">File</label>
          <div class="cp-file-picker">
            <button class="cp-btn" onClick={handleFilePick}>
              Choose WAV
            </button>
            <span class="cp-file-name" title={filePath() ?? ""}>
              {fileName()}
            </span>
          </div>
          <div class="cp-field">
            <label>Position</label>
            <input
              type="range"
              min="0"
              max="1"
              step="0.001"
              value={seekPos()}
              onInput={(e) =>
                handleSeek(parseFloat(e.currentTarget.value))
              }
            />
          </div>
          <div class="cp-field">
            <label>
              <input
                type="checkbox"
                checked={looping()}
                onChange={(e) => handleLoopingChange(e.currentTarget.checked)}
              />
              Loop
            </label>
          </div>
        </section>
      </Show>

      {/* Separator */}
      <div class="cp-separator" />

      {/* Preset controls */}
      <PresetPanel
        onLoad={handlePresetLoad}
        currentSource={buildCurrentSource}
        currentTransform={currentTransform}
        currentGain={gain}
      />

      {/* Separator */}
      <div class="cp-separator" />

      {/* Transform controls */}
      <TransformPanel
        running={running()}
        onSpecChange={setCurrentTransform}
        presetTransform={presetTransform}
      />

      {/* Separator */}
      <div class="cp-separator" />

      {/* Export controls */}
      <section class="cp-section cp-export">
        <label class="cp-section-label">Export</label>
        <Show when={source() !== "file"}>
          <div class="cp-field">
            <label>Duration (s)</label>
            <input
              type="number"
              min="0.1"
              max="600"
              step="0.1"
              value={exportDuration()}
              onInput={(e) =>
                setExportDuration(parseFloat(e.currentTarget.value) || 5)
              }
              class="cp-export-duration"
            />
          </div>
        </Show>
        <button
          class="cp-btn cp-export-btn"
          onClick={handleExport}
          disabled={exporting()}
        >
          {exporting() ? "Exporting..." : "Export WAV"}
        </button>
        <Show when={exporting()}>
          <div class="cp-export-progress">
            <div
              class="cp-export-progress-bar"
              style={{ width: `${Math.round(exportProgress() * 100)}%` }}
            />
            <span class="cp-export-progress-text">
              {Math.round(exportProgress() * 100)}%
            </span>
          </div>
        </Show>
      </section>
    </div>
  );
};

export default ControlPanel;
