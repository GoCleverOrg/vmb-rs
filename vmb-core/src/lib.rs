//! Runtime-agnostic domain types and ports for `vmb-rs`.
//!
//! This crate contains:
//!
//! * [`VmbError`] + [`Result`] — the error type of every port method and
//!   every domain operation.
//! * [`Frame`] + [`PixelFormat`] — the safe, borrowed view of a received
//!   frame handed to user callbacks.
//! * [`CameraInfo`] + [`DiscoveryEvent`] — domain value types.
//! * Opaque handle newtypes ([`CameraHandle`], [`FrameSlotId`], etc.) —
//!   identifiers the domain uses to refer to adapter-owned resources
//!   without leaking FFI pointer types.
//! * [`FrameCallback`] — the erased user closure struct delivered via
//!   a per-camera registry.
//!
//! `vmb-core` has no `unsafe` blocks and no dependency on `vmb-sys`. The
//! companion `vmb-ffi` crate hosts the adapter that links against
//! `libVmbC`; `vmb-fake` hosts a pure-Rust adapter used for unit tests.

#![forbid(unsafe_code)]

pub mod callback;
pub mod camera;
pub mod discovery;
pub mod error;
pub mod frame;
pub mod port;
pub mod system;
pub mod types;

pub use callback::FrameCallback;
pub use camera::Camera;
pub use discovery::{register_camera_discovery, DiscoveryRegistration};
pub use error::{check, error_name, VmbError};
pub use frame::{Frame, PixelFormat};
pub use port::{DiscoveryCallback, VmbRuntime};
pub use system::VmbSystem;
pub use types::{
    CameraHandle, CameraInfo, DiscoveryCallbackId, DiscoveryEvent, DiscoveryRegistrationHandle,
    FrameCallbackId, FrameSlotId,
};

/// Convenience alias: `Result<T, VmbError>`.
pub type Result<T> = std::result::Result<T, VmbError>;
