//! MIDI event types and lock-free message passing channels.

use crossbeam_channel::{bounded, Receiver, Sender};

/// Represents a MIDI event with timing info.
#[derive(Debug, Clone, Copy)]
pub struct MidiEvent {
    /// MIDI status byte (e.g., 0x90 = Note On, 0x80 = Note Off).
    pub status: u8,
    /// MIDI note number (0–127).
    pub note: u8,
    /// Velocity (0–127).
    pub velocity: u8,
    /// Channel (0–15).
    pub channel: u8,
    /// Timestamp in microseconds (from midir).
    pub timestamp_us: u64,
}

impl MidiEvent {
    /// Parse a raw MIDI message (1–3 bytes) into a `MidiEvent`.
    pub fn from_raw(data: &[u8], timestamp_us: u64) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let status_byte = data[0];
        let channel = status_byte & 0x0F;
        let status = status_byte & 0xF0;

        let note = if data.len() > 1 { data[1] } else { 0 };
        let velocity = if data.len() > 2 { data[2] } else { 0 };

        Some(Self {
            status,
            note,
            velocity,
            channel,
            timestamp_us,
        })
    }

    /// Check if this is a Note On event (status 0x90 with velocity > 0).
    pub const fn is_note_on(&self) -> bool {
        self.status == 0x90 && self.velocity > 0
    }

    /// Check if this is a Note Off event (status 0x80, or Note On with velocity 0).
    pub const fn is_note_off(&self) -> bool {
        self.status == 0x80 || (self.status == 0x90 && self.velocity == 0)
    }

    /// Check if this is a Control Change event.
    pub const fn is_control_change(&self) -> bool {
        self.status == 0xB0
    }

    /// For Note On/Off events, get the frequency in Hz.
    pub fn frequency_hz(&self) -> f32 {
        super::convert::midi_to_frequency(f32::from(self.note))
    }
}

/// Sender end for MIDI events (used by MIDI input callback).
pub type MidiEventSender = Sender<MidiEvent>;

/// Receiver end for MIDI events (used by processing thread).
pub type MidiEventReceiver = Receiver<MidiEvent>;

/// Create a bounded MIDI event channel.
///
/// `capacity` is the max number of events that can be buffered.
/// The MIDI callback is real-time and must not block, so the sender
/// uses `try_send` and drops events if the channel is full.
pub fn midi_channel(capacity: usize) -> (MidiEventSender, MidiEventReceiver) {
    bounded(capacity)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_note_on() {
        let raw = [0x90, 60, 100]; // Note On, C4, velocity 100
        let evt = MidiEvent::from_raw(&raw, 0).unwrap();
        assert!(evt.is_note_on());
        assert!(!evt.is_note_off());
        assert_eq!(evt.note, 60);
        assert_eq!(evt.velocity, 100);
        assert_eq!(evt.channel, 0);
    }

    #[test]
    fn parse_note_off() {
        let raw = [0x80, 60, 0];
        let evt = MidiEvent::from_raw(&raw, 0).unwrap();
        assert!(evt.is_note_off());
        assert!(!evt.is_note_on());
    }

    #[test]
    fn note_on_zero_velocity_is_note_off() {
        let raw = [0x90, 60, 0];
        let evt = MidiEvent::from_raw(&raw, 0).unwrap();
        assert!(evt.is_note_off());
    }

    #[test]
    fn channel_extraction() {
        let raw = [0x95, 60, 100]; // Note On, channel 5
        let evt = MidiEvent::from_raw(&raw, 0).unwrap();
        assert_eq!(evt.channel, 5);
    }

    #[test]
    fn midi_channel_send_receive() {
        let (tx, rx) = midi_channel(16);
        let evt = MidiEvent {
            status: 0x90,
            note: 69,
            velocity: 127,
            channel: 0,
            timestamp_us: 0,
        };
        tx.send(evt).unwrap();
        let received = rx.recv().unwrap();
        assert_eq!(received.note, 69);
    }
}
