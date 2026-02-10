#![allow(clippy::expect_used)]
//! spectral-filter: Apply a spectral filter to live audio.
//!
//! Usage: spectral-filter [lowpass|highpass|bandpass] [freq1] [freq2]
//!
//! Examples:
//!   spectral-filter lowpass 1000
//!   spectral-filter highpass 500
//!   spectral-filter bandpass 200 2000

use fourier_engine::params::TransformSpec;
use fourier_engine::processor::Engine;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let transform = parse_transform(&args);
    println!("=== spectral-filter: Real-time spectral filtering ===");
    println!("Transform: {transform:?}");
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
    println!("Sample rate: {sample_rate} Hz\n");

    let fft_size = 2048;
    let hop_size = 1024;

    let (engine, io) =
        Engine::new(sample_rate as f32, fft_size, hop_size).expect("Failed to create engine");
    engine
        .set_transform(transform)
        .expect("Failed to set transform");

    // Wire up input.
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

    // Wire up output.
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

    println!("Filtering... (Ctrl+C to stop)");

    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(snapshot) = engine.latest_snapshot() {
            let n_peaks = snapshot.peaks.len();
            if let Some(peak) = snapshot.peaks.first() {
                println!(
                    "Peak: {:.1} Hz ({:.1} dB) | {} peaks detected",
                    peak.frequency_hz, peak.magnitude_db, n_peaks
                );
            }
        }
    }
}

fn parse_transform(args: &[String]) -> TransformSpec {
    if args.len() < 2 {
        println!("Usage: spectral-filter [lowpass|highpass|bandpass] [freq1] [freq2]");
        println!("Defaulting to lowpass at 2000 Hz");
        return TransformSpec::LowPass { cutoff_hz: 2000.0 };
    }

    match args[1].as_str() {
        "lowpass" | "lp" => {
            let cutoff = args
                .get(2)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(2000.0);
            TransformSpec::LowPass { cutoff_hz: cutoff }
        }
        "highpass" | "hp" => {
            let cutoff = args
                .get(2)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(500.0);
            TransformSpec::HighPass { cutoff_hz: cutoff }
        }
        "bandpass" | "bp" => {
            let low = args
                .get(2)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(200.0);
            let high = args
                .get(3)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(2000.0);
            TransformSpec::BandPass {
                low_hz: low,
                high_hz: high,
            }
        }
        "identity" => TransformSpec::Identity,
        other => {
            eprintln!("Unknown filter type '{other}', using identity");
            TransformSpec::Identity
        }
    }
}
