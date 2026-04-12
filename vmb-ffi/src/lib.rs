//! FFI adapter for the `vmb-core` [`VmbRuntime`] port.
//!
//! [`VmbFfiRuntime`] is the production backend that links against the
//! Vimba X C API. When the `sdk` feature is off, the crate compiles to an
//! empty rlib — [`VmbFfiRuntime`] simply does not exist, which is enough
//! to keep downstream `cargo build` green on hosts without the SDK.
//!
//! [`VmbRuntime`]: vmb_core::VmbRuntime

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(feature = "sdk")]
mod runtime;
#[cfg(feature = "sdk")]
mod state;
#[cfg(feature = "sdk")]
mod trampoline;
#[cfg(feature = "sdk")]
mod util;

#[cfg(feature = "sdk")]
pub use runtime::VmbFfiRuntime;
