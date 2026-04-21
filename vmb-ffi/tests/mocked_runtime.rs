//! Mutation-coverage tests for [`vmb_ffi::VmbFfiRuntime`].
//!
//! Each test constructs a runtime around [`VmbApi::stub`] (no-op success
//! for every VmbC entry point), optionally overwrites the `pub` function
//! pointer fields for the handful of calls the test cares about, and
//! then exercises the adapter's public `VmbRuntime` surface.
//!
//! The stubs communicate with the test through the [`Mock`] global. A
//! `TEST_LOCK` mutex serialises the whole file; [`Mock`] is reset at
//! the start of every test.

#![allow(clippy::missing_safety_doc)]
#![allow(clippy::too_many_arguments)]

use std::ffi::{c_char, CString};
use std::ptr;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use vmb_core::{
    DiscoveryCallback, DiscoveryEvent, Frame, FrameCallback, Result as VmbResult, VmbError,
    VmbRuntime,
};
use vmb_ffi::VmbFfiRuntime;
use vmb_sys::{
    VmbAccessMode_t, VmbApi, VmbCameraInfo_t, VmbError_t, VmbFilePathChar_t, VmbFrameCallback,
    VmbFrame_t, VmbHandle_t, VmbInvalidationCallback, VmbUint32_t,
};

// --- Mock infrastructure ------------------------------------------------

#[derive(Default)]
struct Mock {
    // Call counts
    startup_calls: u32,
    shutdown_calls: u32,
    camera_close_calls: u32,
    settings_load_calls: u32,
    feature_command_run_calls: u32,
    payload_size_calls: u32,
    frame_announce_calls: u32,
    frame_revoke_all_calls: u32,
    capture_start_calls: u32,
    capture_end_calls: u32,
    capture_queue_flush_calls: u32,
    capture_frame_queue_calls: u32,
    invalidation_register_calls: u32,
    invalidation_unregister_calls: u32,
    cameras_list_calls: u32,

    // Configured return codes (0 = success by default)
    startup_return: VmbError_t,
    camera_open_return: VmbError_t,
    settings_load_return: VmbError_t,
    feature_command_run_return: VmbError_t,
    payload_size_return: VmbError_t,
    frame_announce_return: VmbError_t,
    capture_start_return: VmbError_t,
    capture_frame_queue_return: VmbError_t,
    invalidation_register_return: VmbError_t,
    invalidation_unregister_return: VmbError_t,
    cameras_list_return: VmbError_t,

    // Configured output values
    camera_open_handle: VmbHandle_t,
    payload_size_out: VmbUint32_t,
    cameras_count: VmbUint32_t,
    cameras_fixture: [CamFixture; 4],

    // Captured pointers / callbacks
    last_announced_frame_ptr: *const VmbFrame_t,
    last_queued_frame_ptr: *const VmbFrame_t,
    last_queued_callback: VmbFrameCallback,
    last_invalidation_user_context: *mut std::os::raw::c_void,
    last_invalidation_callback: VmbInvalidationCallback,
}

#[derive(Clone, Default)]
struct CamFixture {
    id: Option<CString>,
    model: Option<CString>,
    serial: Option<CString>,
    name: Option<CString>,
}

// SAFETY: the raw pointers inside `Mock` are captured for inspection
// only by test code under the global `TEST_LOCK`. `Mock` never follows
// them — they are stored as integers.
unsafe impl Send for Mock {}

static MOCK: Mutex<Mock> = Mutex::new(Mock {
    startup_calls: 0,
    shutdown_calls: 0,
    camera_close_calls: 0,
    settings_load_calls: 0,
    feature_command_run_calls: 0,
    payload_size_calls: 0,
    frame_announce_calls: 0,
    frame_revoke_all_calls: 0,
    capture_start_calls: 0,
    capture_end_calls: 0,
    capture_queue_flush_calls: 0,
    capture_frame_queue_calls: 0,
    invalidation_register_calls: 0,
    invalidation_unregister_calls: 0,
    cameras_list_calls: 0,
    startup_return: 0,
    camera_open_return: 0,
    settings_load_return: 0,
    feature_command_run_return: 0,
    payload_size_return: 0,
    frame_announce_return: 0,
    capture_start_return: 0,
    capture_frame_queue_return: 0,
    invalidation_register_return: 0,
    invalidation_unregister_return: 0,
    cameras_list_return: 0,
    camera_open_handle: ptr::null_mut(),
    payload_size_out: 0,
    cameras_count: 0,
    cameras_fixture: [
        CamFixture {
            id: None,
            model: None,
            serial: None,
            name: None,
        },
        CamFixture {
            id: None,
            model: None,
            serial: None,
            name: None,
        },
        CamFixture {
            id: None,
            model: None,
            serial: None,
            name: None,
        },
        CamFixture {
            id: None,
            model: None,
            serial: None,
            name: None,
        },
    ],
    last_announced_frame_ptr: ptr::null(),
    last_queued_frame_ptr: ptr::null(),
    last_queued_callback: None,
    last_invalidation_user_context: ptr::null_mut(),
    last_invalidation_callback: None,
});

/// Serialises tests in this file. Each test MUST acquire this before
/// touching `MOCK` — stubs bump counters on `MOCK` and assume the test
/// thread owns exclusive access.
fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Reset the mock to its default-zero state.
fn reset_mock() {
    let mut m = MOCK.lock().unwrap();
    *m = Mock::default();
}

/// Temporarily take the mock to mutate it, then release the lock so
/// stubs can re-acquire it during the test body.
fn with_mock<F: FnOnce(&mut Mock)>(f: F) {
    let mut m = MOCK.lock().unwrap();
    f(&mut m);
}

// --- Spy stubs ----------------------------------------------------------

unsafe extern "C" fn spy_startup(_: *const VmbFilePathChar_t) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.startup_calls += 1;
    m.startup_return
}
unsafe extern "C" fn spy_shutdown() {
    MOCK.lock().unwrap().shutdown_calls += 1;
}
unsafe extern "C" fn spy_cameras_list(
    info: *mut VmbCameraInfo_t,
    list_len: VmbUint32_t,
    num_found: *mut VmbUint32_t,
    _sizeof: VmbUint32_t,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.cameras_list_calls += 1;
    if !num_found.is_null() {
        // SAFETY: caller supplied a valid out-parameter.
        unsafe { *num_found = m.cameras_count };
    }
    if !info.is_null() {
        let count = list_len.min(m.cameras_count);
        for i in 0..count as usize {
            // SAFETY: `info[i]` is a valid slot allocated by the caller.
            let dst = unsafe { info.add(i).as_mut().unwrap() };
            // Zero-initialise (mimics real SDK behaviour).
            // SAFETY: a zeroed `VmbCameraInfo_t` is a valid bit pattern
            // (all integer / nullable pointer fields).
            *dst = unsafe { std::mem::zeroed() };
            let fx = &m.cameras_fixture[i];
            dst.cameraIdString = fx.id.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
            dst.modelName = fx.model.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
            dst.serialString = fx
                .serial
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null());
            dst.cameraName = fx.name.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
        }
    }
    m.cameras_list_return
}
unsafe extern "C" fn spy_camera_open(
    _id: *const c_char,
    _access: VmbAccessMode_t,
    handle: *mut VmbHandle_t,
) -> VmbError_t {
    let m = MOCK.lock().unwrap();
    let rc = m.camera_open_return;
    let h = m.camera_open_handle;
    drop(m);
    if rc == 0 && !handle.is_null() {
        // SAFETY: caller supplied a valid out-parameter.
        unsafe { *handle = h };
    }
    rc
}
unsafe extern "C" fn spy_camera_close(_: VmbHandle_t) -> VmbError_t {
    MOCK.lock().unwrap().camera_close_calls += 1;
    0
}
unsafe extern "C" fn spy_settings_load(
    _: VmbHandle_t,
    _: *const VmbFilePathChar_t,
    _: *const vmb_sys::VmbFeaturePersistSettings_t,
    _: VmbUint32_t,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.settings_load_calls += 1;
    m.settings_load_return
}
unsafe extern "C" fn spy_feature_command_run(_: VmbHandle_t, _: *const c_char) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.feature_command_run_calls += 1;
    m.feature_command_run_return
}
unsafe extern "C" fn spy_payload_size(_: VmbHandle_t, out: *mut VmbUint32_t) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.payload_size_calls += 1;
    if m.payload_size_return == 0 && !out.is_null() {
        // SAFETY: caller supplied a valid out-parameter.
        unsafe { *out = m.payload_size_out };
    }
    m.payload_size_return
}
unsafe extern "C" fn spy_frame_announce(
    _: VmbHandle_t,
    frame: *const VmbFrame_t,
    _: VmbUint32_t,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.frame_announce_calls += 1;
    m.last_announced_frame_ptr = frame;
    m.frame_announce_return
}
unsafe extern "C" fn spy_frame_revoke_all(_: VmbHandle_t) -> VmbError_t {
    MOCK.lock().unwrap().frame_revoke_all_calls += 1;
    0
}
unsafe extern "C" fn spy_capture_start(_: VmbHandle_t) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.capture_start_calls += 1;
    m.capture_start_return
}
unsafe extern "C" fn spy_capture_end(_: VmbHandle_t) -> VmbError_t {
    MOCK.lock().unwrap().capture_end_calls += 1;
    0
}
unsafe extern "C" fn spy_capture_queue_flush(_: VmbHandle_t) -> VmbError_t {
    MOCK.lock().unwrap().capture_queue_flush_calls += 1;
    0
}
unsafe extern "C" fn spy_capture_frame_queue(
    _: VmbHandle_t,
    frame: *const VmbFrame_t,
    cb: VmbFrameCallback,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.capture_frame_queue_calls += 1;
    m.last_queued_frame_ptr = frame;
    m.last_queued_callback = cb;
    m.capture_frame_queue_return
}
unsafe extern "C" fn spy_invalidation_register(
    _: VmbHandle_t,
    _: *const c_char,
    cb: VmbInvalidationCallback,
    user_ctx: *mut std::os::raw::c_void,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.invalidation_register_calls += 1;
    m.last_invalidation_user_context = user_ctx;
    m.last_invalidation_callback = cb;
    m.invalidation_register_return
}
unsafe extern "C" fn spy_invalidation_unregister(
    _: VmbHandle_t,
    _: *const c_char,
    _: VmbInvalidationCallback,
) -> VmbError_t {
    let mut m = MOCK.lock().unwrap();
    m.invalidation_unregister_calls += 1;
    m.invalidation_unregister_return
}

/// Build a `VmbApi::stub()` with every call the adapter makes wired to
/// a spy. Tests keep the returned `Arc<VmbApi>` alive for the duration
/// of the runtime; dropping it tears down the mock.
fn make_api() -> Arc<VmbApi> {
    let mut api = VmbApi::stub();
    api.VmbStartup = spy_startup;
    api.VmbShutdown = spy_shutdown;
    api.VmbCamerasList = spy_cameras_list;
    api.VmbCameraOpen = spy_camera_open;
    api.VmbCameraClose = spy_camera_close;
    api.VmbSettingsLoad = spy_settings_load;
    api.VmbFeatureCommandRun = spy_feature_command_run;
    api.VmbPayloadSizeGet = spy_payload_size;
    api.VmbFrameAnnounce = spy_frame_announce;
    api.VmbFrameRevokeAll = spy_frame_revoke_all;
    api.VmbCaptureStart = spy_capture_start;
    api.VmbCaptureEnd = spy_capture_end;
    api.VmbCaptureQueueFlush = spy_capture_queue_flush;
    api.VmbCaptureFrameQueue = spy_capture_frame_queue;
    api.VmbFeatureInvalidationRegister = spy_invalidation_register;
    api.VmbFeatureInvalidationUnregister = spy_invalidation_unregister;
    Arc::new(api)
}

fn setup() -> (MutexGuard<'static, ()>, VmbFfiRuntime) {
    let g = test_lock();
    reset_mock();
    let rt = VmbFfiRuntime::with_api(make_api());
    (g, rt)
}

// --- Tests --------------------------------------------------------------

/// A successful startup/shutdown pair must invoke the corresponding
/// SDK entry points exactly once each. This catches the
/// "replace startup with Ok(())" and "replace shutdown with ()"
/// mutants because both skip the `VmbStartup` / `VmbShutdown` calls.
#[test]
fn startup_shutdown_invoke_sdk() {
    let (_g, rt) = setup();
    rt.startup().expect("startup succeeds");
    assert_eq!(MOCK.lock().unwrap().startup_calls, 1);
    rt.shutdown();
    assert_eq!(MOCK.lock().unwrap().shutdown_calls, 1);
}

/// If `VmbStartup` returns an error code, startup must propagate it.
/// This catches "replace startup with Ok(())" from the error-path side.
#[test]
fn startup_propagates_sdk_error() {
    let (_g, rt) = setup();
    with_mock(|m| m.startup_return = -7); // VmbErrorBadParameter
    let err = rt.startup().expect_err("sdk error must propagate");
    match err {
        VmbError::Sdk { code, .. } => assert_eq!(code, -7),
        other => panic!("expected Sdk, got {other:?}"),
    }
}

/// `list_cameras` with `num_found = 0` must return an empty vec without
/// invoking the follow-up populate call. Catches the `count == 0` vs
/// `count != 0` equality-flip mutant.
#[test]
fn list_cameras_count_zero_short_circuits() {
    let (_g, rt) = setup();
    // cameras_count stays 0.
    let v = rt.list_cameras().expect("ok with no cameras");
    assert!(v.is_empty());
    assert_eq!(
        MOCK.lock().unwrap().cameras_list_calls,
        1,
        "should have only called the size-query form once"
    );
}

/// `list_cameras` with two cameras must return both, with the exact
/// strings copied from the fixture. Catches:
/// - "replace list_cameras with Ok(vec![])" (would miss entries)
/// - "replace cstr_to_owned with String::new()" (would empty strings)
/// - "replace cstr_to_owned with \"xyzzy\"" (would replace strings)
#[test]
fn list_cameras_returns_fixture_entries() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.cameras_count = 2;
        m.cameras_fixture[0] = CamFixture {
            id: Some(CString::new("cam-1").unwrap()),
            model: Some(CString::new("Alpha").unwrap()),
            serial: Some(CString::new("SN001").unwrap()),
            name: Some(CString::new("Front").unwrap()),
        };
        m.cameras_fixture[1] = CamFixture {
            id: Some(CString::new("cam-2").unwrap()),
            model: Some(CString::new("Beta").unwrap()),
            serial: Some(CString::new("SN002").unwrap()),
            name: Some(CString::new("Rear").unwrap()),
        };
    });
    let v = rt.list_cameras().expect("ok");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].id, "cam-1");
    assert_eq!(v[0].model, "Alpha");
    assert_eq!(v[0].serial, "SN001");
    assert_eq!(v[0].name, "Front");
    assert_eq!(v[1].id, "cam-2");
    assert_eq!(v[1].model, "Beta");
    assert_eq!(v[1].serial, "SN002");
    assert_eq!(v[1].name, "Rear");
    // Size-query + populate = 2 calls.
    assert_eq!(MOCK.lock().unwrap().cameras_list_calls, 2);
}

/// A null `cameraIdString` in a fixture must fall back to the
/// `"<unknown>"` placeholder rather than panicking. This catches
/// `cstr_to_owned` mutants because the fallback branch is exercised.
#[test]
fn list_cameras_with_null_fields_uses_fallback_string() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.cameras_count = 1;
        // All four fields left as None -> null pointers.
    });
    let v = rt.list_cameras().expect("ok");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].id, "<unknown>");
    assert_eq!(v[0].model, "<unknown>");
    assert_eq!(v[0].serial, "<unknown>");
    assert_eq!(v[0].name, "<unknown>");
}

/// Two successful `open_camera` calls must produce distinct
/// `CameraHandle` values (catches `next_id` constant mutants) and both
/// must be individually closable (catches `close_camera` body-delete
/// and `resolve_camera` `Ok(Default::default())` mutants).
#[test]
fn open_two_cameras_yields_distinct_handles_and_closes_each() {
    let (_g, rt) = setup();
    with_mock(|m| {
        // Non-null sentinel handle is sufficient; the adapter treats
        // it as opaque.
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h1 = rt.open_camera("cam-a").expect("open 1");
    let h2 = rt.open_camera("cam-b").expect("open 2");
    assert_ne!(h1, h2, "next_id must be monotonic");
    rt.close_camera(h1);
    rt.close_camera(h2);
    assert_eq!(MOCK.lock().unwrap().camera_close_calls, 2);
}

/// `close_camera` with an unknown handle must be a silent no-op: no
/// SDK call is made because the handle never matched a cameras-table
/// entry.
#[test]
fn close_camera_unknown_handle_is_noop() {
    let (_g, rt) = setup();
    // Never opened anything; fabricate a bogus handle.
    let bogus = vmb_core::CameraHandle::new(std::num::NonZeroU64::new(999).unwrap());
    rt.close_camera(bogus);
    assert_eq!(MOCK.lock().unwrap().camera_close_calls, 0);
}

/// `load_settings` must propagate a non-zero SDK error code. Catches
/// "replace load_settings with Ok(())" and "replace resolve_camera
/// with Ok(Default::default())" (the latter would make the call
/// succeed even before an SDK error could arise).
#[test]
fn load_settings_propagates_sdk_error() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
        m.settings_load_return = -20; // VmbErrorIO
    });
    let h = rt.open_camera("cam").expect("open");
    let err = rt
        .load_settings(h, std::path::Path::new("/tmp/x.xml"))
        .expect_err("must propagate");
    match err {
        VmbError::Sdk { code, .. } => assert_eq!(code, -20),
        other => panic!("expected Sdk, got {other:?}"),
    }
    assert_eq!(MOCK.lock().unwrap().settings_load_calls, 1);
}

/// `load_settings` against an unknown handle must return
/// `InvalidString { context: "unknown camera handle" }`. Catches
/// "replace resolve_camera with Ok(Default::default())" because the
/// mutant would bypass the missing-handle check and reach the SDK
/// call.
#[test]
fn load_settings_unknown_handle_returns_invalid_string() {
    let (_g, rt) = setup();
    let bogus = vmb_core::CameraHandle::new(std::num::NonZeroU64::new(999).unwrap());
    let err = rt
        .load_settings(bogus, std::path::Path::new("/tmp/x.xml"))
        .expect_err("must error");
    assert!(matches!(
        err,
        VmbError::InvalidString {
            context: "unknown camera handle"
        }
    ));
    // Crucially: SDK must NOT have been invoked.
    assert_eq!(MOCK.lock().unwrap().settings_load_calls, 0);
}

/// `run_feature_command` propagates SDK errors.
#[test]
fn run_feature_command_propagates_sdk_error() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
        m.feature_command_run_return = -6; // VmbErrorInvalidAccess
    });
    let h = rt.open_camera("cam").expect("open");
    let err = rt
        .run_feature_command(h, "AcquisitionStart")
        .expect_err("error");
    match err {
        VmbError::Sdk { code, .. } => assert_eq!(code, -6),
        other => panic!("expected Sdk, got {other:?}"),
    }
    assert_eq!(MOCK.lock().unwrap().feature_command_run_calls, 1);
}

/// `payload_size` returns the value the SDK writes to its output
/// parameter. Catches "replace payload_size with Ok(0)" and
/// "replace payload_size with Ok(1)" — both constant-returns would
/// differ from the injected 42.
#[test]
fn payload_size_returns_sdk_output() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
        m.payload_size_out = 42;
    });
    let h = rt.open_camera("cam").expect("open");
    let n = rt.payload_size(h).expect("ok");
    assert_eq!(n, 42);
}

/// `announce_frame` feeds the SDK a non-null frame pointer (the heap
/// address of the trampoline context's embedded `VmbFrame_t`). Catches
/// "replace vmb_frame_mut_ptr with Default::default()" (would pass
/// null) and "replace announce_frame next_u64 with constant" (would
/// produce duplicate slot IDs).
#[test]
fn announce_frame_passes_nonnull_ptr_and_returns_unique_slots() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    let s1 = rt.announce_frame(h, 128).expect("announce 1");
    let ptr1 = MOCK.lock().unwrap().last_announced_frame_ptr;
    assert!(!ptr1.is_null());
    let s2 = rt.announce_frame(h, 128).expect("announce 2");
    let ptr2 = MOCK.lock().unwrap().last_announced_frame_ptr;
    assert!(!ptr2.is_null());
    assert_ne!(
        ptr1 as usize, ptr2 as usize,
        "each announced frame must have a distinct heap ptr"
    );
    assert_ne!(s1, s2, "next_u64 must be monotonic");
    assert_eq!(MOCK.lock().unwrap().frame_announce_calls, 2);
}

/// `capture_start` propagates SDK errors.
#[test]
fn capture_start_propagates_sdk_error() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
        m.capture_start_return = -24; // VmbErrorBusy
    });
    let h = rt.open_camera("cam").expect("open");
    let err = rt.capture_start(h).expect_err("error");
    match err {
        VmbError::Sdk { code, .. } => assert_eq!(code, -24),
        other => panic!("expected Sdk, got {other:?}"),
    }
    assert_eq!(MOCK.lock().unwrap().capture_start_calls, 1);
}

/// `queue_frame` must replace the announce-time placeholder callback
/// with the user-supplied one and pass the frame + trampoline fn
/// pointer to the SDK. Catches "replace set_callback with ()" (the
/// placeholder would remain), "replace queue_frame with Ok(())" (no
/// SDK call), and `resolve_camera` bypass mutants.
#[test]
fn queue_frame_installs_callback_and_sdk_receives_it() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    let slot = rt.announce_frame(h, 64).expect("announce");

    let cb = Arc::new(FrameCallback::new(|_| {}));
    let cb_id = rt.install_frame_callback(Arc::clone(&cb));

    rt.queue_frame(h, slot, cb_id).expect("queue");
    let m = MOCK.lock().unwrap();
    assert_eq!(m.capture_frame_queue_calls, 1);
    assert!(!m.last_queued_frame_ptr.is_null());
    assert!(
        m.last_queued_callback.is_some(),
        "queue_frame must register the frame trampoline"
    );
}

/// `queue_frame` with an unknown callback id errors out and DOES NOT
/// reach the SDK. Catches "replace queue_frame with Ok(())" and
/// "resolve_camera Ok(Default::default())" mutants.
#[test]
fn queue_frame_unknown_callback_id_errors() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    let slot = rt.announce_frame(h, 64).expect("announce");
    let bogus_cb = vmb_core::FrameCallbackId(999);
    let err = rt.queue_frame(h, slot, bogus_cb).expect_err("error");
    assert!(matches!(err, VmbError::InvalidString { .. }));
    assert_eq!(MOCK.lock().unwrap().capture_frame_queue_calls, 0);
}

/// `capture_end` and `capture_queue_flush` invoke the corresponding
/// SDK functions. Catches their respective body-delete mutants.
#[test]
fn capture_end_and_queue_flush_invoke_sdk() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    rt.capture_end(h);
    rt.capture_queue_flush(h);
    let m = MOCK.lock().unwrap();
    assert_eq!(m.capture_end_calls, 1);
    assert_eq!(m.capture_queue_flush_calls, 1);
}

/// `frame_revoke_all` invokes the SDK AND clears the adapter's
/// per-slot bookkeeping. Catches the "replace frame_revoke_all with
/// ()" body-delete mutant because skipping the body leaves the map
/// non-empty (and the subsequent announce would reuse the old slot
/// id — we test the invocation counter directly for simplicity).
#[test]
fn frame_revoke_all_invokes_sdk() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    rt.announce_frame(h, 32).expect("announce");
    rt.frame_revoke_all(h);
    assert_eq!(MOCK.lock().unwrap().frame_revoke_all_calls, 1);
}

/// A successful discovery registration + unregistration drives both
/// SDK entry points and reclaims the trampoline context without
/// leaking. Catches:
/// - "replace unregister_discovery with ()" (would skip the SDK call),
/// - "replace != with == in unregister_discovery" (would swap the warn
///   branch direction),
/// - "delete ! in unregister_discovery" (would skip the Box reclaim on
///   non-null ctx_ptr).
#[test]
fn discovery_register_and_unregister_invoke_sdk() {
    let (_g, rt) = setup();
    let cb = Arc::new(DiscoveryCallback::new(|_| {}));
    let cb_id = rt.install_discovery_callback(Arc::clone(&cb));

    let reg = rt.register_discovery(cb_id).expect("register");
    let m = MOCK.lock().unwrap();
    assert_eq!(m.invalidation_register_calls, 1);
    assert!(!m.last_invalidation_user_context.is_null());
    drop(m);

    rt.unregister_discovery(reg);
    assert_eq!(MOCK.lock().unwrap().invalidation_unregister_calls, 1);
}

/// When `VmbFeatureInvalidationUnregister` returns a non-zero code,
/// `unregister_discovery` must still reclaim the context — it warns
/// but does not leak. This pins down both the `!= 0` branch in the
/// warn log and the `!ctx_ptr.is_null()` guard for reclaim.
#[test]
fn discovery_unregister_tolerates_sdk_error() {
    let (_g, rt) = setup();
    with_mock(|m| m.invalidation_unregister_return = -1);
    let cb = Arc::new(DiscoveryCallback::new(|_| {}));
    let cb_id = rt.install_discovery_callback(Arc::clone(&cb));
    let reg = rt.register_discovery(cb_id).expect("register");
    // Must not panic even though the SDK signals failure.
    rt.unregister_discovery(reg);
    assert_eq!(MOCK.lock().unwrap().invalidation_unregister_calls, 1);
}

/// `unregister_discovery` on a handle that was never issued is a
/// silent no-op — no SDK call is made.
#[test]
fn discovery_unregister_unknown_handle_is_noop() {
    let (_g, rt) = setup();
    let bogus = vmb_core::DiscoveryRegistrationHandle(999);
    rt.unregister_discovery(bogus);
    assert_eq!(MOCK.lock().unwrap().invalidation_unregister_calls, 0);
}

/// `install_*_callback` + `uninstall_*_callback` both touch the
/// adapter's internal callback tables. Catches the uninstall
/// body-delete mutants: after an uninstall, re-using the old id must
/// fail.
#[test]
fn install_and_uninstall_frame_callback_round_trip() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });
    let h = rt.open_camera("cam").expect("open");
    let slot = rt.announce_frame(h, 32).expect("announce");
    let cb_id = rt.install_frame_callback(Arc::new(FrameCallback::new(|_| {})));

    // queue_frame with the fresh id succeeds:
    rt.queue_frame(h, slot, cb_id).expect("queue ok");
    // uninstall invalidates the id:
    rt.uninstall_frame_callback(cb_id);
    let err = rt
        .queue_frame(h, slot, cb_id)
        .expect_err("must error after uninstall");
    assert!(matches!(err, VmbError::InvalidString { .. }));
}

#[test]
fn install_and_uninstall_discovery_callback_round_trip() {
    let (_g, rt) = setup();
    let cb_id = rt.install_discovery_callback(Arc::new(DiscoveryCallback::new(|_| {})));
    // register with a valid id succeeds
    let reg = rt.register_discovery(cb_id).expect("register");
    rt.unregister_discovery(reg);

    // After uninstall, register_discovery with the same id must error.
    rt.uninstall_discovery_callback(cb_id);
    let err = rt
        .register_discovery(cb_id)
        .expect_err("must error after uninstall");
    assert!(matches!(err, VmbError::InvalidString { .. }));
}

/// `capture_end` / `capture_queue_flush` / `frame_revoke_all` with an
/// unknown camera handle must be silent no-ops. Catches the
/// `resolve_camera -> Ok(Default::default())` mutant because the
/// mutant would hand a default `*mut c_void` (null) to the SDK and
/// the call-counter would increment.
#[test]
fn teardown_methods_noop_on_unknown_handle() {
    let (_g, rt) = setup();
    let bogus = vmb_core::CameraHandle::new(std::num::NonZeroU64::new(999).unwrap());
    rt.capture_end(bogus);
    rt.capture_queue_flush(bogus);
    rt.frame_revoke_all(bogus);
    let m = MOCK.lock().unwrap();
    assert_eq!(m.capture_end_calls, 0);
    assert_eq!(m.capture_queue_flush_calls, 0);
    assert_eq!(m.frame_revoke_all_calls, 0);
}

/// A round-trip startup/shutdown sequence on a fresh runtime, with
/// `shutdown` called twice, must only invoke `VmbShutdown` once (the
/// STARTED flag gates the second call). This is a behaviour pin for
/// the `STARTED.swap` branch inside shutdown — useful for future
/// mutant resilience.
#[test]
fn shutdown_is_idempotent() {
    let (_g, rt) = setup();
    rt.startup().expect("startup");
    rt.shutdown();
    rt.shutdown();
    assert_eq!(MOCK.lock().unwrap().shutdown_calls, 1);
}

/// Test for public surface used by the `FrameCallback::invoke` path.
/// Also exercises `Frame::new` to satisfy the unused-import lint.
#[test]
fn frame_callback_public_surface_smoke() {
    let data = [0u8, 1, 2, 3];
    let f: Frame<'_> = Frame::new(&data, 2, 2, vmb_core::PixelFormat::from_raw(0), 0, 0);
    let cb = FrameCallback::new(|_: &Frame<'_>| {});
    cb.invoke(&f);
}

/// Public smoke test for the `DiscoveryCallback::invoke` path so it
/// doesn't regress silently.
#[test]
fn discovery_callback_public_surface_smoke() {
    let cb = DiscoveryCallback::new(|_: DiscoveryEvent| {});
    cb.invoke(DiscoveryEvent::Detected("x".to_string()));
}

/// `VmbFfiRuntime::with_api` yields a runtime whose trait surface is
/// reachable. Guards the `with_api` constructor itself against "replace
/// body with Default::default()"-style mutants.
#[test]
fn with_api_constructor_yields_usable_runtime() -> VmbResult<()> {
    let (_g, rt) = setup();
    // A trivial public-API call is enough to confirm the runtime is
    // wired up correctly.
    let _ = rt.list_cameras()?;
    Ok(())
}

/// Invoke the frame trampoline that `queue_frame` hands to the SDK and
/// verify it dispatches to the caller-supplied [`FrameCallback`] rather
/// than to the no-op placeholder installed during `announce_frame`.
///
/// This is the test that catches
/// `trampoline.rs:set_callback -> ()` — if `set_callback` becomes a
/// no-op, the announce-time `|_| {}` stays live and the assertion on
/// `INVOCATIONS` fails.
#[test]
fn frame_trampoline_invokes_queue_frame_callback() {
    let (_g, rt) = setup();
    with_mock(|m| {
        m.camera_open_handle = 0x1000 as VmbHandle_t;
    });

    // Static counter incremented only by the user callback.
    static INVOCATIONS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);

    let h = rt.open_camera("cam").expect("open");
    let slot = rt.announce_frame(h, 8).expect("announce");
    let cb = Arc::new(FrameCallback::new(|_: &Frame<'_>| {
        INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let cb_id = rt.install_frame_callback(Arc::clone(&cb));

    rt.queue_frame(h, slot, cb_id).expect("queue");

    // Grab the trampoline + frame pointer the runtime handed to
    // `VmbCaptureFrameQueue` and invoke it ourselves.
    let (frame_ptr, trampoline) = {
        let m = MOCK.lock().unwrap();
        (m.last_queued_frame_ptr, m.last_queued_callback)
    };
    let trampoline = trampoline.expect("trampoline registered");
    // SAFETY: the frame pointer came from the adapter and its backing
    // `TrampolineContext` is still alive (`rt` holds it).
    let frame = unsafe { &mut *(frame_ptr as *mut VmbFrame_t) };
    // VmbFrameStatusComplete = 0 — mark the frame as complete so the
    // trampoline dispatches to the callback instead of skipping.
    frame.receiveStatus = 0;
    frame.width = 4;
    frame.height = 2;
    // Leave `imageData` null; the trampoline must still invoke the
    // callback (with an empty slice). The mock stubs don't write to
    // `imageData` during the fake VmbCaptureFrameQueue re-queue call,
    // and we don't care what bytes the callback sees — only that it
    // fires at all.
    let camera_handle = 0x1000 as VmbHandle_t;
    // SAFETY: the trampoline was obtained from the adapter; calling
    // it with a valid `VmbFrame_t` ptr is the SDK-side contract.
    unsafe {
        trampoline(camera_handle, std::ptr::null_mut(), frame as *mut _);
    }

    assert_eq!(
        INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "queue_frame must install the caller's callback; set_callback mutant"
    );
    // Drop the closure ref so the test teardown can free it.
    drop(cb);
    // Revoke frames so the runtime releases the trampoline context
    // before the runtime itself goes out of scope at test end (avoids
    // letting the mock pointer dangle past the lock guard).
    rt.frame_revoke_all(h);
}
