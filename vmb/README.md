# vmb

A safe Rust wrapper around Allied Vision's **Vimba X** C API (`VmbC`).

This crate builds on [`vmb-sys`](https://crates.io/crates/vmb-sys) and exposes
an ergonomic, `Result`-based API for discovering and operating Vimba X cameras
from Rust. Most users should depend on `vmb` rather than on `vmb-sys` directly.

The Vimba X C library is loaded dynamically at runtime — there is **no
Cargo feature** and **no build-time dependency** on the SDK. Missing-SDK
conditions surface as `VmbError::LoadFailed { .. }` when you call
`vmb::real()`.

## Usage

```toml
[dependencies]
vmb = "0.3"
```

```rust,ignore
let system = vmb::real()?;                // loads libVmbC + starts the runtime
let cameras = system.list_cameras()?;
for info in &cameras {
    println!("{}: {} ({})", info.id, info.model, info.serial);
}
```

## SDK installation

The Vimba X SDK is a proprietary vendor SDK from Allied Vision. Download it
from <https://www.alliedvision.com/> and install it on any host that
will actually call `vmb::real()` successfully.

On Linux, set `VIMBA_X_HOME` to the SDK install root, or make sure the
system dynamic linker can find `libVmbC.so` (via `LD_LIBRARY_PATH`,
`/etc/ld.so.conf`, or a symlink into `/usr/local/lib`). On macOS,
the default `/Library/Frameworks/VmbC.framework` location is
auto-discovered; override via `VIMBA_X_HOME` for custom installs.

Hosts without the SDK can still build and link against `vmb`; they just
can't load it at runtime. This allows CI, dev environments, and
contributors without hardware access to compile downstream crates that
depend on `vmb`.

See the [`vmb-sys` README](https://github.com/GoCleverOrg/vmb-rs/blob/main/vmb-sys/README.md)
for complete runtime discovery details.

## License

Dual-licensed under MIT or Apache-2.0 at your option.
