//! RAII owner for the Vimba X runtime lifecycle.
//!
//! [`VmbSystem`] is the composition root for a running Vimba session. It
//! is generic over any [`VmbRuntime`]: use the production FFI adapter in
//! release code, an in-memory fake in tests.
//!
//! Construction calls [`VmbRuntime::startup`]; `Drop` calls
//! [`VmbRuntime::shutdown`]. The "singleton" invariant (the FFI SDK is
//! process-global) is enforced by the runtime — dropping a live system
//! while another `VmbSystem` exists for the same runtime is prevented
//! inside the FFI adapter, not here.

use std::sync::Arc;

use crate::camera::Camera;
use crate::discovery::{register_camera_discovery, DiscoveryRegistration};
use crate::port::VmbRuntime;
use crate::types::{CameraInfo, DiscoveryEvent};
use crate::Result;

/// Owns the Vimba X runtime lifecycle.
///
/// On construction, calls [`VmbRuntime::startup`]; on drop, calls
/// [`VmbRuntime::shutdown`].
pub struct VmbSystem<R: VmbRuntime> {
    runtime: Arc<R>,
}

impl<R: VmbRuntime> VmbSystem<R> {
    /// Start the Vimba runtime with the given backend.
    ///
    /// Returns [`crate::VmbError::AlreadyStarted`] if another
    /// `VmbSystem` built on the same process-global runtime is still
    /// alive (applies to [`VmbFfiRuntime`] — fakes are per-instance).
    ///
    /// [`VmbFfiRuntime`]: #production-adapter
    pub fn startup(runtime: R) -> Result<Self> {
        let runtime = Arc::new(runtime);
        runtime.startup()?;
        Ok(Self { runtime })
    }

    /// Borrow the underlying runtime. Rarely needed by user code — most
    /// operations are exposed directly on [`VmbSystem`], [`Camera`], or
    /// [`DiscoveryRegistration`].
    pub fn runtime(&self) -> &R {
        &self.runtime
    }

    /// Enumerate currently-visible cameras.
    pub fn list_cameras(&self) -> Result<Vec<CameraInfo>> {
        self.runtime.list_cameras()
    }

    /// Open a camera by its transport-layer ID.
    pub fn open_camera(&self, id: &str) -> Result<Camera<R>> {
        Camera::open(self.runtime.clone(), id)
    }

    /// Register a camera-discovery subscription; the callback is invoked
    /// for every plug / unplug / reachability event the runtime
    /// surfaces.
    pub fn register_discovery<F>(&self, callback: F) -> Result<DiscoveryRegistration<R>>
    where
        F: Fn(DiscoveryEvent) + Send + Sync + 'static,
    {
        register_camera_discovery(self.runtime.clone(), callback)
    }
}

impl<R: VmbRuntime> Drop for VmbSystem<R> {
    fn drop(&mut self) {
        self.runtime.shutdown();
    }
}

impl<R: VmbRuntime> std::fmt::Debug for VmbSystem<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VmbSystem").finish_non_exhaustive()
    }
}
