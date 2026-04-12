//! Safe view over a Vimba X frame.
//!
//! A [`Frame`] is handed to user callbacks and borrows into the adapter's
//! internal buffer for the duration of the call. It is intentionally
//! **not** `'static` — the caller MUST either consume the data
//! synchronously (e.g. copy it out via [`Frame::to_vec`]) or forward it
//! through a bounded channel. Once the callback returns, the adapter
//! re-queues the buffer and the underlying bytes may be overwritten at
//! any moment.

/// Borrowed view of a single received frame.
///
/// The lifetime `'a` is tied to the adapter-owned buffer that fed this
/// invocation; callers must not escape `Frame` past the end of their
/// callback.
pub struct Frame<'a> {
    pub(crate) data: &'a [u8],
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Decoded pixel format.
    pub pixel_format: PixelFormat,
    /// Wall-clock timestamp captured by the adapter at frame arrival,
    /// expressed in nanoseconds since the Unix epoch.
    ///
    /// This is intentionally NOT the GenICam `Timestamp` register on
    /// `VmbFrame_t`, which is camera-clock-relative (counts from the
    /// camera's last power-on for most Allied Vision USB models) and
    /// therefore unsuitable for wall-clock-aware downstream consumers.
    pub timestamp_ns: u64,
    /// Monotonically increasing frame identifier assigned by the SDK.
    pub frame_id: u64,
}

impl<'a> Frame<'a> {
    /// Build a borrowed frame view. Intended to be called by a
    /// [`VmbRuntime`] adapter (either the real FFI adapter or an
    /// in-memory fake) after it has decoded the SDK's frame descriptor
    /// into plain Rust types.
    ///
    /// [`VmbRuntime`]: crate::types
    pub fn new(
        data: &'a [u8],
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        timestamp_ns: u64,
        frame_id: u64,
    ) -> Self {
        Self {
            data,
            width,
            height,
            pixel_format,
            timestamp_ns,
            frame_id,
        }
    }

    /// Raw byte view of the frame.
    pub fn data(&self) -> &[u8] {
        self.data
    }

    /// Copy the frame bytes into a new [`Vec<u8>`]. This is the typical
    /// callback path: copy out before returning so the adapter can
    /// re-queue.
    pub fn to_vec(&self) -> Vec<u8> {
        self.data.to_vec()
    }

    /// Length of the borrowed buffer in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the borrowed buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Minimal pixel format representation.
///
/// Vimba's pixel format space is enormous (PFNC codes); we explicitly map
/// only the formats our pipeline currently cares about and pass everything
/// else through as [`PixelFormat::Other`] so the consumer can interpret it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit mono, one byte per pixel (`VmbPixelFormatMono8` = 0x0108_0001).
    Mono8,
    /// 8-bit BGR, three bytes per pixel (`VmbPixelFormatBgr8` = 0x0218_0015).
    Bgr8,
    /// Anything else — the consumer must interpret the raw PFNC code.
    Other(u32),
}

impl PixelFormat {
    /// Map a raw `VmbPixelFormat_t` / PFNC code.
    pub fn from_raw(raw: u32) -> Self {
        // Values are in `vmb_sys::bindings::VmbPixelFormatType`:
        //   VmbPixelFormatMono8 = 17_301_505  (0x0108_0001)
        //   VmbPixelFormatBgr8  = 35_127_317  (0x0218_0015)
        match raw {
            0x0108_0001 => PixelFormat::Mono8,
            0x0218_0015 => PixelFormat::Bgr8,
            other => PixelFormat::Other(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_format_maps_known_codes() {
        assert_eq!(PixelFormat::from_raw(0x0108_0001), PixelFormat::Mono8);
        assert_eq!(PixelFormat::from_raw(0x0218_0015), PixelFormat::Bgr8);
    }

    #[test]
    fn pixel_format_preserves_unknown_codes() {
        assert_eq!(
            PixelFormat::from_raw(0xdead_beef),
            PixelFormat::Other(0xdead_beef)
        );
    }

    #[test]
    fn frame_basic_accessors() {
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let frame = Frame::new(&data, 2, 4, PixelFormat::Mono8, 123, 7);
        assert_eq!(frame.len(), 8);
        assert!(!frame.is_empty());
        assert_eq!(frame.to_vec(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(frame.data(), &data);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 4);
        assert_eq!(frame.pixel_format, PixelFormat::Mono8);
        assert_eq!(frame.timestamp_ns, 123);
        assert_eq!(frame.frame_id, 7);
    }

    #[test]
    fn frame_is_empty_when_no_bytes() {
        let frame = Frame::new(&[], 0, 0, PixelFormat::Mono8, 0, 0);
        assert_eq!(frame.len(), 0);
        assert!(frame.is_empty());
        assert_eq!(frame.to_vec(), Vec::<u8>::new());
    }
}
