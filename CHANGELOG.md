# Changelog

All notable changes to the `vmb-rs` workspace will be documented in this
file. The format is based on [Keep a Changelog](https://keepachangelog.com),
and this project adheres to [Semantic Versioning](https://semver.org).

## [0.2.0] — unreleased

### Changed — breaking

- The workspace has been split into four crates. `vmb` is now a thin
  facade over `vmb-core` (domain + ports) and `vmb-ffi` (production
  adapter). A new `vmb-fake` crate ships an in-memory adapter for
  downstream unit tests.
- `VmbSystem`, `Camera`, and `DiscoveryRegistration` are now generic
  over the `VmbRuntime` trait. Call sites that named any of these types
  concretely need updating — see the migration section below.
- `VmbSystem::startup()` now takes a `VmbRuntime` argument. Use
  `vmb::real()` for the shorthand that wires the production FFI
  adapter; use `VmbSystem::startup(my_runtime)` for custom or test
  runtimes.
- `Camera::list()` and `Camera::open(id)` are gone. Use
  `system.list_cameras()` and `system.open_camera(id)` instead.
- `vmb::register_camera_discovery(cb)` is gone. Use
  `system.register_discovery(cb)` instead.
- The `Camera`, `VmbSystem`, and `DiscoveryRegistration` types no
  longer carry raw FFI pointers in their public state; all SDK handles
  live inside the FFI adapter.

### Added

- `VmbRuntime` trait + `FrameCallback` / `DiscoveryCallback` wrapper
  types in `vmb-core`.
- `VmbFfiRuntime` production adapter in `vmb-ffi` (links against
  `libVmbC`).
- `FakeVmbRuntime` in `vmb-fake` with programmable failure injection
  (`fail_nth`, `fail_next`), synchronous callback delivery
  (`emit_discovery`, `deliver_frame`), and a call log (`calls()`).
- Complete mira-parity developer infrastructure: `Makefile` with
  `rust-{fmt,clippy,deny,shear,typos,taplo,lint,test,nextest,
  test-features,mutants*}` targets, `.cargo/mutants.toml`,
  `.config/nextest.toml`, `mutants-bench.sh` harness,
  `[profile.mutants-bench]`, `rustfmt.toml` + `clippy.toml` +
  `deny.toml` + `_typos.toml` + `.taplo.toml` + `rust-toolchain.toml`.
- CI gains `feature-powerset`, `toml-lint`, `deny`, `shear`, and
  `mutants` jobs.
- Extensive unit tests in `vmb-core/tests/` against the fake — mutation
  score on the domain layer is **100 %** (74/74 caught, 17 unviable).

### Removed

- The process-global `STARTED: AtomicBool` in `vmb` (replaced by a
  per-runtime flag in `vmb-ffi`).
- `TrampolineContext` and the frame/discovery `extern "C"` trampolines
  from `vmb/src/` (moved to `vmb-ffi/src/trampoline.rs`).
- The `vmb::error::cstr_to_owned` helper from the public `vmb` surface
  (internal to `vmb-ffi` now).

### Migration from 0.1.x

```rust,ignore
// 0.1.x
use vmb::{Camera, VmbSystem};
let _system = VmbSystem::startup()?;
let cameras = Camera::list()?;
let mut camera = Camera::open(&cameras[0].id)?;
let _reg = vmb::register_camera_discovery(|ev| { /* ... */ })?;
```

```rust,ignore
// 0.2.0
use vmb::{Camera, VmbSystem};
let system = vmb::real()?;                        // or VmbSystem::startup(custom_runtime)
let cameras = system.list_cameras()?;
let mut camera = system.open_camera(&cameras[0].id)?;
let _reg = system.register_discovery(|ev| { /* ... */ })?;
```

If you keep concrete `Camera` / `VmbSystem` types in your own type
signatures, use the `RealVmbSystem` alias:

```rust,ignore
use vmb::{Camera, RealVmbSystem, VmbFfiRuntime};

struct MyApp {
    system: RealVmbSystem,
    camera: Camera<VmbFfiRuntime>,
}
```

## [0.1.0] — 2025-xx-xx

Initial release — safe RAII wrapper over `libVmbC` with camera
enumeration, capture, and hot-plug discovery.
