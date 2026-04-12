//! Vimba camera discovery events (plug / unplug / reachability).
//!
//! [`DiscoveryRegistration`] is an RAII handle returned by
//! [`register_camera_discovery`]; it keeps the subscription alive until
//! dropped. The discovery callback is invoked on whatever thread the
//! runtime fires it on, so the closure must be `Send + Sync`.
//!
//! The domain layer is deliberately agnostic about what the callback is
//! allowed to do; the FFI adapter's docs warn callers that certain
//! Vimba APIs must not be called from inside a discovery event handler.

use std::sync::Arc;

use crate::port::{DiscoveryCallback, VmbRuntime};
use crate::types::{DiscoveryCallbackId, DiscoveryEvent, DiscoveryRegistrationHandle};
use crate::Result;

/// Live camera-discovery registration. Unregistering happens on drop.
pub struct DiscoveryRegistration<R: VmbRuntime> {
    runtime: Arc<R>,
    handle: Option<DiscoveryRegistrationHandle>,
    callback_id: Option<DiscoveryCallbackId>,
}

/// Register a discovery callback on the given runtime.
///
/// Usually called via
/// [`VmbSystem::register_discovery`](crate::system::VmbSystem::register_discovery)
/// rather than directly.
pub fn register_camera_discovery<R, F>(
    runtime: Arc<R>,
    callback: F,
) -> Result<DiscoveryRegistration<R>>
where
    R: VmbRuntime,
    F: Fn(DiscoveryEvent) + Send + Sync + 'static,
{
    let cb = Arc::new(DiscoveryCallback::new(callback));
    let callback_id = runtime.install_discovery_callback(cb);
    match runtime.register_discovery(callback_id) {
        Ok(handle) => Ok(DiscoveryRegistration {
            runtime,
            handle: Some(handle),
            callback_id: Some(callback_id),
        }),
        Err(e) => {
            // Registration failed — no trampoline will ever fire for
            // this callback, so it's safe to reclaim the closure here
            // and propagate the error.
            runtime.uninstall_discovery_callback(callback_id);
            Err(e)
        }
    }
}

impl<R: VmbRuntime> Drop for DiscoveryRegistration<R> {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.runtime.unregister_discovery(h);
        }
        if let Some(id) = self.callback_id.take() {
            self.runtime.uninstall_discovery_callback(id);
        }
    }
}

impl<R: VmbRuntime> std::fmt::Debug for DiscoveryRegistration<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryRegistration")
            .field("handle", &self.handle)
            .field("callback_id", &self.callback_id)
            .finish()
    }
}
