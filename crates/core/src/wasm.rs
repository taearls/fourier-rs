//! WASM bindings for `fourier-core` DSP operations.
//!
//! Provides JavaScript-callable wrappers around the core DSP types.
//! All types use `wasm_bindgen` for seamless interop with JS/TS.
//!
//! Activated by the `wasm` feature flag:
//! ```sh
//! cargo build --target wasm32-unknown-unknown -p fourier-core --features wasm
//! ```

// wasm_bindgen generates non-const glue code, so const fn is not applicable.
#![allow(clippy::missing_const_for_fn, clippy::new_without_default)]

use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// FFT Processor
// ---------------------------------------------------------------------------

/// Real-valued FFT/IFFT processor for spectral analysis.
///
/// Wraps the inner `FftProcessor`, exposing forward/inverse transforms that
/// operate on `Float32Array` buffers passed from JavaScript.
#[wasm_bindgen]
pub struct WasmFftProcessor {
    inner: crate::fft::FftProcessor,
    /// Pre-allocated complex spectrum buffer (interleaved [re, im, re, im, …]).
    spectrum_buf: Vec<f32>,
}

#[wasm_bindgen]
impl WasmFftProcessor {
    /// Create a new FFT processor for the given frame size.
    ///
    /// `fft_size` must be even (power of 2 recommended).
    #[wasm_bindgen(constructor)]
    pub fn new(fft_size: usize) -> Self {
        let inner = crate::fft::FftProcessor::new(fft_size);
        let n_bins = inner.complex_bins();
        Self {
            inner,
            spectrum_buf: vec![0.0; n_bins * 2],
        }
    }

    /// Number of complex frequency bins (N/2 + 1).
    #[wasm_bindgen(getter, js_name = "complexBins")]
    pub fn complex_bins(&self) -> usize {
        self.inner.complex_bins()
    }

    /// The FFT frame size.
    #[wasm_bindgen(getter, js_name = "fftSize")]
    pub fn fft_size(&self) -> usize {
        self.inner.fft_size()
    }

    /// Perform forward FFT on time-domain samples.
    ///
    /// `input` must have length `fft_size`. Returns interleaved complex
    /// spectrum as `Float32Array` of length `2 * (fft_size/2 + 1)`:
    /// `[re0, im0, re1, im1, …]`.
    pub fn forward(&mut self, input: &mut [f32]) -> Result<Vec<f32>, JsError> {
        let n_bins = self.inner.complex_bins();
        let mut spectrum = self.inner.alloc_spectrum();
        self.inner
            .forward(input, &mut spectrum)
            .map_err(|e| JsError::new(&e.to_string()))?;

        // Pack complex values into interleaved f32 array.
        self.spectrum_buf.resize(n_bins * 2, 0.0);
        for (i, c) in spectrum.iter().enumerate() {
            self.spectrum_buf[i * 2] = c.re;
            self.spectrum_buf[i * 2 + 1] = c.im;
        }
        Ok(self.spectrum_buf.clone())
    }

    /// Perform inverse FFT on a complex spectrum.
    ///
    /// `spectrum` must be interleaved `[re0, im0, re1, im1, …]` with length
    /// `2 * (fft_size/2 + 1)`. Returns time-domain `Float32Array` of length
    /// `fft_size`, normalized so that forward→inverse is identity.
    pub fn inverse(&mut self, spectrum: &[f32]) -> Result<Vec<f32>, JsError> {
        let n_bins = self.inner.complex_bins();
        if spectrum.len() != n_bins * 2 {
            return Err(JsError::new(&format!(
                "spectrum length must be {} (2 * {}), got {}",
                n_bins * 2,
                n_bins,
                spectrum.len()
            )));
        }

        let mut complex: Vec<num_complex::Complex<f32>> = spectrum
            .chunks_exact(2)
            .map(|pair| num_complex::Complex::new(pair[0], pair[1]))
            .collect();

        let mut output = self.inner.alloc_time_buffer();
        self.inner
            .inverse(&mut complex, &mut output)
            .map_err(|e| JsError::new(&e.to_string()))?;
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Oscillator
// ---------------------------------------------------------------------------

/// Phase-continuous oscillator generating standard audio waveforms.
#[wasm_bindgen]
pub struct WasmOscillator {
    inner: crate::oscillator::Oscillator,
}

#[wasm_bindgen]
impl WasmOscillator {
    /// Create a new oscillator.
    ///
    /// `waveform`: `"sine"`, `"square"`, `"sawtooth"`, or `"triangle"`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        waveform: &str,
        frequency: f32,
        amplitude: f32,
        sample_rate: f32,
    ) -> Result<Self, JsError> {
        let wt = parse_waveform(waveform)?;
        Ok(Self {
            inner: crate::oscillator::Oscillator::new(wt, frequency, amplitude, sample_rate),
        })
    }

    /// Generate `num_samples` of waveform output.
    pub fn generate(&mut self, num_samples: usize) -> Vec<f32> {
        let mut output = vec![0.0; num_samples];
        self.inner.generate(&mut output);
        output
    }

    /// Set oscillator frequency in Hz.
    #[wasm_bindgen(setter)]
    pub fn set_frequency(&mut self, frequency: f32) {
        self.inner.set_frequency(frequency);
    }

    /// Set output amplitude.
    #[wasm_bindgen(setter)]
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.inner.set_amplitude(amplitude);
    }

    /// Set the waveform type.
    #[wasm_bindgen(setter)]
    pub fn set_waveform(&mut self, waveform: &str) -> Result<(), JsError> {
        self.inner.set_waveform(parse_waveform(waveform)?);
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn frequency(&self) -> f32 {
        self.inner.frequency()
    }

    #[wasm_bindgen(getter)]
    pub fn amplitude(&self) -> f32 {
        self.inner.amplitude()
    }

    #[wasm_bindgen(getter)]
    pub fn phase(&self) -> f32 {
        self.inner.phase()
    }

    #[wasm_bindgen(getter, js_name = "sampleRate")]
    pub fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

fn parse_waveform(s: &str) -> Result<crate::oscillator::WaveformType, JsError> {
    match s.to_ascii_lowercase().as_str() {
        "sine" => Ok(crate::oscillator::WaveformType::Sine),
        "square" => Ok(crate::oscillator::WaveformType::Square),
        "sawtooth" => Ok(crate::oscillator::WaveformType::Sawtooth),
        "triangle" => Ok(crate::oscillator::WaveformType::Triangle),
        _ => Err(JsError::new(&format!(
            "unknown waveform '{s}': expected sine, square, sawtooth, or triangle"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Noise Generator
// ---------------------------------------------------------------------------

/// White and pink noise generator.
#[wasm_bindgen]
pub struct WasmNoiseGenerator {
    inner: crate::noise::NoiseGenerator,
}

#[wasm_bindgen]
impl WasmNoiseGenerator {
    /// Create a new noise generator.
    ///
    /// `noise_type`: `"white"` or `"pink"`.
    #[wasm_bindgen(constructor)]
    pub fn new(noise_type: &str, amplitude: f32, sample_rate: f32) -> Result<Self, JsError> {
        let nt = parse_noise_type(noise_type)?;
        Ok(Self {
            inner: crate::noise::NoiseGenerator::new(nt, amplitude, sample_rate),
        })
    }

    /// Generate `num_samples` of noise.
    pub fn generate(&mut self, num_samples: usize) -> Vec<f32> {
        let mut output = vec![0.0; num_samples];
        self.inner.generate(&mut output);
        output
    }

    #[wasm_bindgen(setter)]
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.inner.set_amplitude(amplitude);
    }

    #[wasm_bindgen(setter, js_name = "noiseType")]
    pub fn set_noise_type(&mut self, noise_type: &str) -> Result<(), JsError> {
        self.inner.set_noise_type(parse_noise_type(noise_type)?);
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn amplitude(&self) -> f32 {
        self.inner.amplitude()
    }

    #[wasm_bindgen(getter, js_name = "sampleRate")]
    pub fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

fn parse_noise_type(s: &str) -> Result<crate::noise::NoiseType, JsError> {
    match s.to_ascii_lowercase().as_str() {
        "white" => Ok(crate::noise::NoiseType::White),
        "pink" => Ok(crate::noise::NoiseType::Pink),
        _ => Err(JsError::new(&format!(
            "unknown noise type '{s}': expected white or pink"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Window Function
// ---------------------------------------------------------------------------

/// Pre-computed window function table.
#[wasm_bindgen]
pub struct WasmWindowFunction {
    inner: crate::window::WindowFunction,
}

#[wasm_bindgen]
impl WasmWindowFunction {
    /// Create a new window function.
    ///
    /// `window_type`: `"hann"`, `"hamming"`, `"blackman"`, or `"rectangular"`.
    #[wasm_bindgen(constructor)]
    pub fn new(window_type: &str, size: usize) -> Result<Self, JsError> {
        let wt = parse_window_type(window_type)?;
        Ok(Self {
            inner: crate::window::WindowFunction::new(wt, size),
        })
    }

    /// Apply the window to a buffer in-place and return it.
    pub fn apply(&self, buffer: &mut [f32]) {
        self.inner.apply(buffer);
    }

    /// Return the window coefficient table.
    pub fn table(&self) -> Vec<f32> {
        self.inner.table().to_vec()
    }

    #[wasm_bindgen(getter)]
    pub fn size(&self) -> usize {
        self.inner.size()
    }

    /// Coherent gain (sum of coefficients / N).
    #[wasm_bindgen(getter, js_name = "coherentGain")]
    pub fn coherent_gain(&self) -> f32 {
        self.inner.coherent_gain()
    }
}

fn parse_window_type(s: &str) -> Result<crate::window::WindowType, JsError> {
    match s.to_ascii_lowercase().as_str() {
        "hann" => Ok(crate::window::WindowType::Hann),
        "hamming" => Ok(crate::window::WindowType::Hamming),
        "blackman" => Ok(crate::window::WindowType::Blackman),
        "rectangular" | "rect" => Ok(crate::window::WindowType::Rectangular),
        _ => Err(JsError::new(&format!(
            "unknown window type '{s}': expected hann, hamming, blackman, or rectangular"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Spectral Analysis Utilities
// ---------------------------------------------------------------------------

/// Compute the magnitude spectrum (linear) from an interleaved complex spectrum.
///
/// Input: `[re0, im0, re1, im1, …]`. Returns magnitudes per bin.
#[wasm_bindgen(js_name = "magnitudeSpectrum")]
pub fn magnitude_spectrum(interleaved_spectrum: &[f32]) -> Vec<f32> {
    interleaved_spectrum
        .chunks_exact(2)
        .map(|pair| pair[0].hypot(pair[1]))
        .collect()
}

/// Compute the magnitude spectrum in dB from an interleaved complex spectrum.
#[wasm_bindgen(js_name = "magnitudeSpectrumDb")]
pub fn magnitude_spectrum_db(interleaved_spectrum: &[f32]) -> Vec<f32> {
    interleaved_spectrum
        .chunks_exact(2)
        .map(|pair| {
            let mag = pair[0].hypot(pair[1]);
            if mag > 0.0 {
                20.0 * mag.log10()
            } else {
                f32::NEG_INFINITY
            }
        })
        .collect()
}

/// Convert a frequency bin index to frequency in Hz.
#[wasm_bindgen(js_name = "binToFrequency")]
pub fn bin_to_frequency(bin_index: usize, sample_rate: f32, fft_size: usize) -> f32 {
    crate::spectral::bin_to_frequency(bin_index, sample_rate, fft_size)
}

/// Convert a frequency in Hz to the nearest bin index.
#[wasm_bindgen(js_name = "frequencyToBin")]
pub fn frequency_to_bin(frequency_hz: f32, sample_rate: f32, fft_size: usize) -> usize {
    crate::spectral::frequency_to_bin(frequency_hz, sample_rate, fft_size)
}

// ---------------------------------------------------------------------------
// Spectral Transforms
// ---------------------------------------------------------------------------

/// A spectral transform that can be applied to frequency-domain data.
///
/// Wraps the various built-in transforms behind a single JS-friendly type.
/// Use the static factory methods to create specific transforms.
#[wasm_bindgen]
pub struct WasmTransform {
    inner: Box<dyn crate::transform::SpectralTransform>,
}

#[wasm_bindgen]
impl WasmTransform {
    /// Create an identity (pass-through) transform.
    pub fn identity() -> Self {
        Self {
            inner: Box::new(crate::transform::IdentityTransform),
        }
    }

    /// Create a brick-wall low-pass filter.
    #[wasm_bindgen(js_name = "lowPass")]
    pub fn low_pass(cutoff_hz: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::LowPassFilter { cutoff_hz }),
        }
    }

    /// Create a brick-wall high-pass filter.
    #[wasm_bindgen(js_name = "highPass")]
    pub fn high_pass(cutoff_hz: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::HighPassFilter { cutoff_hz }),
        }
    }

    /// Create a band-pass filter.
    #[wasm_bindgen(js_name = "bandPass")]
    pub fn band_pass(low_hz: f32, high_hz: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::BandPassFilter { low_hz, high_hz }),
        }
    }

    /// Create a spectral gain transform.
    pub fn gain(gain: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::SpectralGain { gain }),
        }
    }

    /// Create a pitch shift transform.
    ///
    /// `semitones`: +12 = up one octave, -12 = down one octave.
    #[wasm_bindgen(js_name = "pitchShift")]
    pub fn pitch_shift(semitones: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::PitchShift::new(semitones)),
        }
    }

    /// Create a spectral freeze transform.
    ///
    /// `frozen`: whether to start in frozen mode.
    #[wasm_bindgen(js_name = "spectralFreeze")]
    pub fn spectral_freeze(frozen: bool, sample_rate: f32, hop_size: usize) -> Self {
        Self {
            inner: Box::new(crate::transform::SpectralFreeze::new(
                frozen,
                sample_rate,
                hop_size,
            )),
        }
    }

    /// Create a spectral delay transform.
    ///
    /// - `delay_frames`: number of spectral frames to delay (>=1)
    /// - `feedback`: 0.0-0.95, how much delayed signal feeds back
    /// - `mix`: 0.0-1.0, dry/wet blend
    #[wasm_bindgen(js_name = "spectralDelay")]
    pub fn spectral_delay(delay_frames: usize, feedback: f32, mix: f32) -> Self {
        Self {
            inner: Box::new(crate::transform::SpectralDelay::new(
                delay_frames,
                feedback,
                mix,
            )),
        }
    }

    /// Apply this transform to an interleaved complex spectrum in-place.
    ///
    /// `spectrum` is `[re0, im0, re1, im1, …]` with length `2 * (fft_size/2 + 1)`.
    pub fn process(&mut self, spectrum: &mut [f32], sample_rate: f32, fft_size: usize) {
        let mut complex: Vec<num_complex::Complex<f32>> = spectrum
            .chunks_exact(2)
            .map(|pair| num_complex::Complex::new(pair[0], pair[1]))
            .collect();

        self.inner.process(&mut complex, sample_rate, fft_size);

        // Write back to interleaved format.
        for (i, c) in complex.iter().enumerate() {
            spectrum[i * 2] = c.re;
            spectrum[i * 2 + 1] = c.im;
        }
    }

    /// Human-readable name of this transform.
    pub fn name(&self) -> String {
        self.inner.name().to_owned()
    }
}

// ---------------------------------------------------------------------------
// Additive Synthesizer
// ---------------------------------------------------------------------------

/// Additive synthesizer that generates sound by summing sine wave partials.
#[wasm_bindgen]
pub struct WasmAdditiveSynth {
    inner: crate::additive::AdditiveSynth,
}

#[wasm_bindgen]
impl WasmAdditiveSynth {
    /// Create an additive synth with a harmonic series.
    ///
    /// Generates `num_harmonics` partials at `f`, `2f`, `3f`, …
    /// with 1/n amplitude rolloff.
    #[wasm_bindgen(constructor)]
    pub fn new(fundamental: f32, num_harmonics: usize, sample_rate: f32) -> Self {
        let partials = crate::additive::harmonic_series(fundamental, num_harmonics);
        Self {
            inner: crate::additive::AdditiveSynth::new(&partials, sample_rate),
        }
    }

    /// Create from explicit partial data.
    ///
    /// `partials_data` is a flat array: `[freq0, amp0, phase0, freq1, amp1, phase1, …]`.
    #[wasm_bindgen(js_name = "fromPartials")]
    pub fn from_partials(partials_data: &[f32], sample_rate: f32) -> Result<Self, JsError> {
        if !partials_data.len().is_multiple_of(3) {
            return Err(JsError::new(
                "partials_data length must be a multiple of 3 (freq, amp, phase per partial)",
            ));
        }
        let partials: Vec<crate::additive::Partial> = partials_data
            .chunks_exact(3)
            .map(|chunk| crate::additive::Partial {
                frequency: chunk[0],
                amplitude: chunk[1],
                phase: chunk[2],
            })
            .collect();
        Ok(Self {
            inner: crate::additive::AdditiveSynth::new(&partials, sample_rate),
        })
    }

    /// Generate `num_samples` of output.
    pub fn generate(&mut self, num_samples: usize) -> Vec<f32> {
        let mut output = vec![0.0; num_samples];
        self.inner.generate(&mut output);
        output
    }

    /// Add a partial. Returns the index of the new partial.
    #[wasm_bindgen(js_name = "addPartial")]
    pub fn add_partial(&mut self, frequency: f32, amplitude: f32, phase: f32) -> usize {
        self.inner.add_partial(crate::additive::Partial {
            frequency,
            amplitude,
            phase,
        })
    }

    /// Remove the partial at `index`.
    #[wasm_bindgen(js_name = "removePartial")]
    pub fn remove_partial(&mut self, index: usize) {
        self.inner.remove_partial(index);
    }

    /// Set the frequency of a partial (Hz).
    #[wasm_bindgen(js_name = "setPartialFrequency")]
    pub fn set_partial_frequency(&mut self, index: usize, frequency: f32) {
        self.inner.set_partial_frequency(index, frequency);
    }

    /// Set the amplitude of a partial.
    #[wasm_bindgen(js_name = "setPartialAmplitude")]
    pub fn set_partial_amplitude(&mut self, index: usize, amplitude: f32) {
        self.inner.set_partial_amplitude(index, amplitude);
    }

    /// Number of partials.
    #[wasm_bindgen(getter, js_name = "numPartials")]
    pub fn num_partials(&self) -> usize {
        self.inner.num_partials()
    }

    #[wasm_bindgen(getter, js_name = "sampleRate")]
    pub fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

// ---------------------------------------------------------------------------
// Overlap-Add Processor
// ---------------------------------------------------------------------------

/// Streaming overlap-add FFT processor for real-time spectral processing.
///
/// Feed samples in, apply a transform, and pull processed output.
#[wasm_bindgen]
pub struct WasmOverlapAddProcessor {
    inner: crate::overlap_add::OverlapAddProcessor,
    transform: Box<dyn crate::transform::SpectralTransform>,
}

#[wasm_bindgen]
impl WasmOverlapAddProcessor {
    /// Create a new OLA processor.
    ///
    /// - `fft_size`: FFT frame size (power of 2, e.g. 2048)
    /// - `hop_size`: hop size (e.g. `fft_size/2` for 50% overlap)
    /// - `window_type`: `"hann"`, `"hamming"`, `"blackman"`, or `"rectangular"`
    /// - `sample_rate`: audio sample rate in Hz
    #[wasm_bindgen(constructor)]
    pub fn new(
        fft_size: usize,
        hop_size: usize,
        window_type: &str,
        sample_rate: f32,
    ) -> Result<Self, JsError> {
        let wt = parse_window_type(window_type)?;
        let config = crate::overlap_add::OlaConfig {
            fft_size,
            hop_size,
            window_type: wt,
            sample_rate,
        };
        Ok(Self {
            inner: crate::overlap_add::OverlapAddProcessor::new(config),
            transform: Box::new(crate::transform::IdentityTransform),
        })
    }

    /// Set the spectral transform to apply during processing.
    ///
    /// Takes ownership of the `WasmTransform`.
    #[wasm_bindgen(js_name = "setTransform")]
    pub fn set_transform(&mut self, transform: WasmTransform) {
        self.transform = transform.inner;
    }

    /// Push input samples into the processor.
    ///
    /// Internally processes complete frames through FFT → transform → IFFT.
    #[wasm_bindgen(js_name = "pushSamples")]
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.inner.push_samples(samples, self.transform.as_mut());
    }

    /// Pull processed output samples.
    ///
    /// Returns up to `max_samples` of processed audio. May return fewer
    /// if not enough input has been pushed yet.
    #[wasm_bindgen(js_name = "pullSamples")]
    pub fn pull_samples(&mut self, max_samples: usize) -> Vec<f32> {
        let mut output = vec![0.0; max_samples];
        let n = self.inner.pull_samples(&mut output);
        output.truncate(n);
        output
    }

    /// Number of processed samples available for reading.
    #[wasm_bindgen(getter, js_name = "availableSamples")]
    pub fn available_samples(&self) -> usize {
        self.inner.available_samples()
    }

    /// Get the latest spectrum snapshot as interleaved `[re, im, …]`.
    #[wasm_bindgen(js_name = "latestSpectrum")]
    pub fn latest_spectrum(&self) -> Vec<f32> {
        let spectrum = self.inner.latest_spectrum();
        let mut out = Vec::with_capacity(spectrum.len() * 2);
        for c in spectrum {
            out.push(c.re);
            out.push(c.im);
        }
        out
    }
}
