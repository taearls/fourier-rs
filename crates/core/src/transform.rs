//! User-defined frequency-domain transforms.
//!
//! The [`SpectralTransform`] trait defines the interface for any operation
//! applied in the frequency domain between the FFT and IFFT stages.

use num_complex::Complex;

/// A single frequency bin with its metadata.
#[derive(Debug, Clone, Copy)]
pub struct FrequencyBin {
    /// Bin index (0 = DC, N/2 = Nyquist).
    pub index: usize,
    /// Frequency in Hz that this bin represents.
    pub frequency_hz: f32,
    /// Complex value (magnitude + phase).
    pub value: Complex<f32>,
}

impl FrequencyBin {
    #[inline]
    pub fn magnitude(&self) -> f32 {
        self.value.norm()
    }

    #[inline]
    pub fn phase(&self) -> f32 {
        self.value.arg()
    }

    /// Reconstruct the complex value from polar form.
    #[inline]
    pub fn from_polar(magnitude: f32, phase: f32) -> Complex<f32> {
        Complex::new(magnitude * phase.cos(), magnitude * phase.sin())
    }
}

/// Trait for frequency-domain transforms applied between FFT and IFFT.
///
/// Implementations receive the full complex spectrum and may modify it
/// in any way. The spectrum has `fft_size/2 + 1` bins due to real-valued
/// FFT conjugate symmetry.
pub trait SpectralTransform: Send {
    /// Apply the transform to the spectrum in-place.
    ///
    /// - `spectrum`: mutable slice of `fft_size/2 + 1` complex bins.
    /// - `sample_rate`: current audio sample rate in Hz.
    /// - `fft_size`: the FFT frame size (number of time-domain samples).
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize);

    /// Human-readable name for this transform.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Built-in transforms
// ---------------------------------------------------------------------------

/// Identity transform — passes audio through unmodified. Useful for testing.
pub struct IdentityTransform;

impl SpectralTransform for IdentityTransform {
    fn process(&mut self, _spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        // No-op.
    }

    fn name(&self) -> &'static str {
        "Identity"
    }
}

/// Brick-wall low-pass filter: zero all bins above the cutoff frequency.
pub struct LowPassFilter {
    pub cutoff_hz: f32,
}

impl SpectralTransform for LowPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq > self.cutoff_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &'static str {
        "Low-Pass Filter"
    }
}

/// Brick-wall high-pass filter: zero all bins below the cutoff frequency.
pub struct HighPassFilter {
    pub cutoff_hz: f32,
}

impl SpectralTransform for HighPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq < self.cutoff_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &'static str {
        "High-Pass Filter"
    }
}

/// Band-pass filter: keep only bins within [`low_hz`, `high_hz`].
pub struct BandPassFilter {
    pub low_hz: f32,
    pub high_hz: f32,
}

impl SpectralTransform for BandPassFilter {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let freq = i as f32 * bin_width;
            if freq < self.low_hz || freq > self.high_hz {
                *bin = Complex::new(0.0, 0.0);
            }
        }
    }

    fn name(&self) -> &'static str {
        "Band-Pass Filter"
    }
}

/// Spectral gain: multiply all bins by a constant factor.
pub struct SpectralGain {
    pub gain: f32,
}

impl SpectralTransform for SpectralGain {
    fn process(&mut self, spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        for bin in spectrum.iter_mut() {
            *bin *= self.gain;
        }
    }

    fn name(&self) -> &'static str {
        "Spectral Gain"
    }
}

// ---------------------------------------------------------------------------
// Parametric EQ
// ---------------------------------------------------------------------------

/// The type of EQ band filter shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BandType {
    /// Bell/peak filter: boost or cut centered at the target frequency.
    Peak,
    /// Low shelf: boost or cut frequencies below the target frequency.
    LowShelf,
    /// High shelf: boost or cut frequencies above the target frequency.
    HighShelf,
}

/// A single parametric EQ band.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EqBand {
    /// Center frequency in Hz.
    pub frequency: f32,
    /// Gain in dB (positive = boost, negative = cut).
    pub gain_db: f32,
    /// Q factor controlling bandwidth (higher = narrower).
    pub q: f32,
    /// Filter shape.
    pub band_type: BandType,
}

/// Parametric equalizer operating in the spectral domain.
///
/// Each band applies a smooth gain curve to the magnitude spectrum.
/// Multiple bands are combined multiplicatively (gains in dB are summed).
pub struct ParametricEq {
    /// The EQ bands to apply.
    pub bands: Vec<EqBand>,
}

impl ParametricEq {
    pub const fn new(bands: Vec<EqBand>) -> Self {
        Self { bands }
    }

    /// Compute the linear gain for a single band at the given frequency.
    ///
    /// `gain_linear` is the pre-computed `10^(gain_db/20)` for this band.
    fn band_gain(band: &EqBand, gain_linear: f32, freq_hz: f32) -> f32 {
        match band.band_type {
            BandType::Peak => {
                // Bell curve: gain = 1 + (G - 1) / (1 + (f/f0 - f0/f)^2 * Q^2)
                // where G is linear gain, f0 is center freq, Q controls bandwidth.
                let ratio = freq_hz / band.frequency;
                // Avoid division by zero at DC.
                if ratio <= 0.0 {
                    return 1.0;
                }
                let x = (ratio - 1.0 / ratio) * band.q;
                1.0 + (gain_linear - 1.0) / (1.0 + x * x)
            }
            BandType::LowShelf => {
                // Smooth transition: full gain below frequency, unity above.
                // Uses a sigmoid-like curve controlled by Q.
                let ratio = freq_hz / band.frequency;
                if ratio <= 0.0 {
                    return gain_linear;
                }
                let x = ratio.ln() * band.q;
                let sigmoid = 1.0 / (1.0 + x.exp());
                (gain_linear - 1.0).mul_add(sigmoid, 1.0)
            }
            BandType::HighShelf => {
                // Smooth transition: unity below frequency, full gain above.
                let ratio = freq_hz / band.frequency;
                if ratio <= 0.0 {
                    return 1.0;
                }
                let x = ratio.ln() * band.q;
                let sigmoid = 1.0 / (1.0 + (-x).exp());
                (gain_linear - 1.0).mul_add(sigmoid, 1.0)
            }
        }
    }
}

impl SpectralTransform for ParametricEq {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        // Pre-compute linear gains once per band to avoid redundant powf() per bin.
        let band_gains: Vec<f32> = self
            .bands
            .iter()
            .map(|band| {
                if band.frequency <= 0.0 || band.q <= 0.0 {
                    1.0
                } else {
                    10.0_f32.powf(band.gain_db / 20.0)
                }
            })
            .collect();

        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            // Skip DC bin (freq = 0).
            if i == 0 {
                continue;
            }
            let freq = i as f32 * bin_width;
            // Multiply gains from all bands (sum in dB domain).
            let mut total_gain = 1.0_f32;
            for (band, &gain_linear) in self.bands.iter().zip(&band_gains) {
                if (gain_linear - 1.0).abs() < f32::EPSILON {
                    continue;
                }
                total_gain *= Self::band_gain(band, gain_linear, freq);
            }
            *bin *= total_gain;
        }
    }

    fn name(&self) -> &'static str {
        "Parametric EQ"
    }
}

// ---------------------------------------------------------------------------
// Spectral Freeze
// ---------------------------------------------------------------------------

/// Spectral freeze: captures the current spectrum and continuously
/// resynthesizes it as a sustained sound.
///
/// When activated (`frozen = true`), the transform captures the current
/// spectral frame (magnitudes + phases). While frozen, it outputs the
/// captured spectrum instead of the live input, with a smooth crossfade
/// on toggle to avoid clicks.
///
/// This is a **spectral snapshot** freeze: the exact magnitudes and phases
/// from the capture moment are replayed each frame. Because the phases are
/// static, the resulting tone has a characteristic "metallic" or "resonant"
/// quality rather than a natural pad-like sustain.
pub struct SpectralFreeze {
    /// Whether the freeze effect is active.
    frozen: bool,
    /// Whether a spectrum has been captured for the current freeze activation.
    has_captured: bool,
    /// Captured spectrum (magnitudes) from the moment of freeze activation.
    captured_magnitudes: Vec<f32>,
    /// Captured spectrum (phases) from the moment of freeze activation.
    captured_phases: Vec<f32>,
    /// Current crossfade position: 0.0 = fully live, 1.0 = fully frozen.
    crossfade_pos: f32,
    /// Crossfade increment per frame, computed from sample rate and FFT/hop sizes.
    /// Targets ~75ms crossfade duration.
    crossfade_step: f32,
}

impl SpectralFreeze {
    /// Create a new spectral freeze effect.
    ///
    /// - `frozen`: initial freeze state.
    /// - `sample_rate`: audio sample rate in Hz.
    /// - `hop_size`: OLA hop size (frames between successive FFT windows).
    pub fn new(frozen: bool, sample_rate: f32, hop_size: usize) -> Self {
        // Target crossfade duration: ~75ms.
        // Frames per second = sample_rate / hop_size.
        // Number of frames in 75ms = (sample_rate / hop_size) * 0.075.
        // Step per frame = 1.0 / num_frames.
        let frames_per_sec = sample_rate / hop_size as f32;
        let crossfade_frames = (frames_per_sec * 0.075).max(1.0);
        let crossfade_step = 1.0 / crossfade_frames;

        Self {
            frozen,
            has_captured: false,
            captured_magnitudes: Vec::new(),
            captured_phases: Vec::new(),
            crossfade_pos: if frozen { 1.0 } else { 0.0 },
            crossfade_step,
        }
    }

    /// Set the freeze state. When transitioning to frozen, the next call
    /// to `process` will capture the spectrum.
    pub const fn set_frozen(&mut self, frozen: bool) {
        if frozen && !self.frozen {
            self.has_captured = false;
        }
        self.frozen = frozen;
    }

    /// Returns whether the freeze is currently active.
    pub const fn is_frozen(&self) -> bool {
        self.frozen
    }
}

impl SpectralTransform for SpectralFreeze {
    fn process(&mut self, spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        // Update crossfade position toward target.
        let target = if self.frozen { 1.0 } else { 0.0 };
        if (self.crossfade_pos - target).abs() > f32::EPSILON {
            if self.crossfade_pos < target {
                self.crossfade_pos = (self.crossfade_pos + self.crossfade_step).min(1.0);
            } else {
                self.crossfade_pos = (self.crossfade_pos - self.crossfade_step).max(0.0);
            }
        }

        // Capture spectrum when transitioning to frozen (first frame after activation).
        if self.frozen && !self.has_captured {
            self.captured_magnitudes = spectrum.iter().map(|c| c.norm()).collect();
            self.captured_phases = spectrum.iter().map(|c| c.arg()).collect();
            self.has_captured = true;
        }

        // If we have no captured data yet, pass through.
        if !self.has_captured {
            return;
        }

        // If fully live (crossfade = 0), nothing to do.
        if self.crossfade_pos < f32::EPSILON {
            // When unfrozen and crossfade complete, release captured data.
            if !self.frozen {
                self.captured_magnitudes.clear();
                self.captured_phases.clear();
                self.has_captured = false;
            }
            return;
        }

        // Interpolate between live spectrum and captured spectrum.
        let mix = self.crossfade_pos;
        for (i, bin) in spectrum.iter_mut().enumerate() {
            let frozen_bin =
                FrequencyBin::from_polar(self.captured_magnitudes[i], self.captured_phases[i]);
            // Linear interpolation in complex domain for smooth crossfade.
            *bin = *bin * (1.0 - mix) + frozen_bin * mix;
        }
    }

    fn name(&self) -> &'static str {
        "Spectral Freeze"
    }
}

// ---------------------------------------------------------------------------
// Pitch Shift
// ---------------------------------------------------------------------------

/// Spectral pitch shifter: remaps frequency bins to shift pitch up or down.
///
/// Uses the formula `source_bin = output_bin / 2^(semitones/12)` to remap
/// spectral bins. Linear interpolation is used for fractional bin positions
/// to avoid audible artifacts.
///
/// - `+12` semitones shifts up one octave (doubles all frequencies).
/// - `-12` semitones shifts down one octave (halves all frequencies).
/// - `0` semitones is identity (no change).
pub struct PitchShift {
    /// Pitch shift amount in semitones.
    pub shift_semitones: f32,
}

impl PitchShift {
    pub const fn new(shift_semitones: f32) -> Self {
        Self { shift_semitones }
    }
}

impl SpectralTransform for PitchShift {
    fn process(&mut self, spectrum: &mut [Complex<f32>], _sample_rate: f32, _fft_size: usize) {
        let num_bins = spectrum.len();
        if num_bins == 0 {
            return;
        }

        // No shift: identity.
        if self.shift_semitones.abs() < 1e-6 {
            return;
        }

        let shift_ratio = (self.shift_semitones / 12.0).exp2();
        let mut shifted = vec![Complex::new(0.0, 0.0); num_bins];

        // For each output bin, find the corresponding source bin.
        // source_bin = output_bin / shift_ratio
        for (i, out_bin) in shifted.iter_mut().enumerate().skip(1) {
            let src_idx = i as f32 / shift_ratio;

            // Source bin out of range — leave output bin at zero.
            if src_idx < 0.0 || src_idx >= (num_bins - 1) as f32 {
                continue;
            }

            let src_floor = src_idx.floor() as usize;
            let src_ceil = (src_floor + 1).min(num_bins - 1);
            let frac = src_idx - src_floor as f32;

            // Linear interpolation of magnitude.
            let mag_a = spectrum[src_floor].norm();
            let mag_b = spectrum[src_ceil].norm();
            let mag = mag_a.mul_add(1.0 - frac, mag_b * frac);

            // Phase from nearest source bin.
            let phase = if frac < 0.5 {
                spectrum[src_floor].arg()
            } else {
                spectrum[src_ceil].arg()
            };

            *out_bin = FrequencyBin::from_polar(mag, phase);
        }

        // Preserve DC bin (bin 0) — pitch shifting doesn't affect DC.
        shifted[0] = spectrum[0];

        spectrum.copy_from_slice(&shifted);
    }

    fn name(&self) -> &'static str {
        "Pitch Shift"
    }
}

/// Chain of transforms applied in sequence.
pub struct TransformChain {
    transforms: Vec<Box<dyn SpectralTransform>>,
}

impl TransformChain {
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    pub fn push(&mut self, transform: Box<dyn SpectralTransform>) {
        self.transforms.push(transform);
    }

    pub fn len(&self) -> usize {
        self.transforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }
}

impl Default for TransformChain {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectralTransform for TransformChain {
    fn process(&mut self, spectrum: &mut [Complex<f32>], sample_rate: f32, fft_size: usize) {
        for t in &mut self.transforms {
            t.process(spectrum, sample_rate, fft_size);
        }
    }

    fn name(&self) -> &'static str {
        "Transform Chain"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: create a flat spectrum (all bins = 1.0) and run a transform,
    // returning the resulting magnitudes per bin.
    // -----------------------------------------------------------------------
    fn apply_eq_to_flat_spectrum(
        eq: &mut ParametricEq,
        sample_rate: f32,
        fft_size: usize,
    ) -> Vec<f32> {
        let num_bins = fft_size / 2 + 1;
        let mut spectrum: Vec<Complex<f32>> =
            (0..num_bins).map(|_| Complex::new(1.0, 0.0)).collect();
        eq.process(&mut spectrum, sample_rate, fft_size);
        spectrum.iter().map(|c| c.norm()).collect()
    }

    fn freq_to_bin(freq: f32, sample_rate: f32, fft_size: usize) -> usize {
        (freq / (sample_rate / fft_size as f32)).round() as usize
    }

    // -----------------------------------------------------------------------
    // Peak band tests
    // -----------------------------------------------------------------------

    #[test]
    fn peak_band_boosts_center_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 2.0,
            band_type: BandType::Peak,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let center_bin = freq_to_bin(1000.0, 44100.0, 4096);
        let far_bin = freq_to_bin(10000.0, 44100.0, 4096);

        // Center should be boosted significantly above unity.
        assert!(
            mags[center_bin] > 3.0,
            "center bin magnitude {} should be > 3.0 (12 dB boost ~ 4x)",
            mags[center_bin]
        );
        // Far-away frequency should be near unity.
        assert!(
            (mags[far_bin] - 1.0).abs() < 0.1,
            "far bin magnitude {} should be near 1.0",
            mags[far_bin]
        );
    }

    #[test]
    fn peak_band_cuts_center_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: -12.0,
            q: 2.0,
            band_type: BandType::Peak,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let center_bin = freq_to_bin(1000.0, 44100.0, 4096);

        // Center should be attenuated.
        assert!(
            mags[center_bin] < 0.5,
            "center bin magnitude {} should be < 0.5 (-12 dB cut ~ 0.25x)",
            mags[center_bin]
        );
    }

    #[test]
    fn peak_band_bell_curve_shape() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 4.0,
            band_type: BandType::Peak,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let center_bin = freq_to_bin(1000.0, 44100.0, 4096);
        let near_bin = freq_to_bin(1200.0, 44100.0, 4096);
        let far_bin = freq_to_bin(5000.0, 44100.0, 4096);

        // Bell shape: center > near > far.
        assert!(
            mags[center_bin] > mags[near_bin],
            "center ({}) should be higher than nearby ({})",
            mags[center_bin],
            mags[near_bin]
        );
        assert!(
            mags[near_bin] > mags[far_bin],
            "nearby ({}) should be higher than far ({})",
            mags[near_bin],
            mags[far_bin]
        );
    }

    // -----------------------------------------------------------------------
    // Q parameter tests
    // -----------------------------------------------------------------------

    #[test]
    fn higher_q_produces_narrower_peak() {
        let sample_rate = 44100.0;
        let fft_size = 4096;

        // Low Q = wide band.
        let mut eq_low_q = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 0.5,
            band_type: BandType::Peak,
        }]);
        let mags_low = apply_eq_to_flat_spectrum(&mut eq_low_q, sample_rate, fft_size);

        // High Q = narrow band.
        let mut eq_high_q = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 8.0,
            band_type: BandType::Peak,
        }]);
        let mags_high = apply_eq_to_flat_spectrum(&mut eq_high_q, sample_rate, fft_size);

        // At a frequency offset (2000 Hz), the low-Q EQ should have more
        // gain than the high-Q EQ (wider spread).
        let offset_bin = freq_to_bin(2000.0, sample_rate, fft_size);
        assert!(
            mags_low[offset_bin] > mags_high[offset_bin],
            "low Q should have more gain at offset: low={}, high={}",
            mags_low[offset_bin],
            mags_high[offset_bin]
        );
    }

    // -----------------------------------------------------------------------
    // Low shelf tests
    // -----------------------------------------------------------------------

    #[test]
    fn low_shelf_boosts_below_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 2.0,
            band_type: BandType::LowShelf,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let low_bin = freq_to_bin(200.0, 44100.0, 4096);
        let high_bin = freq_to_bin(10000.0, 44100.0, 4096);

        // Below shelf frequency: boosted.
        assert!(
            mags[low_bin] > 2.0,
            "low bin magnitude {} should be boosted",
            mags[low_bin]
        );
        // Well above shelf frequency: near unity.
        assert!(
            (mags[high_bin] - 1.0).abs() < 0.1,
            "high bin magnitude {} should be near 1.0",
            mags[high_bin]
        );
    }

    #[test]
    fn low_shelf_cuts_below_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: -12.0,
            q: 2.0,
            band_type: BandType::LowShelf,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let low_bin = freq_to_bin(200.0, 44100.0, 4096);

        assert!(
            mags[low_bin] < 0.5,
            "low bin magnitude {} should be cut",
            mags[low_bin]
        );
    }

    // -----------------------------------------------------------------------
    // High shelf tests
    // -----------------------------------------------------------------------

    #[test]
    fn high_shelf_boosts_above_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 12.0,
            q: 2.0,
            band_type: BandType::HighShelf,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let low_bin = freq_to_bin(100.0, 44100.0, 4096);
        let high_bin = freq_to_bin(10000.0, 44100.0, 4096);

        // Well above shelf frequency: boosted.
        assert!(
            mags[high_bin] > 2.0,
            "high bin magnitude {} should be boosted",
            mags[high_bin]
        );
        // Well below shelf frequency: near unity.
        assert!(
            (mags[low_bin] - 1.0).abs() < 0.15,
            "low bin magnitude {} should be near 1.0",
            mags[low_bin]
        );
    }

    #[test]
    fn high_shelf_cuts_above_frequency() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: -12.0,
            q: 2.0,
            band_type: BandType::HighShelf,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let high_bin = freq_to_bin(10000.0, 44100.0, 4096);

        assert!(
            mags[high_bin] < 0.5,
            "high bin magnitude {} should be cut",
            mags[high_bin]
        );
    }

    // -----------------------------------------------------------------------
    // Multiple band combination
    // -----------------------------------------------------------------------

    #[test]
    fn multiple_bands_combine_correctly() {
        let mut eq = ParametricEq::new(vec![
            EqBand {
                frequency: 500.0,
                gain_db: 6.0,
                q: 2.0,
                band_type: BandType::Peak,
            },
            EqBand {
                frequency: 4000.0,
                gain_db: 6.0,
                q: 2.0,
                band_type: BandType::Peak,
            },
        ]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        let bin_500 = freq_to_bin(500.0, 44100.0, 4096);
        let bin_4000 = freq_to_bin(4000.0, 44100.0, 4096);
        let bin_2000 = freq_to_bin(2000.0, 44100.0, 4096);

        // Both centers should be boosted.
        assert!(mags[bin_500] > 1.5, "500 Hz band should be boosted");
        assert!(mags[bin_4000] > 1.5, "4000 Hz band should be boosted");
        // Between the two peaks, gain should be lower.
        assert!(
            mags[bin_2000] < mags[bin_500] && mags[bin_2000] < mags[bin_4000],
            "2000 Hz (between peaks) should be lower than either peak center"
        );
    }

    #[test]
    fn zero_gain_is_unity() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 1000.0,
            gain_db: 0.0,
            q: 2.0,
            band_type: BandType::Peak,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        // All bins (except DC) should remain at 1.0.
        for (i, &mag) in mags.iter().enumerate().skip(1) {
            assert!(
                (mag - 1.0).abs() < 1e-5,
                "bin {i}: magnitude {mag} should be 1.0 with 0 dB gain"
            );
        }
    }

    #[test]
    fn empty_bands_is_identity() {
        let mut eq = ParametricEq::new(vec![]);
        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 1024);
        for &mag in &mags[1..] {
            assert!((mag - 1.0).abs() < 1e-6, "empty EQ should be identity");
        }
    }

    #[test]
    fn dc_bin_is_untouched() {
        let mut eq = ParametricEq::new(vec![EqBand {
            frequency: 100.0,
            gain_db: 24.0,
            q: 1.0,
            band_type: BandType::LowShelf,
        }]);

        let mags = apply_eq_to_flat_spectrum(&mut eq, 44100.0, 4096);
        assert!(
            (mags[0] - 1.0).abs() < 1e-6,
            "DC bin should be untouched, got {}",
            mags[0]
        );
    }

    #[test]
    fn parametric_eq_name() {
        let eq = ParametricEq::new(vec![]);
        assert_eq!(eq.name(), "Parametric EQ");
    }

    // -----------------------------------------------------------------------
    // Existing tests
    // -----------------------------------------------------------------------

    #[test]
    fn low_pass_zeros_high_bins() {
        let mut spectrum: Vec<Complex<f32>> = (0..513).map(|_| Complex::new(1.0, 0.0)).collect();

        let sample_rate = 44100.0;
        let fft_size = 1024;
        let mut lp = LowPassFilter { cutoff_hz: 1000.0 };

        lp.process(&mut spectrum, sample_rate, fft_size);

        let bin_width = sample_rate / fft_size as f32;
        for (i, bin) in spectrum.iter().enumerate() {
            let freq = i as f32 * bin_width;
            if freq > 1000.0 {
                assert!(bin.norm() < 1e-6, "bin {i} at {freq:.1} Hz should be zero");
            } else {
                assert!(
                    (bin.norm() - 1.0).abs() < 1e-6,
                    "bin {i} should be untouched"
                );
            }
        }
    }

    #[test]
    fn chain_applies_in_order() {
        let mut chain = TransformChain::new();
        chain.push(Box::new(SpectralGain { gain: 2.0 }));
        chain.push(Box::new(SpectralGain { gain: 0.5 }));

        let mut spectrum = vec![Complex::new(1.0, 0.0); 513];
        chain.process(&mut spectrum, 44100.0, 1024);

        // 2.0 * 0.5 = 1.0, should be unchanged.
        for bin in &spectrum {
            assert!((bin.re - 1.0).abs() < 1e-6);
        }
    }

    // -----------------------------------------------------------------------
    // SpectralFreeze tests
    // -----------------------------------------------------------------------

    /// Helper: create a known spectrum with distinct magnitudes per bin.
    fn make_known_spectrum(num_bins: usize) -> Vec<Complex<f32>> {
        (0..num_bins)
            .map(|i| {
                let mag = (i as f32 + 1.0) * 0.1;
                let phase = (i as f32) * 0.5;
                FrequencyBin::from_polar(mag, phase)
            })
            .collect()
    }

    #[test]
    fn freeze_captures_spectrum_on_activation() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let hop_size = 512;
        let num_bins = fft_size / 2 + 1;

        // Create a freeze that starts frozen.
        let mut freeze = SpectralFreeze::new(true, sample_rate, hop_size);
        let mut spectrum = make_known_spectrum(num_bins);
        let original_mags: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();

        // Process once to capture.
        freeze.process(&mut spectrum, sample_rate, fft_size);

        // After enough frames to fully crossfade, the output should match captured.
        // Run many frames to complete the crossfade.
        for _ in 0..200 {
            // Feed a different spectrum (zeros) — freeze should ignore it.
            let mut zero_input = vec![Complex::new(0.0, 0.0); num_bins];
            freeze.process(&mut zero_input, sample_rate, fft_size);

            // Once fully frozen, output should match the captured spectrum.
            if (freeze.crossfade_pos - 1.0).abs() < f32::EPSILON {
                for (i, bin) in zero_input.iter().enumerate() {
                    let expected_mag = original_mags[i];
                    let actual_mag = bin.norm();
                    assert!(
                        (actual_mag - expected_mag).abs() < 0.01,
                        "bin {i}: expected mag {expected_mag:.4}, got {actual_mag:.4}"
                    );
                }
                return;
            }
        }

        panic!("crossfade did not complete");
    }

    #[test]
    fn freeze_output_is_stable() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let hop_size = 512;
        let num_bins = fft_size / 2 + 1;

        let mut freeze = SpectralFreeze::new(true, sample_rate, hop_size);

        // Capture a known spectrum.
        let mut frame = make_known_spectrum(num_bins);
        freeze.process(&mut frame, sample_rate, fft_size);

        // Run until fully frozen.
        for _ in 0..200 {
            frame = vec![Complex::new(0.0, 0.0); num_bins];
            freeze.process(&mut frame, sample_rate, fft_size);
        }

        // Capture the frozen output.
        let mut frozen_a = vec![Complex::new(0.0, 0.0); num_bins];
        freeze.process(&mut frozen_a, sample_rate, fft_size);

        let mut frozen_b = vec![Complex::new(0.0, 0.0); num_bins];
        freeze.process(&mut frozen_b, sample_rate, fft_size);

        // Both should be identical (stable output).
        for (i, (a, b)) in frozen_a.iter().zip(&frozen_b).enumerate() {
            assert!(
                (a.re - b.re).abs() < 1e-6 && (a.im - b.im).abs() < 1e-6,
                "bin {i}: frozen output should be stable, got ({}, {}) vs ({}, {})",
                a.re,
                a.im,
                b.re,
                b.im
            );
        }
    }

    #[test]
    fn freeze_unfrozen_passes_through() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let hop_size = 512;
        let num_bins = fft_size / 2 + 1;

        // Start unfrozen.
        let mut freeze = SpectralFreeze::new(false, sample_rate, hop_size);

        let original: Vec<Complex<f32>> = (0..num_bins)
            .map(|i| Complex::new(i as f32 * 0.01, -(i as f32) * 0.005))
            .collect();
        let mut spectrum = original.clone();

        freeze.process(&mut spectrum, sample_rate, fft_size);

        // Should be pass-through (identical to input).
        for (i, (orig, out)) in original.iter().zip(&spectrum).enumerate() {
            assert!(
                (orig.re - out.re).abs() < 1e-6 && (orig.im - out.im).abs() < 1e-6,
                "bin {i}: unfrozen should pass through"
            );
        }
    }

    #[test]
    fn freeze_crossfade_is_smooth() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let hop_size = 512;
        let num_bins = fft_size / 2 + 1;

        let mut freeze = SpectralFreeze::new(false, sample_rate, hop_size);

        // Process a few frames unfrozen.
        for _ in 0..5 {
            let mut spectrum = make_known_spectrum(num_bins);
            freeze.process(&mut spectrum, sample_rate, fft_size);
        }

        // Activate freeze.
        freeze.set_frozen(true);

        // Track crossfade values — they should monotonically increase.
        let mut prev_pos = 0.0_f32;
        for _ in 0..200 {
            let mut spectrum = vec![Complex::new(0.0, 0.0); num_bins];
            freeze.process(&mut spectrum, sample_rate, fft_size);

            assert!(
                freeze.crossfade_pos >= prev_pos - f32::EPSILON,
                "crossfade should increase monotonically: {} -> {}",
                prev_pos,
                freeze.crossfade_pos
            );
            prev_pos = freeze.crossfade_pos;

            if (freeze.crossfade_pos - 1.0).abs() < f32::EPSILON {
                break;
            }
        }

        assert!(
            (freeze.crossfade_pos - 1.0).abs() < f32::EPSILON,
            "crossfade should reach 1.0"
        );
    }

    #[test]
    fn freeze_toggle_off_crossfades_back() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let hop_size = 512;
        let num_bins = fft_size / 2 + 1;

        let mut freeze = SpectralFreeze::new(true, sample_rate, hop_size);

        // Capture and complete crossfade to frozen.
        for _ in 0..200 {
            let mut spectrum = make_known_spectrum(num_bins);
            freeze.process(&mut spectrum, sample_rate, fft_size);
        }
        assert!(
            (freeze.crossfade_pos - 1.0).abs() < f32::EPSILON,
            "should be fully frozen"
        );

        // Now unfreeze.
        freeze.set_frozen(false);

        // Crossfade should decrease back to 0.
        let mut prev_pos = 1.0_f32;
        for _ in 0..200 {
            let mut spectrum = make_known_spectrum(num_bins);
            freeze.process(&mut spectrum, sample_rate, fft_size);

            assert!(
                freeze.crossfade_pos <= prev_pos + f32::EPSILON,
                "crossfade should decrease: {} -> {}",
                prev_pos,
                freeze.crossfade_pos
            );
            prev_pos = freeze.crossfade_pos;

            if freeze.crossfade_pos < f32::EPSILON {
                break;
            }
        }

        assert!(
            freeze.crossfade_pos < f32::EPSILON,
            "crossfade should reach 0.0 after unfreezing"
        );
    }

    #[test]
    fn freeze_getters() {
        let freeze = SpectralFreeze::new(true, 44100.0, 512);
        assert!(freeze.is_frozen());

        let freeze2 = SpectralFreeze::new(false, 44100.0, 512);
        assert!(!freeze2.is_frozen());
    }

    #[test]
    fn freeze_name() {
        let freeze = SpectralFreeze::new(false, 44100.0, 512);
        assert_eq!(freeze.name(), "Spectral Freeze");
    }

    #[test]
    fn freeze_empty_spectrum_no_panic() {
        let mut freeze = SpectralFreeze::new(true, 44100.0, 512);
        let mut spectrum: Vec<Complex<f32>> = Vec::new();
        // Should not panic on empty spectrum.
        freeze.process(&mut spectrum, 44100.0, 0);
    }

    // -----------------------------------------------------------------------
    // PitchShift tests
    // -----------------------------------------------------------------------

    /// Helper: create a spectrum with a single peak at the given frequency.
    fn make_sine_spectrum(freq_hz: f32, sample_rate: f32, fft_size: usize) -> Vec<Complex<f32>> {
        let num_bins = fft_size / 2 + 1;
        let bin_width = sample_rate / fft_size as f32;
        let target_bin = (freq_hz / bin_width).round() as usize;

        let mut spectrum = vec![Complex::new(0.0, 0.0); num_bins];
        if target_bin < num_bins {
            spectrum[target_bin] = Complex::new(1.0, 0.0);
        }
        spectrum
    }

    /// Helper: find the bin index with the largest magnitude (excluding DC).
    fn find_peak_bin(spectrum: &[Complex<f32>]) -> usize {
        spectrum
            .iter()
            .enumerate()
            .skip(1) // skip DC
            .max_by(|(_, a), (_, b)| a.norm().partial_cmp(&b.norm()).unwrap())
            .map(|(i, _)| i)
            .unwrap()
    }

    #[test]
    fn pitch_shift_up_octave_doubles_frequency() {
        let sample_rate = 44100.0;
        let fft_size = 4096;
        let bin_width = sample_rate / fft_size as f32;

        let mut spectrum = make_sine_spectrum(440.0, sample_rate, fft_size);
        let mut ps = PitchShift::new(12.0); // +12 semitones = one octave up

        ps.process(&mut spectrum, sample_rate, fft_size);

        let peak_bin = find_peak_bin(&spectrum);
        let peak_freq = peak_bin as f32 * bin_width;

        // +12 semitones should move 440 Hz to ~880 Hz.
        assert!(
            (peak_freq - 880.0).abs() < bin_width * 2.0,
            "expected peak near 880 Hz, got {peak_freq:.1} Hz (bin {peak_bin})"
        );
    }

    #[test]
    fn pitch_shift_down_octave_halves_frequency() {
        let sample_rate = 44100.0;
        let fft_size = 4096;
        let bin_width = sample_rate / fft_size as f32;

        let mut spectrum = make_sine_spectrum(880.0, sample_rate, fft_size);
        let mut ps = PitchShift::new(-12.0); // -12 semitones = one octave down

        ps.process(&mut spectrum, sample_rate, fft_size);

        let peak_bin = find_peak_bin(&spectrum);
        let peak_freq = peak_bin as f32 * bin_width;

        // -12 semitones should move 880 Hz to ~440 Hz.
        assert!(
            (peak_freq - 440.0).abs() < bin_width * 2.0,
            "expected peak near 440 Hz, got {peak_freq:.1} Hz (bin {peak_bin})"
        );
    }

    #[test]
    fn pitch_shift_down_fifth() {
        let sample_rate = 44100.0;
        let fft_size = 4096;
        let bin_width = sample_rate / fft_size as f32;

        let mut spectrum = make_sine_spectrum(440.0, sample_rate, fft_size);
        let mut ps = PitchShift::new(-7.0); // -7 semitones = down a fifth

        ps.process(&mut spectrum, sample_rate, fft_size);

        let peak_bin = find_peak_bin(&spectrum);
        let peak_freq = peak_bin as f32 * bin_width;
        let expected = 440.0 * 2.0_f32.powf(-7.0 / 12.0); // ~293.66 Hz

        assert!(
            (peak_freq - expected).abs() < bin_width * 2.0,
            "expected peak near {expected:.1} Hz, got {peak_freq:.1} Hz"
        );
    }

    #[test]
    fn pitch_shift_zero_is_identity() {
        let sample_rate = 44100.0;
        let fft_size = 1024;

        let original = make_sine_spectrum(440.0, sample_rate, fft_size);
        let mut spectrum = original.clone();
        let mut ps = PitchShift::new(0.0);

        ps.process(&mut spectrum, sample_rate, fft_size);

        // With 0 shift, spectrum should be unchanged.
        for (i, (orig, shifted)) in original.iter().zip(&spectrum).enumerate() {
            assert!(
                (orig.re - shifted.re).abs() < 1e-6 && (orig.im - shifted.im).abs() < 1e-6,
                "bin {i}: zero shift should be identity"
            );
        }
    }

    #[test]
    fn pitch_shift_fractional_uses_interpolation() {
        let sample_rate = 44100.0;
        let fft_size = 4096;
        let bin_width = sample_rate / fft_size as f32;

        let mut spectrum = make_sine_spectrum(440.0, sample_rate, fft_size);
        let mut ps = PitchShift::new(1.0); // +1 semitone

        ps.process(&mut spectrum, sample_rate, fft_size);

        let peak_bin = find_peak_bin(&spectrum);
        let peak_freq = peak_bin as f32 * bin_width;
        let expected = 440.0 * 2.0_f32.powf(1.0 / 12.0); // ~466.16 Hz

        assert!(
            (peak_freq - expected).abs() < bin_width * 2.0,
            "expected peak near {expected:.1} Hz, got {peak_freq:.1} Hz"
        );

        // Verify the peak magnitude is nonzero (interpolation produced a value).
        assert!(
            spectrum[peak_bin].norm() > 0.1,
            "interpolated peak should have significant magnitude"
        );
    }

    #[test]
    fn pitch_shift_preserves_dc_bin() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let num_bins = fft_size / 2 + 1;

        let mut spectrum = vec![Complex::new(0.0, 0.0); num_bins];
        spectrum[0] = Complex::new(0.5, 0.0); // DC offset

        let original_dc = spectrum[0];
        let mut ps = PitchShift::new(7.0);

        ps.process(&mut spectrum, sample_rate, fft_size);

        assert!(
            (spectrum[0].re - original_dc.re).abs() < 1e-6
                && (spectrum[0].im - original_dc.im).abs() < 1e-6,
            "DC bin should be preserved"
        );
    }

    #[test]
    fn pitch_shift_large_up_shift_clears_high_bins() {
        let sample_rate = 44100.0;
        let fft_size = 1024;
        let num_bins = fft_size / 2 + 1;

        // Place a peak near Nyquist.
        let mut spectrum = vec![Complex::new(0.0, 0.0); num_bins];
        spectrum[num_bins - 2] = Complex::new(1.0, 0.0);

        let mut ps = PitchShift::new(12.0); // +12 up
        ps.process(&mut spectrum, sample_rate, fft_size);

        // The original peak was near Nyquist; shifting up should push it out of range.
        // Most bins should be zero (except DC which is preserved).
        let energy: f32 = spectrum[1..].iter().map(|c| c.norm()).sum();
        assert!(
            energy < 0.01,
            "shifting near-Nyquist peak up should produce near-zero energy, got {energy}"
        );
    }

    #[test]
    fn pitch_shift_name() {
        let ps = PitchShift::new(5.0);
        assert_eq!(ps.name(), "Pitch Shift");
    }

    #[test]
    fn pitch_shift_empty_spectrum_no_panic() {
        let mut ps = PitchShift::new(12.0);
        let mut spectrum: Vec<Complex<f32>> = Vec::new();
        // Should not panic on empty spectrum.
        ps.process(&mut spectrum, 44100.0, 0);
    }

    #[test]
    fn pitch_shift_negative_fractional() {
        let sample_rate = 44100.0;
        let fft_size = 4096;
        let bin_width = sample_rate / fft_size as f32;

        let mut spectrum = make_sine_spectrum(1000.0, sample_rate, fft_size);
        let mut ps = PitchShift::new(-3.5); // -3.5 semitones

        ps.process(&mut spectrum, sample_rate, fft_size);

        let peak_bin = find_peak_bin(&spectrum);
        let peak_freq = peak_bin as f32 * bin_width;
        let expected = 1000.0 * 2.0_f32.powf(-3.5 / 12.0);

        assert!(
            (peak_freq - expected).abs() < bin_width * 2.0,
            "expected peak near {expected:.1} Hz, got {peak_freq:.1} Hz"
        );
    }
}
