//! FFI adapter for the `vmb-core` [`VmbRuntime`] port.
//!
//! [`VmbFfiRuntime`] is the production backend that loads `libVmbC` at
//! runtime (via [`libloading`], wrapped by [`vmb_sys::VmbApi`]) and
//! dispatches every `VmbRuntime` call through the resolved function
//! pointers.
//!
//! Construction is fallible: if the Vimba X shared library is not
//! present on the host, [`VmbFfiRuntime::new`] returns
//! `Err(VmbError::LoadFailed { .. })`.
//!
//! [`VmbRuntime`]: vmb_core::VmbRuntime

#![deny(unsafe_op_in_unsafe_fn)]

mod runtime;
mod state;
mod trampoline;
mod util;

pub use runtime::VmbFfiRuntime;
