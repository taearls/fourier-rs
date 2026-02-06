//! MIDI port enumeration and I/O via midir.

use midir::{MidiInputConnection, MidiOutputConnection};

use crate::message::{MidiEvent, MidiEventSender};

/// Lists available MIDI input port names.
pub fn list_midi_input_ports() -> Vec<String> {
    let Ok(midi_in) = midir::MidiInput::new("fourier-list-in") else {
        return Vec::new();
    };
    midi_in
        .ports()
        .iter()
        .filter_map(|p| midi_in.port_name(p).ok())
        .collect()
}

/// Lists available MIDI output port names.
pub fn list_midi_output_ports() -> Vec<String> {
    let Ok(midi_out) = midir::MidiOutput::new("fourier-list-out") else {
        return Vec::new();
    };
    midi_out
        .ports()
        .iter()
        .filter_map(|p| midi_out.port_name(p).ok())
        .collect()
}

/// An active MIDI input connection that routes events to a channel sender.
pub struct MidiInput {
    _connection: MidiInputConnection<()>,
    port_name: String,
}

impl MidiInput {
    /// Open the MIDI input port at the given index and route all incoming
    /// messages to the provided `sender`.
    ///
    /// The sender uses `try_send` to avoid blocking the MIDI callback.
    pub fn open(port_index: usize, sender: MidiEventSender) -> Result<Self, String> {
        let midi_in = midir::MidiInput::new("fourier-midi-in")
            .map_err(|e| format!("Failed to create MIDI input: {e}"))?;

        let ports = midi_in.ports();
        let port = ports
            .get(port_index)
            .ok_or_else(|| format!("MIDI input port index {port_index} out of range"))?;

        let port_name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| "unknown".to_string());

        let connection = midi_in
            .connect(
                port,
                "fourier-in",
                move |timestamp_us, data, ()| {
                    if let Some(event) = MidiEvent::from_raw(data, timestamp_us) {
                        // Non-blocking send: drop event if channel is full.
                        let _ = sender.try_send(event);
                    }
                },
                (),
            )
            .map_err(|e| format!("Failed to connect MIDI input: {e}"))?;

        Ok(Self {
            _connection: connection,
            port_name,
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

/// An active MIDI output connection for sending MIDI events.
pub struct MidiOutput {
    connection: MidiOutputConnection,
    port_name: String,
}

impl MidiOutput {
    /// Open the MIDI output port at the given index.
    pub fn open(port_index: usize) -> Result<Self, String> {
        let midi_out = midir::MidiOutput::new("fourier-midi-out")
            .map_err(|e| format!("Failed to create MIDI output: {e}"))?;

        let ports = midi_out.ports();
        let port = ports
            .get(port_index)
            .ok_or_else(|| format!("MIDI output port index {port_index} out of range"))?;

        let port_name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| "unknown".to_string());

        let connection = midi_out
            .connect(port, "fourier-out")
            .map_err(|e| format!("Failed to connect MIDI output: {e}"))?;

        Ok(Self {
            connection,
            port_name,
        })
    }

    /// Send a Note On message.
    pub fn note_on(&mut self, channel: u8, note: u8, velocity: u8) -> Result<(), String> {
        let msg = [0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F];
        self.connection
            .send(&msg)
            .map_err(|e| format!("Failed to send Note On: {e}"))
    }

    /// Send a Note Off message.
    pub fn note_off(&mut self, channel: u8, note: u8) -> Result<(), String> {
        let msg = [0x80 | (channel & 0x0F), note & 0x7F, 0];
        self.connection
            .send(&msg)
            .map_err(|e| format!("Failed to send Note Off: {e}"))
    }

    /// Send a Control Change message.
    pub fn control_change(&mut self, channel: u8, controller: u8, value: u8) -> Result<(), String> {
        let msg = [0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F];
        self.connection
            .send(&msg)
            .map_err(|e| format!("Failed to send CC: {e}"))
    }

    /// Send raw MIDI bytes.
    pub fn send_raw(&mut self, data: &[u8]) -> Result<(), String> {
        self.connection
            .send(data)
            .map_err(|e| format!("Failed to send MIDI: {e}"))
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}
