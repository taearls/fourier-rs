//! fourier-midi: MIDI I/O, frequency↔MIDI conversion, and message routing.
//!
//! Wraps `midir` for cross-platform MIDI port management and provides
//! utilities for converting between frequency (Hz) and MIDI note numbers.

pub mod convert;
pub mod io;
pub mod message;

pub use convert::{frequency_to_midi, midi_to_frequency, midi_note_name};
pub use io::{MidiInput, MidiOutput, list_midi_input_ports, list_midi_output_ports};
pub use message::{MidiEvent, MidiEventReceiver, MidiEventSender};
