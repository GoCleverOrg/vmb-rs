//! User frame-callback wrapper.
//!
//! [`FrameCallback`] type-erases a user closure into a boxed trait object
//! that accepts a [`Frame`] for any lifetime. It is delivered to the user
//! on whatever thread the adapter fires the callback on, so the closure
//! must be `Send + Sync + 'static`.
//!
//! The adapter-side trampolines (`extern "C" fn` in `vmb-ffi`; synchronous
//! dispatch in `vmb-fake`) hold references to `FrameCallback` values via a
//! registry keyed by [`crate::FrameCallbackId`] and invoke them via
//! [`FrameCallback::invoke`].

use crate::frame::Frame;

/// User-provided frame callback, type-erased into a boxed trait object.
///
/// The closure is higher-rank over the frame lifetime: it accepts a
/// `&Frame<'a>` for any `'a`, which matches how adapters conjure up a
/// fresh lifetime on every invocation.
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

    /// Invoke the closure with the given frame. Called by adapters.
    pub fn invoke(&self, frame: &Frame<'_>) {
        (self.inner)(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::PixelFormat;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn frame_callback_dispatches_to_closure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let cb = FrameCallback::new(move |_frame| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let data = vec![0u8; 16];
        let frame = Frame::new(&data, 4, 4, PixelFormat::Mono8, 0, 0);

        cb.invoke(&frame);
        cb.invoke(&frame);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn frame_callback_receives_frame_metadata() {
        let seen = Arc::new(std::sync::Mutex::new(None));
        let seen_clone = seen.clone();
        let cb = FrameCallback::new(move |frame: &Frame<'_>| {
            *seen_clone.lock().unwrap() = Some((
                frame.width,
                frame.height,
                frame.pixel_format,
                frame.frame_id,
                frame.timestamp_ns,
                frame.data().to_vec(),
            ));
        });

        let data = vec![9u8, 8, 7];
        cb.invoke(&Frame::new(&data, 3, 1, PixelFormat::Bgr8, 42, 11));

        let got = seen.lock().unwrap().clone().expect("callback not called");
        assert_eq!(got.0, 3);
        assert_eq!(got.1, 1);
        assert_eq!(got.2, PixelFormat::Bgr8);
        assert_eq!(got.3, 11);
        assert_eq!(got.4, 42);
        assert_eq!(got.5, vec![9, 8, 7]);
    }
}
