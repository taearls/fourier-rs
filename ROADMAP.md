# Fourier-RS Roadmap

> **Last updated:** 2026-02-12
>
> Real-time audio DSP framework &rarr; Tauri desktop synthesizer with Fourier analysis

---

## Open Issues Summary

**1 open issue** across 7 phases (28 completed)

| Priority | Count | Issues |
|----------|-------|--------|
| :red_circle: Critical | 0 | &mdash; |
| :yellow_circle: High | 0 | &mdash; |
| :green_circle: Medium | 0 | &mdash; |
| :large_blue_circle: Low | 1 | #28 |

---

## Existing Foundations (Completed)

The following capabilities already exist in the codebase:

- **FFT / IFFT** &mdash; `crates/core/` forward and inverse transforms
- **Window functions** &mdash; Hann, Hamming, Blackman, etc.
- **Overlap-add streaming** &mdash; real-time OLA processor
- **Spectral transforms** &mdash; low-pass, high-pass, band-pass, gain (`SpectralTransform` trait)
- **Lock-free audio I/O** &mdash; `crates/audio-io/` via cpal
- **MIDI support** &mdash; `crates/midi/`
- **Engine orchestrator** &mdash; `crates/engine/` with parameter messaging and spectral snapshots
- **FFI bindings** &mdash; `crates/ffi/`
- **Oscillator** &mdash; Sine, Square, Sawtooth, Triangle waveforms in `crates/core/`
- **Noise generators** &mdash; `NoiseGenerator` with White (xorshift64 PRNG) and Pink (Voss-McCartney, 16 rows) noise in `crates/core/`; engine's noise sources refactored to use `fourier-core::NoiseGenerator`
- **Additive synthesis** &mdash; `AdditiveSynth` in `crates/core/` with `Partial` struct, per-partial phase tracking, `generate()` buffer-filling method, and `harmonic_series()` helper; `Partial` type owned by `fourier-core` and re-exported by `fourier-engine`; engine's `AdditiveSource` delegates to `AdditiveSynth`
- **Engine source integration** &mdash; `SourceSpec` enum, `AudioSource` trait, oscillator/noise/additive sources in `crates/engine/`
- **WAV file reading** &mdash; `crates/file-io/` with `AudioBuffer`, `load_wav()`, format normalization
- **WAV file writing** &mdash; `save_wav()` with `WavFormat` enum (I16, I24, F32), sample conversion with clamping, roundtrip-verified
- **Audio buffer playback** &mdash; `SourceSpec::AudioBuffer` with `Arc<AudioBuffer>`, looping, and `ParamMessage::Seek` position control; mono mixdown for multi-channel buffers
- **Tauri desktop app** &mdash; `app/` with Tauri v2 + SolidJS frontend, `fourier-engine` dependency
- **Tauri engine commands** &mdash; `start_engine`, `stop_engine`, `set_transform`, `set_gain`, `set_bypass`, `get_devices` via Tauri IPC with TypeScript bindings
- **Spectral snapshot streaming** &mdash; `get_spectrum` Tauri command returning `SpectralSnapshot` (magnitudes, peaks, sample_rate, fft_size, timestamp) with drain-to-latest polling for 60fps frontend consumption
- **Source selection commands** &mdash; `set_source_live_input`, `set_source_oscillator`, `set_source_noise`, `set_source_file` Tauri commands with parameter validation, WAV loading via `fourier-file-io`, and TypeScript bindings
- **Spectrum analyzer** &mdash; `<SpectrumAnalyzer />` SolidJS component with WebGL rendering, log frequency axis (20Hz&ndash;20kHz), dB magnitude axis (-90&ndash;0 dB), line/filled/bars render modes, peak hold with decay, responsive resize, graceful WebGL fallback
- **Control panel** &mdash; `<ControlPanel />` SolidJS component with source selector (Live/Oscillator/Noise/File), oscillator waveform+frequency controls, noise type selector, WAV file picker via native dialog + seek slider + loop toggle, engine start/stop button, master gain slider
- **Parametric EQ** &mdash; `ParametricEq` spectral transform with `EqBand` (frequency, gain_db, q, band_type) and `BandType` enum (Peak, LowShelf, HighShelf); smooth bell curves for Peak, sigmoid transitions for shelves; `TransformSpec::ParametricEq` variant with serde support and TypeScript bindings
- **Spectral freeze** &mdash; `SpectralFreeze` implementing `SpectralTransform` with magnitude+phase capture on activation, continuous frozen spectrum output, smooth crossfade on toggle (~75ms, linear interpolation in complex domain), `TransformSpec::SpectralFreeze { frozen: bool }` variant with serde support
- **Pitch shifting** &mdash; `PitchShift` implementing `SpectralTransform` via spectral bin rotation with linear interpolation for fractional shifts; `shift_semitones: f32` parameter (+12 = up octave, -12 = down octave); DC bin preservation; `TransformSpec::PitchShift { semitones: f32 }` variant with serde support; frontend pitch shift slider (-24 to +24 semitones) in TransformPanel
- **Spectral delay** &mdash; `SpectralDelay` implementing `SpectralTransform` with per-frame ring buffer delay; parameters: `delay_frames: usize` (1&ndash;64), `feedback: f32` (0.0&ndash;0.95 for decaying repetitions), `mix: f32` (0.0&ndash;1.0 dry/wet blend); lazy buffer initialization; `TransformSpec::SpectralDelay` variant with serde support; frontend controls (frames slider, feedback %, mix %) in TransformPanel
- **Transform panel** &mdash; `<TransformPanel />` SolidJS component with transform type selector (Identity, LowPass, HighPass, BandPass, Gain, ParametricEq); per-transform parameter controls (cutoff frequency, band range, gain factor); parametric EQ per-band controls (frequency, gain dB, Q, band type) with add/remove; transform chain mode with add/remove/reorder multiple transforms; all changes immediately sent to engine via `set_transform` Tauri command
- **Waveform oscilloscope** &mdash; `WaveformSnapshot` struct with rolling time-domain sample buffer; `get_waveform` Tauri command with drain-to-latest polling; `<WaveformDisplay />` SolidJS component with WebGL rendering, zero-crossing trigger for stable display, amplitude axis (-1 to +1), time axis (ms), responsive resize; green oscilloscope color scheme; stacked layout with spectrum analyzer
- **Error handling &amp; logging** &mdash; `thiserror`-based error enums (`CoreError`, `AudioIoError`, `EngineError`, `FileIoError`) across all crates; no `String` error types in public APIs; `tracing` instrumentation at engine lifecycle, parameter changes, and error paths; Tauri commands convert `EngineError` to user-friendly strings
- **Preset save/load** &mdash; `Preset` struct with serde support in `crates/engine/src/preset.rs`; `save_preset`, `load_preset`, `list_presets`, `delete_preset` Tauri commands; JSON file storage in user data directory (`~/Library/Application Support/fourier-rs/presets/`); 5 factory presets (Clean Sine, Low-Pass Voice, Octave Up, Warm Pad, Pink Noise Ambience); factory preset protection (cannot overwrite/delete); `<PresetPanel />` SolidJS component with preset dropdown, load/save/delete controls, name input with Enter/Escape keyboard handling
- **Audio export** &mdash; `render_offline()` function in `crates/engine/src/export.rs` for offline OLA rendering independent of live engine; `compute_total_frames()` for audio buffer vs. generated source duration; `export_audio` Tauri command with progress reporting via `export-progress` events; supports all source types (oscillator, noise, audio buffer; live input renders silence); output gain applied; WAV output via `save_wav()` with I16 format; frontend export UI with duration input, progress bar, and native save dialog
- **Tuner display** &mdash; `<TunerDisplay />` SolidJS component with peak frequency detection from spectral snapshots; frequency-to-note mapping (A4=440Hz equal temperament); note name with octave display (e.g. &ldquo;A4&rdquo;, &ldquo;C#5&rdquo;); cents deviation with color-coded indicator (green &le;5ct, yellow 5&ndash;20ct, red &gt;20ct); visual cents bar with centered reference marker; no-signal graceful fallback (&ldquo;--&rdquo;); `tuner-utils.ts` with `getTunerReading()`, MIDI-based pitch math, and `TunerReading` interface
- **WASM compilation** &mdash; `fourier-core` compiles to `wasm32-unknown-unknown` target via `wasm` feature flag; `wasm-bindgen` wrapper types in `crates/core/src/wasm.rs` for all core DSP operations: `WasmFftProcessor` (forward/inverse FFT with interleaved complex format), `WasmOscillator` (all 4 waveforms), `WasmNoiseGenerator` (white/pink), `WasmWindowFunction` (hann/hamming/blackman/rectangular), `WasmTransform` (identity, low-pass, high-pass, band-pass, gain, pitch shift, spectral freeze, spectral delay), `WasmAdditiveSynth` (harmonic series and custom partials), `WasmOverlapAddProcessor` (streaming OLA with configurable transform); free functions: `magnitudeSpectrum`, `magnitudeSpectrumDb`, `binToFrequency`, `frequencyToBin`; JS-friendly string-based enum parsing; example HTML+JS FFT roundtrip demo in `examples/wasm-fft/`

---

## Phase 1: Sound Generation

> **Goal:** Enable the engine to produce sound, not just process mic input
>
> **Effort:** ~1 week &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #2 | ~~Add oscillator module with standard waveforms~~ | :white_check_mark: Done | ~1 day | &mdash; |
| #3 | ~~Add noise generators (white, pink)~~ | :white_check_mark: Done | ~1 day | &mdash; |
| #4 | ~~Add additive synthesis module~~ | :white_check_mark: Done | ~1 day | ~~#2~~ |
| #5 | ~~Integrate sound generation into engine as audio source~~ | :white_check_mark: Done | ~2 days | #2 |

**Key deliverables:**
- `Oscillator` with Sine, Square, Sawtooth, Triangle waveforms
- ~~`NoiseGenerator` with White and Pink noise (Voss-McCartney)~~ :white_check_mark:
- ~~`AdditiveSynth` with per-partial control~~ :white_check_mark:
- `SourceSpec` enum and `ParamMessage::SetSource` in engine

---

## Phase 2: Audio File I/O

> **Goal:** Load and save WAV files, play files through the engine
>
> **Effort:** ~1 week &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #6 | ~~Create fourier-file-io crate with WAV reading~~ | :white_check_mark: Done | ~2 days | &mdash; |
| #7 | ~~Add WAV file writing/export~~ | :white_check_mark: Done | ~1 day | ~~#6~~ |
| #8 | ~~Add loaded audio file as engine source~~ | :white_check_mark: Done | ~2 days | #5, #6 |

**Key deliverables:**
- New `crates/file-io/` crate using `hound`
- `AudioBuffer` struct, `load_wav()`, ~~`save_wav()`~~ :white_check_mark:
- ~~`SourceSpec::AudioBuffer` variant with looping and seek~~ :white_check_mark:

---

## Phase 3: Enhanced DSP

> **Goal:** Richer spectral processing: EQ, delay, freeze, pitch shift
>
> **Effort:** ~2 weeks &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #9 | ~~Add parametric EQ as SpectralTransform~~ | :white_check_mark: Done | ~2 days | &mdash; |
| #10 | ~~Add spectral delay effect~~ | :white_check_mark: Done | ~2 days | &mdash; |
| #11 | ~~Add spectral freeze/hold effect~~ | :white_check_mark: Done | ~1 day | &mdash; |
| #12 | ~~Add pitch shifting via spectral bin rotation~~ | :white_check_mark: Done | ~2 days | &mdash; |

**Key deliverables:**
- ~~`ParametricEq` with Peak/LowShelf/HighShelf bands~~ :white_check_mark:
- ~~`SpectralDelay` with per-frame ring buffers~~ :white_check_mark:
- ~~`SpectralFreeze` with smooth crossfade toggle~~ :white_check_mark:
- ~~`PitchShift` with linear interpolation for fractional shifts~~ :white_check_mark:

---

## Phase 4: Tauri App Setup

> **Goal:** Desktop application shell with engine control from SolidJS frontend
>
> **Effort:** ~2 weeks &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #13 | ~~Scaffold Tauri v2 desktop application with SolidJS~~ | :white_check_mark: Done | ~2 days | &mdash; |
| #14 | ~~Implement Tauri commands for engine lifecycle~~ | :white_check_mark: Done | ~2 days | #13 |
| #15 | ~~Implement spectral snapshot streaming to frontend~~ | :white_check_mark: Done | ~1 day | #14 |
| #16 | ~~Implement Tauri commands for sound source selection~~ | :white_check_mark: Done | ~1 day | ~~#5~~, ~~#8~~, ~~#14~~ |

**Key deliverables:**
- `app/` directory with Tauri v2 + SolidJS
- Tauri commands: `start_engine`, `stop_engine`, `set_transform`, `get_spectrum`, etc.
- ~~30-60fps spectral snapshot polling~~ :white_check_mark:
- ~~Source selection commands (live, oscillator, noise, file)~~ :white_check_mark:

---

## Phase 5: UI Components

> **Goal:** Visual interface for spectrum analysis, waveform display, and controls
>
> **Effort:** ~3 weeks &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #17 | ~~Build spectrum analyzer visualization with WebGL~~ | :white_check_mark: Done | ~3 days | ~~#15~~ |
| #18 | ~~Build waveform/oscilloscope display~~ | :white_check_mark: Done | ~2 days | ~~#14~~, ~~#15~~ |
| #19 | ~~Build transport and source control panel~~ | :white_check_mark: Done | ~2 days | ~~#14~~, ~~#16~~ |
| #20 | ~~Build transform/filter control panel~~ | :white_check_mark: Done | ~2 days | ~~#9~~, ~~#14~~ |
| #21 | ~~Build peak frequency and note detection display~~ | :white_check_mark: Done | ~1 day | ~~#15~~ |

**Key deliverables:**
- ~~`<SpectrumAnalyzer />` &mdash; WebGL, log freq axis, dB magnitude, peak markers~~ :white_check_mark:
- ~~`<WaveformDisplay />` &mdash; WebGL oscilloscope with zero-crossing trigger~~ :white_check_mark:
- ~~`<ControlPanel />` &mdash; source selector, oscillator/noise/file controls, gain~~ :white_check_mark:
- ~~`<TransformPanel />` &mdash; EQ bands, pitch shift, freeze toggle, chain management~~ :white_check_mark:
- ~~`<TunerDisplay />` &mdash; peak frequency, note name, cents deviation~~ :white_check_mark:

---

## Phase 6: Sound Design Workflow

> **Goal:** Presets, export, and serialization for productive sound design
>
> **Effort:** ~1.5 weeks &bull; **Status:** :white_check_mark: Complete

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #22 | ~~Add serde serialization to TransformSpec and SourceSpec~~ | :white_check_mark: Done | ~1 day | &mdash; |
| #23 | ~~Add preset save/load system~~ | :white_check_mark: Done | ~2 days | ~~#22~~, ~~#14~~ |
| #24 | ~~Add audio export (render engine output to WAV)~~ | :white_check_mark: Done | ~2 days | ~~#7~~, ~~#8~~, ~~#14~~ |

**Key deliverables:**
- ~~Serde `Serialize`/`Deserialize` on all param types~~ :white_check_mark:
- ~~`Preset` struct with save/load to JSON, factory presets~~ :white_check_mark:
- ~~`export_audio` command with offline rendering and progress reporting~~ :white_check_mark:

---

## Phase 7: Polish & Web Readiness

> **Goal:** Production quality, CI, and browser-ready WASM
>
> **Effort:** ~2 weeks &bull; **Status:** In progress

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #25 | ~~Add error handling (thiserror) and logging (tracing)~~ | :white_check_mark: Done | ~2 days | &mdash; |
| #26 | ~~Add GitHub Actions CI pipeline~~ | :white_check_mark: Done | ~1 day | &mdash; |
| #51 | ~~Review and optimize CI workflow for reduced overhead~~ | :white_check_mark: Done | ~0.5 day | #26 |
| #27 | ~~Compile fourier-core to WASM target~~ | :white_check_mark: Done | ~2 days | #2 |
| #28 | Add WebAudio integration layer | :large_blue_circle: Low | ~3 days | #27 |
| #30 | ~~Set up development infrastructure (linting, formatting, testing, CI)~~ | :white_check_mark: Done | ~2 days | &mdash; |

**Key deliverables:**
- ~~`EngineError` enum via `thiserror`, `tracing` instrumentation~~ :white_check_mark:
- ~~CI: build, test, clippy, fmt on macOS (pinned 1.93.0 only, nightly removed)~~ :white_check_mark:
- ~~WASM build of fourier-core with `wasm-bindgen` exports~~ :white_check_mark:
- AudioWorklet integration with example web page
- ~~Dev infrastructure: `rust-toolchain.toml`, `rustfmt.toml`, workspace lints, `.editorconfig`, `deny.toml`, `Justfile`~~ :white_check_mark:

---

## MVP Critical Path

The minimum viable product requires completing these issues in order:

```
#2 Oscillators ✅
 └─► #5 Engine source integration ✅
      └─► #13 Tauri scaffold ✅
           └─► #22 Serde serialization ✅
                └─► #14 Engine lifecycle commands ✅
                     ├─► #15 Spectral streaming ✅ ──► #17 Spectrum viz ✅
                     └─► #16 Source commands ✅ ──► #19 Control panel ✅
```

**Critical path issues:** #2 &rarr; #5 &rarr; #13 &rarr; #22 &rarr; #14 &rarr; #15 &rarr; #16 &rarr; #17 &rarr; #19

All 9 critical-path issues are now **complete** :white_check_mark:

---

## Current Sprint

### NOW (Phase 1 Start)

Start with dev infrastructure and the two critical Phase 1 issues that unblock everything:

1. ~~**#30** &mdash; Set up development infrastructure (linting, formatting, testing, CI)~~ `:white_check_mark:`
2. ~~**#2** &mdash; Add oscillator module with standard waveforms~~ `:white_check_mark:`
3. ~~**#5** &mdash; Integrate sound generation into engine as audio source~~ `:white_check_mark:`

### NEXT UP

App shell and engine commands done. Moving to streaming and source commands:

4. ~~**#6** &mdash; Create fourier-file-io crate with WAV reading~~ `:white_check_mark:`
5. ~~**#13** &mdash; Scaffold Tauri v2 desktop application with SolidJS~~ `:white_check_mark:`
6. ~~**#22** &mdash; Add serde serialization~~ `:white_check_mark:`
7. ~~**#14** &mdash; Implement Tauri commands for engine lifecycle~~ `:white_check_mark:`
8. ~~**#15** &mdash; Implement spectral snapshot streaming to frontend~~ `:white_check_mark:`
9. ~~**#8** &mdash; Add loaded audio file as engine source~~ `:white_check_mark:`
10. ~~**#16** &mdash; Implement Tauri commands for sound source selection~~ `:white_check_mark:`
11. ~~**#17** &mdash; Build spectrum analyzer visualization with WebGL~~ `:white_check_mark:`
12. ~~**#19** &mdash; Build transport and source control panel~~ `:white_check_mark:`
13. ~~**#7** &mdash; Add WAV file writing/export~~ `:white_check_mark:`
14. ~~**#25** &mdash; Add error handling and logging~~ `:white_check_mark:`
15. ~~**#18** &mdash; Build waveform/oscilloscope display~~ `:white_check_mark:`
16. ~~**#21** &mdash; Build peak frequency and note detection display~~ `:white_check_mark:`

### PARALLEL TRACKS

These can proceed independently alongside the critical path:

- **DSP track:** ~~#9 (Parametric EQ)~~, ~~#10 (Spectral delay)~~, ~~#11 (Freeze)~~, ~~#12 (Pitch shift)~~ :white_check_mark:
- **File I/O track:** ~~#6~~, ~~#7~~ (WAV read/write) :white_check_mark:
- **Polish track:** ~~#25~~, ~~#26~~ (error handling, CI) :white_check_mark:

---

## Recommended Implementation Order

### Batch 1: Foundations (Week 1)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~1~~ | ~~#30 Dev infrastructure~~ | ~~Establish linting, formatting, CI before new code~~ :white_check_mark: |
| ~~2~~ | ~~#2 Oscillator module~~ | ~~Unblocks all sound generation~~ :white_check_mark: |
| ~~3~~ | ~~#5 Engine source integration~~ | ~~Connects generators to pipeline~~ :white_check_mark: |
| ~~4~~ | ~~#3 Noise generators~~ | ~~Parallel with #5, simple module~~ :white_check_mark: |

### Batch 2: File I/O + DSP (Week 2)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~5~~ | ~~#6 WAV reading~~ | ~~Unblocks file playback~~ :white_check_mark: |
| ~~6~~ | ~~#7 WAV writing~~ | ~~Small addition to #6~~ :white_check_mark: |
| ~~7~~ | ~~#9 Parametric EQ~~ | ~~Key DSP feature~~ :white_check_mark: |
| ~~8~~ | ~~#4 Additive synthesis~~ | ~~Depends on #2, enriches sources~~ :white_check_mark: |

### Batch 3: App Shell (Week 3)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~9~~ | ~~#13 Tauri scaffold~~ | ~~Unblocks all frontend work~~ :white_check_mark: |
| ~~10~~ | ~~#22 Serde serialization~~ | ~~Needed for Tauri IPC~~ :white_check_mark: |
| ~~11~~ | ~~#25 Error handling~~ | ~~Clean up before more code~~ :white_check_mark: |
| ~~12~~ | ~~#8 Audio file source~~ | ~~Depends on #5, #6~~ :white_check_mark: |

### Batch 4: Engine Commands (Week 4)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~13~~ | ~~#14 Engine lifecycle commands~~ | ~~Core Tauri API~~ :white_check_mark: |
| ~~14~~ | ~~#15 Spectral streaming~~ | ~~Enables visualization~~ :white_check_mark: |
| ~~15~~ | ~~#16 Source selection commands~~ | ~~Enables source UI~~ :white_check_mark: |

### Batch 5: UI (Weeks 5-6)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~16~~ | ~~#17 Spectrum analyzer (WebGL)~~ | ~~Flagship visualization~~ :white_check_mark: |
| ~~17~~ | ~~#19 Transport/source control panel~~ | ~~Core user interaction~~ :white_check_mark: |
| ~~18~~ | ~~#18 Waveform display~~ | ~~Second visualization~~ :white_check_mark: |
| ~~19~~ | ~~#20 Transform control panel~~ | ~~Effect parameter UI~~ :white_check_mark: |
| ~~20~~ | ~~#21 Note detection display~~ | ~~Tuner feature~~ :white_check_mark: |

### Batch 6: Workflow + DSP Extras (Week 7)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~21~~ | ~~#23 Preset system~~ | ~~UX polish~~ :white_check_mark: |
| ~~22~~ | ~~#24 Audio export~~ | ~~Render to file~~ :white_check_mark: |
| ~~23~~ | ~~#11 Spectral freeze~~ | ~~Creative effect~~ :white_check_mark: |
| ~~24~~ | ~~#12 Pitch shifting~~ | ~~Creative effect~~ :white_check_mark: |

### Batch 7: Future (Week 8+)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~25~~ | ~~#10 Spectral delay~~ | ~~Niche effect~~ :white_check_mark: |
| ~~26~~ | ~~#26 CI pipeline~~ | ~~Stable + nightly matrix, caching~~ :white_check_mark: |
| ~~27~~ | ~~#51 Optimize CI workflow~~ | ~~Drop nightly, reduce overhead~~ :white_check_mark: |
| ~~28~~ | ~~#27 WASM compilation~~ | ~~Web readiness~~ :white_check_mark: |
| 29 | #28 WebAudio integration | Browser demo |

---

## Issue Status Summary

| Phase | Total | Critical | High | Medium | Low | Done |
|-------|-------|----------|------|--------|-----|------|
| 1 &mdash; Sound Gen | 4 | 0 | 0 | 0 | 0 | 4 |
| 2 &mdash; File I/O | 3 | 0 | 0 | 0 | 0 | 3 |
| 3 &mdash; DSP | 4 | 0 | 0 | 0 | 0 | 4 |
| 4 &mdash; Tauri | 4 | 0 | 0 | 0 | 0 | 4 |
| 5 &mdash; UI | 5 | 0 | 0 | 0 | 0 | 5 |
| 6 &mdash; Workflow | 3 | 0 | 0 | 0 | 0 | 3 |
| 7 &mdash; Polish/Web | 6 | 0 | 0 | 0 | 1 | 5 |
| **Total** | **29** | **0** | **0** | **0** | **1** | **28** |

---

## Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| DSP / Core | Rust | Performance, safety, WASM target |
| Audio I/O | cpal | Cross-platform audio |
| File I/O | hound | WAV read/write |
| Desktop | Tauri v2 | Native performance, small binary |
| Frontend | SolidJS | Fine-grained reactivity for 60fps |
| Visualization | WebGL | Hardware-accelerated rendering |
| Future Web | wasm-bindgen + AudioWorklet | Browser-ready DSP |

---

## Changelog

### 2026-02-12
- **Completed #27** (compile fourier-core to WASM target) &mdash; added `wasm` feature flag to `fourier-core` Cargo.toml with optional `wasm-bindgen` dependency; added `wasm-bindgen = "0.2"` to workspace dependencies; created `crates/core/src/wasm.rs` module gated behind `#[cfg(feature = "wasm")]` with comprehensive `wasm_bindgen` wrapper types for all core DSP operations; `WasmFftProcessor` with `forward()` and `inverse()` methods using interleaved `[re, im, …]` format for JS `Float32Array` interop; `WasmOscillator` with string-based waveform selection (`"sine"`, `"square"`, `"sawtooth"`, `"triangle"`), `generate(num_samples)` returning `Vec<f32>`, and getter/setter properties; `WasmNoiseGenerator` with string-based noise type (`"white"`, `"pink"`); `WasmWindowFunction` with `apply()`, `table()`, and `coherentGain` getter; `WasmTransform` with static factory methods (`identity()`, `lowPass()`, `highPass()`, `bandPass()`, `gain()`, `pitchShift()`, `spectralFreeze()`, `spectralDelay()`) wrapping the `SpectralTransform` trait behind a single JS-friendly type; `WasmAdditiveSynth` with harmonic series constructor and `fromPartials()` for custom partial data; `WasmOverlapAddProcessor` for streaming FFT-based spectral processing with configurable transform; free functions `magnitudeSpectrum()`, `magnitudeSpectrumDb()`, `binToFrequency()`, `frequencyToBin()` for spectral analysis utilities; all wrappers use `JsError` for error reporting; `cargo build --target wasm32-unknown-unknown -p fourier-core --features wasm` succeeds; no `std` dependencies that break WASM; zero clippy warnings; all 112 existing tests pass unchanged; example HTML+JS FFT roundtrip demo in `examples/wasm-fft/index.html` with spectrum visualization, wasm-pack instructions, and JS fallback mode
- **Completed #10** (spectral delay effect) &mdash; created `SpectralDelay` struct implementing `SpectralTransform` in `crates/core/src/transform.rs`; stores past spectral frames in a ring buffer and mixes delayed spectrum with current input; parameters: `delay_frames: usize` (number of spectral frames to delay, &ge;1), `feedback: f32` (clamped to 0.0&ndash;0.95, feeds output back into delay line for decaying repetitions), `mix: f32` (clamped to 0.0&ndash;1.0, dry/wet blend); lazy ring buffer initialization on first `process()` call (adapts to any FFT size); output formula: `output = dry * (1-mix) + delayed * mix`; delay line stores `input + delayed * feedback` for each frame; ring buffer wraps correctly with modular arithmetic; `TransformSpec::SpectralDelay { delay_frames, feedback, mix }` variant in `crates/engine/src/params.rs` with full serde support; wired into `build_transform()` in `processor.rs`; re-exported `SpectralDelay` from `fourier-core` and `fourier-engine` crate roots; TypeScript `SpectralDelay` variant added to `TransformSpec` type in `app/src/bindings.ts`; `<TransformPanel />` updated with "Spectral Delay" option and controls: frames slider (1&ndash;64), feedback slider (0&ndash;95%), mix slider (0&ndash;100%) in both single and chain modes; 9 new unit tests in `fourier-core`: delay delays spectrum, dry/wet mix, feedback produces decay, ring buffer wraps, feedback clamped, mix clamped, zero delay frames clamped, name, empty spectrum no panic; 3 new serde roundtrip tests in `fourier-engine`: SpectralDelay spec, in-chain, param message; 2 engine integration tests: spectral delay produces output with oscillator source, rapid parameter switching does not panic; all 275 tests pass; completes Phase 3 (Enhanced DSP) with all 4 issues done

### 2026-02-11
- **Completed #24** (audio export &mdash; render engine output to WAV) &mdash; created `crates/engine/src/export.rs` with `render_offline()` function for offline OLA rendering completely independent of the live engine (no shared state, no ring buffers); `RenderConfig` struct with `sample_rate`, `fft_size`, and `output_gain` fields; builds its own OLA processor, source, and transform from specs; processes audio in hop-sized chunks, applies output gain, and reports progress via callback every ~1%; `compute_total_frames()` determines render length from audio buffer frame count or duration * sample rate; `SilenceSource` fallback for `LiveInput` during offline render; feeds extra input (`total_frames + fft_size`) to account for OLA pipeline latency; safety valve prevents infinite loops if pipeline never produces output; `export_audio` synchronous Tauri command in `app/src-tauri/src/lib.rs` (Tauri v2 runs sync commands on thread pool, keeping live audio uninterrupted); validates sample rate &gt; 0, FFT size &ge; 2 and power of two, duration &gt; 0, gain &ge; 0; emits `export-progress` events via `app.emit()` for frontend progress tracking; writes output via `fourier_file_io::save_wav()` with `WavFormat::I16`; `ExportProgress` struct for event payload; TypeScript bindings in `app/src/bindings.ts`: `ExportProgress` interface, `exportAudio()` wrapper (path, source, transform, gain, duration, sample rate, FFT size), `onExportProgress()` event listener returning `UnlistenFn`; frontend export UI in `<ControlPanel />`: duration input (0.1&ndash;600s, shown for non-file sources), "Export WAV" button opening native save dialog via `@tauri-apps/plugin-dialog` `save()`, progress bar with percentage during export, disabled button during export; CSS styles for export panel (`.cp-export`, `.cp-export-progress`, `.cp-export-progress-bar`); added `core:event:default` and `dialog:allow-save` to Tauri capabilities; 13 new unit tests in `fourier-engine`: oscillator/noise/audio buffer render output, live input silence, transform chain, progress callback fires, progress reaches 100%, output gain applied, zero frames, pitch shift, compute_total_frames variants; made `build_transform` public in `processor.rs` for export module access; all 261 tests pass; completes Phase 6 (Sound Design Workflow) with all 3 issues done
- **Completed #23** (preset save/load system) &mdash; created `crates/engine/src/preset.rs` with `Preset` struct (`name: String`, `source: SourceSpec`, `transform: TransformSpec`, `gain: f32`) with full serde `Serialize`/`Deserialize` support; `PresetInfo` struct for listing (`name`, `is_factory`); `factory_presets()` function returning 5 built-in presets: "Clean Sine" (440Hz sine, identity transform, 0.75 gain), "Low-Pass Voice" (live input, 2kHz low-pass, 0.8 gain), "Octave Up" (live input, +12 semitone pitch shift, 0.75 gain), "Warm Pad" (220Hz sawtooth, low-pass + EQ chain, 0.6 gain), "Pink Noise Ambience" (pink noise, 4kHz low-pass, 0.4 gain); re-exported `Preset`, `PresetInfo`, `factory_presets` from `fourier-engine` crate root; 4 Tauri commands in `app/src-tauri/src/lib.rs`: `save_preset(name, source, transform, gain)` serializes preset to JSON in user data directory (`~/Library/Application Support/fourier-rs/presets/`), `load_preset(name)` checks factory presets first then user directory, `list_presets()` returns factory + user presets sorted (factory first, then alphabetical), `delete_preset(name)` removes user preset files; factory preset protection (cannot overwrite or delete); filename sanitization for safe filesystem storage; `PresetInfo` and `Preset` TypeScript interfaces in `app/src/bindings.ts` with `savePreset()`, `loadPreset()`, `listPresets()`, `deletePreset()` command wrappers; `<PresetPanel />` SolidJS component in `app/src/PresetPanel.tsx` with preset dropdown (factory presets marked with star), load button, save dialog with name input (Enter to save, Escape to cancel), delete button for user presets only; integrated into `<ControlPanel />` between source controls and transform panel; `TransformPanel` extended with `onSpecChange` callback and `presetTransform` accessor for bidirectional preset communication; preset loading applies source, transform, and gain to both UI state and running engine; 7 new unit tests in `fourier-engine`: preset roundtrip (identity, oscillator+chain, noise), factory presets valid, factory preset names unique, preset info roundtrip, JSON human-readable; added `dirs` and `serde_json` dependencies to `fourier-app`; all 248 tests pass
- **Completed #12** (pitch shifting via spectral bin rotation) &mdash; created `PitchShift` struct implementing `SpectralTransform` in `crates/core/src/transform.rs`; remaps spectral bins using `source_bin = output_bin / 2^(semitones/12)` formula; linear interpolation of magnitude for fractional bin positions; phase taken from nearest source bin; DC bin preserved unchanged; zero-shift early-return optimization (identity); `shift_semitones: f32` parameter (+12 = up one octave, -12 = down one octave, -7 = down a fifth); `TransformSpec::PitchShift { semitones: f32 }` variant in `crates/engine/src/params.rs` with serde support; wired into `build_transform()` in `processor.rs`; re-exported `PitchShift` from `fourier-core` and `fourier-engine` crate roots; TypeScript `PitchShift` variant added to `TransformSpec` type in `app/src/bindings.ts`; `<TransformPanel />` updated with "Pitch Shift" option and semitones slider (-24 to +24 st, 0.1 step) in both single and chain modes; 10 new unit tests in `fourier-core`: up octave doubles frequency, down octave halves, down fifth, zero is identity, fractional uses interpolation, preserves DC bin, large shift clears high bins, name, empty spectrum no panic, negative fractional; 4 new serde roundtrip tests in `fourier-engine`: positive/negative semitones, in-chain, param message; 2 engine integration tests: pitch shift produces output with oscillator source, rapid pitch shift switching does not panic; all 241 tests pass
- **Completed #11** (spectral freeze/hold effect) &mdash; created `SpectralFreeze` struct implementing `SpectralTransform` in `crates/core/src/transform.rs`; when activated (`frozen = true`), captures current spectral frame magnitudes and phases; while frozen, outputs captured spectrum instead of live input; smooth crossfade on toggle (~75ms duration) via linear interpolation in complex domain between live and frozen spectra; `crossfade_pos` ramps from 0.0 (fully live) to 1.0 (fully frozen) at a rate computed from `sample_rate / hop_size`; captured data released after unfreezing completes; `set_frozen(bool)` and `is_frozen()` accessors; `TransformSpec::SpectralFreeze { frozen: bool }` variant in `crates/engine/src/params.rs` with serde support; wired into `build_transform()` in `processor.rs` passing `sample_rate` and `hop_size` for crossfade timing; re-exported `SpectralFreeze` from `fourier-core` and `fourier-engine` crate roots; 8 new unit tests in `fourier-core`: freeze captures spectrum on activation, frozen output is stable, unfrozen passes through, crossfade is smooth (monotonically increasing), toggle off crossfades back (monotonically decreasing), getters, name, empty spectrum no panic; 4 new serde roundtrip tests in `fourier-engine`: frozen/unfrozen variants, in-chain, param message; 2 engine integration tests: freeze produces output with oscillator source, rapid freeze toggle does not panic; all 225 tests pass
- **Completed #4** (additive synthesis module) &mdash; created `crates/core/src/additive.rs` with `Partial` struct (`frequency`, `amplitude`, `phase` fields, serde `Serialize`/`Deserialize`), `AdditiveSynth` struct with `Vec<PartialState>` runtime state and `sample_rate`, `generate(&mut self, output: &mut [f32])` method that sums sine wave partials with per-partial phase tracking (wraps at 2&pi; to prevent precision loss, continuous across calls), `harmonic_series(fundamental, num_harmonics)` helper generating partials at f, 2f, 3f, &hellip; with 1/n amplitude rolloff; `num_partials()` and `sample_rate()` getters; re-exported `Partial`, `AdditiveSynth`, `harmonic_series` from `fourier-core` crate root; moved `Partial` ownership from `fourier-engine` params to `fourier-core` additive module &mdash; engine re-exports via `pub use fourier_core::Partial`; refactored engine `AdditiveSource` to delegate to `fourier_core::AdditiveSynth` instead of inline implementation; re-exported `AdditiveSynth` from `fourier-engine` crate root; 19 new unit tests in `fourier-core`: single partial peak frequency, single partial matches sine oscillator, multiple partials have expected peaks, summing N partials produces correct output, harmonic series correct frequencies/amplitudes/zero harmonics/zero phase/spectral peaks, phase continuous across generate calls, phase continuous for multiple partials, empty partials silence, empty buffer no-op, zero amplitude silence, num_partials getter, sample_rate getter, Partial serde roundtrip, Vec&lt;Partial&gt; serde roundtrip; 2 doc tests (AdditiveSynth example, harmonic_series example); all 196 existing tests pass; completes Phase 1 (Sound Generation) with all 4 issues done
- **Completed #51** (review and optimize CI workflow) &mdash; removed nightly Rust toolchain from the CI matrix for build, clippy, and test jobs; project pins Rust 1.93.0 via `rust-toolchain.toml` so nightly runs provided no meaningful signal while doubling CI cost; removed `needs: build` dependency from clippy and test jobs so all four main jobs (fmt, build, clippy, test) run in parallel; reduced CI from 8 jobs to 5 (fmt, build, clippy, test, deny); removed `fail-fast: false` and `continue-on-error` (no longer needed without matrix); kept `Swatinem/rust-cache@v2` for cargo build caching; deny job unchanged on ubuntu-latest; simplified job names (no toolchain suffix)
- **Completed #3** (noise generators: white, pink) &mdash; created `crates/core/src/noise.rs` with `NoiseGenerator` struct and `NoiseType` enum (`White`, `Pink`); `NoiseGenerator::new(noise_type, amplitude, sample_rate)` constructor with `generate(&mut self, output: &mut [f32])` buffer-filling method; white noise via xorshift64 PRNG (deterministic, no external dependencies) mapping upper 24 bits to `[-1.0, +1.0)` float range; pink noise via Voss-McCartney algorithm with 16 octave rows, trailing-zeros scheduling for per-row update timing, normalization factor `1/(NUM_ROWS+1)`, and running-sum accumulator for O(1) per-sample generation; PRNG extracted to module-level `prng_next_u64`/`prng_next_f32` free functions to satisfy borrow checker when iterating pink rows; getter/setter methods (`set_amplitude`, `set_noise_type`, `amplitude()`, `noise_type()`, `sample_rate()`) with `const` where possible; serde `Serialize`/`Deserialize` on `NoiseType`; re-exported `NoiseGenerator` and `NoiseType` from `fourier-core` crate root; refactored `fourier-engine` to use `fourier_core::NoiseGenerator` instead of duplicating white/pink noise implementations &mdash; replaced `WhiteNoiseSource` and `PinkNoiseSource` structs in `crates/engine/src/source.rs` with unified `NoiseSource` wrapper delegating to `NoiseGenerator`; `NoiseType` in `crates/engine/src/params.rs` changed from local enum to `pub use fourier_core::NoiseType` re-export; 15 new unit tests in `fourier-core`: white noise energy, amplitude bounds, approximately flat spectrum (averaged over 32 FFT frames with octave band comparison), pink noise energy, amplitude bounds, approximately &minus;3dB/octave rolloff (averaged over 64 FFT frames across 4 octave pairs), pink more-low-than-high total energy, white-flatter-than-pink comparative spectral analysis, property getters, noise type switching, amplitude energy scaling (`0.25&sup2; = 0.0625` ratio verification), zero amplitude silence, empty buffer no-op, serde roundtrip, deterministic output; all 178 existing tests pass
- **Completed #26** (GitHub Actions CI pipeline) &mdash; upgraded `.github/workflows/ci.yml` with stable + nightly Rust toolchain matrix; nightly jobs allowed to fail via `continue-on-error`; `dtolnay/rust-toolchain` action for explicit toolchain management; `Swatinem/rust-cache` for build caching on clippy, test, and build jobs; `fail-fast: false` ensures all matrix combinations run to completion; fmt job uses stable-only (formatting is toolchain-independent); all four required jobs (build, test, clippy, fmt) run on `macos-latest`; triggers on push to main and PRs; deny job unchanged on ubuntu-latest; CI badge already present in README.md
- **Completed #21** (peak frequency and note detection display) &mdash; created `<TunerDisplay />` SolidJS component in `app/src/TunerDisplay.tsx` displaying detected pitch as a musical note with tuning accuracy; receives `SpectralSnapshot` via props and extracts strongest peak within 20Hz&ndash;10kHz above -60dB threshold; `tuner-utils.ts` utility module with `getTunerReading()` function performing frequency-to-MIDI conversion (`midi = 69 + 12 * log2(freq / 440)`), note name mapping via chromatic lookup table, octave calculation, and cents deviation (`(fractionalMidi - nearestMidi) * 100`); displays note label (e.g. &ldquo;A4&rdquo;, &ldquo;C#5&rdquo;), frequency in Hz, and cents deviation with color-coded visual indicator; color coding: green (#22c55e) within &pm;5 cents (in tune), yellow (#eab308) &pm;5&ndash;20 cents (close), red (#ef4444) beyond &pm;20 cents (out of tune); horizontal cents bar with centered reference marker, circular indicator sliding left/right proportional to deviation (&pm;50 cent range), smooth CSS transitions; no-signal graceful fallback showing &ldquo;--&rdquo; for note, frequency, and cents when no valid peak detected; integrated into `App.tsx` viz-stack below waveform display; styled in `styles.css` matching dark theme with `var(--border)` separator, `var(--fg)` text, and tabular-nums for stable numeric readout; `TunerReading` and `TunerColor` TypeScript types exported for potential reuse; all 164 existing tests pass; completes Phase 5 (UI Components) with all 5 issues done
- **Completed #18** (waveform/oscilloscope display) &mdash; added `WaveformSnapshot` struct in `crates/engine/src/processor.rs` with `samples: Vec<f32>`, `sample_rate`, `fft_size`, `timestamp_ms`; rolling waveform buffer (4&times;fft_size capacity) in the processing loop captures time-domain output samples pre-gain for visualization; `waveform_buf_push()` and `build_waveform_snapshot()` helpers extract chronologically-ordered samples from the circular buffer; separate bounded channel (capacity 4) with drain-to-latest `Engine::latest_waveform()` method; `get_waveform` Tauri command in `app/src-tauri/src/lib.rs` mirrors `get_spectrum` pattern; `WaveformSnapshot` TypeScript interface and `getWaveform()` wrapper in `app/src/bindings.ts`; `<WaveformDisplay />` SolidJS component in `app/src/WaveformDisplay.tsx` with WebGL rendering via `drawWaveform()` method (green oscilloscope color #22c55e), zero-crossing trigger via `findRisingZeroCrossing()` for stable periodic signal display, amplitude axis (-1 to +1) with center-line emphasis, time axis (ms) with adaptive tick intervals, responsive resize via ResizeObserver, 2D canvas overlay for axis labels and grid; stacked layout with spectrum analyzer above waveform below in `.viz-stack` flex column; waveform display window capped at 50ms for readable oscilloscope view; utility functions in `app/src/waveform-utils.ts` for vertex building, zero-crossing detection, and axis formatting; `App.tsx` polls both `getSpectrum()` and `getWaveform()` in parallel via `Promise.all` for synchronized 60fps updates; 2 new engine tests (waveform snapshot from live input, waveform from oscillator source); all 164 tests pass; extends Phase 5 (UI Components)

### 2026-02-10
- **Completed #25** (error handling and logging) &mdash; added `thiserror` and `tracing` to workspace dependencies; created `CoreError` enum in `crates/core/src/error.rs` with `FftForwardFailed` and `FftInverseFailed` variants; changed `FftProcessor::forward()` and `inverse()` to return `Result<(), CoreError>` instead of panicking; updated `OverlapAddProcessor::process_frame()` to log FFT errors via `tracing::error!` and gracefully skip failed frames; created `AudioIoError` enum in `crates/audio-io/src/error.rs` with `ConfigQueryFailed`, `UnsupportedSampleFormat`, `StreamBuildFailed`, `StreamPlayFailed` variants wrapping cpal error types; replaced all `Result<_, String>` in `AudioStream::open_input/open_output` with `Result<_, AudioIoError>`; replaced `eprintln!` in audio callbacks with `tracing::error!`; created `EngineError` enum in `crates/engine/src/error.rs` with `ThreadSpawnFailed`, `ChannelSendFailed`, `AudioIo`, `FileIo` variants composing lower-level errors via `#[from]`; changed `Engine::new()` to return `Result<(Self, EngineIo), EngineError>` instead of panicking on thread spawn; changed all `Engine` public methods (`send_param`, `set_transform`, `set_output_gain`, `set_bypass`, `set_source`, `seek`) to return `Result<_, EngineError>` instead of `Result<_, String>`; converted `FileIoError` in `crates/file-io` from manual `Display`/`Error` impls to `thiserror` derive macros; added `tracing` instrumentation: `info!` at engine start/stop, `debug!` at processing thread start, transform/source/gain/bypass changes, shutdown signal, engine drop; Tauri commands convert `EngineError` to user-friendly strings via `engine_err()` helper at the IPC boundary; updated FFI `engine_create()` to return null on failure instead of panicking; updated CLI examples to handle `Result` from `Engine::new()`; all 162 existing tests pass; re-exported `EngineError`, `CoreError`, `AudioIoError` from respective crate roots; begins Phase 7 (Polish &amp; Web Readiness)
- **Completed #7** (WAV file writing/export) &mdash; added `save_wav(path, buffer, format)` function and `WavFormat` enum (`I16`, `I24`, `F32`) in `crates/file-io/src/wav.rs`; f32 samples clamped to [-1.0, 1.0] before conversion; 16-bit writes via `f64::from(sample) * i16::MAX` with rounding; 24-bit writes via `f64::from(sample) * 8_388_607.0` with rounding; 32-bit float passthrough with clamping; validates non-zero channel count; proper WAV header via hound `WavWriter::create` + `finalize`; exported `save_wav` and `WavFormat` from crate root; 16 new tests: roundtrip tests for all 3 formats in mono and stereo, sine wave roundtrips (i16 and f32), empty buffer, single sample, out-of-range clamping, zero channels error, large buffer (10s stereo), all-formats validity check, `WavFormat` Debug/Eq/Copy trait verification; completes Phase 2 (Audio File I/O) with all 3 issues done
- **Completed #20** (transform/filter control panel) &mdash; created `<TransformPanel />` SolidJS component in `app/src/TransformPanel.tsx` with reactive state management; transform type selector dropdown supporting Identity (None), LowPass, HighPass, BandPass, Gain, and ParametricEq; per-transform parameter controls: cutoff frequency slider (20&ndash;20,000 Hz) for LowPass/HighPass, low/high frequency sliders for BandPass, gain factor slider (0.0&ndash;2.0x) for Gain; parametric EQ per-band controls with frequency slider (20&ndash;20,000 Hz), gain slider (-24 to +24 dB), Q factor slider (0.1&ndash;10.0), band type selector (Peak/Low Shelf/High Shelf), and add/remove band buttons; transform chain mode with toggle switch enabling multiple transforms in sequence, per-entry type selector and parameters, reorder (up/down) and remove buttons; all parameter changes immediately sent to engine via `setTransform()` Tauri command with `TransformSpec` union; chain mode builds `TransformSpec::Chain` with nested transforms; integrated into `ControlPanel.tsx` below source controls with visual separator; consistent dark theme styling matching existing control panel; extends Phase 5 (UI Components)
- **Completed #9** (parametric EQ as SpectralTransform) &mdash; implemented `ParametricEq` struct implementing `SpectralTransform` in `crates/core/src/transform.rs`; `EqBand` struct with `frequency: f32`, `gain_db: f32`, `q: f32`, `band_type: BandType` fields; `BandType` enum with `Peak`, `LowShelf`, `HighShelf` variants; Peak bands apply bell-shaped gain curve using `(f/f0 - f0/f) * Q` bandwidth formula; LowShelf/HighShelf use sigmoid-based smooth transitions controlled by Q; multiple bands combine multiplicatively (summed in dB domain); DC bin left untouched; added `TransformSpec::ParametricEq { bands: Vec<EqBand> }` variant in `crates/engine/src/params.rs` with full serde support; wired `build_transform()` in `processor.rs` to construct `ParametricEq` from spec; re-exported `BandType`, `EqBand`, `ParametricEq` from `fourier-core` and `fourier-engine` crate roots; added `BandType`, `EqBand`, and `ParametricEq` TypeScript types in `app/src/bindings.ts`; 15 new unit tests in `fourier-core`: peak boost/cut/bell shape, Q bandwidth control, low shelf boost/cut, high shelf boost/cut, multi-band combination, zero gain unity, empty bands identity, DC untouched, name check; 7 new tests in `fourier-engine`: serde roundtrips for ParametricEq spec (single, empty, in chain), EqBand, BandType variants, ParamMessage with ParametricEq; 2 integration tests: engine processes audio through parametric EQ, rapid transform switching; begins Phase 3 (Enhanced DSP)

### 2026-02-08
- **Completed #19** (transport and source control panel) &mdash; created `<ControlPanel />` SolidJS component in `app/src/ControlPanel.tsx` with reactive state management; source selector with 2x2 radio button grid switching between Live Input, Oscillator, Noise, and File sources; oscillator sub-panel with waveform dropdown (Sine/Square/Sawtooth/Triangle) and frequency slider (20&ndash;20,000 Hz) sending `setSourceOscillator()` on change; noise sub-panel with type dropdown (White/Pink) sending `setSourceNoise()` on change; file sub-panel with native WAV file picker via `@tauri-apps/plugin-dialog` `open()`, seek position slider (0.0&ndash;1.0) sending `seekSource()`, and loop checkbox sending `setSourceFile()` reload; engine start/stop button toggling `startEngine(44100, 2048)` / `stopEngine()` with visual state (green vs red); master gain slider (0&ndash;100%) sending `setGain()` in real-time; all controls auto-apply to running engine on parameter change; error display bar (click to dismiss); added `seek_source` Tauri command in `app/src-tauri/src/lib.rs` wrapping `engine.seek()`; registered `tauri-plugin-dialog` with capability permissions (`dialog:default`, `dialog:allow-open`); added `@tauri-apps/plugin-dialog` npm dependency and `tauri-plugin-dialog` Cargo dependency; updated `App.tsx` with side-by-side layout (`app-body` flex container) placing `<SpectrumAnalyzer />` and `<ControlPanel />` horizontally; updated `styles.css` with 280px fixed-width control panel, dark surface theme, styled range inputs, radio button grid, field layouts, and responsive file picker; completes the MVP critical path (all 9 critical-path issues now done)
- **Completed #17** (spectrum analyzer visualization with WebGL) &mdash; created `<SpectrumAnalyzer />` SolidJS component in `app/src/SpectrumAnalyzer.tsx` with WebGL-based real-time spectrum rendering at 60fps; log frequency axis mapping 20Hz&ndash;20kHz with labeled ticks at standard audio frequencies (20, 50, 100, 200, 500, 1k, 2k, 5k, 10k, 20k Hz); dB magnitude axis mapping -90dB to 0dB with labeled ticks; three configurable render modes: line (LINE_STRIP), filled (TRIANGLE_STRIP with semi-transparent fill + line overlay), and bars (individual quads per bin); peak hold markers with 1s hold time and 40dB/s decay rate; responsive to container/window resize via ResizeObserver with devicePixelRatio-aware scaling; 2D canvas overlay for axis grid lines and labels; graceful fallback message when WebGL is unavailable; `app/src/webgl-renderer.ts` manages WebGL shaders, buffers, and draw calls with reusable pre-allocated buffers for minimal GC pressure; `app/src/spectrum-utils.ts` provides frequency/magnitude mapping utilities (`freqToNorm`, `dbToNorm`, `binToFreq`) and vertex builders for all three render modes; render mode selector UI in App header with line/filled/bars toggle buttons; `requestAnimationFrame`-paced polling loop in `App.tsx` calling `getSpectrum()` for ~60fps data updates; updated `styles.css` with full-viewport layout, stacked canvas positioning, and dark-themed mode selector; begins Phase 5 (UI Components)
- **Completed #16** (Tauri commands for sound source selection) &mdash; added `fourier-core` and `fourier-file-io` dependencies to `fourier-app`; implemented 4 Tauri commands in `app/src-tauri/src/lib.rs`: `set_source_live_input()` switches to microphone/line-in, `set_source_oscillator(waveform, frequency)` creates oscillator source with waveform string validation (Sine/Square/Sawtooth/Triangle) and positive frequency check, `set_source_noise(noise_type)` creates noise source with type string validation (White/Pink), `set_source_file(path, looping)` loads WAV via `fourier_file_io::load_wav()` and wraps in `Arc<AudioBuffer>` for engine playback; all commands validate engine is running and return descriptive errors for invalid parameters; registered all 4 commands in the Tauri invoke handler; added TypeScript wrappers `setSourceLiveInput()`, `setSourceOscillator(waveform, frequency)`, `setSourceNoise(noiseType)`, `setSourceFile(path, looping)` in `app/src/bindings.ts` with typed `WaveformType` and `NoiseType` parameters; completes Phase 4 (Tauri App Setup) with all 4 issues done
- **Completed #8** (loaded audio file as engine source) &mdash; added `fourier-file-io` dependency to `fourier-engine`; added `SourceSpec::AudioBuffer { buffer: Option<Arc<AudioBuffer>>, looping: bool }` variant with `#[serde(skip)]` on the buffer field (runtime-only handle, not serializable); added `ParamMessage::Seek(f32)` for normalized position control (0.0 = start, 1.0 = end); implemented `AudioBufferSource` struct with position tracking, seamless looping (wraps to start at buffer end), mono mixdown for multi-channel buffers via channel averaging, and `seek()` method with clamping; added `seek()` default method to `AudioSource` trait (no-op for non-seekable sources); integrated Seek handling in the processing loop; added `Engine::seek()` convenience method; custom `PartialEq` for `SourceSpec` using `Arc::ptr_eq` for buffer comparison; 17 new tests: 11 unit tests (mono playback, stereo mixdown, looping, seek, seek clamping, empty buffer, build_source for AudioBuffer), 4 integration tests (buffer through OLA pipeline, looping continuity, seek, rapid source switching), 3 serde roundtrip tests (AudioBuffer variant, Seek message, buffer skip verification)

### 2026-02-07
- **Completed #15** (spectral snapshot streaming to frontend) &mdash; added `timestamp_ms` field (milliseconds since Unix epoch) to `SpectralSnapshot` struct in `crates/engine/src/processor.rs`; added `Engine::latest_snapshot()` method that drains all pending snapshots and returns only the most recent one, preventing stale data accumulation when the frontend polls slower than the engine produces; added `get_spectrum` Tauri command in `app/src-tauri/src/lib.rs` returning `Option<SpectralSnapshot>` — returns `null` gracefully when engine is not running or no snapshot is available; registered `get_spectrum` in the Tauri handler list; added TypeScript types `SpectralPeak` and `SpectralSnapshot` interfaces and `getSpectrum()` async wrapper in `app/src/bindings.ts`; design ensures audio thread is never blocked (non-blocking `try_recv` drain loop), bounded channel (capacity 4) drops old snapshots when UI can't keep up, and Mutex lock is held only briefly for the drain operation
- **Completed #14** (Tauri commands for engine lifecycle) &mdash; added `fourier-engine` and `fourier-audio-io` dependencies to `fourier-app`; implemented 6 Tauri commands in `app/src-tauri/src/lib.rs`: `start_engine(sample_rate, fft_size)` initializes engine with default audio devices and OLA processing, `stop_engine()` cleanly shuts down engine and releases streams, `set_transform(spec)` sends `TransformSpec` to processing thread, `set_gain(gain)` sets master output gain, `set_bypass(bypass)` toggles processing bypass, `get_devices()` lists available audio output devices; engine state managed via `Mutex<Option<EngineState>>` in Tauri managed state; `EngineState` holds `Engine` + `AudioStream` handles; `DeviceInfo` serializable struct for frontend; TypeScript type bindings in `app/src/bindings.ts` with typed wrappers for all commands, `TransformSpec`/`SourceSpec`/`DeviceInfo` types mirroring Rust serde representations; `lib.rs` exposes `run()` function called from `main.rs`
- **Completed #22** (serde serialization for TransformSpec and SourceSpec) &mdash; added `serde` and `serde_json` to workspace dependencies; derived `Serialize`/`Deserialize` on `WaveformType` and `SpectralPeak` in `fourier-core`; derived on `EngineParams`, `NoiseType`, `Partial`, `SourceSpec`, `TransformSpec`, `ParamMessage`, and `SpectralSnapshot` in `fourier-engine`; used `#[serde(tag = "type")]` for internally tagged `SourceSpec` and `#[serde(tag = "type", content = "value")]` for adjacently tagged `TransformSpec` and `ParamMessage`; re-exported `TransformSpec` and `SpectralSnapshot` from engine crate root; added `PartialEq` to `TransformSpec`; 18 roundtrip tests covering all types, nested chains, and human-readable JSON verification
- **Completed #13** (Tauri v2 desktop application with SolidJS) &mdash; created `app/` directory with Tauri v2 + SolidJS frontend; `app/src-tauri/Cargo.toml` depends on `fourier-engine` via workspace; Tauri config with 1200x800 window titled "Fourier-RS"; SolidJS frontend with Vite dev server (port 3000) and hot reload; dark-themed landing page; placeholder RGBA icons for macOS/Windows; `pnpm dev`/`pnpm build` scripts; added `app/src-tauri` to workspace members; `serde`, `serde_json`, `tauri`, and `tauri-build` as workspace dependencies

### 2026-02-06
- **Completed #6** (fourier-file-io crate with WAV reading) &mdash; created new `crates/file-io/` crate with `hound` dependency; `AudioBuffer` struct with `samples: Vec<f32>`, `sample_rate: u32`, `channels: u16` and `num_frames()`/`duration_secs()` methods; `load_wav(path)` supporting 16-bit int, 24-bit int, and 32-bit float WAV formats with normalization to [-1.0, 1.0]; mono and stereo interleaved support; `FileIoError` enum with `FileNotFound`, `InvalidFormat`, `UnsupportedFormat`, `Io` variants; 21 unit tests covering all formats, stereo interleaving, clamping, error cases, and normalization accuracy
- **Completed #5** (engine source integration) &mdash; added `SourceSpec` enum (`LiveInput`, `Oscillator`, `Noise`, `Additive`) and `ParamMessage::SetSource` in `crates/engine/src/params.rs`; added `NoiseType` and `Partial` types; created `AudioSource` trait and implementations (`OscillatorSource`, `WhiteNoiseSource`, `PinkNoiseSource`, `AdditiveSource`) in new `crates/engine/src/source.rs`; modified processing loop in `processor.rs` to generate samples from active source instead of only reading mic input; white noise via xorshift64 PRNG, pink noise via Voss-McCartney algorithm; 20 new tests including integration tests for oscillator/noise/additive through OLA pipeline and spectral verification
- **Completed #2** (oscillator module) &mdash; added `Oscillator` struct and `WaveformType` enum (Sine, Square, Sawtooth, Triangle) in `crates/core/src/oscillator.rs`; phase-continuous sample generation with `generate(&mut self, output: &mut [f32])`; re-exported from crate root; 13 unit tests including FFT spectral verification, phase continuity, amplitude bounds, and harmonic content validation
- **Completed #30** (dev infrastructure) &mdash; added `rust-toolchain.toml` (1.93.0), `rustfmt.toml`, `[workspace.lints]` (clippy pedantic + nursery), `.editorconfig`, `deny.toml`, `Justfile`, GitHub Actions CI (`ci.yml`), CI badge in README; fixed all clippy warnings and formatted workspace
- **Added #30** (dev infrastructure) &mdash; linting, formatting, testing, CI setup; critical priority, Phase 7
- Updated issue counts: 27 &rarr; 28 total, 8 &rarr; 9 critical
- Moved #30 to Batch 1 position 1 (establish conventions before new code)
- **Created initial roadmap** with 27 issues across 7 phases
- Phase 1: Sound generation (4 issues) &mdash; oscillators, noise, additive, engine integration
- Phase 2: Audio file I/O (3 issues) &mdash; WAV read/write, file playback
- Phase 3: Enhanced DSP (4 issues) &mdash; parametric EQ, delay, freeze, pitch shift
- Phase 4: Tauri app setup (4 issues) &mdash; scaffold, commands, streaming, source selection
- Phase 5: UI components (5 issues) &mdash; spectrum, waveform, controls, tuner
- Phase 6: Workflow (3 issues) &mdash; serde, presets, export
- Phase 7: Polish & web (4 issues) &mdash; error handling, CI, WASM, WebAudio
- Defined MVP critical path: #2 &rarr; #5 &rarr; #13 &rarr; #22 &rarr; #14 &rarr; #15/#16 &rarr; #17/#19
- Estimated ~8 weeks for full roadmap completion
