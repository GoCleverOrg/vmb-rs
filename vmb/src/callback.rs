//! Frame callback trampoline.
//!
//! This is the most delicate piece of the safe wrapper — it lets idiomatic
//! Rust closures be invoked by the C SDK from its worker threads.
//!
//! ## Ownership model
//!
//! * Each announced frame owns a [`TrampolineContext`], which in turn owns
//!   both the `VmbFrame_t` struct (so the SDK always has a valid pointer to
//!   write into) and the backing `Box<[u8]>` pixel buffer.
//! * The `TrampolineContext` is heap-allocated via `Box<TrampolineContext>`
//!   so its address is stable for the lifetime of the capture session.
//! * A self-referencing pointer (`context[0]` inside the frame) lets the C
//!   trampoline recover the Rust context from a raw `*mut VmbFrame_t`.
//! * The user's closure is held in an `Arc<FrameCallback>` that every
//!   trampoline context shares. This way a single closure can service any
//!   number of announced frames.

use std::slice;
use std::sync::Arc;

use crate::frame::{Frame, PixelFormat};

/// User-provided frame callback, type-erased into a boxed trait object.
///
/// The closure is higher-rank over the frame lifetime: it accepts a
/// `&Frame<'a>` for any `'a`, which matches how the trampoline conjures up
/// a fresh lifetime on every invocation.
pub struct FrameCallback {
    inner: Box<dyn for<'a> Fn(&Frame<'a>) + Send + Sync + 'static>,
}

impl FrameCallback {
    /// Wrap a closure.
    pub fn new<F>(f: F) -> Self
    where
        F: for<'a> Fn(&Frame<'a>) + Send + Sync + 'static,
    {
        Self { inner: Box::new(f) }
    }

    pub(crate) fn invoke<'a>(&self, frame: &Frame<'a>) {
        (self.inner)(frame);
    }
}

/// Per-announced-frame trampoline context.
///
/// Each `VmbFrame_t` has a 4-slot `context` array; we store a pointer to
/// this struct in `context[0]`, and the trampoline uses it to retrieve the
/// [`FrameCallback`] on every frame completion.
///
/// This struct owns both the `VmbFrame_t` (so the SDK always has a valid
/// struct to write into) AND the backing pixel buffer. The backing buffer
/// is a `Box<[u8]>` rather than a `Vec<u8>` because boxed slices have a
/// stable heap address that does not move when this struct moves.
#[repr(C)]
pub(crate) struct TrampolineContext {
    /// The SDK-visible frame descriptor. The `buffer` pointer inside points
    /// into `self.buffer`; `context[0]` is patched to point back at `self`.
    pub(crate) frame: vmb_sys::VmbFrame_t,
    /// Heap storage backing `frame.buffer`. Kept alive for the lifetime of
    /// the context; the SDK writes into it via the raw pointer stored in
    /// `frame.buffer`, so Rust-side field reads are intentionally absent
    /// after construction.
    #[allow(dead_code)]
    buffer: Box<[u8]>,
    /// The user's callback, shared across all announced frames.
    callback: Arc<FrameCallback>,
}

impl TrampolineContext {
    /// Allocate a new context with a `payload_bytes`-sized backing buffer
    /// and the given shared callback.
    pub(crate) fn new(callback: Arc<FrameCallback>, payload_bytes: usize) -> Self {
        let mut buffer: Box<[u8]> = vec![0u8; payload_bytes].into_boxed_slice();
        // Use `as_mut_ptr` (not `as_ptr`) so we hand the SDK a genuine
        // writable pointer under Rust's aliasing model.
        let buffer_ptr = buffer.as_mut_ptr() as *mut std::os::raw::c_void;

        // SAFETY: A zeroed `VmbFrame_t` is the SDK's documented
        // zero-initialization convention — all its fields are either
        // integers or nullable pointers, so zero is a valid bit pattern.
        let mut frame: vmb_sys::VmbFrame_t = unsafe { std::mem::zeroed() };
        frame.buffer = buffer_ptr;
        frame.bufferSize = payload_bytes as u32;

        Self {
            frame,
            buffer,
            callback,
        }
    }

    /// Return a mutable pointer to the underlying `VmbFrame_t`, patching
    /// `context[0]` to point at the enclosing `TrampolineContext` first.
    ///
    /// This must be called **after** the `TrampolineContext` has been placed
    /// into its final `Box` (so `self` has a stable address). In practice
    /// the caller owns a `Box<TrampolineContext>` and the `Box`'s heap
    /// address is stable, so we can safely embed it in `context[0]`.
    pub(crate) fn vmb_frame_mut_ptr(&mut self) -> *mut vmb_sys::VmbFrame_t {
        self.frame.context[0] = self as *mut _ as *mut std::os::raw::c_void;
        &mut self.frame as *mut _
    }
}

/// C-ABI trampoline that Vimba invokes on every received frame.
///
/// Retrieves the `TrampolineContext` from `context[0]` and dispatches to
/// the user's closure.
///
/// # Safety
///
/// This function is called by the Vimba SDK's worker threads. It must:
/// - Never unwind into C (we catch panics at the boundary).
/// - Never take references that outlive the callback invocation.
/// - Assume `frame` is a valid pointer to a `VmbFrame_t` for the duration
///   of the call and that `context[0]` points at a live `TrampolineContext`
///   whose lifetime is owned by the enclosing `Camera`.
pub(crate) unsafe extern "C" fn frame_callback_trampoline(
    camera_handle: vmb_sys::VmbHandle_t,
    _stream_handle: vmb_sys::VmbHandle_t,
    frame_ptr: *mut vmb_sys::VmbFrame_t,
) {
    // Catch panics at the FFI boundary — unwinding into C is UB.
    let _ = std::panic::catch_unwind(|| {
        if frame_ptr.is_null() {
            return;
        }

        // SAFETY: the SDK promises `frame_ptr` is valid for the duration of
        // this call. We only hold the reference for the duration of this
        // closure.
        let frame = unsafe { &mut *frame_ptr };

        // Retrieve the trampoline context from context[0].
        let ctx_ptr = frame.context[0] as *const TrampolineContext;
        if ctx_ptr.is_null() {
            return;
        }
        // SAFETY: The `TrampolineContext` is owned by the enclosing
        // `Camera`, which keeps it alive until `stop_capture` / drop. Vimba
        // guarantees no frame callbacks run after `VmbCaptureEnd` returns,
        // so this reference is valid for the duration of the call.
        let ctx = unsafe { &*ctx_ptr };

        // Only dispatch completed frames. We still re-queue incomplete ones
        // below so the capture loop keeps turning.
        // `VmbFrameStatusComplete = 0`.
        if frame.receiveStatus == 0 {
            let data_ptr = frame.imageData as *const u8;
            let data_len = frame.bufferSize as usize;
            let data_slice: &[u8] = if data_ptr.is_null() || data_len == 0 {
                &[]
            } else {
                // SAFETY: the SDK filled `imageData` with at least
                // `bufferSize` valid bytes for this frame.
                unsafe { slice::from_raw_parts(data_ptr, data_len) }
            };

            // Wall-clock host timestamp for the frame, captured at the
            // moment the SDK delivered it. We deliberately do NOT use
            // `frame.timestamp` (the GenICam Timestamp register) because
            // that counter is camera-clock-relative — typically counts
            // from the camera's last power-on, which produces nonsensical
            // dates downstream (e.g. "1970_01_22" upload-key partitions).
            // The legacy `camera_app` C++ binary used the host clock for
            // the same reason.
            let host_ts_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);

            let rust_frame = Frame {
                data: data_slice,
                width: frame.width,
                height: frame.height,
                pixel_format: PixelFormat::from_raw(frame.pixelFormat),
                timestamp_ns: host_ts_ns,
                frame_id: frame.frameID,
            };

            ctx.callback.invoke(&rust_frame);
        }

        // Re-queue the frame so the SDK keeps delivering. If the capture
        // has just been torn down, Vimba will reject the queue call with a
        // non-success code; we intentionally ignore that — we cannot
        // propagate it out of a C-ABI callback anyway.
        // SAFETY: `camera_handle` and `frame_ptr` were both supplied by the
        // SDK as valid on entry; the trampoline function pointer is
        // ourselves and is safe to pass back.
        unsafe {
            let _ = vmb_sys::VmbCaptureFrameQueue(
                camera_handle,
                frame_ptr as *const _,
                Some(frame_callback_trampoline),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn frame_callback_dispatches_to_closure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let cb = FrameCallback::new(move |_frame| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Build a fake Frame (pure-Rust path — no C trampoline).
        let data = vec![0u8; 16];
        let frame = Frame {
            data: &data,
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Mono8,
            timestamp_ns: 0,
            frame_id: 0,
        };

        cb.invoke(&frame);
        cb.invoke(&frame);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn trampoline_context_allocates_buffer() {
        let cb = Arc::new(FrameCallback::new(|_| {}));
        let ctx = TrampolineContext::new(cb, 1024);
        assert_eq!(ctx.frame.bufferSize, 1024);
        assert!(!ctx.frame.buffer.is_null());
        assert_eq!(ctx.buffer.len(), 1024);
    }

    #[test]
    fn trampoline_context_patches_self_pointer() {
        let cb = Arc::new(FrameCallback::new(|_| {}));
        let mut ctx = Box::new(TrampolineContext::new(cb, 64));
        let expected = ctx.as_mut() as *mut TrampolineContext as *mut std::os::raw::c_void;
        let _ = ctx.vmb_frame_mut_ptr();
        assert_eq!(ctx.frame.context[0], expected);
    }

    /// Exercise the `unsafe extern "C" fn frame_callback_trampoline` path
    /// end-to-end: wire up a `TrampolineContext`, populate the underlying
    /// `VmbFrame_t` as if the SDK had written into it, invoke the C-ABI
    /// trampoline directly with null camera / stream handles, and assert
    /// that the user closure was dispatched with the expected metadata.
    ///
    /// On a host with the Vimba X SDK installed (our macOS CI + dev
    /// environment), the trampoline tail-calls `VmbCaptureFrameQueue` with a
    /// null handle; the SDK returns an error code which the trampoline
    /// discards, so the test remains deterministic.
    #[test]
    fn trampoline_dispatches_frame_via_extern_c_path() {
        // Captured metadata shared between the closure and the assertion.
        #[derive(Default, Clone)]
        struct Captured {
            width: u32,
            height: u32,
            pixel_format: Option<PixelFormat>,
            frame_id: u64,
            timestamp_ns: u64,
            data_len: usize,
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let captured = Arc::new(std::sync::Mutex::new(Captured::default()));
        let counter_cb = counter.clone();
        let captured_cb = captured.clone();
        let cb = Arc::new(FrameCallback::new(move |frame: &Frame<'_>| {
            counter_cb.fetch_add(1, Ordering::SeqCst);
            let mut g = captured_cb.lock().expect("captured mutex poisoned");
            g.width = frame.width;
            g.height = frame.height;
            g.pixel_format = Some(frame.pixel_format);
            g.frame_id = frame.frame_id;
            g.timestamp_ns = frame.timestamp_ns;
            g.data_len = frame.data.len();
        }));

        // Heap-allocated backing buffer — the SDK normally writes pixel
        // bytes into `frame.buffer`; we mirror the same pointer in
        // `frame.imageData` so the trampoline sees non-empty image data.
        let mut buffer: Box<[u8]> = vec![0u8; 16].into_boxed_slice();
        for (i, b) in buffer.iter_mut().enumerate() {
            *b = i as u8;
        }
        let buffer_ptr = buffer.as_mut_ptr();

        // Build the trampoline context in a Box so its heap address is
        // stable, then patch `context[0]` via `vmb_frame_mut_ptr()`.
        let mut ctx = Box::new(TrampolineContext::new(cb.clone(), 16));
        let frame_ptr = ctx.vmb_frame_mut_ptr();

        // SAFETY: We own the only live reference to the `TrampolineContext`
        // via `ctx`; `frame_ptr` points into it. We fill in the fields the
        // trampoline reads (imageData/width/height/pixelFormat/status/
        // frameID/timestamp) to simulate an SDK-completed frame.
        unsafe {
            (*frame_ptr).imageData = buffer_ptr;
            (*frame_ptr).bufferSize = 16;
            (*frame_ptr).width = 4;
            (*frame_ptr).height = 4;
            (*frame_ptr).pixelFormat = 0x0108_0001; // VmbPixelFormatMono8
            (*frame_ptr).receiveStatus = 0; // VmbFrameStatusComplete
            (*frame_ptr).frameID = 42;
            (*frame_ptr).timestamp = 1_234_567_890;
        }

        // Directly invoke the C-ABI trampoline. `VmbCaptureFrameQueue` will
        // be called at the end with a null handle and return an error which
        // the trampoline discards.
        unsafe {
            frame_callback_trampoline(std::ptr::null_mut(), std::ptr::null_mut(), frame_ptr);
        }

        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "trampoline must dispatch exactly once"
        );
        let g = captured.lock().expect("captured mutex poisoned");
        assert_eq!(g.width, 4);
        assert_eq!(g.height, 4);
        assert_eq!(g.pixel_format, Some(PixelFormat::Mono8));
        assert_eq!(g.frame_id, 42);
        // The trampoline must populate `timestamp_ns` with wall-clock
        // (host) nanoseconds since the Unix epoch — NOT the camera's
        // GenICam Timestamp register, which is camera-clock-relative
        // (typically counts from power-on). Downstream consumers
        // (upload-key date partition, clip event timestamps) require
        // wall-clock time for correct date math. We ensure this by
        // checking the captured value is clearly a wall-clock value
        // and clearly NOT the camera value we wrote into VmbFrame_t.
        assert_ne!(
            g.timestamp_ns, 1_234_567_890,
            "trampoline must NOT pass through the camera timestamp"
        );
        const JAN_1_2024_NS: u64 = 1_704_067_200_000_000_000;
        assert!(
            g.timestamp_ns >= JAN_1_2024_NS,
            "trampoline must populate wall-clock ns since epoch (>= 2024-01-01); got {}",
            g.timestamp_ns
        );
        assert_eq!(g.data_len, 16);

        // Keep `buffer` alive until after the trampoline call: dropping the
        // Box before the SAFETY comment above would dangle `imageData`.
        drop(buffer);
    }
}
