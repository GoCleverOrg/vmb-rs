//! Small FFI-adjacent helpers used only by the adapter.

/// Convert a possibly-null C string pointer to an owned `String`, falling
/// back to `fallback` when the pointer is null or the bytes are not valid
/// UTF-8.
///
/// # Safety
///
/// If `ptr` is non-null, it MUST point to a NUL-terminated C string
/// owned by someone else for the duration of the call. The caller is
/// responsible for any thread-safety invariants of the underlying memory.
pub(crate) fn cstr_to_owned(ptr: *const std::os::raw::c_char, fallback: &str) -> String {
    if ptr.is_null() {
        return fallback.to_string();
    }
    // SAFETY: caller guarantees `ptr` is a valid NUL-terminated C string.
    let cstr = unsafe { std::ffi::CStr::from_ptr(ptr) };
    cstr.to_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| fallback.to_string())
}
