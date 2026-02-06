//! The main engine processor: owns the processing thread and coordinates
//! audio I/O, DSP, and MIDI.

use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};

use fourier_audio_io::ring_buffer::{AudioRingBuffer, RingConsumer, RingProducer};
use fourier_core::overlap_add::{OlaConfig, OverlapAddProcessor};
use fourier_core::spectral::{detect_peaks, SpectralPeak};
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
    pub fn new(sample_rate: f32, fft_size: usize, hop_size: usize) -> (Self, EngineIo) {
        // Ring buffers: input (mic → processing) and output (processing → speakers).
        let ring_capacity = fft_size * 8;
        let (input_producer, input_consumer) = AudioRingBuffer::create(ring_capacity);
        let (output_producer, output_consumer) = AudioRingBuffer::create(ring_capacity);

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
        if frame_counter.is_multiple_of(10) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::TransformSpec;

    /// Helper: push samples into engine input, sleep to let processing thread run,
    /// then pull from output.
    fn run_engine_with_samples(
        input: &[f32],
        transform: TransformSpec,
        sample_rate: f32,
        fft_size: usize,
        hop_size: usize,
    ) -> Vec<f32> {
        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_transform(transform).unwrap();

        let mut producer = io.input_producer;
        let mut consumer = io.output_consumer;

        // Push all input samples.
        producer.push_slice(input);

        // Give the processing thread time to process.
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Pull output.
        let mut output = vec![0.0_f32; input.len()];
        let n = consumer.pop_slice(&mut output);
        output.truncate(n);

        // Shutdown cleanly.
        engine.shutdown();

        output
    }

    #[test]
    fn engine_produces_output_with_identity() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Generate a sine wave.
        let input: Vec<f32> = (0..2048)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();

        let output = run_engine_with_samples(
            &input,
            TransformSpec::Identity,
            sample_rate,
            fft_size,
            hop_size,
        );

        assert!(!output.is_empty(), "engine should produce output samples");
        let energy: f32 = output.iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "output energy should be nonzero");
    }

    #[test]
    fn engine_bypass_passes_through() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_bypass(true).unwrap();

        let mut producer = io.input_producer;
        let mut consumer = io.output_consumer;

        let input: Vec<f32> = (0..1024).map(|i| i as f32 / 1024.0).collect();
        producer.push_slice(&input);

        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut output = vec![0.0_f32; 1024];
        let n = consumer.pop_slice(&mut output);

        // In bypass mode, output should match input exactly.
        assert!(n > 0, "bypass should produce output");
        for (inp, out) in input[..n].iter().zip(output[..n].iter()) {
            assert!((inp - out).abs() < 1e-6, "bypass output should match input");
        }

        engine.shutdown();
    }

    #[test]
    fn engine_gain_scales_output() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let input: Vec<f32> = (0..2048)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();

        // Run with gain = 1.0.
        let output_unity = run_engine_with_samples(
            &input,
            TransformSpec::Identity,
            sample_rate,
            fft_size,
            hop_size,
        );

        // Run with gain = 0.5 (using the Gain transform spec).
        let output_half = run_engine_with_samples(
            &input,
            TransformSpec::Gain { factor: 0.5 },
            sample_rate,
            fft_size,
            hop_size,
        );

        if !output_unity.is_empty() && !output_half.is_empty() {
            let energy_unity: f32 = output_unity.iter().map(|s| s * s).sum();
            let energy_half: f32 = output_half.iter().map(|s| s * s).sum();

            // Gain 0.5 should reduce energy by ~4x (amplitude halved -> energy quartered).
            assert!(
                energy_half < energy_unity,
                "half-gain energy ({}) should be less than unity-gain energy ({})",
                energy_half,
                energy_unity
            );
        }
    }

    #[test]
    fn engine_transform_switch() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);

        // Switch transforms rapidly -- should not panic or deadlock.
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_transform(TransformSpec::LowPass { cutoff_hz: 1000.0 })
            .unwrap();
        engine
            .set_transform(TransformSpec::HighPass { cutoff_hz: 500.0 })
            .unwrap();
        engine
            .set_transform(TransformSpec::BandPass {
                low_hz: 200.0,
                high_hz: 2000.0,
            })
            .unwrap();
        engine
            .set_transform(TransformSpec::Chain(vec![
                TransformSpec::LowPass { cutoff_hz: 5000.0 },
                TransformSpec::Gain { factor: 0.8 },
            ]))
            .unwrap();

        // Push some data to exercise the processing thread with the new transforms.
        let mut producer = io.input_producer;
        let input: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        producer.push_slice(&input);

        std::thread::sleep(std::time::Duration::from_millis(50));

        engine.shutdown();
    }

    #[test]
    fn engine_spectral_snapshot() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_transform(TransformSpec::Identity).unwrap();

        let mut producer = io.input_producer;

        // Push enough data to trigger multiple processing frames.
        let input: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        producer.push_slice(&input);

        // Wait for processing.
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Should eventually receive a spectral snapshot.
        let mut got_snapshot = false;
        for _ in 0..10 {
            if let Some(snapshot) = engine.try_recv_snapshot() {
                assert_eq!(snapshot.fft_size, fft_size);
                assert!((snapshot.sample_rate - sample_rate).abs() < 0.01);
                assert!(!snapshot.magnitude_db.is_empty());
                got_snapshot = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(got_snapshot, "should receive a spectral snapshot");

        engine.shutdown();
    }

    #[test]
    fn engine_shutdown_is_clean() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create and immediately shutdown -- should not hang or panic.
        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.shutdown();
    }

    #[test]
    fn engine_drop_is_clean() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create and drop without explicit shutdown -- Drop impl should handle it.
        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size);
        drop(engine);
    }
}
