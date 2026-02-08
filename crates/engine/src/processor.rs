//! The main engine processor: owns the processing thread and coordinates
//! audio I/O, DSP, and MIDI.

use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};

use fourier_audio_io::ring_buffer::{AudioRingBuffer, RingConsumer, RingProducer};
use fourier_core::overlap_add::{OlaConfig, OverlapAddProcessor};
use fourier_core::spectral::{detect_peaks, SpectralPeak};
use fourier_core::transform::{
    BandPassFilter, HighPassFilter, IdentityTransform, LowPassFilter, SpectralGain,
    SpectralTransform, TransformChain,
};

use crate::params::{EngineParams, ParamMessage, SourceSpec, TransformSpec};
use crate::source::{build_source, AudioSource};

/// Snapshot of spectral data sent from the processing thread to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectralSnapshot {
    /// Magnitude spectrum in dB.
    pub magnitude_db: Vec<f32>,
    /// Detected peaks.
    pub peaks: Vec<SpectralPeak>,
    /// Sample rate at time of snapshot.
    pub sample_rate: f32,
    /// FFT size at time of snapshot.
    pub fft_size: usize,
    /// Timestamp in milliseconds since Unix epoch.
    pub timestamp_ms: f64,
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
        #[allow(clippy::expect_used)]
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
            .map_err(|e| format!("Failed to send param: {e}"))
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

    /// Set the audio source.
    pub fn set_source(&self, spec: SourceSpec) -> Result<(), String> {
        self.send_param(ParamMessage::SetSource(spec))
    }

    /// Drain all pending snapshots and return only the most recent one.
    ///
    /// This prevents stale data from accumulating in the channel when the
    /// frontend polls slower than the engine produces snapshots.
    pub fn latest_snapshot(&self) -> Option<SpectralSnapshot> {
        let mut latest = None;
        while let Ok(snapshot) = self.snapshot_rx.try_recv() {
            latest = Some(snapshot);
        }
        latest
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

/// Build a concrete `SpectralTransform` from a `TransformSpec`.
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
#[allow(clippy::needless_pass_by_value)]
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

    // Current audio source. `None` means live input (read from ring buffer).
    let mut source: Option<Box<dyn AudioSource>> = None;

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
                ParamMessage::SetSource(spec) => {
                    source = build_source(&spec, config.sample_rate);
                }
                ParamMessage::Shutdown => return,
            }
        }

        // 2. Get input samples — either from a generated source or the live input ring buffer.
        let n_read = if let Some(ref mut src) = source {
            // Generated source: fill the entire chunk.
            src.generate(&mut input_chunk);
            chunk_size
        } else {
            // Live input: read whatever is available from the ring buffer.
            let n = input.pop_slice(&mut input_chunk);
            if n == 0 {
                // No input available; sleep briefly to avoid busy-spinning.
                thread::sleep(std::time::Duration::from_micros(500));
                continue;
            }
            n
        };

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
                let timestamp_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
                let snapshot = SpectralSnapshot {
                    magnitude_db,
                    peaks,
                    sample_rate: config.sample_rate,
                    fft_size: config.fft_size,
                    timestamp_ms,
                };
                // Non-blocking: drop snapshot if UI can't keep up.
                let _ = snapshot_tx.try_send(snapshot);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        thread::sleep(std::time::Duration::from_millis(100));

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

        thread::sleep(std::time::Duration::from_millis(100));

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
                "half-gain energy ({energy_half}) should be less than unity-gain energy ({energy_unity})"
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

        thread::sleep(std::time::Duration::from_millis(50));

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
        thread::sleep(std::time::Duration::from_millis(200));

        // Should eventually receive a spectral snapshot.
        let mut got_snapshot = false;
        for _ in 0..10 {
            if let Some(snapshot) = engine.latest_snapshot() {
                assert_eq!(snapshot.fft_size, fft_size);
                assert!((snapshot.sample_rate - sample_rate).abs() < 0.01);
                assert!(!snapshot.magnitude_db.is_empty());
                got_snapshot = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
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

    // --- Source integration tests ---

    #[test]
    fn engine_oscillator_source_produces_output() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 440.0,
                amplitude: 1.0,
            })
            .unwrap();

        let mut consumer = io.output_consumer;

        // Give the processing thread time to generate and process.
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 4096];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "oscillator source should produce output");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "oscillator output should have nonzero energy");

        engine.shutdown();
    }

    #[test]
    fn engine_noise_source_produces_output() {
        use crate::params::NoiseType;

        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::Noise {
                noise_type: NoiseType::White,
                amplitude: 0.5,
            })
            .unwrap();

        let mut consumer = io.output_consumer;
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 4096];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "noise source should produce output");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "noise output should have nonzero energy");

        engine.shutdown();
    }

    #[test]
    fn engine_additive_source_produces_output() {
        use crate::params::Partial;

        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::Additive {
                partials: vec![
                    Partial {
                        frequency: 440.0,
                        amplitude: 1.0,
                        phase: 0.0,
                    },
                    Partial {
                        frequency: 880.0,
                        amplitude: 0.5,
                        phase: 0.0,
                    },
                ],
            })
            .unwrap();

        let mut consumer = io.output_consumer;
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 4096];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "additive source should produce output");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "additive output should have nonzero energy");

        engine.shutdown();
    }

    #[test]
    fn engine_source_switch_to_live_input() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);

        // Start with oscillator source.
        engine
            .set_source(SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 440.0,
                amplitude: 1.0,
            })
            .unwrap();

        thread::sleep(std::time::Duration::from_millis(50));

        // Switch back to live input — should not panic.
        engine.set_source(SourceSpec::LiveInput).unwrap();

        // Push live input data.
        let mut producer = io.input_producer;
        let input: Vec<f32> = (0..2048)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        producer.push_slice(&input);

        thread::sleep(std::time::Duration::from_millis(100));

        let mut consumer = io.output_consumer;
        let mut output = vec![0.0_f32; 4096];
        let n = consumer.pop_slice(&mut output);

        assert!(
            n > 0,
            "engine should produce output after switching back to live input"
        );

        engine.shutdown();
    }

    #[test]
    fn engine_oscillator_through_lowpass_spectral_result() {
        let fft_size = 1024;
        let hop_size = 512;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);

        // Set a low-pass filter that should pass a 200 Hz sine.
        engine
            .set_transform(TransformSpec::LowPass { cutoff_hz: 500.0 })
            .unwrap();
        engine
            .set_source(SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 200.0,
                amplitude: 1.0,
            })
            .unwrap();

        thread::sleep(std::time::Duration::from_millis(300));

        // The spectral snapshot should show the 200 Hz peak.
        let mut got_peak = false;
        for _ in 0..20 {
            if let Some(snapshot) = engine.latest_snapshot() {
                // Find the bin with maximum energy.
                if let Some((peak_bin, _)) = snapshot
                    .magnitude_db
                    .iter()
                    .enumerate()
                    .skip(1) // skip DC
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                {
                    let bin_freq =
                        peak_bin as f32 * snapshot.sample_rate / snapshot.fft_size as f32;
                    let bin_width = snapshot.sample_rate / snapshot.fft_size as f32;
                    // Peak should be near 200 Hz.
                    if (bin_freq - 200.0).abs() <= bin_width * 2.0 {
                        got_peak = true;
                        break;
                    }
                }
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            got_peak,
            "spectral snapshot should show peak near 200 Hz for oscillator through low-pass"
        );

        // Also verify output has energy.
        let mut consumer = io.output_consumer;
        let mut output = vec![0.0_f32; 8192];
        let n = consumer.pop_slice(&mut output);
        assert!(n > 0, "should produce output");

        engine.shutdown();
    }

    #[test]
    fn engine_source_switch_does_not_deadlock() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size);

        // Rapid source switching should not cause deadlock or panic.
        for _ in 0..10 {
            engine
                .set_source(SourceSpec::Oscillator {
                    waveform: fourier_core::WaveformType::Square,
                    frequency: 440.0,
                    amplitude: 0.5,
                })
                .unwrap();
            engine.set_source(SourceSpec::LiveInput).unwrap();
            engine
                .set_source(SourceSpec::Noise {
                    noise_type: crate::params::NoiseType::Pink,
                    amplitude: 0.3,
                })
                .unwrap();
        }

        thread::sleep(std::time::Duration::from_millis(50));
        engine.shutdown();
    }
}
