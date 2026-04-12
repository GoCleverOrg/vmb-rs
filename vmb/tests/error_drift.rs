//! Drift guard: ensure `vmb_core::error_name` stays in sync with bindgen's
//! `Debug` impl on `VmbErrorType`. If the bindings are regenerated and a
//! variant is renamed, this test will fail for the sampled codes.
//!
//! The guard lives here (in the facade crate) rather than in `vmb-core`
//! because it needs both the hand-written name table and the bindgen
//! output, which live in different crates by design.

use vmb::error_name;
use vmb_sys::VmbErrorType;

#[test]
fn error_name_matches_bindgen_debug_for_sampled_variants() {
    let cases = [
        VmbErrorType::VmbErrorSuccess,
        VmbErrorType::VmbErrorInternalFault,
        VmbErrorType::VmbErrorBadHandle,
        VmbErrorType::VmbErrorBadParameter,
        VmbErrorType::VmbErrorTimeout,
        VmbErrorType::VmbErrorIO,
    ];
    for variant in cases {
        let code = variant as i32;
        let name = error_name(code);
        let debug_name = format!("{variant:?}");
        assert_eq!(
            name, debug_name,
            "drift for code {code}: handwritten={name}, bindgen={debug_name}"
        );
    }
}
