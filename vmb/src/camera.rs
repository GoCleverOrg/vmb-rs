//! Safe wrapper around an opened Vimba X camera.

use std::ffi::CString;
use std::mem;
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use tracing::debug;

use crate::callback::{frame_callback_trampoline, FrameCallback, TrampolineContext};
use crate::error::{check, cstr_to_owned, Result, VmbError};
use crate::frame::Frame;

/// Vimba X GenICam command features issued during capture lifecycle.
/// These names are part of the GenICam standard camera description and
/// are stable across SDK versions, but hoisting them to consts makes
/// SDK-upgrade audits a one-grep review.
const FEATURE_ACQUISITION_START: &str = "AcquisitionStart";
const FEATURE_ACQUISITION_STOP: &str = "AcquisitionStop";

/// Metadata for a discoverable camera.
#[derive(Debug, Clone)]
pub struct CameraInfo {
    /// The transport-layer camera ID (e.g. `"DEV_1AB22C00F5B8"`).
    pub id: String,
    /// Human-readable model name reported by the device.
    pub model: String,
    /// Serial number reported by the device.
    pub serial: String,
    /// User-configurable friendly name.
    pub name: String,
}

/// Open handle to a Vimba camera.
///
/// Dropping the camera cleanly ends any running capture (via
/// [`Camera::stop_capture`]) and calls `VmbCameraClose`.
pub struct Camera {
    handle: vmb_sys::VmbHandle_t,
    id: String,
    /// Boxed trampoline contexts kept alive for the duration of capture.
    ///
    /// Each `Box<TrampolineContext>` owns its `VmbFrame_t` and backing
    /// pixel buffer. Dropping this `Vec` (on `stop_capture` or `Drop`)
    /// frees all contexts.
    ///
    /// The `Box` is load-bearing: `TrampolineContext::vmb_frame_mut_ptr`
    /// stores a raw `self`-pointer in `frame.context[0]` that the C SDK
    /// dereferences on every callback. If a `Vec<TrampolineContext>`
    /// reallocated on push, the underlying contexts would move, silently
    /// invalidating those stored self-pointers. Boxing each context
    /// pins its heap address independently of any Vec reallocations.
    #[allow(clippy::vec_box)]
    trampolines: Vec<Box<TrampolineContext>>,
}

// SAFETY: `VmbHandle_t` is a raw pointer but the SDK is documented to be
// thread-safe across cameras, and we never alias the handle. The user of
// the wrapper is responsible for not invoking capture operations
// concurrently on the same camera.
unsafe impl Send for Camera {}

impl Camera {
    /// Enumerate cameras currently visible to the Vimba runtime.
    ///
    /// Requires that `VmbSystem::startup` has already been called.
    pub fn list() -> Result<Vec<CameraInfo>> {
        // Two-pass: first query count with null buffer, then allocate
        // and refill. Per the SDK docs, the size parameter is ignored
        // when the output buffer is null.
        let mut count: u32 = 0;
        // SAFETY: null buffer + zero list length is the documented
        // size-query form. `count` is a valid out-parameter.
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

        // Allocate `count` zero-initialized `VmbCameraInfo_t` slots. The
        // struct is `#[repr(C)]` and POD (pointers + ints), so zero is a
        // valid initial bit pattern.
        let mut buf: Vec<vmb_sys::VmbCameraInfo_t> = vec![unsafe { mem::zeroed() }; count as usize];

        let mut num_found: u32 = 0;
        // SAFETY: `buf.as_mut_ptr()` points to `count` valid slots;
        // `num_found` is a valid out-parameter.
        unsafe {
            check(vmb_sys::VmbCamerasList(
                buf.as_mut_ptr(),
                count,
                &mut num_found,
                mem::size_of::<vmb_sys::VmbCameraInfo_t>() as u32,
            ))?;
        }
        buf.truncate(num_found as usize);

        let out = buf
            .into_iter()
            .map(|raw| CameraInfo {
                id: cstr_to_owned(raw.cameraIdString, "<unknown>"),
                model: cstr_to_owned(raw.modelName, "<unknown>"),
                serial: cstr_to_owned(raw.serialString, "<unknown>"),
                name: cstr_to_owned(raw.cameraName, "<unknown>"),
            })
            .collect();
        Ok(out)
    }

    /// Open a camera by its Vimba camera ID string with full (exclusive)
    /// access.
    pub fn open(camera_id: &str) -> Result<Self> {
        let c_id = CString::new(camera_id).map_err(|_| VmbError::InvalidString {
            context: "camera_id",
        })?;
        let mut handle: vmb_sys::VmbHandle_t = ptr::null_mut();

        // `VmbAccessModeType::VmbAccessModeFull = 1` in the bindings.
        let access_mode = vmb_sys::VmbAccessModeType::VmbAccessModeFull as u32;

        // SAFETY: `c_id` lives until the end of this call; `handle` is a
        // valid out-parameter.
        unsafe {
            check(vmb_sys::VmbCameraOpen(
                c_id.as_ptr(),
                access_mode,
                &mut handle,
            ))?;
        }

        Ok(Self {
            handle,
            id: camera_id.to_string(),
            trampolines: Vec::new(),
        })
    }

    /// Load a Vimba settings XML (day/night profile). The SDK is
    /// responsible for parsing the file and writing all features.
    pub fn load_settings(&self, path: &Path) -> Result<()> {
        let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            VmbError::InvalidString {
                context: "settings_xml path",
            }
        })?;

        // `VmbSettingsLoad` accepts a null settings struct for defaults:
        // persist features except LUT, max 5 iterations, errors only.
        // SAFETY: `c_path` lives until end of call; null settings + zero
        // size is the documented "use defaults" form.
        unsafe {
            check(vmb_sys::VmbSettingsLoad(
                self.handle,
                c_path.as_ptr(),
                ptr::null(),
                0,
            ))?;
        }
        Ok(())
    }

    /// Start continuous capture.
    ///
    /// The closure is invoked for every received frame; it MUST be fast and
    /// immediately copy the frame bytes (Vimba re-queues the buffer as soon
    /// as the callback returns). The closure runs on a Vimba SDK worker
    /// thread and must be `Send + Sync`.
    ///
    /// `num_buffers` is the number of frame buffers to pre-announce; 4 is
    /// a reasonable default for most use cases.
    ///
    /// # Cleanup contract
    ///
    /// All SDK resources claimed between `VmbFrameAnnounce` and the final
    /// `Ok(())` (announced frames, capture state, queued buffers,
    /// `AcquisitionStart`) are unwound on any error path before returning.
    /// This guarantees that `self.trampolines` is only populated when the
    /// SDK is fully primed — preventing a latent use-after-free where the
    /// SDK would otherwise hold pointers into trampoline contexts that
    /// get dropped when `Camera::Drop` skips `stop_capture`.
    ///
    /// Cleanup uses `VmbCaptureEnd`, `VmbCaptureQueueFlush`, and
    /// `VmbFrameRevokeAll`, all of which are documented as safe no-ops
    /// when their preconditions aren't met. Individual cleanup errors are
    /// deliberately swallowed so the original failure is propagated.
    pub fn start_capture<F>(&mut self, num_buffers: usize, callback: F) -> Result<()>
    where
        F: for<'a> Fn(&Frame<'a>) + Send + Sync + 'static,
    {
        if !self.trampolines.is_empty() {
            return Err(VmbError::CaptureAlreadyRunning);
        }

        // Inner helper: returns Ok if everything succeeded, Err otherwise.
        // On Err the caller below performs SDK-side cleanup.
        let result: Result<()> = (|| {
            let mut payload: u32 = 0;
            // SAFETY: `payload` is a valid out-parameter.
            unsafe {
                check(vmb_sys::VmbPayloadSizeGet(self.handle, &mut payload))?;
            }
            debug!(payload_bytes = payload, "allocated Vimba frame buffers");

            let cb: Arc<FrameCallback> = Arc::new(FrameCallback::new(callback));

            for _ in 0..num_buffers {
                let mut tramp = Box::new(TrampolineContext::new(cb.clone(), payload as usize));
                let frame_ptr = tramp.vmb_frame_mut_ptr();
                // SAFETY: `frame_ptr` points into heap memory owned by
                // `tramp`, which is pushed onto `self.trampolines` below
                // and remains alive until `stop_capture` / drop.
                unsafe {
                    check(vmb_sys::VmbFrameAnnounce(
                        self.handle,
                        frame_ptr as *const _,
                        mem::size_of::<vmb_sys::VmbFrame_t>() as u32,
                    ))?;
                }
                self.trampolines.push(tramp);
            }

            // SAFETY: `self.handle` is a valid opened camera handle.
            unsafe {
                check(vmb_sys::VmbCaptureStart(self.handle))?;
            }

            for tramp in self.trampolines.iter_mut() {
                let frame_ptr = tramp.vmb_frame_mut_ptr();
                // SAFETY: `frame_ptr` still points at the same
                // heap-allocated frame; the trampoline function pointer
                // has static linkage.
                unsafe {
                    check(vmb_sys::VmbCaptureFrameQueue(
                        self.handle,
                        frame_ptr as *const _,
                        Some(frame_callback_trampoline),
                    ))?;
                }
            }

            let cmd =
                CString::new(FEATURE_ACQUISITION_START).map_err(|_| VmbError::InvalidString {
                    context: FEATURE_ACQUISITION_START,
                })?;
            // SAFETY: `cmd` lives until end of call.
            unsafe {
                check(vmb_sys::VmbFeatureCommandRun(self.handle, cmd.as_ptr()))?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                // Best-effort cleanup. Ignore individual errors because
                // we're already propagating the original failure. All
                // three calls are documented as safe no-ops when their
                // preconditions aren't met.
                // SAFETY: `self.handle` is a valid opened camera handle;
                // these calls tear down any SDK-side state claimed above.
                unsafe {
                    let _ = vmb_sys::VmbCaptureEnd(self.handle);
                    let _ = vmb_sys::VmbCaptureQueueFlush(self.handle);
                    let _ = vmb_sys::VmbFrameRevokeAll(self.handle);
                }
                // Drop any trampolines we pushed; the SDK no longer
                // references them after `VmbFrameRevokeAll`.
                self.trampolines.clear();
                Err(e)
            }
        }
    }

    /// Stop an in-progress capture. Safe to call when no capture is
    /// running — the call is a no-op in that case.
    pub fn stop_capture(&mut self) -> Result<()> {
        if self.trampolines.is_empty() {
            return Ok(());
        }

        // Best-effort teardown. We intentionally swallow individual errors
        // below because: (a) we cannot recover from a partial teardown
        // failure mid-shutdown, and (b) returning early would leave the
        // capture state inconsistent. Callers who need precise error
        // reporting should use `Camera::id()` to log.
        let stop_cmd = match CString::new(FEATURE_ACQUISITION_STOP) {
            Ok(c) => c,
            Err(_) => {
                return Err(VmbError::InvalidString {
                    context: FEATURE_ACQUISITION_STOP,
                });
            }
        };

        // SAFETY: `self.handle` is a valid opened camera handle. Each of
        // these calls is documented as safe to issue in the stop sequence,
        // and each one tolerates being called when the underlying state
        // has already been torn down (returning a non-success code we
        // deliberately ignore here).
        unsafe {
            let _ = vmb_sys::VmbFeatureCommandRun(self.handle, stop_cmd.as_ptr());
            let _ = vmb_sys::VmbCaptureEnd(self.handle);
            let _ = vmb_sys::VmbCaptureQueueFlush(self.handle);
            let _ = vmb_sys::VmbFrameRevokeAll(self.handle);
        }

        // After VmbCaptureEnd + VmbFrameRevokeAll, the SDK guarantees no
        // more callbacks will fire, so it is safe to drop the trampoline
        // contexts (and their buffers) here.
        self.trampolines.clear();
        Ok(())
    }

    /// The camera ID originally passed to [`Camera::open`].
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        if !self.trampolines.is_empty() {
            let _ = self.stop_capture();
        }
        // SAFETY: `self.handle` was returned by a successful
        // `VmbCameraOpen` and has not been closed yet.
        unsafe {
            let _ = vmb_sys::VmbCameraClose(self.handle);
        }
    }
}
