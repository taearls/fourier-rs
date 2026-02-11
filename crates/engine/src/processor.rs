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
    BandPassFilter, HighPassFilter, IdentityTransform, LowPassFilter, ParametricEq, SpectralGain,
    SpectralTransform, TransformChain,
};

use crate::error::EngineError;
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

/// Snapshot of time-domain audio samples sent from the processing thread to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveformSnapshot {
    /// Recent time-domain audio samples (post-processing, pre-output-gain).
    pub samples: Vec<f32>,
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
    /// Receive waveform snapshots from the processing thread.
    waveform_rx: Receiver<WaveformSnapshot>,
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
    ) -> Result<(Self, EngineIo), EngineError> {
        // Ring buffers: input (mic → processing) and output (processing → speakers).
        let ring_capacity = fft_size * 8;
        let (input_producer, input_consumer) = AudioRingBuffer::create(ring_capacity);
        let (output_producer, output_consumer) = AudioRingBuffer::create(ring_capacity);

        // Parameter channel: UI → processing.
        let (param_tx, param_rx) = bounded(64);

        // Spectral snapshot channel: processing → UI.
        let (snapshot_tx, snapshot_rx) = bounded(4);

        // Waveform snapshot channel: processing → UI.
        let (waveform_tx, waveform_rx) = bounded(4);

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
                    waveform_tx,
                );
            })?;

        tracing::info!(sample_rate, fft_size, hop_size, "Engine started");

        let engine = Self {
            processing_thread: Some(thread),
            param_tx,
            snapshot_rx,
            waveform_rx,
        };

        let io = EngineIo {
            input_producer,
            output_consumer,
        };

        Ok((engine, io))
    }

    /// Send a parameter change to the processing thread.
    pub fn send_param(&self, msg: ParamMessage) -> Result<(), EngineError> {
        self.param_tx.try_send(msg).map_err(|e| match e {
            crossbeam_channel::TrySendError::Full(_) => EngineError::ChannelFull,
            crossbeam_channel::TrySendError::Disconnected(_) => EngineError::ChannelDisconnected,
        })
    }

    /// Set the spectral transform.
    pub fn set_transform(&self, spec: TransformSpec) -> Result<(), EngineError> {
        self.send_param(ParamMessage::SetTransform(spec))
    }

    /// Set output gain.
    pub fn set_output_gain(&self, gain: f32) -> Result<(), EngineError> {
        self.send_param(ParamMessage::SetOutputGain(gain))
    }

    /// Set bypass mode.
    pub fn set_bypass(&self, bypass: bool) -> Result<(), EngineError> {
        self.send_param(ParamMessage::SetBypass(bypass))
    }

    /// Set the audio source.
    pub fn set_source(&self, spec: SourceSpec) -> Result<(), EngineError> {
        self.send_param(ParamMessage::SetSource(spec))
    }

    /// Seek to a normalized position in the current audio source.
    ///
    /// `position` is clamped to `[0.0, 1.0]` where 0.0 is the start and
    /// 1.0 is the end. Has no effect if the current source doesn't support
    /// seeking (e.g. oscillators, noise).
    pub fn seek(&self, position: f32) -> Result<(), EngineError> {
        self.send_param(ParamMessage::Seek(position))
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

    /// Drain all pending waveform snapshots and return only the most recent one.
    pub fn latest_waveform(&self) -> Option<WaveformSnapshot> {
        let mut latest = None;
        while let Ok(waveform) = self.waveform_rx.try_recv() {
            latest = Some(waveform);
        }
        latest
    }

    /// Shut down the engine and join the processing thread.
    pub fn shutdown(mut self) {
        tracing::info!("Engine shutting down");
        let _ = self.param_tx.send(ParamMessage::Shutdown);
        if let Some(handle) = self.processing_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        tracing::debug!("Engine dropped, stopping processing thread");
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
        TransformSpec::ParametricEq { bands } => Box::new(ParametricEq::new(bands.clone())),
        TransformSpec::Chain(specs) => {
            let mut chain = TransformChain::new();
            for s in specs {
                chain.push(build_transform(s));
            }
            Box::new(chain)
        }
    }
}

/// Get the current timestamp in milliseconds since Unix epoch.
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Append samples to the rolling waveform buffer for oscilloscope display.
///
/// `write_pos` wraps within `[0, capacity)` to avoid unbounded growth.
fn waveform_buf_push(buf: &mut [f32], write_pos: &mut usize, count: &mut usize, samples: &[f32]) {
    let capacity = buf.len();
    for &s in samples {
        buf[*write_pos] = s;
        *write_pos = (*write_pos + 1) % capacity;
        *count += 1;
    }
}

/// Send spectral and waveform snapshots to the UI (non-blocking).
#[allow(clippy::too_many_arguments)]
fn send_snapshots(
    ola: &OverlapAddProcessor,
    config: &OlaConfig,
    params: &EngineParams,
    snapshot_tx: &Sender<SpectralSnapshot>,
    waveform_tx: &Sender<WaveformSnapshot>,
    waveform_buf: &[f32],
    waveform_write_pos: usize,
    waveform_sample_count: usize,
) {
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
            timestamp_ms: now_ms(),
        };
        let _ = snapshot_tx.try_send(snapshot);
    }

    if let Some(waveform) = build_waveform_snapshot(
        waveform_buf,
        waveform_write_pos,
        waveform_sample_count,
        config.sample_rate,
        config.fft_size,
    ) {
        let _ = waveform_tx.try_send(waveform);
    }
}

/// Build a `WaveformSnapshot` from the rolling waveform buffer.
fn build_waveform_snapshot(
    waveform_buf: &[f32],
    waveform_write_pos: usize,
    waveform_sample_count: usize,
    sample_rate: f32,
    fft_size: usize,
) -> Option<WaveformSnapshot> {
    let capacity = waveform_buf.len();
    let total = waveform_sample_count.min(capacity);
    if total == 0 {
        return None;
    }

    // Cap to ~50ms of audio to reduce IPC payload (matches frontend display window).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_display_samples = (sample_rate * 0.05).round() as usize;
    let total = total.min(max_display_samples);

    // write_pos is always < capacity (wraps via modular arithmetic).
    // Read the most recent `total` samples ending at write_pos.
    let start = (waveform_write_pos + capacity - total) % capacity;
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        samples.push(waveform_buf[(start + i) % capacity]);
    }

    Some(WaveformSnapshot {
        samples,
        sample_rate,
        fft_size,
        timestamp_ms: now_ms(),
    })
}

/// The main processing loop running on its own thread.
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
fn processing_loop(
    config: OlaConfig,
    mut input: RingConsumer,
    mut output: RingProducer,
    param_rx: Receiver<ParamMessage>,
    snapshot_tx: Sender<SpectralSnapshot>,
    waveform_tx: Sender<WaveformSnapshot>,
) {
    tracing::debug!(
        fft_size = config.fft_size,
        hop_size = config.hop_size,
        sample_rate = config.sample_rate,
        "Processing thread started"
    );

    let mut ola = OverlapAddProcessor::new(config.clone());
    let mut transform: Box<dyn SpectralTransform> = Box::new(IdentityTransform);
    let mut params = EngineParams::default();

    // Current audio source. `None` means live input (read from ring buffer).
    let mut source: Option<Box<dyn AudioSource>> = None;

    // Scratch buffers — pre-allocated, no allocation in the loop.
    let chunk_size = config.hop_size;
    let mut input_chunk = vec![0.0_f32; chunk_size];
    let mut output_chunk = vec![0.0_f32; chunk_size];

    // Rolling buffer for waveform display. Stores up to 4 × fft_size recent
    // output samples (pre-gain) for oscilloscope visualization.
    let waveform_capacity = config.fft_size * 4;
    let mut waveform_buf = vec![0.0_f32; waveform_capacity];
    let mut waveform_write_pos: usize = 0;
    let mut waveform_sample_count: usize = 0;

    let mut frame_counter: u64 = 0;

    loop {
        // 1. Check for parameter updates (non-blocking).
        while let Ok(msg) = param_rx.try_recv() {
            match msg {
                ParamMessage::SetOutputGain(g) => {
                    tracing::debug!(gain = g, "Output gain changed");
                    params.output_gain = g;
                }
                ParamMessage::SetBypass(b) => {
                    tracing::debug!(bypass = b, "Bypass mode changed");
                    params.bypass = b;
                }
                ParamMessage::SetPeakThreshold(t) => params.peak_threshold_db = t,
                ParamMessage::SetTransform(spec) => {
                    tracing::debug!("Transform changed");
                    transform = build_transform(&spec);
                }
                ParamMessage::SetSource(spec) => {
                    tracing::debug!("Audio source changed");
                    source = build_source(&spec, config.sample_rate);
                }
                ParamMessage::Seek(pos) => {
                    if let Some(ref mut src) = source {
                        src.seek(pos);
                    }
                }
                ParamMessage::Shutdown => {
                    tracing::debug!("Processing thread received shutdown");
                    return;
                }
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
            waveform_buf_push(
                &mut waveform_buf,
                &mut waveform_write_pos,
                &mut waveform_sample_count,
                input_slice,
            );
        } else {
            // 3. Push into OLA processor, applying the current transform.
            ola.push_samples(input_slice, transform.as_mut());

            // 4. Pull processed output.
            let n_out = ola.pull_samples(&mut output_chunk);
            if n_out > 0 {
                // Capture pre-gain samples for waveform display.
                waveform_buf_push(
                    &mut waveform_buf,
                    &mut waveform_write_pos,
                    &mut waveform_sample_count,
                    &output_chunk[..n_out],
                );
                // Apply output gain.
                if (params.output_gain - 1.0).abs() > 1e-6 {
                    for s in &mut output_chunk[..n_out] {
                        *s *= params.output_gain;
                    }
                }
                output.push_slice(&output_chunk[..n_out]);
            }
        }

        // 5. Send snapshots to UI periodically (every ~10 frames to avoid flooding).
        frame_counter += 1;
        if frame_counter.is_multiple_of(10) {
            send_snapshots(
                &ola,
                &config,
                &params,
                &snapshot_tx,
                &waveform_tx,
                &waveform_buf,
                waveform_write_pos,
                waveform_sample_count,
            );
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
        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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
    fn engine_waveform_snapshot() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.set_transform(TransformSpec::Identity).unwrap();

        let mut producer = io.input_producer;

        // Push enough data to trigger multiple processing frames.
        let input: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        producer.push_slice(&input);

        // Wait for processing.
        thread::sleep(std::time::Duration::from_millis(200));

        // Should eventually receive a waveform snapshot.
        let mut got_waveform = false;
        for _ in 0..10 {
            if let Some(waveform) = engine.latest_waveform() {
                assert_eq!(waveform.fft_size, fft_size);
                assert!((waveform.sample_rate - sample_rate).abs() < 0.01);
                assert!(!waveform.samples.is_empty());
                // Samples should have some nonzero energy (processed sine wave).
                let energy: f32 = waveform.samples.iter().map(|s| s * s).sum();
                assert!(energy > 0.0, "waveform should have nonzero energy");
                got_waveform = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(got_waveform, "should receive a waveform snapshot");

        engine.shutdown();
    }

    #[test]
    fn engine_waveform_from_oscillator_source() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 440.0,
                amplitude: 1.0,
            })
            .unwrap();

        // Don't need the consumer, but keep io alive.
        let _consumer = io.output_consumer;

        thread::sleep(std::time::Duration::from_millis(300));

        let mut got_waveform = false;
        for _ in 0..10 {
            if let Some(waveform) = engine.latest_waveform() {
                assert!(!waveform.samples.is_empty());
                // Waveform samples should be within [-1, 1] range (pre-gain).
                let max_abs = waveform
                    .samples
                    .iter()
                    .map(|s| s.abs())
                    .fold(0.0_f32, f32::max);
                assert!(
                    max_abs <= 1.5,
                    "waveform samples should be approximately in [-1, 1], got max_abs={max_abs}"
                );
                got_waveform = true;
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            got_waveform,
            "should receive waveform from oscillator source"
        );

        engine.shutdown();
    }

    #[test]
    fn engine_shutdown_is_clean() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create and immediately shutdown -- should not hang or panic.
        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.shutdown();
    }

    #[test]
    fn engine_drop_is_clean() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create and drop without explicit shutdown -- Drop impl should handle it.
        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        drop(engine);
    }

    // --- Source integration tests ---

    #[test]
    fn engine_oscillator_source_produces_output() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

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

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

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

        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

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

    // --- AudioBuffer source integration tests ---

    #[test]
    fn engine_audio_buffer_source_produces_output() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create a mono buffer with a 440 Hz sine wave (1 second).
        let num_frames = sample_rate as usize;
        let samples: Vec<f32> = (0..num_frames)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        let buffer = std::sync::Arc::new(fourier_file_io::AudioBuffer {
            samples,
            sample_rate: sample_rate as u32,
            channels: 1,
        });

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::AudioBuffer {
                buffer: Some(buffer),
                looping: false,
            })
            .unwrap();

        let mut consumer = io.output_consumer;
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 8192];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "audio buffer source should produce output");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(
            energy > 0.0,
            "audio buffer output should have nonzero energy"
        );

        engine.shutdown();
    }

    #[test]
    fn engine_audio_buffer_looping_produces_continuous_output() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        // Create a very short buffer (256 samples) to force looping.
        let num_frames = 256;
        let samples: Vec<f32> = (0..num_frames)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        let buffer = std::sync::Arc::new(fourier_file_io::AudioBuffer {
            samples,
            sample_rate: sample_rate as u32,
            channels: 1,
        });

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::AudioBuffer {
                buffer: Some(buffer),
                looping: true,
            })
            .unwrap();

        let mut consumer = io.output_consumer;

        // Wait long enough that the buffer would have been exhausted without looping.
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 8192];
        let n = consumer.pop_slice(&mut output);

        // With looping, should produce more output than the buffer length.
        assert!(
            n > num_frames,
            "looping buffer should produce more output ({n}) than buffer length ({num_frames})"
        );

        engine.shutdown();
    }

    #[test]
    fn engine_audio_buffer_seek() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let num_frames = sample_rate as usize;
        let samples: Vec<f32> = (0..num_frames)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        let buffer = std::sync::Arc::new(fourier_file_io::AudioBuffer {
            samples,
            sample_rate: sample_rate as u32,
            channels: 1,
        });

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();
        engine.set_transform(TransformSpec::Identity).unwrap();
        engine
            .set_source(SourceSpec::AudioBuffer {
                buffer: Some(buffer),
                looping: false,
            })
            .unwrap();

        // Seek to the middle.
        engine.seek(0.5).unwrap();

        let mut consumer = io.output_consumer;
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 8192];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "should produce output after seek");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "output after seek should have energy");

        engine.shutdown();
    }

    #[test]
    fn engine_audio_buffer_switch_and_rapid_source_changes() {
        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let samples: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate).sin())
            .collect();
        let buffer = std::sync::Arc::new(fourier_file_io::AudioBuffer {
            samples,
            sample_rate: sample_rate as u32,
            channels: 1,
        });

        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

        // Rapid switching between audio buffer and other sources.
        for _ in 0..10 {
            engine
                .set_source(SourceSpec::AudioBuffer {
                    buffer: Some(buffer.clone()),
                    looping: true,
                })
                .unwrap();
            engine.set_source(SourceSpec::LiveInput).unwrap();
            engine
                .set_source(SourceSpec::Oscillator {
                    waveform: fourier_core::WaveformType::Sine,
                    frequency: 440.0,
                    amplitude: 1.0,
                })
                .unwrap();
        }

        thread::sleep(std::time::Duration::from_millis(50));
        engine.shutdown();
    }

    // --- Parametric EQ integration tests ---

    #[test]
    fn engine_parametric_eq_processes_audio() {
        use fourier_core::transform::{BandType, EqBand};

        let fft_size = 1024;
        let hop_size = 512;
        let sample_rate = 44100.0;

        let (engine, io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

        // Apply a parametric EQ with a boost at 1000 Hz.
        engine
            .set_transform(TransformSpec::ParametricEq {
                bands: vec![EqBand {
                    frequency: 1000.0,
                    gain_db: 6.0,
                    q: 2.0,
                    band_type: BandType::Peak,
                }],
            })
            .unwrap();

        // Use oscillator source at 1000 Hz (should be boosted).
        engine
            .set_source(SourceSpec::Oscillator {
                waveform: fourier_core::WaveformType::Sine,
                frequency: 1000.0,
                amplitude: 0.5,
            })
            .unwrap();

        let mut consumer = io.output_consumer;
        thread::sleep(std::time::Duration::from_millis(200));

        let mut output = vec![0.0_f32; 8192];
        let n = consumer.pop_slice(&mut output);

        assert!(n > 0, "parametric EQ should produce output");
        let energy: f32 = output[..n].iter().map(|s| s * s).sum();
        assert!(energy > 0.0, "output should have nonzero energy");

        engine.shutdown();
    }

    #[test]
    fn engine_parametric_eq_switch_does_not_panic() {
        use fourier_core::transform::{BandType, EqBand};

        let fft_size = 256;
        let hop_size = 128;
        let sample_rate = 44100.0;

        let (engine, _io) = Engine::new(sample_rate, fft_size, hop_size).unwrap();

        // Rapidly switch between parametric EQ and other transforms.
        for _ in 0..10 {
            engine
                .set_transform(TransformSpec::ParametricEq {
                    bands: vec![
                        EqBand {
                            frequency: 200.0,
                            gain_db: 6.0,
                            q: 1.0,
                            band_type: BandType::LowShelf,
                        },
                        EqBand {
                            frequency: 2000.0,
                            gain_db: -3.0,
                            q: 4.0,
                            band_type: BandType::Peak,
                        },
                        EqBand {
                            frequency: 8000.0,
                            gain_db: 3.0,
                            q: 1.5,
                            band_type: BandType::HighShelf,
                        },
                    ],
                })
                .unwrap();
            engine.set_transform(TransformSpec::Identity).unwrap();
        }

        thread::sleep(std::time::Duration::from_millis(50));
        engine.shutdown();
    }
}
