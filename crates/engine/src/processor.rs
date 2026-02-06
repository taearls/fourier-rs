//! The main engine processor: owns the processing thread and coordinates
//! audio I/O, DSP, and MIDI.

use std::thread;

use crossbeam_channel::{Receiver, Sender, bounded};

use fourier_audio_io::ring_buffer::{AudioRingBuffer, RingConsumer, RingProducer};
use fourier_core::overlap_add::{OlaConfig, OverlapAddProcessor};
use fourier_core::spectral::{SpectralPeak, detect_peaks};
use fourier_core::transform::{
    BandPassFilter, HighPassFilter, IdentityTransform, LowPassFilter, SpectralGain,
    SpectralTransform, TransformChain,
};

use crate::params::{EngineParams, ParamMessage, TransformSpec};

/// Snapshot of spectral data sent from the processing thread to the UI.
#[derive(Debug, Clone)]
pub struct SpectralSnapshot {
    /// Magnitude spectrum in dB.
    pub magnitude_db: Vec<f32>,
    /// Detected peaks.
    pub peaks: Vec<SpectralPeak>,
    /// Sample rate at time of snapshot.
    pub sample_rate: f32,
    /// FFT size at time of snapshot.
    pub fft_size: usize,
}

/// The main audio-fourier engine.
///
/// Call [`Engine::new`] to create the engine and get back ring buffer
/// halves for connecting to audio I/O, plus channel endpoints for
/// parameters and spectral data.
pub struct Engine {
    /// Handle to the processing thread.
    processing_thread: Option<thread::JoinHandle<()>>,
    /// Send parameter changes to the processing thread.
    param_tx: Sender<ParamMessage>,
    /// Receive spectral snapshots from the processing thread.
    snapshot_rx: Receiver<SpectralSnapshot>,
}

/// Returned by [`Engine::new`] — the I/O endpoints that must be connected
/// to audio streams.
pub struct EngineIo {
    /// Feed captured audio samples into this producer.
    pub input_producer: RingProducer,
    /// Read processed audio from this consumer for playback.
    pub output_consumer: RingConsumer,
}

impl Engine {
    /// Create a new engine with the given configuration.
    ///
    /// Returns `(engine, io)` where:
    /// - `engine` provides control and spectral data.
    /// - `io` provides the ring buffer halves to connect to audio streams.
    pub fn new(
        sample_rate: f32,
        fft_size: usize,
        hop_size: usize,
    ) -> (Self, EngineIo) {
        // Ring buffers: input (mic → processing) and output (processing → speakers).
        let ring_capacity = fft_size * 8;
        let (input_producer, input_consumer) = AudioRingBuffer::new(ring_capacity);
        let (output_producer, output_consumer) = AudioRingBuffer::new(ring_capacity);

        // Parameter channel: UI → processing.
        let (param_tx, param_rx) = bounded(64);

        // Spectral snapshot channel: processing → UI.
        let (snapshot_tx, snapshot_rx) = bounded(4);

        let ola_config = OlaConfig {
            fft_size,
            hop_size,
            window_type: fourier_core::window::WindowType::Hann,
            sample_rate,
        };

        // Spawn the processing thread.
        let thread = thread::Builder::new()
            .name("fourier-processing".to_string())
            .spawn(move || {
                processing_loop(
                    ola_config,
                    input_consumer,
                    output_producer,
                    param_rx,
                    snapshot_tx,
                );
            })
            .expect("Failed to spawn processing thread");

        let engine = Self {
            processing_thread: Some(thread),
            param_tx,
            snapshot_rx,
        };

        let io = EngineIo {
            input_producer,
            output_consumer,
        };

        (engine, io)
    }

    /// Send a parameter change to the processing thread.
    pub fn send_param(&self, msg: ParamMessage) -> Result<(), String> {
        self.param_tx
            .try_send(msg)
            .map_err(|e| format!("Failed to send param: {}", e))
    }

    /// Set the spectral transform.
    pub fn set_transform(&self, spec: TransformSpec) -> Result<(), String> {
        self.send_param(ParamMessage::SetTransform(spec))
    }

    /// Set output gain.
    pub fn set_output_gain(&self, gain: f32) -> Result<(), String> {
        self.send_param(ParamMessage::SetOutputGain(gain))
    }

    /// Set bypass mode.
    pub fn set_bypass(&self, bypass: bool) -> Result<(), String> {
        self.send_param(ParamMessage::SetBypass(bypass))
    }

    /// Try to receive the latest spectral snapshot (non-blocking).
    pub fn try_recv_snapshot(&self) -> Option<SpectralSnapshot> {
        self.snapshot_rx.try_recv().ok()
    }

    /// Shut down the engine and join the processing thread.
    pub fn shutdown(mut self) {
        let _ = self.param_tx.send(ParamMessage::Shutdown);
        if let Some(handle) = self.processing_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.param_tx.try_send(ParamMessage::Shutdown);
        if let Some(handle) = self.processing_thread.take() {
            let _ = handle.join();
        }
    }
}

/// Build a concrete SpectralTransform from a TransformSpec.
fn build_transform(spec: &TransformSpec) -> Box<dyn SpectralTransform> {
    match spec {
        TransformSpec::Identity => Box::new(IdentityTransform),
        TransformSpec::LowPass { cutoff_hz } => Box::new(LowPassFilter {
            cutoff_hz: *cutoff_hz,
        }),
        TransformSpec::HighPass { cutoff_hz } => Box::new(HighPassFilter {
            cutoff_hz: *cutoff_hz,
        }),
        TransformSpec::BandPass { low_hz, high_hz } => Box::new(BandPassFilter {
            low_hz: *low_hz,
            high_hz: *high_hz,
        }),
        TransformSpec::Gain { factor } => Box::new(SpectralGain { gain: *factor }),
        TransformSpec::Chain(specs) => {
            let mut chain = TransformChain::new();
            for s in specs {
                chain.push(build_transform(s));
            }
            Box::new(chain)
        }
    }
}

/// The main processing loop running on its own thread.
fn processing_loop(
    config: OlaConfig,
    mut input: RingConsumer,
    mut output: RingProducer,
    param_rx: Receiver<ParamMessage>,
    snapshot_tx: Sender<SpectralSnapshot>,
) {
    let mut ola = OverlapAddProcessor::new(config.clone());
    let mut transform: Box<dyn SpectralTransform> = Box::new(IdentityTransform);
    let mut params = EngineParams::default();

    // Scratch buffers — pre-allocated, no allocation in the loop.
    let chunk_size = config.hop_size;
    let mut input_chunk = vec![0.0_f32; chunk_size];
    let mut output_chunk = vec![0.0_f32; chunk_size];

    let mut frame_counter: u64 = 0;

    loop {
        // 1. Check for parameter updates (non-blocking).
        while let Ok(msg) = param_rx.try_recv() {
            match msg {
                ParamMessage::SetOutputGain(g) => params.output_gain = g,
                ParamMessage::SetBypass(b) => params.bypass = b,
                ParamMessage::SetPeakThreshold(t) => params.peak_threshold_db = t,
                ParamMessage::SetTransform(spec) => {
                    transform = build_transform(&spec);
                }
                ParamMessage::Shutdown => return,
            }
        }

        // 2. Read available input samples.
        let n_read = input.pop_slice(&mut input_chunk);
        if n_read == 0 {
            // No input available; sleep briefly to avoid busy-spinning.
            thread::sleep(std::time::Duration::from_micros(500));
            continue;
        }

        let input_slice = &input_chunk[..n_read];

        if params.bypass {
            // Bypass: pass input directly to output.
            output.push_slice(input_slice);
        } else {
            // 3. Push into OLA processor, applying the current transform.
            ola.push_samples(input_slice, transform.as_mut());

            // 4. Pull processed output.
            let n_out = ola.pull_samples(&mut output_chunk);
            if n_out > 0 {
                // Apply output gain.
                if (params.output_gain - 1.0).abs() > 1e-6 {
                    for s in &mut output_chunk[..n_out] {
                        *s *= params.output_gain;
                    }
                }
                output.push_slice(&output_chunk[..n_out]);
            }
        }

        // 5. Send spectral snapshot to UI periodically (every ~10 frames to avoid flooding).
        frame_counter += 1;
        if frame_counter % 10 == 0 {
            let spectrum = ola.latest_spectrum();
            if !spectrum.is_empty() {
                let magnitude_db = fourier_core::spectral::magnitude_spectrum_db(spectrum);
                let peaks = detect_peaks(
                    spectrum,
                    config.sample_rate,
                    config.fft_size,
                    params.peak_threshold_db,
                );
                let snapshot = SpectralSnapshot {
                    magnitude_db,
                    peaks,
                    sample_rate: config.sample_rate,
                    fft_size: config.fft_size,
                };
                // Non-blocking: drop snapshot if UI can't keep up.
                let _ = snapshot_tx.try_send(snapshot);
            }
        }
    }
}
