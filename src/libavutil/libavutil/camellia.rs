//! Safe wrappers over `libavutil/camellia.h`.

use super::ffi;

/// Wraps: av_camellia_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_camellia_size }
}
