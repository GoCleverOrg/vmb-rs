//! Vimba camera discovery events (plug / unplug).
//!
//! Wraps `VmbFeatureInvalidationRegister` on the global `gVmbHandle` with
//! the `EventCameraDiscovery` feature. A registration stays active until
//! the returned [`DiscoveryRegistration`] is dropped.
//!
//! The user callback runs on a Vimba SDK worker thread. Per the Vimba X
//! C API manual, the following calls are **forbidden** inside this
//! callback: `VmbCameraOpen`, `VmbCameraClose`, any `VmbFeature*Set`,
//! and `VmbFeatureCommandRun`. The callback should therefore only record
//! events (e.g. push into a channel) and let a reconciler thread do the
//! actual open/close work.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};

use tracing::{debug, warn};

use crate::error::{check, cstr_to_owned, Result, VmbError};

/// Vimba X camera discovery event feature + enum value names.
const FEATURE_EVENT_CAMERA_DISCOVERY: &str = "EventCameraDiscovery";
const FEATURE_DISCOVERY_CAMERA_ID: &str = "EventCameraDiscoveryCameraID";
const FEATURE_DISCOVERY_TYPE: &str = "EventCameraDiscoveryType";

const DISCOVERY_TYPE_DETECTED: &str = "Detected";
const DISCOVERY_TYPE_MISSING: &str = "Missing";
const DISCOVERY_TYPE_REACHABLE: &str = "Reachable";
const DISCOVERY_TYPE_UNREACHABLE: &str = "Unreachable";

/// Camera discovery event as reported by Vimba.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryEvent {
    /// A new camera became visible (e.g. plugged in).
    Detected(String),
    /// A previously-visible camera disappeared (e.g. unplugged).
    Missing(String),
    /// A camera became reachable again after being unreachable.
    Reachable(String),
    /// A camera became unreachable (e.g. network disruption on GigE).
    Unreachable(String),
}

impl DiscoveryEvent {
    /// The camera ID the event applies to.
    pub fn camera_id(&self) -> &str {
        match self {
            Self::Detected(id)
            | Self::Missing(id)
            | Self::Reachable(id)
            | Self::Unreachable(id) => id,
        }
    }
}

/// Type-erased callback stored in the user-context slot of the Vimba
/// invalidation registration.
type DynDiscoveryCallback = dyn Fn(DiscoveryEvent) + Send + Sync + 'static;

/// The boxed heap allocation we hand to the SDK as `userContext`.
///
/// We need a **thin** pointer to hand through the C `void*` slot, but
/// `Box<dyn Trait>` / `&dyn Trait` / `*const dyn Trait` are all FAT
/// pointers (two words: data + vtable). A `Box<Box<dyn Trait>>` solves
/// this: the inner `Box<dyn Trait>` carries the vtable, and the outer
/// `Box` over a sized type (`Box<dyn Trait>` is `Sized`) is a thin
/// pointer suitable for `*mut c_void`.
type CallbackBox = Box<DynDiscoveryCallback>;

/// Live camera-discovery registration. Unregistering happens on drop.
///
/// The Vimba SDK is documented to wait for in-flight invalidation
/// callbacks to return before `VmbFeatureInvalidationUnregister` itself
/// returns, so once `drop` has finished the SDK will never again touch
/// the raw pointer we gave it.
pub struct DiscoveryRegistration {
    /// Thin raw pointer to a heap-allocated `CallbackBox`
    /// (i.e. `Box<CallbackBox>` leaked via `Box::into_raw`). The
    /// trampoline dereferences this to `&CallbackBox` which in turn
    /// derefs to `&dyn Fn(DiscoveryEvent) + Send + Sync`.
    ///
    /// On drop we reclaim the allocation with `Box::from_raw` AFTER
    /// `VmbFeatureInvalidationUnregister` has returned, so no racing
    /// trampoline invocation can dereference a freed pointer.
    ctx: *mut CallbackBox,
    feature: CString,
}

// SAFETY: `DiscoveryRegistration` owns a heap allocation containing a
// `Box<dyn Fn(...) + Send + Sync>`, which is itself `Send + Sync`. The
// raw `VmbHandle_t` sentinel used for unregistration is a constant, not
// a per-instance pointer, so there is nothing thread-unsafe to move
// between threads.
unsafe impl Send for DiscoveryRegistration {}
unsafe impl Sync for DiscoveryRegistration {}

impl Drop for DiscoveryRegistration {
    fn drop(&mut self) {
        // Best-effort unregister. Errors are logged and swallowed — we
        // are on the drop path and cannot propagate.
        //
        // SAFETY: `G_VMB_HANDLE` is the documented global sentinel and
        // `self.feature` is a valid NUL-terminated C string owned by
        // `self`. The callback pointer matches the one passed to
        // `VmbFeatureInvalidationRegister`. Per the Vimba X C API
        // manual, this call blocks until any in-flight invocation of
        // our trampoline has returned.
        let rc = unsafe {
            vmb_sys::VmbFeatureInvalidationUnregister(
                vmb_sys::G_VMB_HANDLE,
                self.feature.as_ptr(),
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

        // Reclaim the `Box<CallbackBox>` we leaked in
        // `register_camera_discovery`. Because
        // `VmbFeatureInvalidationUnregister` has already returned (and
        // the SDK is documented to wait for in-flight callbacks), no
        // trampoline invocation can still be dereferencing `self.ctx`.
        //
        // SAFETY: `self.ctx` was produced by `Box::into_raw` on a
        // `Box<CallbackBox>` and has not been reclaimed yet.
        if !self.ctx.is_null() {
            drop(unsafe { Box::from_raw(self.ctx) });
        }
    }
}

/// Register a camera-discovery callback on the global Vmb handle. The
/// callback is invoked for every `Detected` / `Missing` / `Reachable` /
/// `Unreachable` event reported by the SDK.
///
/// The returned [`DiscoveryRegistration`] MUST be kept alive for as long
/// as events are desired; dropping it unregisters the callback.
///
/// Must be called AFTER [`crate::VmbSystem::startup`] and BEFORE the
/// [`crate::VmbSystem`] is dropped. The callback runs on a Vimba SDK
/// worker thread; per the Vimba X C API manual it MUST NOT call
/// `VmbCameraOpen`, `VmbCameraClose`, any `VmbFeature*Set`, or
/// `VmbFeatureCommandRun`. Record the event (e.g. into a channel) and
/// perform the actual open/close work on another thread.
pub fn register_camera_discovery<F>(callback: F) -> Result<DiscoveryRegistration>
where
    F: Fn(DiscoveryEvent) + Send + Sync + 'static,
{
    let feature =
        CString::new(FEATURE_EVENT_CAMERA_DISCOVERY).map_err(|_| VmbError::InvalidString {
            context: FEATURE_EVENT_CAMERA_DISCOVERY,
        })?;

    // Double-box the callback so we can hand the SDK a thin `void*`:
    // the inner `Box<dyn Fn...>` carries the vtable (fat), and the
    // outer `Box::into_raw` produces a thin `*mut CallbackBox`.
    let inner: CallbackBox = Box::new(callback);
    let outer: Box<CallbackBox> = Box::new(inner);
    let ctx_ptr: *mut CallbackBox = Box::into_raw(outer);

    // SAFETY: `G_VMB_HANDLE` is the documented global sentinel,
    // `feature` is a valid NUL-terminated C string, the trampoline has
    // the required `extern "C"` signature, and `ctx_ptr` points at a
    // heap-allocated `CallbackBox` that stays live until
    // `DiscoveryRegistration::drop` reclaims it, which itself only
    // runs after `VmbFeatureInvalidationUnregister` has returned.
    let rc = unsafe {
        vmb_sys::VmbFeatureInvalidationRegister(
            vmb_sys::G_VMB_HANDLE,
            feature.as_ptr(),
            Some(discovery_trampoline),
            ctx_ptr as *mut c_void,
        )
    };
    if let Err(e) = check(rc) {
        // Registration failed — no trampoline will ever fire for this
        // pointer, so it's safe to reclaim the leaked box here and
        // propagate the error to the caller.
        //
        // SAFETY: we just leaked `ctx_ptr` via `Box::into_raw` on the
        // line above and the SDK did not accept ownership (the call
        // returned an error).
        drop(unsafe { Box::from_raw(ctx_ptr) });
        return Err(e);
    }

    debug!("camera discovery registered on EventCameraDiscovery");
    Ok(DiscoveryRegistration {
        ctx: ctx_ptr,
        feature,
    })
}

/// C-ABI trampoline invoked by the Vimba SDK on each discovery event.
///
/// # Safety
///
/// Called only by the Vimba SDK with a `user_context` pointer that we
/// originally supplied via `Box::into_raw` on a `Box<CallbackBox>` and
/// that is still live (the owning [`DiscoveryRegistration`] only
/// reclaims the allocation after `VmbFeatureInvalidationUnregister`
/// returns, and that call blocks until in-flight callbacks have
/// finished). Must not unwind into C — any panic is caught and
/// swallowed.
unsafe extern "C" fn discovery_trampoline(
    handle: vmb_sys::VmbHandle_t,
    _feature_name: *const c_char,
    user_context: *mut c_void,
) {
    let _ = std::panic::catch_unwind(|| {
        if user_context.is_null() {
            return;
        }
        // SAFETY: `user_context` is the thin `*mut CallbackBox` we
        // originally handed to `VmbFeatureInvalidationRegister` via
        // `Box::into_raw`. It is still live because the owning
        // `DiscoveryRegistration` only reclaims it AFTER
        // `VmbFeatureInvalidationUnregister` has returned, and the
        // SDK is documented to wait for in-flight callbacks before
        // that unregister call returns. Dereferencing the outer box
        // gives us a `&CallbackBox` which derefs to the inner
        // `dyn Fn(DiscoveryEvent) + Send + Sync`.
        let callback_box: &CallbackBox = unsafe { &*(user_context as *const CallbackBox) };
        let callback: &DynDiscoveryCallback = &**callback_box;

        // Query the camera ID via the EventCameraDiscoveryCameraID
        // feature.
        //
        // SAFETY: `handle` is the handle the SDK associates with this
        // discovery event — for `EventCameraDiscovery` that is the
        // global `gVmbHandle`, which is the same module on which
        // `EventCameraDiscoveryCameraID` is exposed.
        let id = unsafe { read_string_feature(handle, FEATURE_DISCOVERY_CAMERA_ID) };
        let id = match id {
            Some(id) => id,
            None => {
                warn!("discovery callback: could not read EventCameraDiscoveryCameraID");
                return;
            }
        };

        // Query the event type via the EventCameraDiscoveryType enum
        // feature.
        //
        // SAFETY: same argument as the previous `handle` use.
        let kind = unsafe { read_enum_feature(handle, FEATURE_DISCOVERY_TYPE) };
        let kind = match kind {
            Some(k) => k,
            None => {
                warn!(
                    camera_id = %id,
                    "discovery callback: could not read EventCameraDiscoveryType"
                );
                return;
            }
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

        callback(event);
    });
}

/// Read a Vimba string feature into an owned `String`. Returns `None` on
/// SDK error or non-UTF-8 data.
///
/// # Safety
///
/// `handle` must be a valid Vmb handle appropriate to the named feature.
unsafe fn read_string_feature(handle: vmb_sys::VmbHandle_t, name: &str) -> Option<String> {
    let c_name = CString::new(name).ok()?;
    let mut buf = [0u8; 256];
    let mut filled: u32 = 0;
    // SAFETY: `c_name` lives until the end of the call; `buf` is valid
    // for `buf.len()` bytes; `filled` is a valid out-parameter.
    let rc = unsafe {
        vmb_sys::VmbFeatureStringGet(
            handle,
            c_name.as_ptr(),
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u32,
            &mut filled,
        )
    };
    if rc != 0 {
        return None;
    }
    // `filled` includes the trailing NUL byte per Vimba convention.
    // Clamp to `buf.len() - 1` in case the SDK over-reports, and
    // subtract 1 to trim the NUL. `saturating_sub` handles the
    // degenerate `filled == 0` case.
    let end = (filled as usize).saturating_sub(1).min(buf.len() - 1);
    std::str::from_utf8(&buf[..end]).ok().map(|s| s.to_string())
}

/// Read a Vimba enum feature into an owned `String` (the enum entry name).
///
/// # Safety
///
/// `handle` must be a valid Vmb handle appropriate to the named feature.
unsafe fn read_enum_feature(handle: vmb_sys::VmbHandle_t, name: &str) -> Option<String> {
    let c_name = CString::new(name).ok()?;
    let mut value_ptr: *const c_char = std::ptr::null();
    // SAFETY: `c_name` lives until the end of the call; `value_ptr` is
    // a valid out-parameter. `VmbFeatureEnumGet` returns a pointer to
    // API-owned memory that remains valid for the duration of this
    // callback invocation.
    let rc = unsafe { vmb_sys::VmbFeatureEnumGet(handle, c_name.as_ptr(), &mut value_ptr) };
    if rc != 0 || value_ptr.is_null() {
        return None;
    }
    // SAFETY: `value_ptr` is non-null (checked above) and points to a
    // NUL-terminated C string owned by the SDK for the duration of this
    // callback invocation. The shared `cstr_to_owned` helper handles
    // UTF-8 decoding; on non-UTF-8 input it returns the empty fallback,
    // which the caller's match-on-kind then logs as "unknown".
    Some(cstr_to_owned(value_ptr, ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_event_camera_id_accessor() {
        assert_eq!(DiscoveryEvent::Detected("cam1".into()).camera_id(), "cam1");
        assert_eq!(DiscoveryEvent::Missing("cam2".into()).camera_id(), "cam2");
        assert_eq!(DiscoveryEvent::Reachable("cam3".into()).camera_id(), "cam3");
        assert_eq!(
            DiscoveryEvent::Unreachable("cam4".into()).camera_id(),
            "cam4"
        );
    }
}
