//! fourier-ffi: C-ABI exports for Swift/native UI integration.
//!
//! Exposes the engine's functionality through `extern "C"` functions that
//! can be called from Swift, Objective-C, C, or any language supporting
//! the C ABI.
//!
//! This crate compiles to both a cdylib (.dylib/.so) and staticlib (.a).

use std::ptr;

use fourier_engine::processor::{Engine, EngineIo};
use fourier_engine::params::TransformSpec;

/// Opaque handle to the engine (pointer to a heap-allocated Engine).
///
/// Holds both the engine and the I/O endpoints. The I/O can be taken
/// via FFI to connect to native audio streams.
pub struct FfiEngine {
    engine: Engine,
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
        let _ = ffi.engine.set_transform(TransformSpec::LowPass { cutoff_hz });
    }
}

/// Set a high-pass filter transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_highpass(handle: *mut FfiEngine, cutoff_hz: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi.engine.set_transform(TransformSpec::HighPass { cutoff_hz });
    }
}

/// Set a band-pass filter transform.
///
/// # Safety
/// `handle` must be a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn engine_set_bandpass(handle: *mut FfiEngine, low_hz: f32, high_hz: f32) {
    if let Some(ffi) = unsafe { handle.as_ref() } {
        let _ = ffi.engine.set_transform(TransformSpec::BandPass { low_hz, high_hz });
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
/// `handle` must be a valid engine pointer. `out_magnitudes` must point to
/// an array of at least `max_bins` f32 values.
#[no_mangle]
pub unsafe extern "C" fn engine_get_spectrum(
    handle: *mut FfiEngine,
    out_magnitudes: *mut f32,
    max_bins: usize,
) -> usize {
    let ffi = match unsafe { handle.as_ref() } {
        Some(f) => f,
        None => return 0,
    };

    match ffi.engine.try_recv_snapshot() {
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
