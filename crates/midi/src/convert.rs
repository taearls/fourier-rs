//! Frequency ↔ MIDI note conversion utilities.
//!
//! Standard tuning: A4 = 440 Hz = MIDI note 69.
//! Formula: midi_note = 69 + 12 * log2(freq / 440)

/// Convert a frequency in Hz to a MIDI note number (floating-point for
/// pitch accuracy; round to nearest integer for standard note).
///
/// Returns `None` if frequency is <= 0.
pub fn frequency_to_midi(frequency_hz: f32) -> Option<f32> {
    if frequency_hz <= 0.0 {
        return None;
    }
    Some(69.0 + 12.0 * (frequency_hz / 440.0).log2())
}

/// Convert a MIDI note number to frequency in Hz.
///
/// Accepts fractional note numbers for microtonal accuracy.
pub fn midi_to_frequency(midi_note: f32) -> f32 {
    440.0 * 2.0_f32.powf((midi_note - 69.0) / 12.0)
}

/// Get the standard note name for a MIDI note number.
///
/// Returns e.g. "C4", "A#5", "Eb3".
pub fn midi_note_name(midi_note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (midi_note as i32 / 12) - 1;
    let name = NAMES[midi_note as usize % 12];
    format!("{}{}", name, octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_440() {
        let freq = midi_to_frequency(69.0);
        assert!((freq - 440.0).abs() < 0.01);
    }

    #[test]
    fn a4_roundtrip() {
        let midi = frequency_to_midi(440.0).unwrap();
        assert!((midi - 69.0).abs() < 0.01);
    }

    #[test]
    fn middle_c() {
        // MIDI note 60 = C4 ≈ 261.63 Hz
        let freq = midi_to_frequency(60.0);
        assert!((freq - 261.63).abs() < 0.1);
    }

    #[test]
    fn octave_is_double_frequency() {
        let a4 = midi_to_frequency(69.0);
        let a5 = midi_to_frequency(81.0);
        assert!((a5 / a4 - 2.0).abs() < 0.01);
    }

    #[test]
    fn note_names() {
        assert_eq!(midi_note_name(60), "C4");
        assert_eq!(midi_note_name(69), "A4");
        assert_eq!(midi_note_name(0), "C-1");
    }

    #[test]
    fn zero_frequency_returns_none() {
        assert!(frequency_to_midi(0.0).is_none());
        assert!(frequency_to_midi(-100.0).is_none());
    }
}
