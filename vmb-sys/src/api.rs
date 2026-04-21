//! Runtime-loaded handle to the Vimba X C API.
//!
//! [`VmbApi`] holds one function pointer per `VmbC.h` entry point, resolved
//! from `libVmbC` at startup via [`libloading`]. The library handle is kept
//! alive inside the struct so the pointers remain valid for the lifetime
//! of the handle.
//!
//! Production callers instantiate a [`VmbApi`] with [`VmbApi::load`]. Tests
//! that exercise adapter logic without the SDK installed build a baseline
//! handle via [`VmbApi::stub`] (no-op success for every function) and
//! overwrite individual `pub` fields with spy / mock functions.

#![allow(non_snake_case)]

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::bindings::*;
use crate::stubs;

/// Error returned when the Vimba X shared library cannot be loaded or a
/// required symbol cannot be resolved.
#[derive(Debug, Error)]
pub enum VmbLoadError {
    /// The dynamic library at `path` could not be opened.
    #[error("failed to open Vimba X shared library at {path}: {source}")]
    LibraryOpen {
        /// The path that was attempted.
        path: String,
        /// The underlying loader error.
        #[source]
        source: libloading::Error,
    },

    /// The library opened but a required symbol was missing.
    #[error("failed to resolve Vimba X symbol {symbol} in {path}: {source}")]
    SymbolResolution {
        /// The symbol name that could not be resolved.
        symbol: &'static str,
        /// The path of the library the lookup was performed against.
        path: String,
        /// The underlying loader error.
        #[source]
        source: libloading::Error,
    },
}

/// Runtime-loaded Vimba X C API handle.
///
/// Holds the open library plus one function pointer per VmbC entry point.
/// Function-pointer fields are `pub` so adapters can invoke them with the
/// usual `(api.VmbXxx)(args)` syntax. Test-only construction without a
/// real library is available via [`Self::stub`].
pub struct VmbApi {
    /// Keeps the loaded library alive so function pointers remain valid.
    /// `None` for test instances built from raw function pointers.
    _lib: Option<libloading::Library>,

    pub VmbVersionQuery: unsafe extern "C" fn(
        versionInfo: *mut VmbVersionInfo_t,
        sizeofVersionInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbStartup: unsafe extern "C" fn(pathConfiguration: *const VmbFilePathChar_t) -> VmbError_t,
    pub VmbShutdown: unsafe extern "C" fn(),
    pub VmbCamerasList: unsafe extern "C" fn(
        cameraInfo: *mut VmbCameraInfo_t,
        listLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
        sizeofCameraInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbCameraInfoQueryByHandle: unsafe extern "C" fn(
        cameraHandle: VmbHandle_t,
        info: *mut VmbCameraInfo_t,
        sizeofCameraInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbCameraInfoQuery: unsafe extern "C" fn(
        idString: *const ::std::os::raw::c_char,
        info: *mut VmbCameraInfo_t,
        sizeofCameraInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbCameraOpen: unsafe extern "C" fn(
        idString: *const ::std::os::raw::c_char,
        accessMode: VmbAccessMode_t,
        cameraHandle: *mut VmbHandle_t,
    ) -> VmbError_t,
    pub VmbCameraClose: unsafe extern "C" fn(cameraHandle: VmbHandle_t) -> VmbError_t,
    pub VmbFeaturesList: unsafe extern "C" fn(
        handle: VmbHandle_t,
        featureInfoList: *mut VmbFeatureInfo_t,
        listLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
        sizeofFeatureInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureInfoQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        featureInfo: *mut VmbFeatureInfo_t,
        sizeofFeatureInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureListSelected: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        featureInfoList: *mut VmbFeatureInfo_t,
        listLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
        sizeofFeatureInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureAccessQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        isReadable: *mut VmbBool_t,
        isWriteable: *mut VmbBool_t,
    ) -> VmbError_t,
    pub VmbFeatureIntGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *mut VmbInt64_t,
    ) -> VmbError_t,
    pub VmbFeatureIntSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: VmbInt64_t,
    ) -> VmbError_t,
    pub VmbFeatureIntRangeQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        min: *mut VmbInt64_t,
        max: *mut VmbInt64_t,
    ) -> VmbError_t,
    pub VmbFeatureIntIncrementQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *mut VmbInt64_t,
    ) -> VmbError_t,
    pub VmbFeatureIntValidValueSetQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        buffer: *mut VmbInt64_t,
        bufferSize: VmbUint32_t,
        setSize: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureFloatGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *mut f64,
    ) -> VmbError_t,
    pub VmbFeatureFloatSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: f64,
    ) -> VmbError_t,
    pub VmbFeatureFloatRangeQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        min: *mut f64,
        max: *mut f64,
    ) -> VmbError_t,
    pub VmbFeatureFloatIncrementQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        hasIncrement: *mut VmbBool_t,
        value: *mut f64,
    ) -> VmbError_t,
    pub VmbFeatureEnumGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *mut *const ::std::os::raw::c_char,
    ) -> VmbError_t,
    pub VmbFeatureEnumSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *const ::std::os::raw::c_char,
    ) -> VmbError_t,
    pub VmbFeatureEnumRangeQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        nameArray: *mut *const ::std::os::raw::c_char,
        arrayLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureEnumIsAvailable: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *const ::std::os::raw::c_char,
        isAvailable: *mut VmbBool_t,
    ) -> VmbError_t,
    pub VmbFeatureEnumAsInt: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *const ::std::os::raw::c_char,
        intVal: *mut VmbInt64_t,
    ) -> VmbError_t,
    pub VmbFeatureEnumAsString: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        intValue: VmbInt64_t,
        stringValue: *mut *const ::std::os::raw::c_char,
    ) -> VmbError_t,
    pub VmbFeatureEnumEntryGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        featureName: *const ::std::os::raw::c_char,
        entryName: *const ::std::os::raw::c_char,
        featureEnumEntry: *mut VmbFeatureEnumEntry_t,
        sizeofFeatureEnumEntry: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureStringGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        buffer: *mut ::std::os::raw::c_char,
        bufferSize: VmbUint32_t,
        sizeFilled: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureStringSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *const ::std::os::raw::c_char,
    ) -> VmbError_t,
    pub VmbFeatureStringMaxlengthQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        maxLength: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureBoolGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: *mut VmbBool_t,
    ) -> VmbError_t,
    pub VmbFeatureBoolSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        value: VmbBool_t,
    ) -> VmbError_t,
    pub VmbFeatureCommandRun: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
    ) -> VmbError_t,
    pub VmbFeatureCommandIsDone: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        isDone: *mut VmbBool_t,
    ) -> VmbError_t,
    pub VmbFeatureRawGet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        buffer: *mut ::std::os::raw::c_char,
        bufferSize: VmbUint32_t,
        sizeFilled: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureRawSet: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        buffer: *const ::std::os::raw::c_char,
        bufferSize: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureRawLengthQuery: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        length: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFeatureInvalidationRegister: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        callback: VmbInvalidationCallback,
        userContext: *mut ::std::os::raw::c_void,
    ) -> VmbError_t,
    pub VmbFeatureInvalidationUnregister: unsafe extern "C" fn(
        handle: VmbHandle_t,
        name: *const ::std::os::raw::c_char,
        callback: VmbInvalidationCallback,
    ) -> VmbError_t,
    pub VmbPayloadSizeGet:
        unsafe extern "C" fn(handle: VmbHandle_t, payloadSize: *mut VmbUint32_t) -> VmbError_t,
    pub VmbFrameAnnounce: unsafe extern "C" fn(
        handle: VmbHandle_t,
        frame: *const VmbFrame_t,
        sizeofFrame: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbFrameRevoke:
        unsafe extern "C" fn(handle: VmbHandle_t, frame: *const VmbFrame_t) -> VmbError_t,
    pub VmbFrameRevokeAll: unsafe extern "C" fn(handle: VmbHandle_t) -> VmbError_t,
    pub VmbCaptureStart: unsafe extern "C" fn(handle: VmbHandle_t) -> VmbError_t,
    pub VmbCaptureEnd: unsafe extern "C" fn(handle: VmbHandle_t) -> VmbError_t,
    pub VmbCaptureFrameQueue: unsafe extern "C" fn(
        handle: VmbHandle_t,
        frame: *const VmbFrame_t,
        callback: VmbFrameCallback,
    ) -> VmbError_t,
    pub VmbCaptureFrameWait: unsafe extern "C" fn(
        handle: VmbHandle_t,
        frame: *const VmbFrame_t,
        timeout: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbCaptureQueueFlush: unsafe extern "C" fn(handle: VmbHandle_t) -> VmbError_t,
    pub VmbTransportLayersList: unsafe extern "C" fn(
        transportLayerInfo: *mut VmbTransportLayerInfo_t,
        listLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
        sizeofTransportLayerInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbInterfacesList: unsafe extern "C" fn(
        interfaceInfo: *mut VmbInterfaceInfo_t,
        listLength: VmbUint32_t,
        numFound: *mut VmbUint32_t,
        sizeofInterfaceInfo: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbMemoryRead: unsafe extern "C" fn(
        handle: VmbHandle_t,
        address: VmbUint64_t,
        bufferSize: VmbUint32_t,
        dataBuffer: *mut ::std::os::raw::c_char,
        sizeComplete: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbMemoryWrite: unsafe extern "C" fn(
        handle: VmbHandle_t,
        address: VmbUint64_t,
        bufferSize: VmbUint32_t,
        dataBuffer: *const ::std::os::raw::c_char,
        sizeComplete: *mut VmbUint32_t,
    ) -> VmbError_t,
    pub VmbSettingsSave: unsafe extern "C" fn(
        handle: VmbHandle_t,
        filePath: *const VmbFilePathChar_t,
        settings: *const VmbFeaturePersistSettings_t,
        sizeofSettings: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbSettingsLoad: unsafe extern "C" fn(
        handle: VmbHandle_t,
        filePath: *const VmbFilePathChar_t,
        settings: *const VmbFeaturePersistSettings_t,
        sizeofSettings: VmbUint32_t,
    ) -> VmbError_t,
    pub VmbChunkDataAccess: unsafe extern "C" fn(
        frame: *const VmbFrame_t,
        chunkAccessCallback: VmbChunkAccessCallback,
        userContext: *mut ::std::os::raw::c_void,
    ) -> VmbError_t,
}

// SAFETY: `VmbApi` is a collection of raw function pointers plus a held
// library handle. The function pointers are `unsafe extern "C" fn(...)`
// values, which are trivially `Send + Sync`. `libloading::Library` is
// already `Send + Sync`. No interior mutability.
unsafe impl Send for VmbApi {}
unsafe impl Sync for VmbApi {}

impl VmbApi {
    /// Load `libVmbC` at runtime and resolve every required symbol.
    ///
    /// Search order:
    /// 1. `$VIMBA_X_HOME/api/lib/<arch>/libVmbC.<ext>`
    /// 2. `$VIMBA_X_HOME/api/lib/libVmbC.<ext>`
    /// 3. `$VIMBA_X_HOME/VmbC.framework/VmbC` (macOS framework layout)
    /// 4. The platform-default dynamic-linker search path
    ///    (`LD_LIBRARY_PATH`, `/etc/ld.so.conf`, `DYLD_LIBRARY_PATH`, …)
    ///    via the bare file name (`libVmbC.so`, `libVmbC.dylib`,
    ///    `VmbC.dll`).
    ///
    /// Returns [`VmbLoadError::LibraryOpen`] if every candidate path
    /// fails to open, and [`VmbLoadError::SymbolResolution`] if the
    /// library opens but a required symbol is missing.
    pub fn load() -> Result<Self, VmbLoadError> {
        let candidates = candidate_paths();
        let (path, lib) = open_first(&candidates)?;
        Self::from_library(path, lib)
    }

    /// Build a [`VmbApi`] that keeps `lib` alive and resolves each
    /// function pointer from it by symbol name.
    ///
    /// Exposed so integration tests can load a synthetic `.so` built
    /// from the test suite (or a real install) and reuse the same
    /// symbol-resolution logic as production.
    pub fn from_library(path: String, lib: libloading::Library) -> Result<Self, VmbLoadError> {
        macro_rules! sym {
            ($lib:expr, $path:expr, $name:ident, $ty:ty) => {{
                let symbol_name = concat!(stringify!($name), "\0");
                let symbol: libloading::Symbol<$ty> = unsafe { $lib.get(symbol_name.as_bytes()) }
                    .map_err(|source| {
                    VmbLoadError::SymbolResolution {
                        symbol: stringify!($name),
                        path: $path.clone(),
                        source,
                    }
                })?;
                *symbol
            }};
        }

        Ok(Self {
            VmbVersionQuery: sym!(lib, path, VmbVersionQuery, _),
            VmbStartup: sym!(lib, path, VmbStartup, _),
            VmbShutdown: sym!(lib, path, VmbShutdown, _),
            VmbCamerasList: sym!(lib, path, VmbCamerasList, _),
            VmbCameraInfoQueryByHandle: sym!(lib, path, VmbCameraInfoQueryByHandle, _),
            VmbCameraInfoQuery: sym!(lib, path, VmbCameraInfoQuery, _),
            VmbCameraOpen: sym!(lib, path, VmbCameraOpen, _),
            VmbCameraClose: sym!(lib, path, VmbCameraClose, _),
            VmbFeaturesList: sym!(lib, path, VmbFeaturesList, _),
            VmbFeatureInfoQuery: sym!(lib, path, VmbFeatureInfoQuery, _),
            VmbFeatureListSelected: sym!(lib, path, VmbFeatureListSelected, _),
            VmbFeatureAccessQuery: sym!(lib, path, VmbFeatureAccessQuery, _),
            VmbFeatureIntGet: sym!(lib, path, VmbFeatureIntGet, _),
            VmbFeatureIntSet: sym!(lib, path, VmbFeatureIntSet, _),
            VmbFeatureIntRangeQuery: sym!(lib, path, VmbFeatureIntRangeQuery, _),
            VmbFeatureIntIncrementQuery: sym!(lib, path, VmbFeatureIntIncrementQuery, _),
            VmbFeatureIntValidValueSetQuery: sym!(lib, path, VmbFeatureIntValidValueSetQuery, _),
            VmbFeatureFloatGet: sym!(lib, path, VmbFeatureFloatGet, _),
            VmbFeatureFloatSet: sym!(lib, path, VmbFeatureFloatSet, _),
            VmbFeatureFloatRangeQuery: sym!(lib, path, VmbFeatureFloatRangeQuery, _),
            VmbFeatureFloatIncrementQuery: sym!(lib, path, VmbFeatureFloatIncrementQuery, _),
            VmbFeatureEnumGet: sym!(lib, path, VmbFeatureEnumGet, _),
            VmbFeatureEnumSet: sym!(lib, path, VmbFeatureEnumSet, _),
            VmbFeatureEnumRangeQuery: sym!(lib, path, VmbFeatureEnumRangeQuery, _),
            VmbFeatureEnumIsAvailable: sym!(lib, path, VmbFeatureEnumIsAvailable, _),
            VmbFeatureEnumAsInt: sym!(lib, path, VmbFeatureEnumAsInt, _),
            VmbFeatureEnumAsString: sym!(lib, path, VmbFeatureEnumAsString, _),
            VmbFeatureEnumEntryGet: sym!(lib, path, VmbFeatureEnumEntryGet, _),
            VmbFeatureStringGet: sym!(lib, path, VmbFeatureStringGet, _),
            VmbFeatureStringSet: sym!(lib, path, VmbFeatureStringSet, _),
            VmbFeatureStringMaxlengthQuery: sym!(lib, path, VmbFeatureStringMaxlengthQuery, _),
            VmbFeatureBoolGet: sym!(lib, path, VmbFeatureBoolGet, _),
            VmbFeatureBoolSet: sym!(lib, path, VmbFeatureBoolSet, _),
            VmbFeatureCommandRun: sym!(lib, path, VmbFeatureCommandRun, _),
            VmbFeatureCommandIsDone: sym!(lib, path, VmbFeatureCommandIsDone, _),
            VmbFeatureRawGet: sym!(lib, path, VmbFeatureRawGet, _),
            VmbFeatureRawSet: sym!(lib, path, VmbFeatureRawSet, _),
            VmbFeatureRawLengthQuery: sym!(lib, path, VmbFeatureRawLengthQuery, _),
            VmbFeatureInvalidationRegister: sym!(lib, path, VmbFeatureInvalidationRegister, _),
            VmbFeatureInvalidationUnregister: sym!(lib, path, VmbFeatureInvalidationUnregister, _),
            VmbPayloadSizeGet: sym!(lib, path, VmbPayloadSizeGet, _),
            VmbFrameAnnounce: sym!(lib, path, VmbFrameAnnounce, _),
            VmbFrameRevoke: sym!(lib, path, VmbFrameRevoke, _),
            VmbFrameRevokeAll: sym!(lib, path, VmbFrameRevokeAll, _),
            VmbCaptureStart: sym!(lib, path, VmbCaptureStart, _),
            VmbCaptureEnd: sym!(lib, path, VmbCaptureEnd, _),
            VmbCaptureFrameQueue: sym!(lib, path, VmbCaptureFrameQueue, _),
            VmbCaptureFrameWait: sym!(lib, path, VmbCaptureFrameWait, _),
            VmbCaptureQueueFlush: sym!(lib, path, VmbCaptureQueueFlush, _),
            VmbTransportLayersList: sym!(lib, path, VmbTransportLayersList, _),
            VmbInterfacesList: sym!(lib, path, VmbInterfacesList, _),
            VmbMemoryRead: sym!(lib, path, VmbMemoryRead, _),
            VmbMemoryWrite: sym!(lib, path, VmbMemoryWrite, _),
            VmbSettingsSave: sym!(lib, path, VmbSettingsSave, _),
            VmbSettingsLoad: sym!(lib, path, VmbSettingsLoad, _),
            VmbChunkDataAccess: sym!(lib, path, VmbChunkDataAccess, _),
            _lib: Some(lib),
        })
    }

    /// Build a [`VmbApi`] whose every function pointer is a no-op that
    /// returns `VmbErrorSuccess` (code `0`). No shared library is
    /// loaded.
    ///
    /// Intended exclusively for test code that wants to start from a
    /// "trivially successful" baseline and then overwrite specific
    /// `pub` function-pointer fields with spies or custom behaviour
    /// (`api.VmbStartup = my_stub;`). Calling any function pointer
    /// that has not been overridden is safe but unlikely to be useful
    /// — the stubs return immediately without writing any output
    /// parameters, so callers that read output-only arguments will
    /// observe the pre-initialised contents (typically zero).
    pub fn stub() -> Self {
        Self {
            _lib: None,
            VmbVersionQuery: stubs::version_query,
            VmbStartup: stubs::startup,
            VmbShutdown: stubs::shutdown,
            VmbCamerasList: stubs::cameras_list,
            VmbCameraInfoQueryByHandle: stubs::camera_info_query_by_handle,
            VmbCameraInfoQuery: stubs::camera_info_query,
            VmbCameraOpen: stubs::camera_open,
            VmbCameraClose: stubs::camera_close,
            VmbFeaturesList: stubs::features_list,
            VmbFeatureInfoQuery: stubs::feature_info_query,
            VmbFeatureListSelected: stubs::feature_list_selected,
            VmbFeatureAccessQuery: stubs::feature_access_query,
            VmbFeatureIntGet: stubs::feature_int_get,
            VmbFeatureIntSet: stubs::feature_int_set,
            VmbFeatureIntRangeQuery: stubs::feature_int_range_query,
            VmbFeatureIntIncrementQuery: stubs::feature_int_increment_query,
            VmbFeatureIntValidValueSetQuery: stubs::feature_int_valid_value_set_query,
            VmbFeatureFloatGet: stubs::feature_float_get,
            VmbFeatureFloatSet: stubs::feature_float_set,
            VmbFeatureFloatRangeQuery: stubs::feature_float_range_query,
            VmbFeatureFloatIncrementQuery: stubs::feature_float_increment_query,
            VmbFeatureEnumGet: stubs::feature_enum_get,
            VmbFeatureEnumSet: stubs::feature_enum_set,
            VmbFeatureEnumRangeQuery: stubs::feature_enum_range_query,
            VmbFeatureEnumIsAvailable: stubs::feature_enum_is_available,
            VmbFeatureEnumAsInt: stubs::feature_enum_as_int,
            VmbFeatureEnumAsString: stubs::feature_enum_as_string,
            VmbFeatureEnumEntryGet: stubs::feature_enum_entry_get,
            VmbFeatureStringGet: stubs::feature_string_get,
            VmbFeatureStringSet: stubs::feature_string_set,
            VmbFeatureStringMaxlengthQuery: stubs::feature_string_maxlength_query,
            VmbFeatureBoolGet: stubs::feature_bool_get,
            VmbFeatureBoolSet: stubs::feature_bool_set,
            VmbFeatureCommandRun: stubs::feature_command_run,
            VmbFeatureCommandIsDone: stubs::feature_command_is_done,
            VmbFeatureRawGet: stubs::feature_raw_get,
            VmbFeatureRawSet: stubs::feature_raw_set,
            VmbFeatureRawLengthQuery: stubs::feature_raw_length_query,
            VmbFeatureInvalidationRegister: stubs::feature_invalidation_register,
            VmbFeatureInvalidationUnregister: stubs::feature_invalidation_unregister,
            VmbPayloadSizeGet: stubs::payload_size_get,
            VmbFrameAnnounce: stubs::frame_announce,
            VmbFrameRevoke: stubs::frame_revoke,
            VmbFrameRevokeAll: stubs::frame_revoke_all,
            VmbCaptureStart: stubs::capture_start,
            VmbCaptureEnd: stubs::capture_end,
            VmbCaptureFrameQueue: stubs::capture_frame_queue,
            VmbCaptureFrameWait: stubs::capture_frame_wait,
            VmbCaptureQueueFlush: stubs::capture_queue_flush,
            VmbTransportLayersList: stubs::transport_layers_list,
            VmbInterfacesList: stubs::interfaces_list,
            VmbMemoryRead: stubs::memory_read,
            VmbMemoryWrite: stubs::memory_write,
            VmbSettingsSave: stubs::settings_save,
            VmbSettingsLoad: stubs::settings_load,
            VmbChunkDataAccess: stubs::chunk_data_access,
        }
    }
}

/// Candidate library filename for the current platform.
#[cfg(target_os = "macos")]
const LIB_FILE_NAME: &str = "libVmbC.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
const LIB_FILE_NAME: &str = "libVmbC.so";
#[cfg(target_os = "windows")]
const LIB_FILE_NAME: &str = "VmbC.dll";

/// Architecture subdirectory name used by older Vimba X Linux installers.
pub(crate) fn arch_dir() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        ""
    }
}

/// Assemble the ordered list of library paths to try, given the current
/// environment and platform. The last entry is always the bare file name,
/// which triggers the platform's default linker search path.
pub(crate) fn candidate_paths() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(home) = env::var("VIMBA_X_HOME") {
        let home = PathBuf::from(home);
        let arch = arch_dir();
        if !arch.is_empty() {
            out.push(
                home.join("api")
                    .join("lib")
                    .join(arch)
                    .join(LIB_FILE_NAME)
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        out.push(
            home.join("api")
                .join("lib")
                .join(LIB_FILE_NAME)
                .to_string_lossy()
                .into_owned(),
        );
        // macOS framework layout: `$VIMBA_X_HOME/VmbC.framework/VmbC`.
        #[cfg(target_os = "macos")]
        out.push(
            home.join("VmbC.framework")
                .join("VmbC")
                .to_string_lossy()
                .into_owned(),
        );
    }
    // macOS default framework install location.
    #[cfg(target_os = "macos")]
    out.push("/Library/Frameworks/VmbC.framework/VmbC".to_string());
    // Fall back to the platform's default loader search path.
    out.push(LIB_FILE_NAME.to_string());
    out
}

/// Try each candidate path in order; return the first library that opens.
fn open_first(candidates: &[String]) -> Result<(String, libloading::Library), VmbLoadError> {
    let mut last_err: Option<(String, libloading::Error)> = None;
    for path in candidates {
        match unsafe { libloading::Library::new(as_os(path)) } {
            Ok(lib) => return Ok((path.clone(), lib)),
            Err(e) => last_err = Some((path.clone(), e)),
        }
    }
    let (path, source) = last_err.expect("candidate_paths() never returns an empty Vec");
    Err(VmbLoadError::LibraryOpen { path, source })
}

fn as_os(s: &str) -> &OsStr {
    Path::new(s).as_os_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Setting `VIMBA_X_HOME` to an empty directory yields a candidate
    /// list that starts with the probed SDK paths and ends with the bare
    /// file name — no path entries are silently dropped.
    #[test]
    fn candidate_paths_honours_vimba_x_home() {
        let tmp = std::env::temp_dir().join("vmb-sys-candidate-paths");
        // This test mutates VIMBA_X_HOME. cargo test runs tests in
        // parallel threads in the same process, so wrap the block in
        // a global mutex to avoid races with other tests that read or
        // write the variable.
        let _g = test_env_guard();
        // SAFETY: single-threaded region (test_env_guard holds a lock).
        unsafe {
            env::set_var("VIMBA_X_HOME", &tmp);
        }
        let paths = candidate_paths();
        // SAFETY: see above.
        unsafe {
            env::remove_var("VIMBA_X_HOME");
        }

        assert!(
            paths.iter().any(|p| p.contains("vmb-sys-candidate-paths")),
            "expected VIMBA_X_HOME-derived candidate in {paths:?}"
        );
        assert_eq!(
            paths.last().map(String::as_str),
            Some(LIB_FILE_NAME),
            "bare file name must be the final fallback"
        );
    }

    /// When the arch is detectable, `candidate_paths` emits a path that
    /// includes the arch subdirectory. This guards the `!arch.is_empty()`
    /// branch that only adds the arch-specific path for known arches.
    #[test]
    fn candidate_paths_includes_arch_subdir_for_known_arches() {
        let arch = arch_dir();
        if arch.is_empty() {
            // Nothing to assert on a target whose arch we don't
            // enumerate — the test is trivially satisfied.
            return;
        }

        let tmp = std::env::temp_dir().join("vmb-sys-arch-probe");
        let _g = test_env_guard();
        // SAFETY: single-threaded region.
        unsafe {
            env::set_var("VIMBA_X_HOME", &tmp);
        }
        let paths = candidate_paths();
        // SAFETY: same.
        unsafe {
            env::remove_var("VIMBA_X_HOME");
        }

        let arch_separator = std::path::MAIN_SEPARATOR.to_string();
        let arch_fragment = format!("{arch_separator}{arch}{arch_separator}");
        assert!(
            paths.iter().any(|p| p.contains(&arch_fragment)),
            "expected {arch_fragment:?} in one of {paths:?}"
        );
    }

    /// `arch_dir()` must return one of the known Vimba X SDK
    /// architecture names (or empty on unsupported targets). This pins
    /// down the mapping so that "replace with empty string" and
    /// "replace with xyzzy" mutants on the known-arch branches are
    /// both caught.
    #[test]
    fn arch_dir_returns_known_value_for_current_target() {
        let arch = arch_dir();
        if cfg!(target_arch = "x86_64") {
            assert_eq!(arch, "x86_64");
        } else if cfg!(target_arch = "aarch64") {
            assert_eq!(arch, "arm64");
        } else if cfg!(target_arch = "arm") {
            assert_eq!(arch, "arm");
        } else {
            assert_eq!(arch, "");
        }
    }

    /// With no `VIMBA_X_HOME` the candidate list still contains the
    /// bare file name (so the system loader search path is consulted).
    #[test]
    fn candidate_paths_without_home_still_has_bare_fallback() {
        let _g = test_env_guard();
        // SAFETY: single-threaded region.
        unsafe {
            env::remove_var("VIMBA_X_HOME");
        }
        let paths = candidate_paths();
        assert_eq!(paths.last().map(String::as_str), Some(LIB_FILE_NAME));
    }

    /// `VmbApi::load()` returns a `LibraryOpen` error on hosts where the
    /// SDK is not installed and `VIMBA_X_HOME` is pointed at an empty
    /// directory. This is the primary runtime-failure contract the
    /// refactor introduces.
    #[test]
    fn load_returns_library_open_when_sdk_is_missing() {
        let tmp = std::env::temp_dir().join("vmb-sys-no-sdk-here");
        std::fs::create_dir_all(&tmp).ok();

        let _g = test_env_guard();
        // SAFETY: single-threaded region.
        unsafe {
            env::set_var("VIMBA_X_HOME", &tmp);
        }
        let res = VmbApi::load();
        // SAFETY: same.
        unsafe {
            env::remove_var("VIMBA_X_HOME");
        }

        // On a host where the SDK is genuinely present on the loader
        // path this test may legitimately succeed. We accept either
        // outcome — the important negative case is that the call does
        // not panic and does not produce a linker error.
        match res {
            Err(VmbLoadError::LibraryOpen { path, .. }) => {
                assert!(
                    !path.is_empty(),
                    "LibraryOpen must report which path failed"
                );
            }
            Err(VmbLoadError::SymbolResolution { .. }) => {
                // Extremely unlikely (loader found the lib but lacked a
                // symbol); accept it as non-panicking behaviour.
            }
            Ok(_api) => {
                // Running on a host with a real Vimba X install.
            }
        }
    }

    /// Display of `LibraryOpen` must mention both the path and the
    /// underlying source — this is what makes the error actionable.
    #[test]
    fn library_open_display_includes_path() {
        let err = VmbLoadError::LibraryOpen {
            path: "/does/not/exist/libVmbC.so".to_string(),
            source: fake_libloading_error(),
        };
        let s = format!("{err}");
        assert!(
            s.contains("/does/not/exist/libVmbC.so"),
            "missing path in {s}"
        );
    }

    /// Display of `SymbolResolution` must mention the symbol and path.
    #[test]
    fn symbol_resolution_display_includes_symbol_and_path() {
        let err = VmbLoadError::SymbolResolution {
            symbol: "VmbStartup",
            path: "/opt/VimbaX/libVmbC.so".to_string(),
            source: fake_libloading_error(),
        };
        let s = format!("{err}");
        assert!(s.contains("VmbStartup"));
        assert!(s.contains("/opt/VimbaX/libVmbC.so"));
    }

    /// `open_first` returns the last tried path in its error, so
    /// operators can see what was attempted.
    #[test]
    fn open_first_reports_last_attempted_path() {
        let candidates = vec![
            "/definitely/not/here/libVmbC.so".to_string(),
            "/also/not/here/libVmbC.so".to_string(),
        ];
        let err = open_first(&candidates).expect_err("must fail on bogus paths");
        let VmbLoadError::LibraryOpen { path, .. } = err else {
            panic!("expected LibraryOpen");
        };
        assert_eq!(path, "/also/not/here/libVmbC.so");
    }

    /// Trigger a real `libloading::Error` value. The simplest way is to
    /// attempt to open a path we know does not exist; the returned
    /// error has variant-agnostic `Display`.
    fn fake_libloading_error() -> libloading::Error {
        let bogus = Path::new("/__vmb_sys_nonexistent_lib__");
        unsafe { libloading::Library::new(bogus) }.expect_err("must fail for nonexistent path")
    }

    use std::sync::Mutex;

    /// Serialise tests that touch `VIMBA_X_HOME`. Tests within this
    /// module run on the Rust test harness' thread pool; a global lock
    /// prevents one test's env-var write from leaking into another's
    /// assertion.
    fn test_env_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
}
