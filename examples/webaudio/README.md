# WebAudio Integration Demo

Real-time spectral audio processing in the browser using fourier-core compiled to WASM, running inside an AudioWorklet.

## Pipeline

```
Microphone → getUserMedia → AudioWorklet (WASM OLA processor) → Speakers
                                    ↓
                            Spectrum snapshots → Canvas visualization
```

## Features

- **AudioWorklet processor** loads and runs the WASM module in the audio thread
- **Microphone input** via `getUserMedia` (echo cancellation disabled for clean signal)
- **Real-time spectral processing** through the WASM OLA (overlap-add) processor
- **Audio output** to speakers via `AudioContext.destination`
- **Spectrum visualization** with log-frequency axis (20 Hz–20 kHz) and dB magnitude
- **Transform controls**: identity, low-pass, high-pass, band-pass, gain, pitch shift, spectral delay
- **Configurable FFT**: size (512–4096) and window function (Hann, Hamming, Blackman, Rectangular)
- **JS fallback mode**: works without WASM using the native `AnalyserNode` for visualization

## Prerequisites

- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```sh
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

## Build

```sh
cd examples/webaudio
./build.sh
```

Or manually:

```sh
wasm-pack build crates/core \
  --target no-modules \
  --features wasm \
  --out-dir ../../examples/webaudio/pkg \
  --out-name fourier_core
```

## Run

Serve the directory with any HTTP server (required for WASM and AudioWorklet):

```sh
cd examples/webaudio
python3 -m http.server 8080
```

Then open http://localhost:8080 in Chrome or Firefox.

## Browser Compatibility

- **Chrome 66+**: Full AudioWorklet + WASM support
- **Firefox 76+**: Full AudioWorklet + WASM support
- **Safari 14.1+**: AudioWorklet supported; WASM in worklet may require feature flags

## Architecture

### Main Thread (`index.html`)
- Creates `AudioContext` and requests microphone access
- Fetches the wasm-bindgen glue JS and WASM binary
- Bundles glue JS + worklet processor JS into a single blob URL
- Loads the combined script as an `AudioWorklet` module
- Sends the WASM binary to the worklet for initialization
- Receives spectrum snapshots and renders them on a canvas

### Audio Thread (`fourier-worklet.js`)
- Runs as an `AudioWorkletProcessor` in the audio rendering thread
- Initializes `wasm_bindgen` with the WASM binary received from the main thread
- Creates a `WasmOverlapAddProcessor` for streaming FFT-based spectral processing
- Processes 128-sample audio frames through the WASM OLA pipeline
- Supports dynamic transform switching via message passing
- Posts spectrum snapshots to the main thread for visualization

### Message Protocol

**Main → Worklet:**
| Message | Fields | Description |
|---------|--------|-------------|
| `init-wasm` | `wasmBytes`, `fftSize`, `hopSize`, `windowType` | Initialize WASM and create OLA processor |
| `setTransform` | `transform`, `params` | Change the spectral transform |
| `bypass` | `value` | Toggle audio bypass |

**Worklet → Main:**
| Message | Fields | Description |
|---------|--------|-------------|
| `ready` | — | WASM initialized successfully |
| `spectrum` | `data` (Float32Array, transferable) | Interleaved complex spectrum snapshot |
| `error` | `message` | Error description |

## Files

| File | Description |
|------|-------------|
| `index.html` | Main page with UI, visualization, and AudioContext setup |
| `fourier-worklet.js` | AudioWorkletProcessor with WASM OLA integration |
| `build.sh` | Build script (wasm-pack wrapper) |
| `pkg/` | Generated WASM package (after build) |
