//! Contract test for [`FakeVmbRuntime::pre_open_camera`] — the
//! scenario hook that lets consumer tests exercise an
//! "already-open" guard at an `open_and_start`-shaped seam.
//!
//! This test shows both states of the consumer-side guard being
//! driven deterministically through the fake, which is the
//! definition-of-done: the mutant that deletes `!` from
//! `if state.cameras.contains_key(id)` is detectable because the
//! `OpenCamera` call log differs between the two states.

use std::collections::HashMap;

use vmb_core::{port::VmbRuntime, CameraHandle};
use vmb_fake::{FakeCall, FakeVmbRuntime};

/// Consumer-shaped guard: mirrors the
/// `if state.cameras.contains_key(id) { return Ok(()) }` early-return
/// in `mira-ingest/src/source/vimba.rs::open_and_start`.
///
/// Returns `true` if the guard short-circuited.
fn open_and_start_guarded(
    runtime: &FakeVmbRuntime,
    state: &mut HashMap<String, CameraHandle>,
    id: &str,
) -> bool {
    if state.contains_key(id) {
        return true;
    }
    let handle = runtime.open_camera(id).expect("open_camera");
    state.insert(id.to_string(), handle);
    false
}

#[test]
fn proceeds_when_camera_not_yet_open() {
    let fake = FakeVmbRuntime::new();
    let mut state: HashMap<String, CameraHandle> = HashMap::new();

    let short_circuited = open_and_start_guarded(&fake, &mut state, "cam-a");

    assert!(!short_circuited, "guard must not short-circuit on fresh state");
    assert_eq!(
        fake.calls(),
        vec![FakeCall::OpenCamera("cam-a".into())],
        "OpenCamera must be issued exactly once",
    );
    assert!(state.contains_key("cam-a"));
}

#[test]
fn short_circuits_when_camera_already_open() {
    let fake = FakeVmbRuntime::new();
    let mut state: HashMap<String, CameraHandle> = HashMap::new();

    // Drive the fake into the "already open" state without running
    // the full open sequence. Mirror the resulting handle into the
    // consumer state so the consumer-side guard observes it.
    let preset = fake.pre_open_camera("cam-a");
    state.insert("cam-a".into(), preset);

    let short_circuited = open_and_start_guarded(&fake, &mut state, "cam-a");

    assert!(short_circuited, "guard must short-circuit when already open");
    assert!(
        fake.calls().is_empty(),
        "no runtime calls expected on the short-circuit path, got {:?}",
        fake.calls(),
    );
    assert_eq!(fake.handle_for("cam-a"), Some(preset));
}

#[test]
fn pre_open_does_not_consume_failure_injection() {
    use vmb_core::error::VmbError;
    use vmb_fake::Method;

    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::OpenCamera,
        VmbError::Sdk {
            code: -3,
            message: "not found".into(),
        },
    );

    // Scenario hook bypasses failure injection entirely.
    let _ = fake.pre_open_camera("cam-a");

    // The rigged failure must still fire on the next real call.
    let err = fake.open_camera("cam-b").unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -3, .. }));
}
