import { createEffect, createSignal, For, Show, type Accessor, type Component, type JSX } from "solid-js";
import {
  setTransform,
  type TransformSpec,
  type EqBand,
  type BandType,
} from "./bindings";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type TransformType =
  | "Identity"
  | "LowPass"
  | "HighPass"
  | "BandPass"
  | "Gain"
  | "ParametricEq"
  | "PitchShift"
  | "SpectralDelay";

interface ChainEntry {
  id: number;
  transformType: TransformType;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Default EQ band. */
function defaultBand(): EqBand {
  return { frequency: 1000, gain_db: 0, q: 1.0, band_type: "Peak" };
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

const TransformPanel: Component<{
  running: boolean;
  onSpecChange?: (spec: TransformSpec) => void;
  presetTransform?: Accessor<TransformSpec | null>;
}> = (props) => {
  let nextChainId = 1;
  const [error, setError] = createSignal<string | null>(null);

  // --- Single transform state (used when chain is empty / single mode) -----
  const [transformType, setTransformType] =
    createSignal<TransformType>("Identity");

  // LowPass / HighPass
  const [cutoffHz, setCutoffHz] = createSignal(1000);
  // BandPass
  const [lowHz, setLowHz] = createSignal(300);
  const [highHz, setHighHz] = createSignal(3000);
  // Gain
  const [gainFactor, setGainFactor] = createSignal(1.0);
  // ParametricEq bands
  const [eqBands, setEqBands] = createSignal<EqBand[]>([defaultBand()]);
  // PitchShift semitones
  const [pitchSemitones, setPitchSemitones] = createSignal(0);
  // SpectralDelay
  const [delayFrames, setDelayFrames] = createSignal(8);
  const [delayFeedback, setDelayFeedback] = createSignal(0.5);
  const [delayMix, setDelayMix] = createSignal(0.5);

  // --- Chain state ---------------------------------------------------------
  const [chainEnabled, setChainEnabled] = createSignal(false);
  const [chain, setChain] = createSignal<ChainEntry[]>([]);

  // Per-chain-entry parameters keyed by entry id
  const [chainCutoffs, setChainCutoffs] = createSignal<Record<number, number>>(
    {},
  );
  const [chainLows, setChainLows] = createSignal<Record<number, number>>({});
  const [chainHighs, setChainHighs] = createSignal<Record<number, number>>({});
  const [chainGains, setChainGains] = createSignal<Record<number, number>>({});
  const [chainEqBands, setChainEqBands] = createSignal<
    Record<number, EqBand[]>
  >({});
  const [chainPitchSemitones, setChainPitchSemitones] = createSignal<
    Record<number, number>
  >({});
  const [chainDelayFrames, setChainDelayFrames] = createSignal<
    Record<number, number>
  >({});
  const [chainDelayFeedback, setChainDelayFeedback] = createSignal<
    Record<number, number>
  >({});
  const [chainDelayMix, setChainDelayMix] = createSignal<
    Record<number, number>
  >({});

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

  /** Format Hz values for display. */
  function formatHz(hz: number): string {
    if (hz >= 1000) {
      return `${(hz / 1000).toFixed(1)}k`;
    }
    return `${hz}`;
  }

  // -----------------------------------------------------------------------
  // Build TransformSpec
  // -----------------------------------------------------------------------

  interface SingleSpecParams {
    cutoff: number;
    low: number;
    high: number;
    gain: number;
    bands: EqBand[];
    pitch: number;
    dFrames: number;
    dFeedback: number;
    dMix: number;
  }

  function buildSingleSpec(
    type: TransformType,
    p: SingleSpecParams,
  ): TransformSpec {
    switch (type) {
      case "Identity":
        return { type: "Identity" };
      case "LowPass":
        return { type: "LowPass", value: { cutoff_hz: p.cutoff } };
      case "HighPass":
        return { type: "HighPass", value: { cutoff_hz: p.cutoff } };
      case "BandPass":
        return {
          type: "BandPass",
          value: {
            low_hz: Math.min(p.low, p.high),
            high_hz: Math.max(p.low, p.high),
          },
        };
      case "Gain":
        return { type: "Gain", value: { factor: p.gain } };
      case "ParametricEq":
        return { type: "ParametricEq", value: { bands: p.bands } };
      case "PitchShift":
        return { type: "PitchShift", value: { semitones: p.pitch } };
      case "SpectralDelay":
        return {
          type: "SpectralDelay",
          value: {
            delay_frames: Math.round(p.dFrames),
            feedback: p.dFeedback,
            mix: p.dMix,
          },
        };
    }
  }

  function buildSpec(): TransformSpec {
    if (!chainEnabled()) {
      return buildSingleSpec(transformType(), {
        cutoff: cutoffHz(),
        low: lowHz(),
        high: highHz(),
        gain: gainFactor(),
        bands: eqBands(),
        pitch: pitchSemitones(),
        dFrames: delayFrames(),
        dFeedback: delayFeedback(),
        dMix: delayMix(),
      });
    }

    const entries = chain();
    if (entries.length === 0) {
      return { type: "Identity" };
    }

    const specs = entries.map((entry) =>
      buildSingleSpec(entry.transformType, {
        cutoff: chainCutoffs()[entry.id] ?? 1000,
        low: chainLows()[entry.id] ?? 300,
        high: chainHighs()[entry.id] ?? 3000,
        gain: chainGains()[entry.id] ?? 1.0,
        bands: chainEqBands()[entry.id] ?? [defaultBand()],
        pitch: chainPitchSemitones()[entry.id] ?? 0,
        dFrames: chainDelayFrames()[entry.id] ?? 8,
        dFeedback: chainDelayFeedback()[entry.id] ?? 0.5,
        dMix: chainDelayMix()[entry.id] ?? 0.5,
      }),
    );

    if (specs.length === 1) {
      return specs[0]!;
    }
    return { type: "Chain", value: specs };
  }

  // -----------------------------------------------------------------------
  // Apply
  // -----------------------------------------------------------------------

  async function applyTransform(): Promise<void> {
    const spec = buildSpec();
    props.onSpecChange?.(spec);
    if (!props.running) return;
    await withError(async () => {
      await setTransform(spec);
    });
  }

  /** Apply a transform spec from a preset, updating internal state to match. */
  function applyPresetSpec(spec: TransformSpec): void {
    // Disable chain mode and set simple transform state from preset.
    setChainEnabled(false);
    setChain([]);

    switch (spec.type) {
      case "Identity":
        setTransformType("Identity");
        break;
      case "LowPass":
        setTransformType("LowPass");
        setCutoffHz(spec.value.cutoff_hz);
        break;
      case "HighPass":
        setTransformType("HighPass");
        setCutoffHz(spec.value.cutoff_hz);
        break;
      case "BandPass":
        setTransformType("BandPass");
        setLowHz(spec.value.low_hz);
        setHighHz(spec.value.high_hz);
        break;
      case "Gain":
        setTransformType("Gain");
        setGainFactor(spec.value.factor);
        break;
      case "ParametricEq":
        setTransformType("ParametricEq");
        setEqBands([...spec.value.bands]);
        break;
      case "PitchShift":
        setTransformType("PitchShift");
        setPitchSemitones(spec.value.semitones);
        break;
      case "SpectralDelay":
        setTransformType("SpectralDelay");
        setDelayFrames(spec.value.delay_frames);
        setDelayFeedback(spec.value.feedback);
        setDelayMix(spec.value.mix);
        break;
      case "Chain":
        // Enable chain mode and populate entries.
        setChainEnabled(true);
        const entries: ChainEntry[] = [];
        const newCutoffs: Record<number, number> = {};
        const newLows: Record<number, number> = {};
        const newHighs: Record<number, number> = {};
        const newGains: Record<number, number> = {};
        const newBands: Record<number, EqBand[]> = {};
        const newPitch: Record<number, number> = {};
        const newDelayFrames: Record<number, number> = {};
        const newDelayFeedback: Record<number, number> = {};
        const newDelayMix: Record<number, number> = {};

        for (const item of spec.value) {
          const id = nextChainId++;
          let transformType: TransformType = "Identity";
          switch (item.type) {
            case "Identity": transformType = "Identity"; break;
            case "LowPass":
              transformType = "LowPass";
              newCutoffs[id] = item.value.cutoff_hz;
              break;
            case "HighPass":
              transformType = "HighPass";
              newCutoffs[id] = item.value.cutoff_hz;
              break;
            case "BandPass":
              transformType = "BandPass";
              newLows[id] = item.value.low_hz;
              newHighs[id] = item.value.high_hz;
              break;
            case "Gain":
              transformType = "Gain";
              newGains[id] = item.value.factor;
              break;
            case "ParametricEq":
              transformType = "ParametricEq";
              newBands[id] = [...item.value.bands];
              break;
            case "PitchShift":
              transformType = "PitchShift";
              newPitch[id] = item.value.semitones;
              break;
            case "SpectralDelay":
              transformType = "SpectralDelay";
              newDelayFrames[id] = item.value.delay_frames;
              newDelayFeedback[id] = item.value.feedback;
              newDelayMix[id] = item.value.mix;
              break;
          }
          entries.push({ id, transformType });
        }

        setChain(entries);
        setChainCutoffs(newCutoffs);
        setChainLows(newLows);
        setChainHighs(newHighs);
        setChainGains(newGains);
        setChainEqBands(newBands);
        setChainPitchSemitones(newPitch);
        setChainDelayFrames(newDelayFrames);
        setChainDelayFeedback(newDelayFeedback);
        setChainDelayMix(newDelayMix);
        break;
    }

    // Notify parent and apply to engine.
    const newSpec = buildSpec();
    props.onSpecChange?.(newSpec);
  }

  // Watch for preset transform changes from parent.
  createEffect(() => {
    const presetSpec = props.presetTransform?.();
    if (presetSpec) {
      applyPresetSpec(presetSpec);
    }
  });

  // -----------------------------------------------------------------------
  // Single-mode handlers
  // -----------------------------------------------------------------------

  async function handleTransformTypeChange(t: TransformType): Promise<void> {
    setTransformType(t);
    await applyTransform();
  }

  async function handleCutoffChange(v: number): Promise<void> {
    setCutoffHz(v);
    await applyTransform();
  }

  async function handleLowHzChange(v: number): Promise<void> {
    setLowHz(v);
    await applyTransform();
  }

  async function handleHighHzChange(v: number): Promise<void> {
    setHighHz(v);
    await applyTransform();
  }

  async function handleGainFactorChange(v: number): Promise<void> {
    setGainFactor(v);
    await applyTransform();
  }

  async function handlePitchSemitonesChange(v: number): Promise<void> {
    setPitchSemitones(v);
    await applyTransform();
  }

  async function handleDelayFramesChange(v: number): Promise<void> {
    setDelayFrames(v);
    await applyTransform();
  }

  async function handleDelayFeedbackChange(v: number): Promise<void> {
    setDelayFeedback(v);
    await applyTransform();
  }

  async function handleDelayMixChange(v: number): Promise<void> {
    setDelayMix(v);
    await applyTransform();
  }

  // --- EQ band handlers (single mode) ---

  async function handleBandChange(
    index: number,
    field: keyof EqBand,
    value: number | BandType,
  ): Promise<void> {
    setEqBands((prev) =>
      prev.map((b, i) => (i === index ? { ...b, [field]: value } : b)),
    );
    await applyTransform();
  }

  async function addBand(): Promise<void> {
    setEqBands((prev) => [...prev, defaultBand()]);
    await applyTransform();
  }

  async function removeBand(index: number): Promise<void> {
    setEqBands((prev) => prev.filter((_, i) => i !== index));
    await applyTransform();
  }

  // -----------------------------------------------------------------------
  // Chain handlers
  // -----------------------------------------------------------------------

  async function toggleChain(): Promise<void> {
    setChainEnabled((prev) => !prev);
    await applyTransform();
  }

  async function addChainEntry(): Promise<void> {
    const id = nextChainId++;
    setChain((prev) => [...prev, { id, transformType: "Identity" }]);
    await applyTransform();
  }

  function removeKey<T>(record: Record<number, T>, key: number): Record<number, T> {
    const copy = { ...record };
    delete copy[key];
    return copy;
  }

  async function removeChainEntry(id: number): Promise<void> {
    setChain((prev) => prev.filter((e) => e.id !== id));
    setChainCutoffs((prev) => removeKey(prev, id));
    setChainLows((prev) => removeKey(prev, id));
    setChainHighs((prev) => removeKey(prev, id));
    setChainGains((prev) => removeKey(prev, id));
    setChainEqBands((prev) => removeKey(prev, id));
    setChainPitchSemitones((prev) => removeKey(prev, id));
    setChainDelayFrames((prev) => removeKey(prev, id));
    setChainDelayFeedback((prev) => removeKey(prev, id));
    setChainDelayMix((prev) => removeKey(prev, id));
    await applyTransform();
  }

  async function moveChainEntry(index: number, dir: -1 | 1): Promise<void> {
    const target = index + dir;
    setChain((prev) => {
      if (target < 0 || target >= prev.length) return prev;
      const copy = [...prev];
      [copy[index], copy[target]] = [copy[target]!, copy[index]!];
      return copy;
    });
    await applyTransform();
  }

  async function handleChainTransformType(
    id: number,
    t: TransformType,
  ): Promise<void> {
    setChain((prev) =>
      prev.map((e) => (e.id === id ? { ...e, transformType: t } : e)),
    );
    if (t === "ParametricEq" && !chainEqBands()[id]) {
      setChainEqBands((prev) => ({ ...prev, [id]: [defaultBand()] }));
    }
    await applyTransform();
  }

  async function handleChainParam(
    id: number,
    param: "cutoff" | "low" | "high" | "gain" | "pitch" | "delayFrames" | "delayFeedback" | "delayMix",
    value: number,
  ): Promise<void> {
    switch (param) {
      case "cutoff":
        setChainCutoffs((prev) => ({ ...prev, [id]: value }));
        break;
      case "low":
        setChainLows((prev) => ({ ...prev, [id]: value }));
        break;
      case "high":
        setChainHighs((prev) => ({ ...prev, [id]: value }));
        break;
      case "gain":
        setChainGains((prev) => ({ ...prev, [id]: value }));
        break;
      case "pitch":
        setChainPitchSemitones((prev) => ({ ...prev, [id]: value }));
        break;
      case "delayFrames":
        setChainDelayFrames((prev) => ({ ...prev, [id]: value }));
        break;
      case "delayFeedback":
        setChainDelayFeedback((prev) => ({ ...prev, [id]: value }));
        break;
      case "delayMix":
        setChainDelayMix((prev) => ({ ...prev, [id]: value }));
        break;
    }
    await applyTransform();
  }

  async function handleChainBandChange(
    entryId: number,
    bandIndex: number,
    field: keyof EqBand,
    value: number | BandType,
  ): Promise<void> {
    setChainEqBands((prev) => ({
      ...prev,
      [entryId]: (prev[entryId] ?? [defaultBand()]).map((b, i) =>
        i === bandIndex ? { ...b, [field]: value } : b,
      ),
    }));
    await applyTransform();
  }

  async function addChainBand(entryId: number): Promise<void> {
    setChainEqBands((prev) => ({
      ...prev,
      [entryId]: [...(prev[entryId] ?? []), defaultBand()],
    }));
    await applyTransform();
  }

  async function removeChainBand(
    entryId: number,
    bandIndex: number,
  ): Promise<void> {
    setChainEqBands((prev) => ({
      ...prev,
      [entryId]: (prev[entryId] ?? []).filter((_, i) => i !== bandIndex),
    }));
    await applyTransform();
  }

  // -----------------------------------------------------------------------
  // Sub-renderers
  // -----------------------------------------------------------------------

  /** Render parameter controls for a given transform type (reused in single & chain modes). */
  function renderParams(
    type: TransformType,
    opts: {
      cutoff: number;
      onCutoff: (v: number) => void;
      low: number;
      onLow: (v: number) => void;
      high: number;
      onHigh: (v: number) => void;
      gain: number;
      onGain: (v: number) => void;
      bands: EqBand[];
      onBandChange: (i: number, field: keyof EqBand, v: number | BandType) => void;
      onAddBand: () => void;
      onRemoveBand: (i: number) => void;
      pitch: number;
      onPitch: (v: number) => void;
      dFrames: number;
      onDFrames: (v: number) => void;
      dFeedback: number;
      onDFeedback: (v: number) => void;
      dMix: number;
      onDMix: (v: number) => void;
    },
  ): JSX.Element {
    return (
      <>
        <Show when={type === "LowPass" || type === "HighPass"}>
          <div class="cp-field">
            <label>Cutoff</label>
            <input
              type="range"
              min="20"
              max="20000"
              step="1"
              value={opts.cutoff}
              onInput={(e) => opts.onCutoff(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{formatHz(opts.cutoff)} Hz</span>
          </div>
        </Show>

        <Show when={type === "BandPass"}>
          <div class="cp-field">
            <label>Low</label>
            <input
              type="range"
              min="20"
              max="20000"
              step="1"
              value={opts.low}
              onInput={(e) => opts.onLow(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{formatHz(opts.low)} Hz</span>
          </div>
          <div class="cp-field">
            <label>High</label>
            <input
              type="range"
              min="20"
              max="20000"
              step="1"
              value={opts.high}
              onInput={(e) => opts.onHigh(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{formatHz(opts.high)} Hz</span>
          </div>
        </Show>

        <Show when={type === "Gain"}>
          <div class="cp-field">
            <label>Factor</label>
            <input
              type="range"
              min="0"
              max="2"
              step="0.01"
              value={opts.gain}
              onInput={(e) => opts.onGain(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{opts.gain.toFixed(2)}x</span>
          </div>
        </Show>

        <Show when={type === "ParametricEq"}>
          <div class="tp-eq-bands">
            <For each={opts.bands}>
              {(band, i) => (
                <div class="tp-eq-band">
                  <div class="tp-eq-band-header">
                    <span class="tp-eq-band-label">Band {i() + 1}</span>
                    <button
                      class="tp-btn-sm tp-btn-remove"
                      onClick={() => opts.onRemoveBand(i())}
                      title="Remove band"
                    >
                      &times;
                    </button>
                  </div>
                  <div class="cp-field">
                    <label>Type</label>
                    <select
                      value={band.band_type}
                      onChange={(e) =>
                        opts.onBandChange(
                          i(),
                          "band_type",
                          e.currentTarget.value as BandType,
                        )
                      }
                    >
                      <option value="Peak">Peak</option>
                      <option value="LowShelf">Low Shelf</option>
                      <option value="HighShelf">High Shelf</option>
                    </select>
                  </div>
                  <div class="cp-field">
                    <label>Freq</label>
                    <input
                      type="range"
                      min="20"
                      max="20000"
                      step="1"
                      value={band.frequency}
                      onInput={(e) =>
                        opts.onBandChange(
                          i(),
                          "frequency",
                          parseFloat(e.currentTarget.value),
                        )
                      }
                    />
                    <span class="tp-value">{formatHz(band.frequency)}</span>
                  </div>
                  <div class="cp-field">
                    <label>Gain</label>
                    <input
                      type="range"
                      min="-24"
                      max="24"
                      step="0.1"
                      value={band.gain_db}
                      onInput={(e) =>
                        opts.onBandChange(
                          i(),
                          "gain_db",
                          parseFloat(e.currentTarget.value),
                        )
                      }
                    />
                    <span class="tp-value">
                      {band.gain_db > 0 ? "+" : ""}
                      {band.gain_db.toFixed(1)} dB
                    </span>
                  </div>
                  <div class="cp-field">
                    <label>Q</label>
                    <input
                      type="range"
                      min="0.1"
                      max="10"
                      step="0.1"
                      value={band.q}
                      onInput={(e) =>
                        opts.onBandChange(
                          i(),
                          "q",
                          parseFloat(e.currentTarget.value),
                        )
                      }
                    />
                    <span class="tp-value">{band.q.toFixed(1)}</span>
                  </div>
                </div>
              )}
            </For>
            <button class="cp-btn tp-add-band" onClick={opts.onAddBand}>
              + Add Band
            </button>
          </div>
        </Show>

        <Show when={type === "PitchShift"}>
          <div class="cp-field">
            <label>Shift</label>
            <input
              type="range"
              min="-24"
              max="24"
              step="0.1"
              value={opts.pitch}
              onInput={(e) => opts.onPitch(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">
              {opts.pitch > 0 ? "+" : ""}
              {opts.pitch.toFixed(1)} st
            </span>
          </div>
        </Show>

        <Show when={type === "SpectralDelay"}>
          <div class="cp-field">
            <label>Frames</label>
            <input
              type="range"
              min="1"
              max="64"
              step="1"
              value={opts.dFrames}
              onInput={(e) => opts.onDFrames(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{Math.round(opts.dFrames)}</span>
          </div>
          <div class="cp-field">
            <label>Feedback</label>
            <input
              type="range"
              min="0"
              max="0.95"
              step="0.01"
              value={opts.dFeedback}
              onInput={(e) => opts.onDFeedback(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{(opts.dFeedback * 100).toFixed(0)}%</span>
          </div>
          <div class="cp-field">
            <label>Mix</label>
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={opts.dMix}
              onInput={(e) => opts.onDMix(parseFloat(e.currentTarget.value))}
            />
            <span class="tp-value">{(opts.dMix * 100).toFixed(0)}%</span>
          </div>
        </Show>
      </>
    );
  }

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <div class="transform-panel">
      {/* Error display */}
      <Show when={error()}>
        <div class="cp-error" onClick={clearError}>
          {error()}
        </div>
      </Show>

      {/* Chain toggle */}
      <section class="cp-section">
        <div class="tp-chain-toggle">
          <label class="cp-section-label">Transform</label>
          <label class="tp-toggle-label">
            <input
              type="checkbox"
              checked={chainEnabled()}
              onChange={() => toggleChain()}
            />
            Chain
          </label>
        </div>
      </section>

      {/* Single transform mode */}
      <Show when={!chainEnabled()}>
        <section class="cp-section">
          <div class="cp-field">
            <label>Type</label>
            <select
              value={transformType()}
              onChange={(e) =>
                handleTransformTypeChange(
                  e.currentTarget.value as TransformType,
                )
              }
            >
              <option value="Identity">None</option>
              <option value="LowPass">Low Pass</option>
              <option value="HighPass">High Pass</option>
              <option value="BandPass">Band Pass</option>
              <option value="Gain">Gain</option>
              <option value="ParametricEq">Parametric EQ</option>
              <option value="PitchShift">Pitch Shift</option>
              <option value="SpectralDelay">Spectral Delay</option>
            </select>
          </div>

          {renderParams(transformType(), {
            cutoff: cutoffHz(),
            onCutoff: (v) => handleCutoffChange(v),
            low: lowHz(),
            onLow: (v) => handleLowHzChange(v),
            high: highHz(),
            onHigh: (v) => handleHighHzChange(v),
            gain: gainFactor(),
            onGain: (v) => handleGainFactorChange(v),
            bands: eqBands(),
            onBandChange: (i, field, v) => handleBandChange(i, field, v),
            onAddBand: () => addBand(),
            onRemoveBand: (i) => removeBand(i),
            pitch: pitchSemitones(),
            onPitch: (v) => handlePitchSemitonesChange(v),
            dFrames: delayFrames(),
            onDFrames: (v) => handleDelayFramesChange(v),
            dFeedback: delayFeedback(),
            onDFeedback: (v) => handleDelayFeedbackChange(v),
            dMix: delayMix(),
            onDMix: (v) => handleDelayMixChange(v),
          })}
        </section>
      </Show>

      {/* Chain mode */}
      <Show when={chainEnabled()}>
        <section class="cp-section tp-chain">
          <For each={chain()}>
            {(entry, index) => (
              <div class="tp-chain-entry">
                <div class="tp-chain-entry-header">
                  <span class="tp-chain-index">{index() + 1}</span>
                  <select
                    class="tp-chain-select"
                    value={entry.transformType}
                    onChange={(e) =>
                      handleChainTransformType(
                        entry.id,
                        e.currentTarget.value as TransformType,
                      )
                    }
                  >
                    <option value="Identity">None</option>
                    <option value="LowPass">Low Pass</option>
                    <option value="HighPass">High Pass</option>
                    <option value="BandPass">Band Pass</option>
                    <option value="Gain">Gain</option>
                    <option value="ParametricEq">Parametric EQ</option>
                    <option value="PitchShift">Pitch Shift</option>
                    <option value="SpectralDelay">Spectral Delay</option>
                  </select>
                  <div class="tp-chain-actions">
                    <button
                      class="tp-btn-sm"
                      onClick={() => moveChainEntry(index(), -1)}
                      disabled={index() === 0}
                      title="Move up"
                    >
                      &uarr;
                    </button>
                    <button
                      class="tp-btn-sm"
                      onClick={() => moveChainEntry(index(), 1)}
                      disabled={index() === chain().length - 1}
                      title="Move down"
                    >
                      &darr;
                    </button>
                    <button
                      class="tp-btn-sm tp-btn-remove"
                      onClick={() => removeChainEntry(entry.id)}
                      title="Remove"
                    >
                      &times;
                    </button>
                  </div>
                </div>

                <div class="tp-chain-entry-params">
                  {renderParams(entry.transformType, {
                    cutoff: chainCutoffs()[entry.id] ?? 1000,
                    onCutoff: (v) => handleChainParam(entry.id, "cutoff", v),
                    low: chainLows()[entry.id] ?? 300,
                    onLow: (v) => handleChainParam(entry.id, "low", v),
                    high: chainHighs()[entry.id] ?? 3000,
                    onHigh: (v) => handleChainParam(entry.id, "high", v),
                    gain: chainGains()[entry.id] ?? 1.0,
                    onGain: (v) => handleChainParam(entry.id, "gain", v),
                    bands: chainEqBands()[entry.id] ?? [defaultBand()],
                    onBandChange: (i, field, v) =>
                      handleChainBandChange(entry.id, i, field, v),
                    onAddBand: () => addChainBand(entry.id),
                    onRemoveBand: (i) => removeChainBand(entry.id, i),
                    pitch: chainPitchSemitones()[entry.id] ?? 0,
                    onPitch: (v) => handleChainParam(entry.id, "pitch", v),
                    dFrames: chainDelayFrames()[entry.id] ?? 8,
                    onDFrames: (v) => handleChainParam(entry.id, "delayFrames", v),
                    dFeedback: chainDelayFeedback()[entry.id] ?? 0.5,
                    onDFeedback: (v) => handleChainParam(entry.id, "delayFeedback", v),
                    dMix: chainDelayMix()[entry.id] ?? 0.5,
                    onDMix: (v) => handleChainParam(entry.id, "delayMix", v),
                  })}
                </div>
              </div>
            )}
          </For>

          <button class="cp-btn tp-add-entry" onClick={addChainEntry}>
            + Add Transform
          </button>
        </section>
      </Show>
    </div>
  );
};

export default TransformPanel;
