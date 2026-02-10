#![allow(clippy::expect_used)]
//! passthrough: Capture audio, run through FFT → identity → IFFT,
//! and play back through the default output device.
//!
//! This is Phase 2 validation: verify that the overlap-add pipeline
//! produces output matching the input (identity transform).

use fourier_engine::params::TransformSpec;
use fourier_engine::processor::Engine;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== passthrough: FFT → Identity → IFFT audio passthrough ===");
    println!("Audio will be captured and played back through the processing pipeline.");
    println!("Press Ctrl+C to stop.\n");

    let host = cpal::default_host();

    let input_device = host
        .default_input_device()
        .expect("No default input device");
    let output_device = host
        .default_output_device()
        .expect("No default output device");

    println!("Input:  {}", input_device.name().unwrap_or_default());
    println!("Output: {}", output_device.name().unwrap_or_default());

    let input_config = input_device
        .default_input_config()
        .expect("No default input config");
    let sample_rate = input_config.sample_rate().0;
    let channels = input_config.channels();
    println!("Sample rate: {sample_rate} Hz, Channels: {channels}\n");

    let fft_size = 2048;
    let hop_size = 1024;

    // Create the engine.
    let (engine, io) =
        Engine::new(sample_rate as f32, fft_size, hop_size).expect("Failed to create engine");
    engine
        .set_transform(TransformSpec::Identity)
        .expect("Failed to set transform");

    // Build input stream → engine's input ring buffer.
    let mut input_producer = io.input_producer;
    let input_stream = input_device
        .build_input_stream(
            &cpal::StreamConfig {
                channels: 1,
                sample_rate: cpal::SampleRate(sample_rate),
                buffer_size: cpal::BufferSize::Default,
            },
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                input_producer.push_slice(data);
            },
            |err| eprintln!("Input error: {err}"),
            None,
        )
        .expect("Failed to build input stream");

    // Build output stream ← engine's output ring buffer.
    let mut output_consumer = io.output_consumer;
    let output_stream = output_device
        .build_output_stream(
            &cpal::StreamConfig {
                channels: 1,
                sample_rate: cpal::SampleRate(sample_rate),
                buffer_size: cpal::BufferSize::Default,
            },
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let n = output_consumer.pop_slice(data);
                // Fill remainder with silence.
                for s in &mut data[n..] {
                    *s = 0.0;
                }
            },
            |err| eprintln!("Output error: {err}"),
            None,
        )
        .expect("Failed to build output stream");

    input_stream.play().expect("Failed to start input");
    output_stream.play().expect("Failed to start output");

    println!("Running... (Ctrl+C to stop)");

    // Main thread: periodically print spectral info.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));

        if let Some(snapshot) = engine.latest_snapshot() {
            if let Some(peak) = snapshot.peaks.first() {
                println!(
                    "Peak: {:.1} Hz ({:.1} dB)",
                    peak.frequency_hz, peak.magnitude_db
                );
            }
        }
    }
}
