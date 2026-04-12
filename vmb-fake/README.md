# vmb-fake

Programmable in-memory [`vmb_core::VmbRuntime`] implementation for unit
tests.

The fake records every port call, exposes a `FakeCall` log for sequence
assertions, and lets the test drive frame / discovery callbacks
synchronously via `deliver_frame` / `emit_discovery`. Each method can be
rigged to return an error on the *n*-th invocation so the domain's
error-unwind paths can be exercised deterministically.

This crate contains **no** `unsafe` blocks and has no dependency on
`vmb-sys` or the Vimba X SDK.
