# vmb-ffi

Production [`vmb_core::VmbRuntime`] implementation that links against the
Allied Vision Vimba X C API (`libVmbC` on Linux, `VmbC.framework` on
macOS) via the `vmb-sys` bindings.

This crate is the sole location of `unsafe` blocks, FFI pointer types,
`extern "C"` trampolines, and SDK-specific constants in the `vmb-rs`
workspace. The domain layer talks to it through the [`VmbRuntime`] trait
only; the fake adapter in `vmb-fake` provides the same trait for tests.

End users should depend on the `vmb` facade crate instead of this one
directly.
