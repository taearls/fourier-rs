//! capture-fft: Capture audio from the default input device, run FFT,
//! and print the peak frequency to the terminal.
//!
//! This is Phase 1 of the build order: validate the audio → FFT pipeline
//! end-to-end as a CLI tool.

use fourier_audio_io::ring_buffer::AudioRingBuffer;
use fourier_core::fft::FftProcessor;
use fourier_core::spectral::detect_peaks;
use fourier_core::window::{WindowFunction, WindowType};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() {
    println!("=== capture-fft: Real-time FFT peak detection ===");
    println!("Press Ctrl+C to stop.\n");

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .expect("No default input device available");
    println!("Input device: {}", device.name().unwrap_or_default());

    let supported_config = device
        .default_input_config()
        .expect("No default input config");
    let sample_rate = supported_config.sample_rate().0 as f32;
    let channels = supported_config.channels() as usize;
    println!("Sample rate: {} Hz, Channels: {}\n", sample_rate, channels);

    let fft_size: usize = 2048;
    let ring_capacity = fft_size * 4;
    let (mut producer, mut consumer) = AudioRingBuffer::new(ring_capacity);

    // Build the input stream — writes captured audio into the ring buffer.
    let stream = device
        .build_input_stream(
            &cpal::StreamConfig {
                channels: channels as u16,
                sample_rate: cpal::SampleRate(sample_rate as u32),
                buffer_size: cpal::BufferSize::Default,
            },
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if channels == 1 {
                    producer.push_slice(data);
                } else {
                    // Mix to mono by averaging channels.
                    for chunk in data.chunks(channels) {
                        let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                        producer.push_slice(&[mono]);
                    }
                }
            },
            |err| eprintln!("Input stream error: {}", err),
            None,
        )
        .expect("Failed to build input stream");

    stream.play().expect("Failed to start input stream");

    // Processing loop: read frames, window, FFT, detect peaks.
    let mut fft = FftProcessor::new(fft_size);
    let window = WindowFunction::new(WindowType::Hann, fft_size);
    let mut frame = vec![0.0_f32; fft_size];
    let mut spectrum = fft.alloc_spectrum();

    loop {
        // Wait for enough samples.
        let n = consumer.pop_slice(&mut frame);
        if n < fft_size {
            // Not enough data yet; sleep briefly and try again.
            std::thread::sleep(std::time::Duration::from_millis(10));
            // Shift partial data to the beginning and read more.
            if n > 0 {
                // Simple approach: just wait for a full frame next time.
                continue;
            }
            continue;
        }

        // Apply window.
        window.apply(&mut frame);

        // Forward FFT.
        fft.forward(&mut frame, &mut spectrum);

        // Detect peaks.
        let peaks = detect_peaks(&spectrum, sample_rate, fft_size, -40.0);

        if let Some(peak) = peaks.first() {
            let midi_note = fourier_midi::frequency_to_midi(peak.frequency_hz);
            let note_str = midi_note
                .map(|n| {
                    let note_num = n.round() as u8;
                    format!(
                        "{} (MIDI {})",
                        fourier_midi::midi_note_name(note_num),
                        note_num
                    )
                })
                .unwrap_or_default();

            println!(
                "Peak: {:.1} Hz ({:.1} dB) {}",
                peak.frequency_hz, peak.magnitude_db, note_str
            );
        }
    }
}
