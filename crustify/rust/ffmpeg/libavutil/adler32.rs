//! Safe wrappers over `libavutil/adler32.h`.

use super::ffi;

/// Wraps: av_adler32_update
///
/// Update an Adler-32 checksum. Start a new stream with `initial = 1`.
pub fn update(initial: u32, input: &[u8]) -> u32 {
    // SAFETY: C reads only `input.len()` bytes and retains no pointer.
    unsafe { ffi::av_adler32_update(initial, input.as_ptr(), input.len()) }
}
