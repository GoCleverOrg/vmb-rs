//! Error type shared by the `vmb-rs` domain and every `VmbRuntime` adapter.
//!
//! Vimba X itself returns `VmbError_t` (a signed 32-bit integer) from every
//! call. This module maps non-success return codes into a rich [`VmbError`]
//! enum that carries both the numeric code and a static human-readable
//! name.
//!
//! The Vimba X C API does not expose a runtime error-to-string function, so
//! the mapping from code to name is performed manually via [`error_name`]
//! below. The list of codes is derived from `VmbErrorType` in
//! `vmb_sys::bindings`; a drift guard in the `vmb` facade verifies the two
//! stay in sync.

use std::path::PathBuf;

use thiserror::Error;

use crate::Result;

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

    /// Failed to load the Vimba X shared library at runtime, or resolve
    /// one of its symbols. Typically surfaced from `VmbFfiRuntime::new`
    /// when the SDK is not installed on the host.
    #[error("failed to load Vimba X runtime: {message}")]
    LoadFailed {
        /// Human-readable description of the loader failure, including
        /// the path(s) tried and the underlying OS error.
        message: String,
    },
}

/// Map a Vimba error code to its static name. Returns `"VmbErrorUnknown"`
/// for codes the wrapper doesn't know about.
///
/// The list mirrors `vmb_sys::bindings::VmbErrorType`. The drift guard in
/// the `vmb` facade's integration tests verifies the two stay in sync.
pub const fn error_name(code: i32) -> &'static str {
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
pub fn check(code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(VmbError::Sdk {
            code,
            message: format!("VmbC call failed ({})", error_name(code)),
        })
    }
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
    fn error_name_covers_every_documented_code() {
        // Exhaustive over the full -41..=1 range (the SDK's current
        // numeric space). Deletion mutants on individual arms are
        // caught because removing any arm would reroute that code to
        // `VmbErrorUnrecognized`.
        let expected: &[(i32, &str)] = &[
            (0, "VmbErrorSuccess"),
            (-1, "VmbErrorInternalFault"),
            (-2, "VmbErrorApiNotStarted"),
            (-3, "VmbErrorNotFound"),
            (-4, "VmbErrorBadHandle"),
            (-5, "VmbErrorDeviceNotOpen"),
            (-6, "VmbErrorInvalidAccess"),
            (-7, "VmbErrorBadParameter"),
            (-8, "VmbErrorStructSize"),
            (-9, "VmbErrorMoreData"),
            (-10, "VmbErrorWrongType"),
            (-11, "VmbErrorInvalidValue"),
            (-12, "VmbErrorTimeout"),
            (-13, "VmbErrorOther"),
            (-14, "VmbErrorResources"),
            (-15, "VmbErrorInvalidCall"),
            (-16, "VmbErrorNoTL"),
            (-17, "VmbErrorNotImplemented"),
            (-18, "VmbErrorNotSupported"),
            (-19, "VmbErrorIncomplete"),
            (-20, "VmbErrorIO"),
            (-21, "VmbErrorValidValueSetNotPresent"),
            (-22, "VmbErrorGenTLUnspecified"),
            (-23, "VmbErrorUnspecified"),
            (-24, "VmbErrorBusy"),
            (-25, "VmbErrorNoData"),
            (-26, "VmbErrorParsingChunkData"),
            (-27, "VmbErrorInUse"),
            (-28, "VmbErrorUnknown"),
            (-29, "VmbErrorXml"),
            (-30, "VmbErrorNotAvailable"),
            (-31, "VmbErrorNotInitialized"),
            (-32, "VmbErrorInvalidAddress"),
            (-33, "VmbErrorAlready"),
            (-34, "VmbErrorNoChunkData"),
            (-35, "VmbErrorUserCallbackException"),
            (-36, "VmbErrorFeaturesUnavailable"),
            (-37, "VmbErrorTLNotFound"),
            (-39, "VmbErrorAmbiguous"),
            (-40, "VmbErrorRetriesExceeded"),
            (-41, "VmbErrorInsufficientBufferCount"),
            (1, "VmbErrorCustom"),
        ];
        for (code, name) in expected {
            assert_eq!(error_name(*code), *name, "wrong name for code {code}");
        }
        // A known gap in the SDK's enum (code `-38` is unused) must
        // fall through to the catch-all.
        assert_eq!(error_name(-38), "VmbErrorUnrecognized");
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

    #[test]
    fn check_success_is_ok() {
        assert!(check(0).is_ok());
    }

    #[test]
    fn check_error_is_sdk_with_code_and_name() {
        match check(-4) {
            Err(VmbError::Sdk { code, message }) => {
                assert_eq!(code, -4);
                assert!(message.contains("VmbErrorBadHandle"));
            }
            other => panic!("expected Err(Sdk), got {other:?}"),
        }
    }

    #[test]
    fn display_invalid_string_includes_context() {
        let err = VmbError::InvalidString {
            context: "camera_id",
        };
        assert!(format!("{err}").contains("camera_id"));
    }

    #[test]
    fn display_frame_too_small_includes_counts() {
        let err = VmbError::FrameTooSmall {
            expected: 100,
            actual: 80,
        };
        let s = format!("{err}");
        assert!(s.contains("100"));
        assert!(s.contains("80"));
    }

    #[test]
    fn display_io_includes_path() {
        let err = VmbError::Io {
            path: PathBuf::from("/tmp/does-not-exist.xml"),
            source: std::io::Error::other("boom"),
        };
        let s = format!("{err}");
        assert!(s.contains("does-not-exist.xml"));
    }

    #[test]
    fn already_started_and_not_started_display() {
        assert!(format!("{}", VmbError::AlreadyStarted).contains("already started"));
        assert!(format!("{}", VmbError::NotStarted).contains("not been started"));
    }

    #[test]
    fn capture_already_running_display() {
        assert!(format!("{}", VmbError::CaptureAlreadyRunning).contains("already running"));
    }

    #[test]
    fn load_failed_display_includes_message() {
        let err = VmbError::LoadFailed {
            message: "libVmbC.so: cannot open shared object file".to_string(),
        };
        let s = format!("{err}");
        assert!(s.contains("libVmbC.so"), "loader message missing from {s}");
        assert!(s.contains("load"), "error descriptor missing from {s}");
    }
}
