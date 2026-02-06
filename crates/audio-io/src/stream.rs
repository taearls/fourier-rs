//! Audio stream creation and management.
//!
//! Wraps cpal stream creation, providing a simpler interface that
//! automatically writes captured audio into a ring buffer (input)
//! and reads from a ring buffer for playback (output).

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream};

use crate::ring_buffer::{RingConsumer, RingProducer};

/// Configuration for audio streams.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_size: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            channels: 1,
            buffer_size: 512,
        }
    }
}

/// Manages an active audio input and/or output stream.
pub struct AudioStream {
    _input_stream: Option<Stream>,
    _output_stream: Option<Stream>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioStream {
    /// Create an input (capture) stream that writes samples into the ring buffer producer.
    ///
    /// The `producer` will be moved into the audio callback thread.
    pub fn open_input(
        device: &Device,
        config: &StreamConfig,
        mut producer: RingProducer,
    ) -> Result<Self, String> {
        let cpal_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
        };

        // Check supported format
        let supported = device
            .supported_input_configs()
            .map_err(|e| format!("Failed to query input configs: {e}"))?;

        let _format = supported
            .into_iter()
            .find(|c| c.sample_format() == SampleFormat::F32)
            .ok_or("No f32 input format supported")?;

        let stream = device
            .build_input_stream(
                &cpal_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Real-time callback: just push into ring buffer.
                    // If the buffer is full, samples are dropped (acceptable
                    // for real-time audio — processing thread must keep up).
                    producer.push_slice(data);
                },
                |err| {
                    eprintln!("Audio input error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start input: {e}"))?;

        Ok(Self {
            _input_stream: Some(stream),
            _output_stream: None,
            sample_rate: config.sample_rate,
            channels: config.channels,
        })
    }

    /// Create an output (playback) stream that reads samples from the ring buffer consumer.
    ///
    /// The `consumer` will be moved into the audio callback thread.
    pub fn open_output(
        device: &Device,
        config: &StreamConfig,
        mut consumer: RingConsumer,
    ) -> Result<Self, String> {
        let cpal_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
        };

        let stream = device
            .build_output_stream(
                &cpal_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Real-time callback: pull from ring buffer.
                    let read = consumer.pop_slice(data);
                    // Fill remainder with silence if buffer underrun.
                    for sample in &mut data[read..] {
                        *sample = 0.0;
                    }
                },
                |err| {
                    eprintln!("Audio output error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start output: {e}"))?;

        Ok(Self {
            _input_stream: None,
            _output_stream: Some(stream),
            sample_rate: config.sample_rate,
            channels: config.channels,
        })
    }
}
