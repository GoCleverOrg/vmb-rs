//! C-ABI trampolines invoked by the Vimba SDK.
//!
//! Both trampolines follow the same shape:
//!
//! 1. Catch any panic — unwinding into C is UB.
//! 2. Recover the Rust-side context from a raw pointer the SDK holds on
//!    our behalf (`frame.context[0]` for frame callbacks,
//!    `user_context` for discovery callbacks).
//! 3. Dispatch to the user closure (and, for frame callbacks, re-queue
//!    the frame via the runtime-loaded VmbC function pointer stored on
//!    the context).
//!
//! The two context structs exist only to own their closure refs, the
//! runtime `Arc<VmbApi>` handle, and (for frame callbacks) the backing
//! pixel buffer. They never escape this crate.

use std::slice;
use std::sync::Arc;

use tracing::warn;
use vmb_core::{DiscoveryCallback, DiscoveryEvent, Frame, FrameCallback, PixelFormat};
use vmb_sys::VmbApi;

use crate::util::cstr_to_owned;

/// Per-announced-frame trampoline context.
///
/// Each `VmbFrame_t` has a 4-slot `context` array; we store a pointer to
/// this struct in `context[0]`, and the trampoline uses it to retrieve
/// the [`FrameCallback`] and runtime-loaded `VmbApi` on every frame
/// completion.
#[repr(C)]
pub(crate) struct TrampolineContext {
    /// The SDK-visible frame descriptor. The `buffer` pointer inside
    /// points into `self.buffer`; `context[0]` is patched to point back
    /// at `self`.
    pub(crate) frame: vmb_sys::VmbFrame_t,
    /// Heap storage backing `frame.buffer`. Kept alive for the lifetime
    /// of the context; the SDK writes into it via the raw pointer stored
    /// in `frame.buffer`, so Rust-side field reads are intentionally
    /// absent after construction.
    #[allow(dead_code)]
    buffer: Box<[u8]>,
    /// The user's callback, shared across all announced frames.
    callback: Arc<FrameCallback>,
    /// Runtime VmbC handle; used by the trampoline to re-queue frames
    /// via `VmbCaptureFrameQueue` from inside the C callback.
    api: Arc<VmbApi>,
}

// SAFETY: `TrampolineContext` holds a `VmbFrame_t` whose raw pointer
// fields (`buffer`, `imageData`, `context[..]`) make it `!Send + !Sync`
// by default. The adapter owns the only Rust reference, moves it
// between threads only while capture is not in flight (announce /
// revoke paths hold the `FfiState.frames` mutex), and the SDK is the
// only other reader — bound by Vimba's documented thread-safety rules.
// The `Arc<VmbApi>` is already `Send + Sync`.
unsafe impl Send for TrampolineContext {}
unsafe impl Sync for TrampolineContext {}

impl TrampolineContext {
    /// Allocate a new context with a `payload_bytes`-sized backing
    /// buffer, the given shared callback, and a VmbC API handle.
    pub(crate) fn new(
        callback: Arc<FrameCallback>,
        payload_bytes: usize,
        api: Arc<VmbApi>,
    ) -> Self {
        let mut buffer: Box<[u8]> = vec![0u8; payload_bytes].into_boxed_slice();
        let buffer_ptr = buffer.as_mut_ptr() as *mut std::os::raw::c_void;

        // SAFETY: A zeroed `VmbFrame_t` is the SDK's documented
        // zero-initialization convention — all its fields are either
        // integers or nullable pointers, so zero is a valid bit pattern.
        let mut frame: vmb_sys::VmbFrame_t = unsafe { std::mem::zeroed() };
        frame.buffer = buffer_ptr;
        frame.bufferSize = payload_bytes as u32;

        Self {
            frame,
            buffer,
            callback,
            api,
        }
    }

    /// Return a mutable pointer to the underlying `VmbFrame_t`, patching
    /// `context[0]` to point at the enclosing `TrampolineContext` first.
    ///
    /// This must be called **after** the `TrampolineContext` has been
    /// placed into its final `Box` (so `self` has a stable address).
    pub(crate) fn vmb_frame_mut_ptr(&mut self) -> *mut vmb_sys::VmbFrame_t {
        self.frame.context[0] = self as *mut _ as *mut std::os::raw::c_void;
        &mut self.frame as *mut _
    }

    /// Swap the stored user callback. Used by the adapter when the
    /// two-step `announce_frame` + `queue_frame` sequence supplies the
    /// callback in the second call.
    pub(crate) fn set_callback(&mut self, callback: Arc<FrameCallback>) {
        self.callback = callback;
    }
}

/// Per-registered-subscription trampoline context for camera discovery.
///
/// Hands the SDK a thin `*mut c_void` pointer while preserving a live
/// `Arc<DiscoveryCallback>` to dispatch to plus the runtime `Arc<VmbApi>`
/// needed to read discovery-event features from inside the callback.
pub(crate) struct DiscoveryTrampolineCtx {
    pub(crate) callback: Arc<DiscoveryCallback>,
    pub(crate) api: Arc<VmbApi>,
}

/// C-ABI trampoline that Vimba invokes on every received frame.
///
/// # Safety
///
/// Called only by the Vimba SDK. Must not unwind into C (panics are
/// caught at the boundary). Must not take references that outlive the
/// callback invocation. The SDK guarantees `frame` is a valid pointer to
/// a `VmbFrame_t` for the duration of the call and that `context[0]`
/// points at a live `TrampolineContext` whose lifetime is owned by the
/// adapter state.
pub(crate) unsafe extern "C" fn frame_callback_trampoline(
    camera_handle: vmb_sys::VmbHandle_t,
    _stream_handle: vmb_sys::VmbHandle_t,
    frame_ptr: *mut vmb_sys::VmbFrame_t,
) {
    // Catch panics at the FFI boundary — unwinding into C is UB.
    let _ = std::panic::catch_unwind(|| {
        if frame_ptr.is_null() {
            return;
        }
        // SAFETY: the SDK promises `frame_ptr` is valid for the duration
        // of this call.
        let frame = unsafe { &mut *frame_ptr };

        let ctx_ptr = frame.context[0] as *const TrampolineContext;
        if ctx_ptr.is_null() {
            return;
        }
        // SAFETY: `TrampolineContext` is owned by `FfiState.frames`,
        // which lives until the matching `capture_end` /
        // `frame_revoke_all` (both of which the SDK blocks on pending
        // callbacks).
        let ctx = unsafe { &*ctx_ptr };

        // `VmbFrameStatusComplete = 0` — dispatch only completed frames.
        if frame.receiveStatus == 0 {
            let data_ptr = frame.imageData as *const u8;
            let data_len = frame.bufferSize as usize;
            let data_slice: &[u8] = if data_ptr.is_null() || data_len == 0 {
                &[]
            } else {
                // SAFETY: the SDK filled `imageData` with at least
                // `bufferSize` valid bytes for this frame.
                unsafe { slice::from_raw_parts(data_ptr, data_len) }
            };

            let host_ts_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);

            let rust_frame = Frame::new(
                data_slice,
                frame.width,
                frame.height,
                PixelFormat::from_raw(frame.pixelFormat),
                host_ts_ns,
                frame.frameID,
            );

            ctx.callback.invoke(&rust_frame);
        }

        // Re-queue the frame so the SDK keeps delivering. Best-effort;
        // error codes are unrecoverable from inside a C callback.
        // SAFETY: `camera_handle` and `frame_ptr` came from the SDK
        // itself; the trampoline function pointer is ourselves; the
        // runtime-loaded VmbC function pointer stays live for the
        // lifetime of `ctx.api`.
        unsafe {
            let _ = (ctx.api.VmbCaptureFrameQueue)(
                camera_handle,
                frame_ptr as *const _,
                Some(frame_callback_trampoline),
            );
        }
    });
}

/// Discovery event feature + enum value names — stable GenICam strings.
pub(crate) const FEATURE_EVENT_CAMERA_DISCOVERY: &str = "EventCameraDiscovery";
const FEATURE_DISCOVERY_CAMERA_ID: &str = "EventCameraDiscoveryCameraID";
const FEATURE_DISCOVERY_TYPE: &str = "EventCameraDiscoveryType";

const DISCOVERY_TYPE_DETECTED: &str = "Detected";
const DISCOVERY_TYPE_MISSING: &str = "Missing";
const DISCOVERY_TYPE_REACHABLE: &str = "Reachable";
const DISCOVERY_TYPE_UNREACHABLE: &str = "Unreachable";

/// C-ABI trampoline invoked by the Vimba SDK on every discovery event.
///
/// # Safety
///
/// Called only by the Vimba SDK with a `user_context` pointer originally
/// obtained via `Box::into_raw::<DiscoveryTrampolineCtx>`. The allocation
/// is freed only after `VmbFeatureInvalidationUnregister` has returned,
/// which the SDK blocks on in-flight callbacks — so it is still live
/// here.
pub(crate) unsafe extern "C" fn discovery_trampoline(
    handle: vmb_sys::VmbHandle_t,
    _feature_name: *const std::os::raw::c_char,
    user_context: *mut std::os::raw::c_void,
) {
    let _ = std::panic::catch_unwind(|| {
        if user_context.is_null() {
            return;
        }
        // SAFETY: `user_context` is the pointer we handed to the SDK via
        // `Box::into_raw` on a `Box<DiscoveryTrampolineCtx>`. Still live
        // (see fn-level safety comment).
        let ctx = unsafe { &*(user_context as *const DiscoveryTrampolineCtx) };

        // SAFETY: same argument as the surrounding fn — the SDK-supplied
        // handle is valid for the duration of this invocation.
        let id = unsafe { read_string_feature(&ctx.api, handle, FEATURE_DISCOVERY_CAMERA_ID) };
        let Some(id) = id else {
            warn!("discovery callback: could not read EventCameraDiscoveryCameraID");
            return;
        };

        // SAFETY: see above.
        let kind = unsafe { read_enum_feature(&ctx.api, handle, FEATURE_DISCOVERY_TYPE) };
        let Some(kind) = kind else {
            warn!(
                camera_id = %id,
                "discovery callback: could not read EventCameraDiscoveryType"
            );
            return;
        };

        let event = match kind.as_str() {
            DISCOVERY_TYPE_DETECTED => DiscoveryEvent::Detected(id),
            DISCOVERY_TYPE_MISSING => DiscoveryEvent::Missing(id),
            DISCOVERY_TYPE_REACHABLE => DiscoveryEvent::Reachable(id),
            DISCOVERY_TYPE_UNREACHABLE => DiscoveryEvent::Unreachable(id),
            other => {
                warn!(kind = other, "unknown EventCameraDiscoveryType value");
                return;
            }
        };

        ctx.callback.invoke(event);
    });
}

/// Read a Vimba string feature into an owned `String`. Returns `None` on
/// SDK error or non-UTF-8 data.
///
/// # Safety
///
/// `handle` must be a valid Vmb handle appropriate to the named feature.
unsafe fn read_string_feature(
    api: &VmbApi,
    handle: vmb_sys::VmbHandle_t,
    name: &str,
) -> Option<String> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut buf = [0u8; 256];
    let mut filled: u32 = 0;
    // SAFETY: `c_name` lives until the end of the call; `buf` is valid
    // for `buf.len()` bytes; `filled` is a valid out-parameter.
    let rc = unsafe {
        (api.VmbFeatureStringGet)(
            handle,
            c_name.as_ptr(),
            buf.as_mut_ptr() as *mut std::os::raw::c_char,
            buf.len() as u32,
            &mut filled,
        )
    };
    if rc != 0 {
        return None;
    }
    // `filled` includes the trailing NUL byte per Vimba convention.
    let end = (filled as usize).saturating_sub(1).min(buf.len() - 1);
    std::str::from_utf8(&buf[..end]).ok().map(|s| s.to_string())
}

/// Read a Vimba enum feature into an owned `String` (the enum entry
/// name).
///
/// # Safety
///
/// `handle` must be a valid Vmb handle appropriate to the named feature.
unsafe fn read_enum_feature(
    api: &VmbApi,
    handle: vmb_sys::VmbHandle_t,
    name: &str,
) -> Option<String> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut value_ptr: *const std::os::raw::c_char = std::ptr::null();
    // SAFETY: `c_name` lives until the end of the call; `value_ptr` is
    // a valid out-parameter.
    let rc = unsafe { (api.VmbFeatureEnumGet)(handle, c_name.as_ptr(), &mut value_ptr) };
    if rc != 0 || value_ptr.is_null() {
        return None;
    }
    // SAFETY: `value_ptr` is non-null and points to SDK-owned memory
    // valid for the duration of this callback invocation.
    Some(cstr_to_owned(value_ptr, ""))
}
