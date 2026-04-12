//! Opaque handle newtypes and plain-data value objects.
//!
//! The domain refers to adapter-owned resources (cameras, announced
//! frames, registered discovery subscriptions, user callbacks) through
//! these identifiers rather than raw pointers. Each adapter maintains
//! its own side-table mapping an ID to whatever concrete representation
//! it needs. This keeps FFI pointer types from leaking into domain code.

use std::num::NonZeroU64;

/// Identifier for an opened camera. Returned by
/// [`crate::VmbRuntime::open_camera`] and accepted by every subsequent
/// camera method.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CameraHandle(NonZeroU64);

impl CameraHandle {
    /// Construct a handle from a non-zero `u64`. Intended for adapter
    /// use only.
    pub fn new(id: NonZeroU64) -> Self {
        Self(id)
    }

    /// The raw numeric id.
    pub fn as_u64(self) -> u64 {
        self.0.get()
    }
}

/// Identifier for an announced frame slot.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrameSlotId(pub u64);

/// Identifier for a user frame-callback stored in a callback registry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrameCallbackId(pub u64);

/// Identifier for a user discovery-callback stored in a callback registry.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiscoveryCallbackId(pub u64);

/// Identifier for an active discovery registration held by the adapter.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiscoveryRegistrationHandle(pub u64);

/// Metadata for a discoverable camera.
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_handle_round_trips_u64() {
        let h = CameraHandle::new(NonZeroU64::new(42).unwrap());
        assert_eq!(h.as_u64(), 42);
    }

    #[test]
    fn camera_handle_equality_and_hashing() {
        use std::collections::HashSet;
        let a = CameraHandle::new(NonZeroU64::new(1).unwrap());
        let b = CameraHandle::new(NonZeroU64::new(1).unwrap());
        let c = CameraHandle::new(NonZeroU64::new(2).unwrap());
        assert_eq!(a, b);
        assert_ne!(a, c);
        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

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

    #[test]
    fn camera_info_equality() {
        let a = CameraInfo {
            id: "id1".into(),
            model: "M".into(),
            serial: "S".into(),
            name: "N".into(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let c = CameraInfo {
            id: "id2".into(),
            ..a.clone()
        };
        assert_ne!(a, c);
    }
}
