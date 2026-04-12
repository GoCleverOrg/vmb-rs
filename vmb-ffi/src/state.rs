//! Internal bookkeeping for [`VmbFfiRuntime`].
//!
//! Everything the adapter needs to correlate the opaque numeric handles
//! passed across the [`VmbRuntime`] port with real FFI pointers lives
//! here. No public API is exposed — this is the adapter's private state.
//!
//! [`VmbRuntime`]: vmb_core::VmbRuntime

use std::collections::HashMap;
use std::ffi::CString;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vmb_core::{
    CameraHandle, DiscoveryCallback, DiscoveryCallbackId, DiscoveryRegistrationHandle,
    FrameCallback, FrameCallbackId, FrameSlotId,
};

use crate::trampoline::{DiscoveryTrampolineCtx, TrampolineContext};

/// Process-global flag that matches the `VmbStartup`/`VmbShutdown`
/// lifecycle of the real SDK. The SDK is a singleton, so the flag is
/// static rather than per-runtime.
pub(crate) static STARTED: AtomicBool = AtomicBool::new(false);

/// Internal bookkeeping shared between [`VmbFfiRuntime`] method calls and
/// the C-ABI trampolines. Reference-counted so trampoline callbacks can
/// hold a back-pointer without blocking runtime Drop.
///
/// [`VmbFfiRuntime`]: crate::VmbFfiRuntime
/// `Send + Sync` wrapper around the raw SDK camera pointer.
///
/// Vimba's thread-safety rules — established per-handle by the SDK —
/// make moving the pointer between threads safe; Rust's type system
/// just can't see that. This newtype encodes the guarantee.
#[derive(Copy, Clone)]
pub(crate) struct RawCamera(pub(crate) vmb_sys::VmbHandle_t);

// SAFETY: see doc comment.
unsafe impl Send for RawCamera {}
unsafe impl Sync for RawCamera {}

pub(crate) struct FfiState {
    pub(crate) cameras: Mutex<HashMap<CameraHandle, RawCamera>>,
    pub(crate) frames: Mutex<HashMap<FrameSlotId, Box<TrampolineContext>>>,
    pub(crate) frame_callbacks: Mutex<HashMap<FrameCallbackId, Arc<FrameCallback>>>,
    pub(crate) discovery_callbacks: Mutex<HashMap<DiscoveryCallbackId, Arc<DiscoveryCallback>>>,
    pub(crate) discovery_regs: Mutex<HashMap<DiscoveryRegistrationHandle, DiscoveryRegState>>,
    counter: AtomicU64,
}

/// Per-discovery-registration state; the adapter uses this to unregister
/// the invalidation listener when the domain drops the subscription.
pub(crate) struct DiscoveryRegState {
    /// The heap allocation handed to the SDK as `user_context`. Kept so
    /// we can reclaim it once `VmbFeatureInvalidationUnregister` has
    /// returned (the SDK is documented to block that call until in-flight
    /// invocations have finished, so reclaim is race-free).
    pub(crate) ctx_ptr: *mut DiscoveryTrampolineCtx,
    /// The feature name supplied to the unregister call (must outlive
    /// the registration).
    pub(crate) feature: CString,
}

// SAFETY: The `ctx_ptr` points at a heap-allocated
// `DiscoveryTrampolineCtx` which is `Send + Sync` on its own (the inner
// `Arc<DiscoveryCallback>` is `Send + Sync`). Moving the raw pointer
// between threads does not introduce aliasing — the SDK is the only
// other reader, and it owns the pointer for the lifetime of the
// registration.
unsafe impl Send for DiscoveryRegState {}
unsafe impl Sync for DiscoveryRegState {}

impl FfiState {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            cameras: Mutex::new(HashMap::new()),
            frames: Mutex::new(HashMap::new()),
            frame_callbacks: Mutex::new(HashMap::new()),
            discovery_callbacks: Mutex::new(HashMap::new()),
            discovery_regs: Mutex::new(HashMap::new()),
            counter: AtomicU64::new(1),
        })
    }

    /// Allocate a fresh ID. IDs are never reused; monotonic forever.
    pub(crate) fn next_id(&self) -> NonZeroU64 {
        NonZeroU64::new(self.counter.fetch_add(1, Ordering::Relaxed))
            .expect("ID counter started at 1 and never wraps in practice")
    }

    pub(crate) fn next_u64(&self) -> u64 {
        self.counter.fetch_add(1, Ordering::Relaxed)
    }
}
