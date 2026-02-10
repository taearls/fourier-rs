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
}
