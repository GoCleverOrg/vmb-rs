//! Integration tests for [`DiscoveryRegistration`] backed by
//! [`FakeVmbRuntime`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vmb_core::{DiscoveryEvent, VmbError, VmbSystem};
use vmb_fake::{FakeCall, FakeVmbRuntime, Method};

#[test]
fn discovery_registration_debug_renders_struct_marker() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake).unwrap();
    let reg = system.register_discovery(|_| {}).unwrap();
    let dbg = format!("{reg:?}");
    assert!(dbg.contains("DiscoveryRegistration"));
    assert!(dbg.contains("callback_id"));
}

#[test]
fn register_and_drop_emits_register_and_unregister() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    {
        let _reg = system.register_discovery(|_event| {}).unwrap();
    }

    let calls = fake.calls();
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::InstallDiscoveryCallback(_))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::RegisterDiscovery(_))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::UnregisterDiscovery(_))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::UninstallDiscoveryCallback(_))));

    // UnregisterDiscovery must come before UninstallDiscoveryCallback so
    // no firing trampoline can observe a removed callback.
    let unreg = calls
        .iter()
        .position(|c| matches!(c, FakeCall::UnregisterDiscovery(_)))
        .unwrap();
    let uninst = calls
        .iter()
        .position(|c| matches!(c, FakeCall::UninstallDiscoveryCallback(_)))
        .unwrap();
    assert!(unreg < uninst);
}

#[test]
fn register_failure_releases_callback_without_leak() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::RegisterDiscovery,
        VmbError::Sdk {
            code: -14,
            message: "resources".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let err = system.register_discovery(|_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -14, .. }));

    let calls = fake.calls();
    // Callback installed, register failed, callback uninstalled.
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::InstallDiscoveryCallback(_))));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::UninstallDiscoveryCallback(_))));
    // No registration handle was ever acquired, so no unregister call.
    assert!(!calls
        .iter()
        .any(|c| matches!(c, FakeCall::UnregisterDiscovery(_))));
}

#[test]
fn emitted_events_reach_all_registered_callbacks() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();

    let seen_a = Arc::new(Mutex::new(Vec::<DiscoveryEvent>::new()));
    let seen_b = Arc::new(Mutex::new(Vec::<DiscoveryEvent>::new()));
    let a = seen_a.clone();
    let b = seen_b.clone();
    let _r1 = system
        .register_discovery(move |e| a.lock().unwrap().push(e))
        .unwrap();
    let _r2 = system
        .register_discovery(move |e| b.lock().unwrap().push(e))
        .unwrap();

    fake.emit_discovery(DiscoveryEvent::Detected("cam-x".into()));
    fake.emit_discovery(DiscoveryEvent::Missing("cam-y".into()));
    fake.emit_discovery(DiscoveryEvent::Reachable("cam-z".into()));
    fake.emit_discovery(DiscoveryEvent::Unreachable("cam-w".into()));

    for seen in [seen_a, seen_b] {
        let s = seen.lock().unwrap();
        assert_eq!(s.len(), 4);
        assert!(matches!(s[0], DiscoveryEvent::Detected(ref id) if id == "cam-x"));
        assert!(matches!(s[1], DiscoveryEvent::Missing(ref id) if id == "cam-y"));
        assert!(matches!(s[2], DiscoveryEvent::Reachable(ref id) if id == "cam-z"));
        assert!(matches!(s[3], DiscoveryEvent::Unreachable(ref id) if id == "cam-w"));
    }
}

#[test]
fn events_after_drop_do_not_fire_callback() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();

    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let reg = system
        .register_discovery(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
    fake.emit_discovery(DiscoveryEvent::Detected("cam1".into()));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    drop(reg);
    fake.emit_discovery(DiscoveryEvent::Detected("cam2".into()));
    // Counter unchanged — callback was uninstalled.
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}
