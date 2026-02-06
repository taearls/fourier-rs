//! fourier-midi: MIDI I/O, frequency↔MIDI conversion, and message routing.
//!
//! Wraps `midir` for cross-platform MIDI port management and provides
//! utilities for converting between frequency (Hz) and MIDI note numbers.

pub mod convert;
pub mod io;
pub mod message;

pub use convert::{frequency_to_midi, midi_note_name, midi_to_frequency};
pub use io::{list_midi_input_ports, list_midi_output_ports, MidiInput, MidiOutput};
pub use message::{MidiEvent, MidiEventReceiver, MidiEventSender};
