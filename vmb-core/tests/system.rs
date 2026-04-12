//! Integration tests for [`VmbSystem`] backed by [`FakeVmbRuntime`].

use vmb_core::{VmbError, VmbSystem};
use vmb_fake::{FakeCall, FakeVmbRuntime, Method};

#[test]
fn startup_succeeds_and_drop_shuts_down() {
    let fake = FakeVmbRuntime::new();
    {
        let _system = VmbSystem::startup(fake.clone()).unwrap();
        assert!(fake.is_started(), "fake reports started after startup");
    }
    assert!(!fake.is_started(), "fake reports stopped after drop");
    assert_eq!(fake.calls(), vec![FakeCall::Startup, FakeCall::Shutdown]);
}

#[test]
fn second_startup_while_first_alive_returns_already_started() {
    let fake = FakeVmbRuntime::new();
    let _system = VmbSystem::startup(fake.clone()).unwrap();
    let err = VmbSystem::startup(fake.clone()).unwrap_err();
    assert!(matches!(err, VmbError::AlreadyStarted));
}

#[test]
fn startup_after_previous_dropped_succeeds_again() {
    let fake = FakeVmbRuntime::new();
    drop(VmbSystem::startup(fake.clone()).unwrap());
    let _system = VmbSystem::startup(fake.clone()).unwrap();
    assert!(fake.is_started());
}

#[test]
fn startup_failure_rolls_back_flag() {
    let fake = FakeVmbRuntime::new();
    fake.fail_next(
        Method::Startup,
        VmbError::Sdk {
            code: -1,
            message: "boom".into(),
        },
    );
    let err = VmbSystem::startup(fake.clone()).unwrap_err();
    assert!(matches!(err, VmbError::Sdk { code: -1, .. }));
    // Must be retryable — flag was rolled back.
    let _system = VmbSystem::startup(fake.clone()).unwrap();
    assert!(fake.is_started());
}
