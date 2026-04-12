//! [`VmbFfiRuntime`] — production [`VmbRuntime`] implementation backed
//! by the Vimba X C API.
//!
//! Every `unsafe { vmb_sys::... }` call in the workspace lives in one of
//! the methods below (or in [`crate::trampoline`]). The mapping is
//! deliberately verbatim w.r.t. the pre-refactor monolithic
//! `vmb/src/{system,camera,discovery}.rs`, so the behaviour is
//! unchanged.

use std::ffi::CString;
use std::mem;
use std::path::Path;
use std::ptr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tracing::{debug, info, warn};
use vmb_core::{
    check, error::VmbError, port::VmbRuntime, CameraHandle, CameraInfo, DiscoveryCallback,
    DiscoveryCallbackId, DiscoveryRegistrationHandle, FrameCallback, FrameCallbackId, FrameSlotId,
    Result,
};

use crate::state::{DiscoveryRegState, FfiState, RawCamera, STARTED};
use crate::trampoline::{
    discovery_trampoline, frame_callback_trampoline, DiscoveryTrampolineCtx, TrampolineContext,
    FEATURE_EVENT_CAMERA_DISCOVERY,
};
use crate::util::cstr_to_owned;

/// Production [`VmbRuntime`] adapter that links against `libVmbC`.
///
/// Cheap to clone (the internal state is `Arc`-shared); pass by value to
/// generic code, clone where multiple owners are needed.
#[derive(Clone)]
pub struct VmbFfiRuntime {
    state: Arc<FfiState>,
}

impl Default for VmbFfiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VmbFfiRuntime {
    /// Create a fresh runtime. Construction does NOT start the SDK —
    /// call [`VmbRuntime::startup`] (typically via
    /// `vmb_core::VmbSystem::startup`) for that.
    pub fn new() -> Self {
        Self {
            state: FfiState::new(),
        }
    }
}

impl VmbRuntime for VmbFfiRuntime {
    // --- Lifecycle ------------------------------------------------------

    fn startup(&self) -> Result<()> {
        if STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(VmbError::AlreadyStarted);
        }
        // SAFETY: `VmbStartup` accepts a null `pathConfiguration` to pick
        // up the default GENICAM_GENTL*_PATH environment variables.
        let rc = unsafe { vmb_sys::VmbStartup(ptr::null()) };
        if let Err(e) = check(rc) {
            STARTED.store(false, Ordering::SeqCst);
            return Err(e);
        }
        info!("Vimba X runtime started");
        Ok(())
    }

    fn shutdown(&self) {
        if STARTED.swap(false, Ordering::SeqCst) {
            // SAFETY: we owned the started-flag, so calling Shutdown is
            // the correct pairing.
            unsafe { vmb_sys::VmbShutdown() };
            debug!("Vimba X runtime shut down");
        }
    }

    // --- Cameras --------------------------------------------------------

    fn list_cameras(&self) -> Result<Vec<CameraInfo>> {
        let mut count: u32 = 0;
        // SAFETY: null buffer + zero list length is the documented
        // size-query form.
        unsafe {
            check(vmb_sys::VmbCamerasList(
                ptr::null_mut(),
                0,
                &mut count,
                mem::size_of::<vmb_sys::VmbCameraInfo_t>() as u32,
            ))?;
        }
        if count == 0 {
            return Ok(Vec::new());
        }

        let mut buf: Vec<vmb_sys::VmbCameraInfo_t> = vec![unsafe { mem::zeroed() }; count as usize];
        let mut num_found: u32 = 0;
        // SAFETY: `buf.as_mut_ptr()` points to `count` valid slots.
        unsafe {
            check(vmb_sys::VmbCamerasList(
                buf.as_mut_ptr(),
                count,
                &mut num_found,
                mem::size_of::<vmb_sys::VmbCameraInfo_t>() as u32,
            ))?;
        }
        buf.truncate(num_found as usize);

        Ok(buf
            .into_iter()
            .map(|raw| CameraInfo {
                id: cstr_to_owned(raw.cameraIdString, "<unknown>"),
                model: cstr_to_owned(raw.modelName, "<unknown>"),
                serial: cstr_to_owned(raw.serialString, "<unknown>"),
                name: cstr_to_owned(raw.cameraName, "<unknown>"),
            })
            .collect())
    }

    fn open_camera(&self, id: &str) -> Result<CameraHandle> {
        let c_id = CString::new(id).map_err(|_| VmbError::InvalidString {
            context: "camera_id",
        })?;
        let mut handle: vmb_sys::VmbHandle_t = ptr::null_mut();
        let access_mode = vmb_sys::VmbAccessModeType::VmbAccessModeFull as u32;
        // SAFETY: `c_id` lives until the end of this call; `handle` is
        // a valid out-parameter.
        unsafe {
            check(vmb_sys::VmbCameraOpen(
                c_id.as_ptr(),
                access_mode,
                &mut handle,
            ))?;
        }
        let camera_handle = CameraHandle::new(self.state.next_id());
        self.state
            .cameras
            .lock()
            .expect("cameras mutex poisoned")
            .insert(camera_handle, RawCamera(handle));
        Ok(camera_handle)
    }

    fn close_camera(&self, h: CameraHandle) {
        let raw = self
            .state
            .cameras
            .lock()
            .expect("cameras mutex poisoned")
            .remove(&h);
        if let Some(raw) = raw {
            // SAFETY: `raw.0` came from a successful `VmbCameraOpen`
            // and has not been closed yet.
            unsafe {
                let _ = vmb_sys::VmbCameraClose(raw.0);
            }
        }
    }

    fn load_settings(&self, h: CameraHandle, path: &Path) -> Result<()> {
        let raw = self.resolve_camera(h)?;
        let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            VmbError::InvalidString {
                context: "settings_xml path",
            }
        })?;
        // SAFETY: `c_path` lives until end of call; null settings + zero
        // size is the documented "use defaults" form.
        unsafe {
            check(vmb_sys::VmbSettingsLoad(
                raw,
                c_path.as_ptr(),
                ptr::null(),
                0,
            ))?;
        }
        Ok(())
    }

    fn run_feature_command(&self, h: CameraHandle, name: &str) -> Result<()> {
        let raw = self.resolve_camera(h)?;
        let cmd = CString::new(name).map_err(|_| VmbError::InvalidString {
            context: "feature_command",
        })?;
        // SAFETY: `cmd` lives until end of call.
        unsafe {
            check(vmb_sys::VmbFeatureCommandRun(raw, cmd.as_ptr()))?;
        }
        Ok(())
    }

    // --- Capture --------------------------------------------------------

    fn payload_size(&self, h: CameraHandle) -> Result<u32> {
        let raw = self.resolve_camera(h)?;
        let mut payload: u32 = 0;
        // SAFETY: `payload` is a valid out-parameter.
        unsafe {
            check(vmb_sys::VmbPayloadSizeGet(raw, &mut payload))?;
        }
        Ok(payload)
    }

    fn announce_frame(&self, h: CameraHandle, size: u32) -> Result<FrameSlotId> {
        let raw = self.resolve_camera(h)?;
        // Resolve the callback later (in queue_frame); here we just set
        // up the context with a placeholder callback. But the
        // TrampolineContext wants an Arc<FrameCallback> at construction.
        // We fix this by deferring: announce allocates the context with
        // a "panicking" placeholder, and queue_frame swaps in the real
        // callback via a dedicated method. See below.
        //
        // Actually simpler: store the announced slot's Box separately
        // from its callback. The TrampolineContext holds both, but at
        // announce time the callback slot is Arc::new(FrameCallback::new
        // (|_| {})) — a no-op — and queue_frame replaces the callback
        // before issuing VmbCaptureFrameQueue.
        let noop: Arc<FrameCallback> = Arc::new(FrameCallback::new(|_| {}));
        let mut ctx = Box::new(TrampolineContext::new(noop, size as usize));
        let frame_ptr = ctx.vmb_frame_mut_ptr();
        // SAFETY: `frame_ptr` points into heap memory owned by `ctx`;
        // we retain `ctx` in `self.state.frames` so the pointer stays
        // valid until `frame_revoke_all`.
        unsafe {
            check(vmb_sys::VmbFrameAnnounce(
                raw,
                frame_ptr as *const _,
                mem::size_of::<vmb_sys::VmbFrame_t>() as u32,
            ))?;
        }
        let slot = FrameSlotId(self.state.next_u64());
        self.state
            .frames
            .lock()
            .expect("frames mutex poisoned")
            .insert(slot, ctx);
        Ok(slot)
    }

    fn capture_start(&self, h: CameraHandle) -> Result<()> {
        let raw = self.resolve_camera(h)?;
        // SAFETY: `raw` is a valid opened camera handle.
        unsafe { check(vmb_sys::VmbCaptureStart(raw)) }
    }

    fn queue_frame(&self, h: CameraHandle, slot: FrameSlotId, cb: FrameCallbackId) -> Result<()> {
        let raw = self.resolve_camera(h)?;
        let callback = self
            .state
            .frame_callbacks
            .lock()
            .expect("frame_callbacks mutex poisoned")
            .get(&cb)
            .cloned()
            .ok_or(VmbError::InvalidString {
                context: "unknown frame callback id",
            })?;

        let mut frames = self.state.frames.lock().expect("frames mutex poisoned");
        let ctx = frames.get_mut(&slot).ok_or(VmbError::InvalidString {
            context: "unknown frame slot id",
        })?;
        // Replace the placeholder callback installed at announce time
        // with the caller-supplied one, then (re-)patch `context[0]` so
        // the stable self-pointer points at the updated context.
        ctx.set_callback(callback);
        let frame_ptr = ctx.vmb_frame_mut_ptr();
        // SAFETY: `frame_ptr` still points at the same heap-allocated
        // frame; the trampoline function pointer has static linkage.
        unsafe {
            check(vmb_sys::VmbCaptureFrameQueue(
                raw,
                frame_ptr as *const _,
                Some(frame_callback_trampoline),
            ))
        }
    }

    fn capture_end(&self, h: CameraHandle) {
        let Ok(raw) = self.resolve_camera(h) else {
            return;
        };
        // SAFETY: best-effort teardown; safe no-op if capture not started.
        unsafe {
            let _ = vmb_sys::VmbCaptureEnd(raw);
        }
    }

    fn capture_queue_flush(&self, h: CameraHandle) {
        let Ok(raw) = self.resolve_camera(h) else {
            return;
        };
        // SAFETY: best-effort teardown.
        unsafe {
            let _ = vmb_sys::VmbCaptureQueueFlush(raw);
        }
    }

    fn frame_revoke_all(&self, h: CameraHandle) {
        let Ok(raw) = self.resolve_camera(h) else {
            return;
        };
        // SAFETY: best-effort teardown.
        unsafe {
            let _ = vmb_sys::VmbFrameRevokeAll(raw);
        }
        // With all SDK-side references dropped, it is safe to release
        // the trampoline contexts (and their backing buffers).
        self.state
            .frames
            .lock()
            .expect("frames mutex poisoned")
            .clear();
    }

    // --- Discovery ------------------------------------------------------

    fn register_discovery(&self, cb: DiscoveryCallbackId) -> Result<DiscoveryRegistrationHandle> {
        let callback = self
            .state
            .discovery_callbacks
            .lock()
            .expect("discovery_callbacks mutex poisoned")
            .get(&cb)
            .cloned()
            .ok_or(VmbError::InvalidString {
                context: "unknown discovery callback id",
            })?;

        let feature =
            CString::new(FEATURE_EVENT_CAMERA_DISCOVERY).map_err(|_| VmbError::InvalidString {
                context: FEATURE_EVENT_CAMERA_DISCOVERY,
            })?;

        let ctx: Box<DiscoveryTrampolineCtx> = Box::new(DiscoveryTrampolineCtx { callback });
        let ctx_ptr: *mut DiscoveryTrampolineCtx = Box::into_raw(ctx);

        // SAFETY: `G_VMB_HANDLE` is the documented global sentinel,
        // `feature` is a valid NUL-terminated C string, the trampoline
        // has the required `extern "C"` signature, and `ctx_ptr` points
        // at a heap allocation that stays live until we reclaim it in
        // `unregister_discovery`.
        let rc = unsafe {
            vmb_sys::VmbFeatureInvalidationRegister(
                vmb_sys::G_VMB_HANDLE,
                feature.as_ptr(),
                Some(discovery_trampoline),
                ctx_ptr as *mut std::os::raw::c_void,
            )
        };
        if let Err(e) = check(rc) {
            // SAFETY: the SDK did not accept ownership of the pointer;
            // reclaim the leaked box.
            drop(unsafe { Box::from_raw(ctx_ptr) });
            return Err(e);
        }

        let handle = DiscoveryRegistrationHandle(self.state.next_u64());
        self.state
            .discovery_regs
            .lock()
            .expect("discovery_regs mutex poisoned")
            .insert(handle, DiscoveryRegState { ctx_ptr, feature });
        debug!("camera discovery registered");
        Ok(handle)
    }

    fn unregister_discovery(&self, r: DiscoveryRegistrationHandle) {
        let state = self
            .state
            .discovery_regs
            .lock()
            .expect("discovery_regs mutex poisoned")
            .remove(&r);
        let Some(state) = state else { return };

        // SAFETY: `G_VMB_HANDLE` + `state.feature` were used at register
        // time; the callback fn pointer matches; this call blocks until
        // in-flight callbacks have returned.
        let rc = unsafe {
            vmb_sys::VmbFeatureInvalidationUnregister(
                vmb_sys::G_VMB_HANDLE,
                state.feature.as_ptr(),
                Some(discovery_trampoline),
            )
        };
        if rc != 0 {
            warn!(
                code = rc,
                "VmbFeatureInvalidationUnregister failed for EventCameraDiscovery"
            );
        } else {
            debug!("camera discovery unregistered");
        }
        if !state.ctx_ptr.is_null() {
            // SAFETY: `ctx_ptr` was produced by `Box::into_raw` in
            // `register_discovery` and has not been reclaimed yet. By
            // contract the SDK has finished with it (unregister blocks
            // on in-flight callbacks).
            drop(unsafe { Box::from_raw(state.ctx_ptr) });
        }
    }

    // --- Callback installation ------------------------------------------

    fn install_frame_callback(&self, cb: Arc<FrameCallback>) -> FrameCallbackId {
        let id = FrameCallbackId(self.state.next_u64());
        self.state
            .frame_callbacks
            .lock()
            .expect("frame_callbacks mutex poisoned")
            .insert(id, cb);
        id
    }

    fn uninstall_frame_callback(&self, id: FrameCallbackId) {
        self.state
            .frame_callbacks
            .lock()
            .expect("frame_callbacks mutex poisoned")
            .remove(&id);
    }

    fn install_discovery_callback(&self, cb: Arc<DiscoveryCallback>) -> DiscoveryCallbackId {
        let id = DiscoveryCallbackId(self.state.next_u64());
        self.state
            .discovery_callbacks
            .lock()
            .expect("discovery_callbacks mutex poisoned")
            .insert(id, cb);
        id
    }

    fn uninstall_discovery_callback(&self, id: DiscoveryCallbackId) {
        self.state
            .discovery_callbacks
            .lock()
            .expect("discovery_callbacks mutex poisoned")
            .remove(&id);
    }
}

impl VmbFfiRuntime {
    /// Look up the raw SDK handle for a given opaque camera handle.
    fn resolve_camera(&self, h: CameraHandle) -> Result<vmb_sys::VmbHandle_t> {
        self.state
            .cameras
            .lock()
            .expect("cameras mutex poisoned")
            .get(&h)
            .map(|r| r.0)
            .ok_or(VmbError::InvalidString {
                context: "unknown camera handle",
            })
    }
}
