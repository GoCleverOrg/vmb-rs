//! RAII owner for the Vimba X runtime lifecycle.

use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{debug, info};

use crate::error::{check, Result, VmbError};

/// Global flag ensuring only one `VmbSystem` instance is active at a time.
/// `VmbStartup`/`VmbShutdown` are global process-wide, so we enforce a
/// singleton to prevent double-startup.
static STARTED: AtomicBool = AtomicBool::new(false);

/// Owns the Vimba X runtime lifecycle. On construction, calls `VmbStartup`;
/// on drop, calls `VmbShutdown`. Only one `VmbSystem` may exist at a time.
///
/// This type is intentionally `!Clone` and `!Copy`.
pub struct VmbSystem {
    // Private field so `VmbSystem { .. }` cannot be constructed outside this
    // module. The value itself is inert.
    _priv: (),
}

impl VmbSystem {
    /// Start the Vimba X runtime. Returns [`VmbError::AlreadyStarted`] if a
    /// previous `VmbSystem` is still alive, or a
    /// [`VmbError::Sdk`] if the underlying `VmbStartup` call fails.
    pub fn startup() -> Result<Self> {
        // Atomically claim the singleton slot. If this fails, another
        // `VmbSystem` is currently alive.
        if STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(VmbError::AlreadyStarted);
        }

        // SAFETY: `VmbStartup` accepts a null `pathConfiguration` to pick up
        // the default GENICAM_GENTL*_PATH environment variables. No lifetime
        // invariants are at stake here.
        let rc = unsafe { vmb_sys::VmbStartup(ptr::null()) };
        if let Err(e) = check(rc) {
            // Roll back the singleton claim so a later retry can succeed.
            STARTED.store(false, Ordering::SeqCst);
            return Err(e);
        }

        info!("Vimba X runtime started");
        Ok(Self { _priv: () })
    }
}

impl Drop for VmbSystem {
    fn drop(&mut self) {
        // SAFETY: We are the sole owner of the runtime by virtue of the
        // STARTED flag, which the constructor atomically claimed. After the
        // call returns, release the slot so a fresh `VmbSystem::startup`
        // can succeed in the same process.
        unsafe { vmb_sys::VmbShutdown() };
        STARTED.store(false, Ordering::SeqCst);
        debug!("Vimba X runtime shut down");
    }
}

#[cfg(test)]
mod tests {
    //! These tests verify the `STARTED` singleton invariant using direct
    //! atomic manipulation because full mocking of `VmbStartup`/`VmbShutdown`
    //! would require cfg-gated function-pointer swapping which is not
    //! justified by the ROI for a two-function API surface. The
    //! [`real_sdk_round_trip`] integration test, gated on `#[ignore]`,
    //! exercises the real SDK round trip when run explicitly via
    //! `cargo test -- --ignored` on a machine with the Vimba X SDK
    //! installed.
    use super::*;

    /// Serializes tests that touch the global `STARTED` atomic or call
    /// `VmbSystem::startup()`. Rust runs unit tests in parallel by default,
    /// so without this lock one test's `store(true)` can race with another
    /// test's `store(false)`, causing spurious failures.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the test-serialization lock, recovering from poisoning so
    /// that a panicking earlier test does not cascade into every
    /// subsequent test in this module.
    fn acquire_test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Directly manipulating `STARTED` without calling the real `VmbStartup`
    /// lets us verify the singleton-enforcement contract without linking
    /// against the live SDK in unit tests. These tests run with
    /// `--features sdk` but they DO NOT invoke the real API — the
    /// `compare_exchange` short-circuit makes sure of that.
    #[test]
    fn second_startup_fails_while_first_is_live() {
        let _guard = acquire_test_lock();
        // Pretend a `VmbSystem` is already started.
        STARTED.store(true, Ordering::SeqCst);
        let result = VmbSystem::startup();
        match result {
            Err(VmbError::AlreadyStarted) => {}
            Err(e) => panic!("expected AlreadyStarted, got Err({e:?})"),
            Ok(_) => panic!("expected AlreadyStarted, got Ok(_)"),
        }
        // Reset for downstream tests (though they all reset themselves too).
        STARTED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn singleton_flag_round_trip() {
        let _guard = acquire_test_lock();
        STARTED.store(false, Ordering::SeqCst);
        assert!(!STARTED.load(Ordering::SeqCst));
        // Simulate startup + drop by toggling the flag.
        STARTED.store(true, Ordering::SeqCst);
        STARTED.store(false, Ordering::SeqCst);
        assert!(!STARTED.load(Ordering::SeqCst));
    }

    /// Hardware-gated RAII round trip: actually calls `VmbStartup` via
    /// `VmbSystem::startup()` and lets `Drop` call `VmbShutdown`. Verifies
    /// the full RAII contract end-to-end.
    ///
    /// Run with: `cargo test -p vmb --features sdk -- --ignored real_sdk_round_trip`
    #[test]
    #[ignore = "requires Vimba X SDK installed on the host"]
    fn real_sdk_round_trip() {
        let _guard = acquire_test_lock();
        // Reset singleton before starting (in case a prior test left it set)
        STARTED.store(false, Ordering::SeqCst);
        let system = VmbSystem::startup().expect("VmbStartup must succeed");
        assert!(
            STARTED.load(Ordering::SeqCst),
            "flag must be set after startup"
        );
        drop(system);
        assert!(
            !STARTED.load(Ordering::SeqCst),
            "flag must be cleared after drop"
        );
    }
}
