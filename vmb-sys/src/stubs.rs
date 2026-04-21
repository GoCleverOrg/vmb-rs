//! Trivial no-op success stubs used by [`crate::VmbApi::stub`].
//!
//! Each function has the exact signature required by the corresponding
//! `VmbApi` field; all ignore their inputs and return `VmbErrorSuccess`
//! (code `0`). Tests that need specific behaviour assign their own spy
//! function pointer over the `pub` field.
//!
//! This file is excluded from mutation testing in `.cargo/mutants.toml`
//! because mutating a no-op stub produces a mutant that has no
//! observable behaviour — the stub's only job is to type-check and
//! return zero, and tests that rely on deeper behaviour overwrite the
//! function pointer before invoking the code under test.

#![allow(non_snake_case)]

use crate::bindings::*;

pub(crate) unsafe extern "C" fn version_query(
    _: *mut VmbVersionInfo_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn startup(_: *const VmbFilePathChar_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn shutdown() {}
pub(crate) unsafe extern "C" fn cameras_list(
    _: *mut VmbCameraInfo_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn camera_info_query_by_handle(
    _: VmbHandle_t,
    _: *mut VmbCameraInfo_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn camera_info_query(
    _: *const ::std::os::raw::c_char,
    _: *mut VmbCameraInfo_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn camera_open(
    _: *const ::std::os::raw::c_char,
    _: VmbAccessMode_t,
    _: *mut VmbHandle_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn camera_close(_: VmbHandle_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn features_list(
    _: VmbHandle_t,
    _: *mut VmbFeatureInfo_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_info_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbFeatureInfo_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_list_selected(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbFeatureInfo_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_access_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbBool_t,
    _: *mut VmbBool_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_int_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbInt64_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_int_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: VmbInt64_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_int_range_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbInt64_t,
    _: *mut VmbInt64_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_int_increment_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbInt64_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_int_valid_value_set_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbInt64_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_float_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut f64,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_float_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: f64,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_float_range_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut f64,
    _: *mut f64,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_float_increment_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbBool_t,
    _: *mut f64,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut *const ::std::os::raw::c_char,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_range_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut *const ::std::os::raw::c_char,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_is_available(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbBool_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_as_int(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbInt64_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_as_string(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: VmbInt64_t,
    _: *mut *const ::std::os::raw::c_char,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_enum_entry_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbFeatureEnumEntry_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_string_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut ::std::os::raw::c_char,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_string_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_string_maxlength_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_bool_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbBool_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_bool_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: VmbBool_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_command_run(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_command_is_done(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbBool_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_raw_get(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut ::std::os::raw::c_char,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_raw_set(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *const ::std::os::raw::c_char,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_raw_length_query(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_invalidation_register(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: VmbInvalidationCallback,
    _: *mut ::std::os::raw::c_void,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn feature_invalidation_unregister(
    _: VmbHandle_t,
    _: *const ::std::os::raw::c_char,
    _: VmbInvalidationCallback,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn payload_size_get(
    _: VmbHandle_t,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn frame_announce(
    _: VmbHandle_t,
    _: *const VmbFrame_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn frame_revoke(_: VmbHandle_t, _: *const VmbFrame_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn frame_revoke_all(_: VmbHandle_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn capture_start(_: VmbHandle_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn capture_end(_: VmbHandle_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn capture_frame_queue(
    _: VmbHandle_t,
    _: *const VmbFrame_t,
    _: VmbFrameCallback,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn capture_frame_wait(
    _: VmbHandle_t,
    _: *const VmbFrame_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn capture_queue_flush(_: VmbHandle_t) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn transport_layers_list(
    _: *mut VmbTransportLayerInfo_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn interfaces_list(
    _: *mut VmbInterfaceInfo_t,
    _: VmbUint32_t,
    _: *mut VmbUint32_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn memory_read(
    _: VmbHandle_t,
    _: VmbUint64_t,
    _: VmbUint32_t,
    _: *mut ::std::os::raw::c_char,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn memory_write(
    _: VmbHandle_t,
    _: VmbUint64_t,
    _: VmbUint32_t,
    _: *const ::std::os::raw::c_char,
    _: *mut VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn settings_save(
    _: VmbHandle_t,
    _: *const VmbFilePathChar_t,
    _: *const VmbFeaturePersistSettings_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn settings_load(
    _: VmbHandle_t,
    _: *const VmbFilePathChar_t,
    _: *const VmbFeaturePersistSettings_t,
    _: VmbUint32_t,
) -> VmbError_t {
    0
}
pub(crate) unsafe extern "C" fn chunk_data_access(
    _: *const VmbFrame_t,
    _: VmbChunkAccessCallback,
    _: *mut ::std::os::raw::c_void,
) -> VmbError_t {
    0
}
