//! fourier-audio-io: Audio I/O via cpal with lock-free ring buffers.
//!
//! Provides device enumeration, input/output stream creation, and
//! SPSC ring buffers for passing audio between the real-time audio
//! callback thread and the processing thread.

pub mod device;
pub mod error;
pub mod ring_buffer;
pub mod stream;

pub use device::{
    default_input_device, default_output_device, list_input_devices, list_output_devices,
    AudioDevice,
};
pub use error::AudioIoError;
pub use ring_buffer::AudioRingBuffer;
pub use stream::{AudioStream, StreamConfig};
