# Fourier-RS Roadmap

> **Last updated:** 2026-02-06
>
> Real-time audio DSP framework &rarr; Tauri desktop synthesizer with Fourier analysis

---

## Open Issues Summary

**27 open issues** across 7 phases (1 completed)

| Priority | Count | Issues |
|----------|-------|--------|
| :red_circle: Critical | 8 | #2, #5, #13, #14, #15, #16, #17, #19 |
| :yellow_circle: High | 9 | #6, #7, #8, #9, #18, #20, #22, #25, #26 |
| :green_circle: Medium | 7 | #3, #4, #11, #12, #21, #23, #24 |
| :large_blue_circle: Low | 3 | #10, #27, #28 |

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

---

## Phase 1: Sound Generation

> **Goal:** Enable the engine to produce sound, not just process mic input
>
> **Effort:** ~1 week &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #2 | Add oscillator module with standard waveforms | :red_circle: Critical | ~1 day | &mdash; |
| #3 | Add noise generators (white, pink) | :green_circle: Medium | ~1 day | &mdash; |
| #4 | Add additive synthesis module | :green_circle: Medium | ~1 day | #2 |
| #5 | Integrate sound generation into engine as audio source | :red_circle: Critical | ~2 days | #2 |

**Key deliverables:**
- `Oscillator` with Sine, Square, Sawtooth, Triangle waveforms
- `NoiseGenerator` with White and Pink noise (Voss-McCartney)
- `AdditiveSynth` with per-partial control
- `SourceSpec` enum and `ParamMessage::SetSource` in engine

---

## Phase 2: Audio File I/O

> **Goal:** Load and save WAV files, play files through the engine
>
> **Effort:** ~1 week &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #6 | Create fourier-file-io crate with WAV reading | :yellow_circle: High | ~2 days | &mdash; |
| #7 | Add WAV file writing/export | :yellow_circle: High | ~1 day | #6 |
| #8 | Add loaded audio file as engine source | :yellow_circle: High | ~2 days | #5, #6 |

**Key deliverables:**
- New `crates/file-io/` crate using `hound`
- `AudioBuffer` struct, `load_wav()`, `save_wav()`
- `SourceSpec::AudioBuffer` variant with looping and seek

---

## Phase 3: Enhanced DSP

> **Goal:** Richer spectral processing: EQ, delay, freeze, pitch shift
>
> **Effort:** ~2 weeks &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #9 | Add parametric EQ as SpectralTransform | :yellow_circle: High | ~2 days | &mdash; |
| #10 | Add spectral delay effect | :large_blue_circle: Low | ~2 days | &mdash; |
| #11 | Add spectral freeze/hold effect | :green_circle: Medium | ~1 day | &mdash; |
| #12 | Add pitch shifting via spectral bin rotation | :green_circle: Medium | ~2 days | &mdash; |

**Key deliverables:**
- `ParametricEq` with Peak/LowShelf/HighShelf bands
- `SpectralDelay` with per-band ring buffers
- `SpectralFreeze` with smooth crossfade toggle
- `PitchShift` with linear interpolation for fractional shifts

---

## Phase 4: Tauri App Setup

> **Goal:** Desktop application shell with engine control from SolidJS frontend
>
> **Effort:** ~2 weeks &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #13 | Scaffold Tauri v2 desktop application with SolidJS | :red_circle: Critical | ~2 days | &mdash; |
| #14 | Implement Tauri commands for engine lifecycle | :red_circle: Critical | ~2 days | #13 |
| #15 | Implement spectral snapshot streaming to frontend | :red_circle: Critical | ~1 day | #14 |
| #16 | Implement Tauri commands for sound source selection | :red_circle: Critical | ~1 day | #5, #8, #14 |

**Key deliverables:**
- `app/` directory with Tauri v2 + SolidJS
- Tauri commands: `start_engine`, `stop_engine`, `set_transform`, `get_spectrum`, etc.
- 30-60fps spectral snapshot polling
- Source selection commands (live, oscillator, noise, file)

---

## Phase 5: UI Components

> **Goal:** Visual interface for spectrum analysis, waveform display, and controls
>
> **Effort:** ~3 weeks &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #17 | Build spectrum analyzer visualization with WebGL | :red_circle: Critical | ~3 days | #15 |
| #18 | Build waveform/oscilloscope display | :yellow_circle: High | ~2 days | #14, #15 |
| #19 | Build transport and source control panel | :red_circle: Critical | ~2 days | #14, #16 |
| #20 | Build transform/filter control panel | :yellow_circle: High | ~2 days | #9, #14 |
| #21 | Build peak frequency and note detection display | :green_circle: Medium | ~1 day | #15 |

**Key deliverables:**
- `<SpectrumAnalyzer />` &mdash; WebGL, log freq axis, dB magnitude, peak markers
- `<WaveformDisplay />` &mdash; WebGL oscilloscope with zero-crossing trigger
- `<ControlPanel />` &mdash; source selector, oscillator/noise/file controls, gain
- `<TransformPanel />` &mdash; EQ bands, pitch shift, freeze toggle, chain management
- `<TunerDisplay />` &mdash; peak frequency, note name, cents deviation

---

## Phase 6: Sound Design Workflow

> **Goal:** Presets, export, and serialization for productive sound design
>
> **Effort:** ~1.5 weeks &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #22 | Add serde serialization to TransformSpec and SourceSpec | :yellow_circle: High | ~1 day | &mdash; |
| #23 | Add preset save/load system | :green_circle: Medium | ~2 days | #22, #14 |
| #24 | Add audio export (render engine output to WAV) | :green_circle: Medium | ~2 days | #7, #8, #14 |

**Key deliverables:**
- Serde `Serialize`/`Deserialize` on all param types
- `Preset` struct with save/load to JSON, factory presets
- `export_audio` command with offline rendering and progress reporting

---

## Phase 7: Polish & Web Readiness

> **Goal:** Production quality, CI, and browser-ready WASM
>
> **Effort:** ~2 weeks &bull; **Status:** Not started

| # | Title | Priority | Effort | Dependencies |
|---|-------|----------|--------|--------------|
| #25 | Add error handling (thiserror) and logging (tracing) | :yellow_circle: High | ~2 days | &mdash; |
| #26 | Add GitHub Actions CI pipeline | :yellow_circle: High | ~1 day | &mdash; |
| #27 | Compile fourier-core to WASM target | :large_blue_circle: Low | ~2 days | #2 |
| #28 | Add WebAudio integration layer | :large_blue_circle: Low | ~3 days | #27 |
| #30 | ~~Set up development infrastructure (linting, formatting, testing, CI)~~ | :white_check_mark: Done | ~2 days | &mdash; |

**Key deliverables:**
- `EngineError` enum via `thiserror`, `tracing` instrumentation
- CI: build, test, clippy, fmt on macOS (stable + nightly)
- WASM build of fourier-core with `wasm-bindgen` exports
- AudioWorklet integration with example web page
- ~~Dev infrastructure: `rust-toolchain.toml`, `rustfmt.toml`, workspace lints, `.editorconfig`, `deny.toml`, `Justfile`~~ :white_check_mark:

---

## MVP Critical Path

The minimum viable product requires completing these issues in order:

```
#2 Oscillators
 └─► #5 Engine source integration
      └─► #13 Tauri scaffold
           └─► #22 Serde serialization
                └─► #14 Engine lifecycle commands
                     ├─► #15 Spectral streaming ──► #17 Spectrum viz
                     └─► #16 Source commands ──────► #19 Control panel
```

**Critical path issues:** #2 &rarr; #5 &rarr; #13 &rarr; #22 &rarr; #14 &rarr; #15 &rarr; #16 &rarr; #17 &rarr; #19

All 9 critical-path issues are labeled `:red_circle: Critical` or `:yellow_circle: High`.

---

## Current Sprint

### NOW (Phase 1 Start)

Start with dev infrastructure and the two critical Phase 1 issues that unblock everything:

1. ~~**#30** &mdash; Set up development infrastructure (linting, formatting, testing, CI)~~ `:white_check_mark:`
2. **#2** &mdash; Add oscillator module with standard waveforms `:red_circle:`
3. **#5** &mdash; Integrate sound generation into engine as audio source `:red_circle:`

### NEXT UP

Once Phase 1 criticals are done:

4. **#6** &mdash; Create fourier-file-io crate with WAV reading `:yellow_circle:`
5. **#13** &mdash; Scaffold Tauri v2 desktop application with SolidJS `:red_circle:`
6. **#22** &mdash; Add serde serialization `:yellow_circle:`
7. **#25** &mdash; Add error handling and logging `:yellow_circle:`

### PARALLEL TRACKS

These can proceed independently alongside the critical path:

- **DSP track:** #9 (Parametric EQ), #11 (Freeze), #12 (Pitch shift)
- **File I/O track:** #6, #7 (WAV read/write)
- **Polish track:** #25, #26 (error handling, CI)

---

## Recommended Implementation Order

### Batch 1: Foundations (Week 1)
| Order | Issue | Rationale |
|-------|-------|-----------|
| ~~1~~ | ~~#30 Dev infrastructure~~ | ~~Establish linting, formatting, CI before new code~~ :white_check_mark: |
| 2 | #2 Oscillator module | Unblocks all sound generation |
| 3 | #5 Engine source integration | Connects generators to pipeline |
| 4 | #3 Noise generators | Parallel with #5, simple module |

### Batch 2: File I/O + DSP (Week 2)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 5 | #6 WAV reading | Unblocks file playback |
| 6 | #7 WAV writing | Small addition to #6 |
| 7 | #9 Parametric EQ | Key DSP feature |
| 8 | #4 Additive synthesis | Depends on #2, enriches sources |

### Batch 3: App Shell (Week 3)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 9 | #13 Tauri scaffold | Unblocks all frontend work |
| 10 | #22 Serde serialization | Needed for Tauri IPC |
| 11 | #25 Error handling | Clean up before more code |
| 12 | #8 Audio file source | Depends on #5, #6 |

### Batch 4: Engine Commands (Week 4)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 13 | #14 Engine lifecycle commands | Core Tauri API |
| 14 | #15 Spectral streaming | Enables visualization |
| 15 | #16 Source selection commands | Enables source UI |

### Batch 5: UI (Weeks 5-6)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 16 | #17 Spectrum analyzer (WebGL) | Flagship visualization |
| 17 | #19 Transport/source control panel | Core user interaction |
| 18 | #18 Waveform display | Second visualization |
| 19 | #20 Transform control panel | Effect parameter UI |
| 20 | #21 Note detection display | Tuner feature |

### Batch 6: Workflow + DSP Extras (Week 7)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 21 | #23 Preset system | UX polish |
| 22 | #24 Audio export | Render to file |
| 23 | #11 Spectral freeze | Creative effect |
| 24 | #12 Pitch shifting | Creative effect |

### Batch 7: Future (Week 8+)
| Order | Issue | Rationale |
|-------|-------|-----------|
| 25 | #10 Spectral delay | Niche effect |
| 26 | #26 CI pipeline (if not covered by #30) | Additional CI beyond #30 |
| 27 | #27 WASM compilation | Web readiness |
| 28 | #28 WebAudio integration | Browser demo |

---

## Issue Status Summary

| Phase | Total | Critical | High | Medium | Low | Done |
|-------|-------|----------|------|--------|-----|------|
| 1 &mdash; Sound Gen | 4 | 2 | 0 | 2 | 0 | 0 |
| 2 &mdash; File I/O | 3 | 0 | 3 | 0 | 0 | 0 |
| 3 &mdash; DSP | 4 | 0 | 1 | 2 | 1 | 0 |
| 4 &mdash; Tauri | 4 | 4 | 0 | 0 | 0 | 0 |
| 5 &mdash; UI | 5 | 2 | 2 | 1 | 0 | 0 |
| 6 &mdash; Workflow | 3 | 0 | 1 | 2 | 0 | 0 |
| 7 &mdash; Polish/Web | 5 | 0 | 2 | 0 | 2 | 1 |
| **Total** | **28** | **8** | **9** | **7** | **3** | **1** |

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

### 2026-02-06
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
