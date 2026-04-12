# vmb-core

Runtime-agnostic domain types and ports for the `vmb-rs` workspace. This
crate defines the error type, frame view, pixel format, opaque handles,
and the `VmbRuntime` port. It has **no** dependency on `vmb-sys` or any
FFI code and contains no `unsafe` blocks.

All `unsafe` FFI and the Vimba SDK integration live in the sibling
`vmb-ffi` crate, which provides the production `VmbRuntime` implementation.
Tests use `vmb-fake`, an in-memory `VmbRuntime` implementation.

End users almost always want the `vmb` facade crate, not this one directly.
