//! WAV file loading using the `hound` crate.

use std::path::Path;

use hound::{SampleFormat, WavReader};

use crate::error::FileIoError;
use crate::AudioBuffer;

/// Load a WAV file into an [`AudioBuffer`].
///
/// Supports 16-bit integer, 24-bit integer, and 32-bit float sample formats.
/// Samples are normalized to the `[-1.0, 1.0]` range. Stereo files are stored
/// with interleaved channels.
///
/// # Errors
///
/// Returns [`FileIoError::FileNotFound`] if the path does not exist,
/// [`FileIoError::InvalidFormat`] if the file cannot be parsed as WAV, or
/// [`FileIoError::UnsupportedFormat`] for sample formats other than the three
/// listed above.
pub fn load_wav(path: &Path) -> Result<AudioBuffer, FileIoError> {
    if !path.exists() {
        return Err(FileIoError::FileNotFound(path.to_path_buf()));
    }

    let reader = WavReader::open(path).map_err(|e| FileIoError::InvalidFormat(e.to_string()))?;
    let spec = reader.spec();
    let channels = spec.channels;
    let sample_rate = spec.sample_rate;
    let bits_per_sample = spec.bits_per_sample;
    let sample_format = spec.sample_format;

    let samples = match sample_format {
        SampleFormat::Int => read_int_samples(reader, bits_per_sample)?,
        SampleFormat::Float => read_float_samples(reader, bits_per_sample)?,
    };

    Ok(AudioBuffer {
        samples,
        sample_rate,
        channels,
    })
}

/// Read integer samples (16-bit or 24-bit) and normalize to `[-1.0, 1.0]`.
fn read_int_samples(
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, FileIoError> {
    let max_value = match bits_per_sample {
        16 => f64::from(i16::MAX),
        24 => f64::from(1_i32 << 23) - 1.0,
        other => {
            return Err(FileIoError::UnsupportedFormat(format!(
                "{other}-bit integer PCM is not supported"
            )));
        }
    };

    let samples: Result<Vec<f32>, _> = reader
        .into_samples::<i32>()
        .map(|s| {
            s.map(|v| {
                #[allow(clippy::cast_possible_truncation)]
                let normalized = (f64::from(v) / max_value) as f32;
                normalized.clamp(-1.0, 1.0)
            })
            .map_err(|e| FileIoError::InvalidFormat(e.to_string()))
        })
        .collect();

    samples
}

/// Read 32-bit float samples (already in `[-1.0, 1.0]` range, but clamp for safety).
fn read_float_samples(
    reader: WavReader<std::io::BufReader<std::fs::File>>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, FileIoError> {
    if bits_per_sample != 32 {
        return Err(FileIoError::UnsupportedFormat(format!(
            "{bits_per_sample}-bit float PCM is not supported"
        )));
    }

    let samples: Result<Vec<f32>, _> = reader
        .into_samples::<f32>()
        .map(|s| {
            s.map(|v| v.clamp(-1.0, 1.0))
                .map_err(|e| FileIoError::InvalidFormat(e.to_string()))
        })
        .collect();

    samples
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};
    use std::path::PathBuf;

    /// Helper: write a WAV file to a temp path and return the path.
    fn write_test_wav(
        spec: WavSpec,
        write_samples: impl FnOnce(&mut WavWriter<std::io::BufWriter<std::fs::File>>),
    ) -> PathBuf {
        let dir = std::env::temp_dir().join("fourier-file-io-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "test_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer = WavWriter::create(&path, spec).unwrap();
        write_samples(&mut writer);
        writer.finalize().unwrap();
        path
    }

    // ---- 16-bit integer tests ----

    #[test]
    fn load_16bit_mono() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let path = write_test_wav(spec, |w| {
            // Write a few known samples
            w.write_sample(0_i16).unwrap(); // silence
            w.write_sample(i16::MAX).unwrap(); // max positive
            w.write_sample(i16::MIN + 1).unwrap(); // near max negative
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 1);
        assert_eq!(buf.sample_rate, 44100);
        assert_eq!(buf.samples.len(), 3);
        assert!((buf.samples[0]).abs() < 1e-6, "silence sample should be ~0");
        assert!(
            (buf.samples[1] - 1.0).abs() < 1e-3,
            "max positive should be ~1.0"
        );
        assert!(
            (buf.samples[2] + 1.0).abs() < 1e-3,
            "max negative should be ~-1.0"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_16bit_stereo() {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let path = write_test_wav(spec, |w| {
            // Frame 1: left=max, right=0
            w.write_sample(i16::MAX).unwrap();
            w.write_sample(0_i16).unwrap();
            // Frame 2: left=0, right=max
            w.write_sample(0_i16).unwrap();
            w.write_sample(i16::MAX).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 2);
        assert_eq!(buf.sample_rate, 48000);
        assert_eq!(buf.samples.len(), 4);
        assert_eq!(buf.num_frames(), 2);
        // Interleaved: [L, R, L, R]
        assert!((buf.samples[0] - 1.0).abs() < 1e-3); // L max
        assert!(buf.samples[1].abs() < 1e-6); // R silence
        assert!(buf.samples[2].abs() < 1e-6); // L silence
        assert!((buf.samples[3] - 1.0).abs() < 1e-3); // R max

        std::fs::remove_file(&path).ok();
    }

    // ---- 24-bit integer tests ----

    #[test]
    fn load_24bit_mono() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 24,
            sample_format: SampleFormat::Int,
        };
        let max_24 = (1_i32 << 23) - 1; // 8388607
        let path = write_test_wav(spec, |w| {
            w.write_sample(0_i32).unwrap();
            w.write_sample(max_24).unwrap();
            w.write_sample(-max_24).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 1);
        assert_eq!(buf.samples.len(), 3);
        assert!(buf.samples[0].abs() < 1e-6);
        assert!((buf.samples[1] - 1.0).abs() < 1e-4);
        assert!((buf.samples[2] + 1.0).abs() < 1e-4);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_24bit_stereo() {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 96000,
            bits_per_sample: 24,
            sample_format: SampleFormat::Int,
        };
        let half_max = 1_i32 << 22; // ~0.5
        let path = write_test_wav(spec, |w| {
            w.write_sample(half_max).unwrap();
            w.write_sample(-half_max).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 2);
        assert_eq!(buf.num_frames(), 1);
        assert!((buf.samples[0] - 0.5).abs() < 0.01);
        assert!((buf.samples[1] + 0.5).abs() < 0.01);

        std::fs::remove_file(&path).ok();
    }

    // ---- 32-bit float tests ----

    #[test]
    fn load_32bit_float_mono() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let path = write_test_wav(spec, |w| {
            w.write_sample(0.0_f32).unwrap();
            w.write_sample(1.0_f32).unwrap();
            w.write_sample(-1.0_f32).unwrap();
            w.write_sample(0.5_f32).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 1);
        assert_eq!(buf.samples.len(), 4);
        assert!((buf.samples[0]).abs() < 1e-6);
        assert!((buf.samples[1] - 1.0).abs() < 1e-6);
        assert!((buf.samples[2] + 1.0).abs() < 1e-6);
        assert!((buf.samples[3] - 0.5).abs() < 1e-6);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_32bit_float_stereo() {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let path = write_test_wav(spec, |w| {
            w.write_sample(0.25_f32).unwrap();
            w.write_sample(-0.75_f32).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.channels, 2);
        assert_eq!(buf.num_frames(), 1);
        assert!((buf.samples[0] - 0.25).abs() < 1e-6);
        assert!((buf.samples[1] + 0.75).abs() < 1e-6);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn float_samples_clamped() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let path = write_test_wav(spec, |w| {
            w.write_sample(2.0_f32).unwrap(); // out of range positive
            w.write_sample(-3.0_f32).unwrap(); // out of range negative
        });

        let buf = load_wav(&path).unwrap();
        assert!((buf.samples[0] - 1.0).abs() < 1e-6, "should clamp to 1.0");
        assert!((buf.samples[1] + 1.0).abs() < 1e-6, "should clamp to -1.0");

        std::fs::remove_file(&path).ok();
    }

    // ---- Error tests ----

    #[test]
    fn error_file_not_found() {
        let path = PathBuf::from("/nonexistent/test.wav");
        let result = load_wav(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, FileIoError::FileNotFound(_)));
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn error_invalid_format() {
        // Write a file with garbage data
        let dir = std::env::temp_dir().join("fourier-file-io-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.wav");
        std::fs::write(&path, b"this is not a wav file").unwrap();

        let result = load_wav(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, FileIoError::InvalidFormat(_)));
        assert!(err.to_string().contains("invalid audio format"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn error_unsupported_8bit() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 8,
            sample_format: SampleFormat::Int,
        };
        let path = write_test_wav(spec, |w| {
            w.write_sample(0_i8).unwrap();
            w.write_sample(127_i8).unwrap();
        });

        let result = load_wav(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, FileIoError::UnsupportedFormat(_)));
        assert!(err.to_string().contains("8-bit"));

        std::fs::remove_file(&path).ok();
    }

    // ---- AudioBuffer method tests ----

    #[test]
    fn audio_buffer_num_frames() {
        let buf = AudioBuffer {
            samples: vec![0.0; 100],
            sample_rate: 44100,
            channels: 2,
        };
        assert_eq!(buf.num_frames(), 50);
    }

    #[test]
    fn audio_buffer_num_frames_mono() {
        let buf = AudioBuffer {
            samples: vec![0.0; 44100],
            sample_rate: 44100,
            channels: 1,
        };
        assert_eq!(buf.num_frames(), 44100);
    }

    #[test]
    fn audio_buffer_duration() {
        let buf = AudioBuffer {
            samples: vec![0.0; 44100],
            sample_rate: 44100,
            channels: 1,
        };
        assert!((buf.duration_secs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn audio_buffer_duration_stereo() {
        let buf = AudioBuffer {
            samples: vec![0.0; 96000],
            sample_rate: 48000,
            channels: 2,
        };
        assert!((buf.duration_secs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn audio_buffer_zero_channels() {
        let buf = AudioBuffer {
            samples: vec![0.0; 10],
            sample_rate: 44100,
            channels: 0,
        };
        assert_eq!(buf.num_frames(), 0);
    }

    #[test]
    fn audio_buffer_zero_sample_rate() {
        let buf = AudioBuffer {
            samples: vec![0.0; 10],
            sample_rate: 0,
            channels: 1,
        };
        assert!((buf.duration_secs()).abs() < 1e-6);
    }

    // ---- Normalization accuracy tests ----

    #[test]
    fn normalization_16bit_quarter() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let quarter = i16::MAX / 4;
        let path = write_test_wav(spec, |w| {
            w.write_sample(quarter).unwrap();
        });

        let buf = load_wav(&path).unwrap();
        assert!((buf.samples[0] - 0.25).abs() < 0.01);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_wav_sine_wave_16bit() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let num_samples = 441; // 10ms at 44100 Hz
        let freq = 440.0_f64;
        let path = write_test_wav(spec, |w| {
            for i in 0..num_samples {
                let t = i as f64 / 44100.0;
                let sample = (2.0 * std::f64::consts::PI * freq * t).sin();
                let quantized = (sample * f64::from(i16::MAX)) as i16;
                w.write_sample(quantized).unwrap();
            }
        });

        let buf = load_wav(&path).unwrap();
        assert_eq!(buf.samples.len(), num_samples);
        // Verify samples are in valid range
        for &s in &buf.samples {
            assert!((-1.0..=1.0).contains(&s));
        }
        // First sample should be ~0 (sin(0) = 0)
        assert!(buf.samples[0].abs() < 0.01);

        std::fs::remove_file(&path).ok();
    }

    // ---- Error trait tests ----

    #[test]
    fn error_display_messages() {
        let e1 = FileIoError::FileNotFound(PathBuf::from("/foo.wav"));
        assert!(e1.to_string().contains("/foo.wav"));

        let e2 = FileIoError::InvalidFormat("bad header".into());
        assert!(e2.to_string().contains("bad header"));

        let e3 = FileIoError::UnsupportedFormat("8-bit".into());
        assert!(e3.to_string().contains("8-bit"));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e4 = FileIoError::Io(io_err);
        assert!(e4.to_string().contains("denied"));
    }

    #[test]
    fn error_source() {
        use std::error::Error;

        let e1 = FileIoError::FileNotFound(PathBuf::from("/foo.wav"));
        assert!(e1.source().is_none());

        let io_err = std::io::Error::other("test");
        let e2 = FileIoError::Io(io_err);
        assert!(e2.source().is_some());
    }

    #[test]
    fn io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let file_err: FileIoError = io_err.into();
        assert!(matches!(file_err, FileIoError::Io(_)));
    }
}
