# vmb-ffi

Production [`vmb_core::VmbRuntime`] implementation backed by the Allied
Vision Vimba X C API (`libVmbC`). The shared library is loaded at
runtime via `vmb_sys::VmbApi` / [`libloading`] — there is no build-time
dependency on the SDK.

This crate is the sole location of `unsafe` blocks, FFI pointer types,
`extern "C"` trampolines, and SDK-specific constants in the `vmb-rs`
workspace. The domain layer talks to it through the [`VmbRuntime`] trait
only; the fake adapter in `vmb-fake` provides the same trait for tests.

`VmbFfiRuntime::new()` returns `Err(VmbError::LoadFailed { .. })` on
hosts where `libVmbC` is not installed. Test code that needs to
construct a runtime without loading a real shared library can use
`VmbFfiRuntime::with_api(Arc::new(mock_api))` with a `VmbApi` built from
dummy or spy function pointers.

End users should depend on the `vmb` facade crate instead of this one
directly.

[`libloading`]: https://crates.io/crates/libloading
