//! Safe Rust wrapper around Allied Vision Vimba X (VmbC).
//!
//! This crate is the publishable facade of the `vmb-rs` workspace. It
//! re-exports the domain types and ports from [`vmb_core`], and — when
//! the `sdk` feature is enabled — the production [`VmbFfiRuntime`]
//! adapter from [`vmb_ffi`] plus a convenience [`real`] constructor that
//! wires the two together.
//!
//! ## Typical use
//!
//! ```no_run
//! # #[cfg(feature = "sdk")]
//! # fn demo() -> vmb::Result<()> {
//! let system = vmb::real()?;                   // starts the Vimba runtime
//! let cameras = system.list_cameras()?;
//! println!("{} camera(s) detected", cameras.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Feature flags
//!
//! * `sdk` (default **off**) — pulls in `vmb-ffi/sdk` which links
//!   against `libVmbC` (Linux) or `VmbC.framework` (macOS). Without
//!   this feature only the domain layer ([`VmbError`], [`Frame`], …)
//!   is exposed and `cargo build` succeeds without the SDK installed.

#![deny(unsafe_op_in_unsafe_fn)]

pub use vmb_core::{
    check, error_name, register_camera_discovery, Camera, CameraHandle, CameraInfo,
    DiscoveryCallback, DiscoveryCallbackId, DiscoveryEvent, DiscoveryRegistration,
    DiscoveryRegistrationHandle, Frame, FrameCallback, FrameCallbackId, FrameSlotId, PixelFormat,
    Result, VmbError, VmbRuntime, VmbSystem,
};

#[cfg(feature = "sdk")]
pub use vmb_ffi::VmbFfiRuntime;

/// Alias for the production `VmbSystem` type, parameterised with the
/// [`VmbFfiRuntime`] adapter.
#[cfg(feature = "sdk")]
pub type RealVmbSystem = vmb_core::VmbSystem<VmbFfiRuntime>;

/// Start the Vimba X runtime via the production FFI adapter.
///
/// Shorthand for `VmbSystem::startup(VmbFfiRuntime::new())`.
#[cfg(feature = "sdk")]
pub fn real() -> Result<RealVmbSystem> {
    vmb_core::VmbSystem::startup(VmbFfiRuntime::new())
}
