//! Safe Rust wrapper around Allied Vision Vimba X (VmbC).
//!
//! This crate wraps the raw `vmb-sys` FFI bindings with RAII-driven resource
//! ownership and a typed error type. It focuses on the minimal surface the
//! `mira-ingest` Vimba source needs: start/shutdown the runtime, enumerate
//! cameras, open a camera, load a settings XML, start/stop capture, and
//! register plug/unplug callbacks.
//!
//! # Feature flags
//!
//! * `sdk` (default **off**) — pulls in `vmb-sys/sdk` which links against
//!   `libVmbC` (Linux) or `VmbC.framework` (macOS). Without this feature the
//!   crate compiles to an empty stub and exposes only the error type.

#![deny(unsafe_op_in_unsafe_fn)]

mod error;

pub use error::{Result, VmbError};

#[cfg(feature = "sdk")]
mod callback;
#[cfg(feature = "sdk")]
mod camera;
#[cfg(feature = "sdk")]
mod discovery;
#[cfg(feature = "sdk")]
mod frame;
#[cfg(feature = "sdk")]
mod system;

#[cfg(feature = "sdk")]
pub use callback::FrameCallback;
#[cfg(feature = "sdk")]
pub use camera::{Camera, CameraInfo};
#[cfg(feature = "sdk")]
pub use discovery::{register_camera_discovery, DiscoveryEvent, DiscoveryRegistration};
#[cfg(feature = "sdk")]
pub use frame::{Frame, PixelFormat};
#[cfg(feature = "sdk")]
pub use system::VmbSystem;
