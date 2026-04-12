//! Integration tests for [`Camera`] backed by [`FakeVmbRuntime`].

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use vmb_core::{CameraInfo, PixelFormat, VmbError, VmbSystem};
use vmb_fake::{FakeCall, FakeVmbRuntime, Method};

fn sample_cameras() -> Vec<CameraInfo> {
    vec![
        CameraInfo {
            id: "DEV_A".into(),
            model: "Alvium".into(),
            serial: "S1".into(),
            name: "front".into(),
        },
        CameraInfo {
            id: "DEV_B".into(),
            model: "Alvium".into(),
            serial: "S2".into(),
            name: "rear".into(),
        },
    ]
}

#[test]
fn list_cameras_returns_configured_set() {
    let fake = FakeVmbRuntime::new();
    fake.set_camera_list(sample_cameras());
    let system = VmbSystem::startup(fake).unwrap();
    let cams = system.list_cameras().unwrap();
    assert_eq!(cams.len(), 2);
    assert_eq!(cams[0].id, "DEV_A");
    assert_eq!(cams[1].model, "Alvium");
}

#[test]
fn list_cameras_propagates_runtime_error() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::ListCameras,
        VmbError::Sdk {
            code: -28,
            message: "unknown".into(),
        },
    );
    let system = VmbSystem::startup(fake).unwrap();
    assert!(matches!(
        system.list_cameras().unwrap_err(),
        VmbError::Sdk { code: -28, .. }
    ));
}

#[test]
fn open_camera_propagates_runtime_error() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::OpenCamera,
        VmbError::Sdk {
            code: -3,
            message: "not found".into(),
        },
    );
    let system = VmbSystem::startup(fake).unwrap();
    let err = system.open_camera("DEV_X").unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -3, .. }));
}

#[test]
fn camera_id_round_trips() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake).unwrap();
    let camera = system.open_camera("DEV_Z").unwrap();
    assert_eq!(camera.id(), "DEV_Z");
}

#[test]
fn camera_debug_includes_id_and_capture_state() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake).unwrap();
    let mut camera = system.open_camera("DEV_DEBUG").unwrap();
    let before = format!("{camera:?}");
    assert!(before.contains("DEV_DEBUG"));
    assert!(before.contains("capture_running"));
    assert!(before.contains("false"));

    camera.start_capture(1, |_| {}).unwrap();
    let after = format!("{camera:?}");
    assert!(after.contains("true"));
}

#[test]
fn system_debug_renders_struct_marker() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake).unwrap();
    let dbg = format!("{system:?}");
    assert!(dbg.contains("VmbSystem"));
}

#[test]
fn load_settings_passes_path_to_runtime() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let camera = system.open_camera("DEV_A").unwrap();
    camera.load_settings(Path::new("/tmp/day.xml")).unwrap();
    let handle = fake.handle_for("DEV_A").unwrap();
    assert!(fake
        .calls()
        .contains(&FakeCall::LoadSettings(handle, "/tmp/day.xml".into())));
    drop(camera);
}

#[test]
fn load_settings_propagates_runtime_error() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::LoadSettings,
        VmbError::Sdk {
            code: -29,
            message: "xml".into(),
        },
    );
    let system = VmbSystem::startup(fake).unwrap();
    let camera = system.open_camera("DEV_A").unwrap();
    assert!(matches!(
        camera.load_settings(Path::new("/tmp/bad.xml")).unwrap_err(),
        VmbError::Sdk { code: -29, .. }
    ));
}

#[test]
fn start_capture_happy_path_issues_exact_sequence() {
    let fake = FakeVmbRuntime::new();
    fake.set_payload_size(2048);
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    camera.start_capture(3, |_frame| {}).unwrap();

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, FakeCall::AnnounceFrame(_, 2048)))
            .count(),
        3,
    );
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, FakeCall::QueueFrame(_, _, _)))
            .count(),
        3
    );
    assert!(calls.iter().any(
        |c| matches!(c, FakeCall::RunFeatureCommand(ch, name) if *ch == h && name == "AcquisitionStart"),
    ));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::CaptureStart(ch) if *ch == h)));

    // AcquisitionStart comes AFTER all queue_frames.
    let acq_start_pos = calls
        .iter()
        .position(
            |c| matches!(c, FakeCall::RunFeatureCommand(_, name) if name == "AcquisitionStart"),
        )
        .unwrap();
    let last_queue_pos = calls
        .iter()
        .rposition(|c| matches!(c, FakeCall::QueueFrame(..)))
        .unwrap();
    assert!(acq_start_pos > last_queue_pos);
}

#[test]
fn start_capture_double_call_returns_already_running() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();
    camera.start_capture(2, |_| {}).unwrap();
    let err = camera.start_capture(2, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::CaptureAlreadyRunning));
}

#[test]
fn start_capture_fails_when_payload_size_fails() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::PayloadSize,
        VmbError::Sdk {
            code: -4,
            message: "bad handle".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();
    let err = camera.start_capture(2, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -4, .. }));
}

#[test]
fn start_capture_unwinds_when_announce_fails_midway() {
    let fake = FakeVmbRuntime::new();
    fake.fail_nth(
        Method::AnnounceFrame,
        2,
        VmbError::Sdk {
            code: -14,
            message: "resources".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    let err = camera.start_capture(4, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -14, .. }));

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
    assert!(calls.contains(&FakeCall::CaptureQueueFlush(h)));
    assert!(calls.contains(&FakeCall::FrameRevokeAll(h)));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::UninstallFrameCallback(_))));
    assert!(!calls.iter().any(|c| matches!(c, FakeCall::CaptureStart(_))));

    // State fully rolled back — second attempt succeeds.
    camera.start_capture(2, |_| {}).unwrap();
}

#[test]
fn start_capture_unwinds_when_capture_start_fails() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::CaptureStart,
        VmbError::Sdk {
            code: -24,
            message: "busy".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    let err = camera.start_capture(2, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -24, .. }));

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
    assert!(calls.contains(&FakeCall::FrameRevokeAll(h)));
    assert_eq!(
        calls
            .iter()
            .filter(|c| matches!(c, FakeCall::AnnounceFrame(_, _)))
            .count(),
        2
    );
    assert!(!calls.iter().any(|c| matches!(c, FakeCall::QueueFrame(..))));
}

#[test]
fn start_capture_unwinds_when_queue_frame_fails() {
    let fake = FakeVmbRuntime::new();
    fake.fail_nth(
        Method::QueueFrame,
        1,
        VmbError::Sdk {
            code: -15,
            message: "invalid".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();
    let err = camera.start_capture(3, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -15, .. }));

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
    assert!(!calls
        .iter()
        .any(|c| matches!(c, FakeCall::RunFeatureCommand(_, n) if n == "AcquisitionStart")));
}

#[test]
fn start_capture_unwinds_when_run_feature_command_fails() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::RunFeatureCommand,
        VmbError::Sdk {
            code: -18,
            message: "not supported".into(),
        },
    );
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    let err = camera.start_capture(2, |_| {}).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -18, .. }));

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
}

#[test]
fn stop_capture_is_noop_when_no_capture_running() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    camera.stop_capture().unwrap();

    let calls = fake.calls();
    assert!(!calls
        .iter()
        .any(|c| matches!(c, FakeCall::CaptureEnd(_) | FakeCall::FrameRevokeAll(_))));
}

#[test]
fn stop_capture_after_start_runs_full_teardown_and_is_idempotent() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();
    camera.start_capture(2, |_| {}).unwrap();
    camera.stop_capture().unwrap();

    let h = fake.handle_for("DEV_A").unwrap();
    let calls = fake.calls();
    assert!(calls.iter().any(
        |c| matches!(c, FakeCall::RunFeatureCommand(ch, n) if *ch == h && n == "AcquisitionStop"),
    ));
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
    assert!(calls.contains(&FakeCall::CaptureQueueFlush(h)));
    assert!(calls.contains(&FakeCall::FrameRevokeAll(h)));
    assert!(calls
        .iter()
        .any(|c| matches!(c, FakeCall::UninstallFrameCallback(_))));

    let count_before = fake.call_count();
    camera.stop_capture().unwrap();
    assert_eq!(fake.call_count(), count_before);
}

#[test]
fn drop_of_camera_with_live_capture_invokes_stop_then_close() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();
    camera.start_capture(2, |_| {}).unwrap();
    let h = fake.handle_for("DEV_A").unwrap();
    drop(camera);

    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CaptureEnd(h)));
    assert!(calls.contains(&FakeCall::CloseCamera(h)));

    let stop_pos = calls
        .iter()
        .position(|c| *c == FakeCall::CaptureEnd(h))
        .unwrap();
    let close_pos = calls
        .iter()
        .position(|c| *c == FakeCall::CloseCamera(h))
        .unwrap();
    assert!(stop_pos < close_pos);
}

#[test]
fn drop_of_idle_camera_closes_but_does_not_stop() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let camera = system.open_camera("DEV_A").unwrap();
    let h = fake.handle_for("DEV_A").unwrap();
    drop(camera);
    let calls = fake.calls();
    assert!(calls.contains(&FakeCall::CloseCamera(h)));
    assert!(!calls.iter().any(|c| matches!(c, FakeCall::CaptureEnd(_))));
}

#[test]
fn deliver_frame_invokes_user_callback_with_metadata() {
    let fake = FakeVmbRuntime::new();
    let system = VmbSystem::startup(fake.clone()).unwrap();
    let mut camera = system.open_camera("DEV_A").unwrap();

    let counter = Arc::new(AtomicUsize::new(0));
    let sizes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let c = counter.clone();
    let s = sizes.clone();
    camera
        .start_capture(1, move |frame| {
            c.fetch_add(1, Ordering::SeqCst);
            s.lock().unwrap().push((frame.len(), frame.pixel_format));
        })
        .unwrap();

    let h = fake.handle_for("DEV_A").unwrap();
    assert!(fake.deliver_frame(h, &[1u8, 2, 3, 4], 2, 2, PixelFormat::Mono8));
    assert!(fake.deliver_frame(h, &[9u8, 8], 1, 2, PixelFormat::Bgr8));

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let s = sizes.lock().unwrap();
    assert_eq!(s[0], (4, PixelFormat::Mono8));
    assert_eq!(s[1], (2, PixelFormat::Bgr8));
}
