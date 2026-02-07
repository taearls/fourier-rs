//! Spectral analysis utilities: peak detection, magnitude computation, etc.

use num_complex::Complex;
use serde::{Deserialize, Serialize};

/// A detected spectral peak.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpectralPeak {
    /// Bin index in the spectrum.
    pub bin_index: usize,
    /// Frequency in Hz.
    pub frequency_hz: f32,
    /// Magnitude (linear scale).
    pub magnitude: f32,
    /// Magnitude in dB (relative to full scale).
    pub magnitude_db: f32,
}

/// Detect spectral peaks above a given magnitude threshold.
///
/// A peak is a bin whose magnitude is greater than both its neighbors
/// and above `threshold_db` (in dBFS).
///
/// Returns peaks sorted by magnitude (highest first).
pub fn detect_peaks(
    spectrum: &[Complex<f32>],
    sample_rate: f32,
    fft_size: usize,
    threshold_db: f32,
) -> Vec<SpectralPeak> {
    let bin_width = sample_rate / fft_size as f32;
    let n_bins = spectrum.len();
    let mut peaks = Vec::new();

    if n_bins < 3 {
        return peaks;
    }

    // Compute magnitudes once.
    let magnitudes: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();

    // Find local maxima (skip DC bin 0 and Nyquist bin).
    for i in 1..(n_bins - 1) {
        let mag = magnitudes[i];
        if mag > magnitudes[i - 1] && mag > magnitudes[i + 1] {
            let mag_db = if mag > 0.0 {
                20.0 * mag.log10()
            } else {
                f32::NEG_INFINITY
            };

            if mag_db >= threshold_db {
                peaks.push(SpectralPeak {
                    bin_index: i,
                    frequency_hz: i as f32 * bin_width,
                    magnitude: mag,
                    magnitude_db: mag_db,
                });
            }
        }
    }

    // Sort by magnitude descending.
    peaks.sort_by(|a, b| {
        b.magnitude
            .partial_cmp(&a.magnitude)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    peaks
}

/// Compute the magnitude spectrum in dB from complex FFT output.
pub fn magnitude_spectrum_db(spectrum: &[Complex<f32>]) -> Vec<f32> {
    spectrum
        .iter()
        .map(|c| {
            let mag = c.norm();
            if mag > 0.0 {
                20.0 * mag.log10()
            } else {
                f32::NEG_INFINITY
            }
        })
        .collect()
}

/// Compute the magnitude spectrum (linear) from complex FFT output.
pub fn magnitude_spectrum(spectrum: &[Complex<f32>]) -> Vec<f32> {
    spectrum.iter().map(|c| c.norm()).collect()
}

/// Convert a frequency bin index to frequency in Hz.
#[inline]
pub fn bin_to_frequency(bin_index: usize, sample_rate: f32, fft_size: usize) -> f32 {
    bin_index as f32 * sample_rate / fft_size as f32
}

/// Convert a frequency in Hz to the nearest bin index.
#[inline]
pub fn frequency_to_bin(frequency_hz: f32, sample_rate: f32, fft_size: usize) -> usize {
    (frequency_hz * fft_size as f32 / sample_rate).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bin_frequency_roundtrip() {
        let sr = 44100.0;
        let fft = 2048;
        let bin = 100;
        let freq = bin_to_frequency(bin, sr, fft);
        let recovered = frequency_to_bin(freq, sr, fft);
        assert_eq!(recovered, bin);
    }

    #[test]
    fn detect_peaks_finds_sinusoid() {
        // Construct a spectrum with a clear peak at bin 50.
        let n_bins = 513; // fft_size=1024 → 513 complex bins
        let mut spectrum = vec![Complex::new(0.0, 0.0); n_bins];
        spectrum[50] = Complex::new(100.0, 0.0);

        let peaks = detect_peaks(&spectrum, 44100.0, 1024, -100.0);
        assert!(!peaks.is_empty());
        assert_eq!(peaks[0].bin_index, 50);
    }
}
