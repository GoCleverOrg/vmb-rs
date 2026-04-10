//! Error type for the safe `vmb` wrapper.
//!
//! Vimba X itself returns `VmbError_t` (a signed 32-bit integer) from every
//! call. This module maps non-success return codes into a rich [`VmbError`]
//! enum that carries both the numeric code and a static human-readable name.
//!
//! The Vimba X C API does not expose a runtime error-to-string function, so
//! the mapping from code to name is performed manually via [`error_name`]
//! below. The list of codes is derived from `VmbErrorType` in
//! `vmb_sys::bindings`.

use std::path::PathBuf;

use thiserror::Error;

/// Convert a possibly-null C string pointer to an owned `String`, falling
/// back to `fallback` when the pointer is null or the bytes are not valid
/// UTF-8. Used by the safe wrapper wherever the SDK hands us a
/// `*const c_char` we need to surface as a Rust string.
///
/// # Safety
///
/// If `ptr` is non-null, it MUST point to a NUL-terminated C string owned
/// by someone else for the duration of the call. The caller is
/// responsible for any thread-safety invariants of the underlying memory.
#[cfg(feature = "sdk")]
pub(crate) fn cstr_to_owned(ptr: *const std::os::raw::c_char, fallback: &str) -> String {
    if ptr.is_null() {
        return fallback.to_string();
    }
    // SAFETY: caller guarantees `ptr` is a valid NUL-terminated C string.
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    cstr.to_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| fallback.to_string())
}

/// Errors returned by the safe `vmb` wrapper.
#[derive(Debug, Error)]
pub enum VmbError {
    /// A non-success return code from a VmbC call.
    #[error("Vimba SDK error {code} ({}): {message}", error_name(*code))]
    Sdk {
        /// The raw `VmbError_t` return code.
        code: i32,
        /// A short description of which call failed.
        message: String,
    },

    /// The Vimba runtime has not been started, or was already shut down.
    #[error("Vimba X runtime has not been started")]
    NotStarted,

    /// Attempted to start the Vimba runtime while a previous `VmbSystem`
    /// instance is still alive. The runtime is process-global and the
    /// wrapper enforces a singleton invariant.
    #[error("Vimba X runtime is already started (singleton violation)")]
    AlreadyStarted,

    /// I/O failure while reading a settings XML file (or a related path).
    #[error("I/O error for {}: {source}", path.display())]
    Io {
        /// The offending path.
        path: PathBuf,
        /// The underlying [`std::io::Error`].
        #[source]
        source: std::io::Error,
    },

    /// A string supplied by the SDK or by the caller was not valid UTF-8 or
    /// contained an interior nul byte.
    #[error("invalid string (non-UTF-8 or interior nul) in {context}")]
    InvalidString {
        /// A static label identifying which string was rejected.
        context: &'static str,
    },

    /// `Camera::start_capture` was called while a previous capture is still
    /// running on the same camera.
    #[error("capture is already running on this camera")]
    CaptureAlreadyRunning,

    /// A received frame was smaller than expected for its declared format.
    #[error("frame too small: expected {expected} bytes, got {actual}")]
    FrameTooSmall {
        /// The declared/expected byte count.
        expected: usize,
        /// The actual byte count delivered by the SDK.
        actual: usize,
    },
}

/// Convenience alias: `Result<T, VmbError>`.
pub type Result<T> = std::result::Result<T, VmbError>;

/// Map a Vimba error code to its static name. Returns `"VmbErrorUnknown"`
/// for codes the wrapper doesn't know about.
///
/// The list mirrors `vmb_sys::bindings::VmbErrorType`. It is `pub(crate)`
/// because callers get the formatted string via `Display` on `VmbError`.
pub(crate) const fn error_name(code: i32) -> &'static str {
    match code {
        0 => "VmbErrorSuccess",
        -1 => "VmbErrorInternalFault",
        -2 => "VmbErrorApiNotStarted",
        -3 => "VmbErrorNotFound",
        -4 => "VmbErrorBadHandle",
        -5 => "VmbErrorDeviceNotOpen",
        -6 => "VmbErrorInvalidAccess",
        -7 => "VmbErrorBadParameter",
        -8 => "VmbErrorStructSize",
        -9 => "VmbErrorMoreData",
        -10 => "VmbErrorWrongType",
        -11 => "VmbErrorInvalidValue",
        -12 => "VmbErrorTimeout",
        -13 => "VmbErrorOther",
        -14 => "VmbErrorResources",
        -15 => "VmbErrorInvalidCall",
        -16 => "VmbErrorNoTL",
        -17 => "VmbErrorNotImplemented",
        -18 => "VmbErrorNotSupported",
        -19 => "VmbErrorIncomplete",
        -20 => "VmbErrorIO",
        -21 => "VmbErrorValidValueSetNotPresent",
        -22 => "VmbErrorGenTLUnspecified",
        -23 => "VmbErrorUnspecified",
        -24 => "VmbErrorBusy",
        -25 => "VmbErrorNoData",
        -26 => "VmbErrorParsingChunkData",
        -27 => "VmbErrorInUse",
        -28 => "VmbErrorUnknown",
        -29 => "VmbErrorXml",
        -30 => "VmbErrorNotAvailable",
        -31 => "VmbErrorNotInitialized",
        -32 => "VmbErrorInvalidAddress",
        -33 => "VmbErrorAlready",
        -34 => "VmbErrorNoChunkData",
        -35 => "VmbErrorUserCallbackException",
        -36 => "VmbErrorFeaturesUnavailable",
        -37 => "VmbErrorTLNotFound",
        -39 => "VmbErrorAmbiguous",
        -40 => "VmbErrorRetriesExceeded",
        -41 => "VmbErrorInsufficientBufferCount",
        1 => "VmbErrorCustom",
        _ => "VmbErrorUnrecognized",
    }
}

/// Convert a raw VmbC return code into a `Result`.
///
/// Returns `Ok(())` if `code == 0` (success), and `Err(VmbError::Sdk { .. })`
/// otherwise. The `message` is a generic placeholder — callers may wrap the
/// result with `.map_err(...)` to add call-site context if they wish.
#[cfg(feature = "sdk")]
pub(crate) fn check(code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(VmbError::Sdk {
            code,
            message: format!("VmbC call failed ({})", error_name(code)),
        })
    }
}

/// Stub `check` for builds without the `sdk` feature. The safe wrapper only
/// invokes VmbC calls under `#[cfg(feature = "sdk")]`, so this should never
/// be reached at runtime.
#[cfg(not(feature = "sdk"))]
#[allow(dead_code)]
pub(crate) fn check(_code: i32) -> Result<()> {
    unreachable!("vmb::error::check called without the `sdk` feature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_name_maps_known_codes() {
        assert_eq!(error_name(0), "VmbErrorSuccess");
        assert_eq!(error_name(-2), "VmbErrorApiNotStarted");
        assert_eq!(error_name(-41), "VmbErrorInsufficientBufferCount");
        assert_eq!(error_name(1), "VmbErrorCustom");
    }

    #[test]
    fn error_name_unknown_code_has_fallback() {
        assert_eq!(error_name(12345), "VmbErrorUnrecognized");
        assert_eq!(error_name(-999), "VmbErrorUnrecognized");
    }

    #[test]
    fn display_includes_error_name() {
        let err = VmbError::Sdk {
            code: -4,
            message: "bad handle".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("VmbErrorBadHandle"));
        assert!(s.contains("bad handle"));
    }

    /// Drift guard: ensure our handwritten `error_name` mapping stays in
    /// sync with bindgen's `Debug` impl on `VmbErrorType`. If bindings are
    /// regenerated and a variant is renamed, this test will fail for the
    /// sampled codes.
    ///
    /// We sample rather than exhaustively iterate because `VmbErrorType`
    /// is `#[non_exhaustive]` and we don't want to add a `num_enum`
    /// dependency just for this test.
    #[test]
    fn error_name_matches_bindgen_debug_for_sampled_variants() {
        use vmb_sys::VmbErrorType;
        let cases = [
            VmbErrorType::VmbErrorSuccess,
            VmbErrorType::VmbErrorInternalFault,
            VmbErrorType::VmbErrorBadHandle,
            VmbErrorType::VmbErrorBadParameter,
            VmbErrorType::VmbErrorTimeout,
            VmbErrorType::VmbErrorIO,
        ];
        for variant in cases {
            let code = variant as i32;
            let name = error_name(code);
            let debug_name = format!("{variant:?}");
            assert_eq!(
                name, debug_name,
                "drift for code {code}: handwritten={name}, bindgen={debug_name}"
            );
        }
    }
}
