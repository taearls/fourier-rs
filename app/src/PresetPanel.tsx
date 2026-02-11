import { createSignal, For, onMount, Show, type Component } from "solid-js";
import {
  savePreset,
  loadPreset,
  listPresets,
  deletePreset,
  type PresetInfo,
  type Preset,
  type SourceSpec,
  type TransformSpec,
} from "./bindings";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

const PresetPanel: Component<{
  onLoad: (preset: Preset) => void;
  currentSource: () => SourceSpec;
  currentTransform: () => TransformSpec;
  currentGain: () => number;
}> = (props) => {
  const [presets, setPresets] = createSignal<PresetInfo[]>([]);
  const [selected, setSelected] = createSignal<string>("");
  const [saveName, setSaveName] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

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

  async function refreshPresets(): Promise<void> {
    const list = await listPresets();
    setPresets(list);
  }

  // -----------------------------------------------------------------------
  // Lifecycle
  // -----------------------------------------------------------------------

  onMount(() => {
    refreshPresets();
  });

  // -----------------------------------------------------------------------
  // Handlers
  // -----------------------------------------------------------------------

  async function handleLoad(): Promise<void> {
    const name = selected();
    if (!name) return;

    await withError(async () => {
      const preset = await loadPreset(name);
      props.onLoad(preset);
    });
  }

  async function handleSave(): Promise<void> {
    const name = saveName().trim();
    if (!name) return;

    await withError(async () => {
      await savePreset(
        name,
        props.currentSource(),
        props.currentTransform(),
        props.currentGain(),
      );
      setSaveName("");
      setSaving(false);
      await refreshPresets();
      setSelected(name);
    });
  }

  async function handleDelete(): Promise<void> {
    const name = selected();
    if (!name) return;

    const info = presets().find((p) => p.name === name);
    if (info?.is_factory) return;

    if (!window.confirm(`Delete preset "${name}"?`)) return;

    await withError(async () => {
      await deletePreset(name);
      setSelected("");
      await refreshPresets();
    });
  }

  function selectedIsFactory(): boolean {
    const name = selected();
    const info = presets().find((p) => p.name === name);
    return info?.is_factory ?? false;
  }

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  return (
    <section class="cp-section preset-panel">
      <label class="cp-section-label">Presets</label>

      <Show when={error()}>
        <div class="cp-error" onClick={clearError}>
          {error()}
        </div>
      </Show>

      {/* Preset selector + load */}
      <div class="preset-row">
        <select
          class="preset-select"
          value={selected()}
          onChange={(e) => setSelected(e.currentTarget.value)}
        >
          <option value="">Select preset...</option>
          <For each={presets()}>
            {(p) => (
              <option value={p.name}>
                {p.is_factory ? `\u2605 ${p.name}` : p.name}
              </option>
            )}
          </For>
        </select>
        <button
          class="cp-btn"
          onClick={handleLoad}
          disabled={!selected()}
        >
          Load
        </button>
      </div>

      {/* Delete button (user presets only) */}
      <Show when={selected() && !selectedIsFactory()}>
        <button
          class="cp-btn preset-delete"
          onClick={handleDelete}
        >
          Delete "{selected()}"
        </button>
      </Show>

      {/* Save controls */}
      <Show when={!saving()}>
        <button class="cp-btn" onClick={() => setSaving(true)}>
          Save Current...
        </button>
      </Show>

      <Show when={saving()}>
        <div class="preset-save">
          <input
            class="preset-name-input"
            type="text"
            placeholder="Preset name"
            value={saveName()}
            onInput={(e) => setSaveName(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleSave();
              if (e.key === "Escape") setSaving(false);
            }}
          />
          <div class="preset-save-actions">
            <button
              class="cp-btn"
              onClick={handleSave}
              disabled={!saveName().trim()}
            >
              Save
            </button>
            <button
              class="cp-btn"
              onClick={() => { setSaving(false); setSaveName(""); }}
            >
              Cancel
            </button>
          </div>
        </div>
      </Show>
    </section>
  );
};

export default PresetPanel;
