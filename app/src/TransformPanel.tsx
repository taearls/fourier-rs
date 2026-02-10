import { createSignal, For, Show, type Component, type JSX } from "solid-js";
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
  | "ParametricEq";

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

const TransformPanel: Component<{ running: boolean }> = (props) => {
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

  function buildSingleSpec(
    type: TransformType,
    cutoff: number,
    low: number,
    high: number,
    gain: number,
    bands: EqBand[],
  ): TransformSpec {
    switch (type) {
      case "Identity":
        return { type: "Identity" };
      case "LowPass":
        return { type: "LowPass", value: { cutoff_hz: cutoff } };
      case "HighPass":
        return { type: "HighPass", value: { cutoff_hz: cutoff } };
      case "BandPass":
        return {
          type: "BandPass",
          value: {
            low_hz: Math.min(low, high),
            high_hz: Math.max(low, high),
          },
        };
      case "Gain":
        return { type: "Gain", value: { factor: gain } };
      case "ParametricEq":
        return { type: "ParametricEq", value: { bands } };
    }
  }

  function buildSpec(): TransformSpec {
    if (!chainEnabled()) {
      return buildSingleSpec(
        transformType(),
        cutoffHz(),
        lowHz(),
        highHz(),
        gainFactor(),
        eqBands(),
      );
    }

    const entries = chain();
    if (entries.length === 0) {
      return { type: "Identity" };
    }

    const specs = entries.map((entry) =>
      buildSingleSpec(
        entry.transformType,
        chainCutoffs()[entry.id] ?? 1000,
        chainLows()[entry.id] ?? 300,
        chainHighs()[entry.id] ?? 3000,
        chainGains()[entry.id] ?? 1.0,
        chainEqBands()[entry.id] ?? [defaultBand()],
      ),
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
    if (!props.running) return;
    await withError(async () => {
      await setTransform(buildSpec());
    });
  }

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
    param: "cutoff" | "low" | "high" | "gain",
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
