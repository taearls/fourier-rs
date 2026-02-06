//! Audio device enumeration and selection via cpal.

use cpal::traits::{DeviceTrait, HostTrait};

/// Describes an available audio device.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// List all available input (capture) devices.
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .filter_map(|d| {
                let name = d.name().ok()?;
                Some(AudioDevice {
                    name,
                    is_input: true,
                    is_output: false,
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// List all available output (playback) devices.
pub fn list_output_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    match host.output_devices() {
        Ok(devices) => devices
            .filter_map(|d| {
                let name = d.name().ok()?;
                Some(AudioDevice {
                    name,
                    is_input: false,
                    is_output: true,
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Get the default input device, if available.
pub fn default_input_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    host.default_input_device()
}

/// Get the default output device, if available.
pub fn default_output_device() -> Option<cpal::Device> {
    let host = cpal::default_host();
    host.default_output_device()
}
