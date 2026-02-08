#![allow(unsafe_code)]
//! fourier-ffi: C-ABI exports for Swift/native UI integration.
//!
//! Exposes the engine's functionality through `extern "C"` functions that
//! can be called from Swift, Objective-C, C, or any language supporting
//! the C ABI.
//!
//! This crate compiles to both a cdylib (.dylib/.so) and staticlib (.a).

use std::ptr;

use fourier_engine::params::TransformSpec;
use fourier_engine::processor::{Engine, EngineIo};

/// Opaque handle to the engine (pointer to a heap-allocated Engine).
///
/// Holds both the engine and the I/O ring buffer endpoints.
///
/// **Note**: The `io` field stores the [`EngineIo`] (input producer + output
/// consumer) so that the ring buffers stay alive for the duration of the
/// engine. Currently, no FFI functions expose the I/O endpoints directly —
/// the FFI layer supports parameter control and spectrum visualization only.
/// To connect audio I/O from native code, additional FFI functions that
/// borrow or take the ring buffer halves will be needed.
pub struct FfiEngine {
    engine: Engine,
    /// Keeps the ring buffer endpoints alive. See struct-level docs for
    /// current limitations.
    _io: Option<EngineIo>,
}

/// Create a new engine instance.
///
/// Returns a pointer to the engine, or null on failure.
/// Caller must eventually call `engine_destroy` to free resources.
#[no_mangle]
pub extern "C" fn engine_create(
    sample_rate: f32,
    fft_size: usize,
    hop_size: usize,
) -> *mut FfiEngine {
    let (engine, io) = Engine::new(sample_rate, fft_size, hop_size);
    let ffi = Box::new(FfiEngine {
        engine,
        _io: Some(io),
    });
    Box::into_raw(ffi)
}

/// Destroy an engine instance and free all resources.
///
/// # Safety
/// `handle` must be a valid pointer returned by `engine_create`, and must
/// not be used after this call.
#[no_mangle]
pub unsafe extern "C" fn engine_destroy(handle: *mut FfiEngine) {
    if !handle.is_null() {
        let ffi = unsafe { Box::from_raw(handle) };
        ffi.engine.shutdown();
    }
}

/// Set the output gain.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_gain(handle: *mut FfiEngine, gain: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi.engine.set_output_gain(gain);
    }
}

/// Set bypass mode.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_bypass(handle: *mut FfiEngine, bypass: bool) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi.engine.set_bypass(bypass);
    }
}

/// Set a low-pass filter transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_lowpass(handle: *mut FfiEngine, cutoff_hz: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi
            .engine
            .set_transform(TransformSpec::LowPass { cutoff_hz });
    }
}

/// Set a high-pass filter transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_highpass(handle: *mut FfiEngine, cutoff_hz: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi
            .engine
            .set_transform(TransformSpec::HighPass { cutoff_hz });
    }
}

/// Set a band-pass filter transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_bandpass(handle: *mut FfiEngine, low_hz: f32, high_hz: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi
            .engine
            .set_transform(TransformSpec::BandPass { low_hz, high_hz });
    }
}

/// Set the identity (passthrough) transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_identity(handle: *mut FfiEngine) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi.engine.set_transform(TransformSpec::Identity);
    }
}

/// Get the latest spectral magnitude data.
///
/// Writes up to `max_bins` magnitude values (in dB) into `out_magnitudes`.
/// Returns the number of bins actually written, or 0 if no data is available.
///
/// # Safety
/// - `handle` must be a valid pointer returned by `engine_create`.
/// - `out_magnitudes` must point to a writable array of at least `max_bins`
///   `f32` values, properly aligned for `f32` (4-byte alignment).
/// - The caller must ensure that `out_magnitudes` remains valid for the
///   duration of this call.
#[no_mangle]
pub unsafe extern "C" fn engine_get_spectrum(
    handle: *mut FfiEngine,
    out_magnitudes: *mut f32,
    max_bins: usize,
) -> usize {
    let Some(ffi) = (unsafe { handle.as_ref() }) else {
        return 0;
    };

    if out_magnitudes.is_null() {
        return 0;
    }

    match ffi.engine.latest_snapshot() {
        Some(snapshot) => {
            let n = snapshot.magnitude_db.len().min(max_bins);
            unsafe {
                ptr::copy_nonoverlapping(snapshot.magnitude_db.as_ptr(), out_magnitudes, n);
            }
            n
        }
        None => 0,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn ffi_create_destroy_lifecycle() {
        let handle = engine_create(44100.0, 256, 128);
        assert!(!handle.is_null());
        unsafe {
            engine_destroy(handle);
        }
    }

    #[test]
    fn ffi_destroy_null_is_safe() {
        // Destroying a null pointer should not panic.
        unsafe {
            engine_destroy(ptr::null_mut());
        }
    }

    #[test]
    fn ffi_set_filters() {
        let handle = engine_create(44100.0, 256, 128);
        assert!(!handle.is_null());

        unsafe {
            engine_set_identity(handle);
            engine_set_lowpass(handle, 1000.0);
            engine_set_highpass(handle, 500.0);
            engine_set_bandpass(handle, 200.0, 2000.0);
            engine_set_gain(handle, 0.5);
            engine_set_bypass(handle, true);
            engine_set_bypass(handle, false);
            engine_destroy(handle);
        }
    }

    #[test]
    fn ffi_get_spectrum_returns_zero_when_no_data() {
        let handle = engine_create(44100.0, 256, 128);
        assert!(!handle.is_null());

        let mut buf = vec![0.0_f32; 256];
        let n = unsafe { engine_get_spectrum(handle, buf.as_mut_ptr(), buf.len()) };
        // No audio data pushed, so no snapshot should be available.
        assert_eq!(n, 0);

        unsafe {
            engine_destroy(handle);
        }
    }

    #[test]
    fn ffi_get_spectrum_null_handle() {
        let mut buf = vec![0.0_f32; 256];
        let n = unsafe { engine_get_spectrum(ptr::null_mut(), buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n, 0);
    }

    #[test]
    fn ffi_get_spectrum_null_buffer() {
        let handle = engine_create(44100.0, 256, 128);
        assert!(!handle.is_null());

        let n = unsafe { engine_get_spectrum(handle, ptr::null_mut(), 256) };
        assert_eq!(n, 0);

        unsafe {
            engine_destroy(handle);
        }
    }

    #[test]
    fn ffi_operations_on_null_handle() {
        // All operations on null handle should be no-ops, not panics.
        unsafe {
            engine_set_gain(ptr::null_mut(), 1.0);
            engine_set_bypass(ptr::null_mut(), true);
            engine_set_lowpass(ptr::null_mut(), 1000.0);
            engine_set_highpass(ptr::null_mut(), 500.0);
            engine_set_bandpass(ptr::null_mut(), 200.0, 2000.0);
            engine_set_identity(ptr::null_mut());
        }
    }
}
