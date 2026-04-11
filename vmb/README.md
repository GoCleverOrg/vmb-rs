# vmb

A safe Rust wrapper around Allied Vision's **Vimba X** C API (`VmbC`).

This crate builds on [`vmb-sys`](https://crates.io/crates/vmb-sys) and exposes
an ergonomic, `Result`-based API for discovering and operating Vimba X cameras
from Rust. Most users should depend on `vmb` rather than on `vmb-sys` directly.

## Feature flags

| Feature | Default | Description |
| --- | --- | --- |
| `sdk`   | off     | Enables linking against the Vimba X SDK via `vmb-sys/sdk`. |

**Without `sdk`** the crate exposes a minimal stub so `cargo build` succeeds
on machines that do not have the Vimba X SDK installed. This allows CI, dev
environments, and contributors without hardware access to compile downstream
crates that depend on `vmb`.

**With `sdk`** the crate links against the real Vimba X SDK and its API can
be used to discover and operate Allied Vision cameras.

## SDK installation

The Vimba X SDK is a proprietary vendor SDK from Allied Vision. Download it
from <https://www.alliedvision.com/> and install it before building with the
`sdk` feature.

On Linux, the build script locates the SDK via `VIMBA_X_HOME` (or falls back
to `/usr/local/VimbaX_2023-4`). On macOS, it expects `VmbC.framework` under
`/Library/Frameworks/` by default, overridable via `VIMBA_X_HOME`.

See the [`vmb-sys` README](https://github.com/GoCleverOrg/vmb-rs/blob/main/vmb-sys/README.md)
for complete SDK discovery details.

## License

Dual-licensed under MIT or Apache-2.0 at your option.
