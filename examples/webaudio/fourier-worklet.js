/**
 * AudioWorklet processor wrapping fourier-core WASM for real-time spectral DSP.
 *
 * Architecture:
 * - The main thread bundles this file with the wasm-bindgen glue JS and loads
 *   the combined script as an AudioWorklet module.
 * - On receiving an 'init-wasm' message with the WASM binary, this processor
 *   initializes wasm_bindgen and creates a WasmOverlapAddProcessor.
 * - Transform changes arrive as messages; spectrum snapshots are posted back.
 *
 * Message protocol (main -> worklet):
 *   { type: 'init-wasm', wasmBytes: ArrayBuffer, fftSize, hopSize, windowType }
 *   { type: 'setTransform', transform: string, params: object }
 *   { type: 'bypass', value: bool }
 *
 * Message protocol (worklet -> main):
 *   { type: 'ready' }
 *   { type: 'spectrum', data: Float32Array }  (transferable)
 *   { type: 'error', message: string }
 */

/* global registerProcessor, AudioWorkletProcessor, sampleRate, wasm_bindgen */

class FourierProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.olaProcessor = null;
    this.ready = false;
    this.bypassed = false;
    this.spectrumCounter = 0;
    // Post spectrum ~5 times/sec. At 48 kHz with 128-sample quanta: 375 quanta/sec.
    this.spectrumInterval = 75;

    this.port.onmessage = (event) => this._handleMessage(event.data);
  }

  async _handleMessage(msg) {
    switch (msg.type) {
      case 'init-wasm':
        await this._initWasm(msg.wasmBytes, msg.fftSize || 2048, msg.hopSize, msg.windowType || 'hann');
        break;
      case 'setTransform':
        this._setTransform(msg.transform, msg.params || {});
        break;
      case 'bypass':
        this.bypassed = !!msg.value;
        break;
    }
  }

  async _initWasm(wasmBytes, fftSize, hopSize, windowType) {
    try {
      // wasm_bindgen is a global function injected by the no-modules glue JS
      // that was bundled into this worklet script by the main thread.
      if (typeof wasm_bindgen === 'undefined') {
        this.port.postMessage({
          type: 'error',
          message: 'wasm_bindgen not available in worklet scope',
        });
        return;
      }

      // Initialize the WASM module with the binary data.
      await wasm_bindgen(wasmBytes);

      // Create the OLA processor.
      const hop = hopSize || Math.floor(fftSize / 2);
      const sr = sampleRate; // AudioWorklet global

      this.olaProcessor = new wasm_bindgen.WasmOverlapAddProcessor(
        fftSize, hop, windowType || 'hann', sr,
      );

      this.ready = true;
      this.port.postMessage({ type: 'ready' });
    } catch (e) {
      this.port.postMessage({ type: 'error', message: `WASM init failed: ${e.message}` });
    }
  }

  _setTransform(transformType, params) {
    if (!this.olaProcessor) return;

    let transform;
    try {
      switch (transformType) {
        case 'identity':
          transform = wasm_bindgen.WasmTransform.identity();
          break;
        case 'lowPass':
          transform = wasm_bindgen.WasmTransform.lowPass(params.cutoff ?? 2000);
          break;
        case 'highPass':
          transform = wasm_bindgen.WasmTransform.highPass(params.cutoff ?? 200);
          break;
        case 'bandPass':
          transform = wasm_bindgen.WasmTransform.bandPass(params.low ?? 200, params.high ?? 2000);
          break;
        case 'gain':
          transform = wasm_bindgen.WasmTransform.gain(params.gain ?? 1.0);
          break;
        case 'pitchShift':
          transform = wasm_bindgen.WasmTransform.pitchShift(params.semitones ?? 0);
          break;
        case 'spectralFreeze':
          transform = wasm_bindgen.WasmTransform.spectralFreeze(
            !!params.frozen, sampleRate, params.hopSize || 1024,
          );
          break;
        case 'spectralDelay':
          transform = wasm_bindgen.WasmTransform.spectralDelay(
            params.delayFrames ?? 4, params.feedback ?? 0.5, params.mix ?? 0.5,
          );
          break;
        default:
          return;
      }
      this.olaProcessor.setTransform(transform);
    } catch (e) {
      this.port.postMessage({ type: 'error', message: `Transform error: ${e.message}` });
    }
  }

  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];

    if (!input || !input[0] || input[0].length === 0) {
      return true;
    }

    // Bypass mode or not yet initialized: pass audio through unchanged.
    if (this.bypassed || !this.olaProcessor) {
      for (let ch = 0; ch < output.length; ch++) {
        if (input[ch]) {
          output[ch].set(input[ch]);
        }
      }
      return true;
    }

    // Process mono (channel 0) through the OLA WASM processor.
    const inputSamples = input[0];
    this.olaProcessor.pushSamples(inputSamples);

    const processed = this.olaProcessor.pullSamples(inputSamples.length);

    // Write to all output channels.
    for (let ch = 0; ch < output.length; ch++) {
      if (processed.length >= inputSamples.length) {
        output[ch].set(processed.subarray(0, inputSamples.length));
      } else {
        // Not enough output yet (OLA pipeline latency); output silence.
        output[ch].fill(0);
      }
    }

    // Periodically send spectrum snapshot to main thread for visualization.
    this.spectrumCounter++;
    if (this.spectrumCounter >= this.spectrumInterval) {
      this.spectrumCounter = 0;
      try {
        const spectrum = this.olaProcessor.latestSpectrum();
        if (spectrum && spectrum.length > 0) {
          // Transfer ownership for zero-copy.
          this.port.postMessage(
            { type: 'spectrum', data: spectrum },
            [spectrum.buffer],
          );
        }
      } catch {
        // Visualization is non-critical; ignore errors.
      }
    }

    return true;
  }
}

registerProcessor('fourier-processor', FourierProcessor);
