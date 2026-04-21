//! Safe Rust wrapper around Allied Vision Vimba X (VmbC).
//!
//! This crate is the publishable facade of the `vmb-rs` workspace. It
//! re-exports the domain types and ports from [`vmb_core`] plus the
//! production [`VmbFfiRuntime`] adapter from [`vmb_ffi`], and provides a
//! convenience [`real`] constructor that wires the two together.
//!
//! The Vimba X C library is loaded dynamically at runtime by
//! [`VmbFfiRuntime::new`]; there is no build-time dependency on the SDK.
//! `cargo build` succeeds on hosts where `libVmbC` is not installed —
//! [`real`] simply returns `Err(VmbError::LoadFailed { .. })` in that
//! case.
//!
//! ## Typical use
//!
//! ```no_run
//! # fn demo() -> vmb::Result<()> {
//! let system = vmb::real()?;                   // loads libVmbC + starts the runtime
//! let cameras = system.list_cameras()?;
//! println!("{} camera(s) detected", cameras.len());
//! # Ok(())
//! # }
//! ```

#![deny(unsafe_op_in_unsafe_fn)]

pub use vmb_core::{
    check, error_name, register_camera_discovery, Camera, CameraHandle, CameraInfo,
    DiscoveryCallback, DiscoveryCallbackId, DiscoveryEvent, DiscoveryRegistration,
    DiscoveryRegistrationHandle, Frame, FrameCallback, FrameCallbackId, FrameSlotId, PixelFormat,
    Result, VmbError, VmbRuntime, VmbSystem,
};
pub use vmb_ffi::VmbFfiRuntime;

/// Alias for the production `VmbSystem` type, parameterised with the
/// [`VmbFfiRuntime`] adapter.
pub type RealVmbSystem = vmb_core::VmbSystem<VmbFfiRuntime>;

/// Load the Vimba X C library and start the runtime.
///
/// Shorthand for `VmbSystem::startup(VmbFfiRuntime::new()?)`.
///
/// Returns [`VmbError::LoadFailed`] if `libVmbC` cannot be loaded on the
/// current host, and [`VmbError::AlreadyStarted`] if another runtime is
/// already alive in this process.
pub fn real() -> Result<RealVmbSystem> {
    let runtime = VmbFfiRuntime::new()?;
    vmb_core::VmbSystem::startup(runtime)
}
