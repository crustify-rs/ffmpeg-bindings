//! Safe wrappers over `libavutil/error.h`.
//!
//! [`describe`] writes the description of an `AVERROR` code into a caller
//! buffer.

use core::ffi::CStr;

use super::ffi;

/// Wraps: av_strerror
///
/// Writes a description of the `AVERROR` code `errnum` into `buf` and returns
/// it, NUL-terminated and truncated to fit:
///
/// - `Ok` with FFmpeg's own description, or the system's for an
///   `AVERROR(errno)` code;
/// - `Err` with the generic "Error number N occurred" when no description is
///   found or the system's does not fit `buf`.
///
/// An empty `buf` has no room for even the terminator: C writes nothing, and
/// the description is `c""`, with the status C reports for a zero-sized
/// buffer. `AV_ERROR_MAX_STRING_SIZE` (64) bytes hold every FFmpeg
/// description.
pub fn describe(errnum: i32, buf: &mut [u8]) -> Result<&CStr, &CStr> {
    // An empty slice's pointer is dangling, which C's `strerror_r` may not
    // receive even for a zero size; a live byte stands in for it, unwritten.
    let mut empty = 0u8;
    let dst = if buf.is_empty() {
        &raw mut empty
    } else {
        buf.as_mut_ptr()
    };
    // SAFETY: `dst` is valid for writes of `buf.len()` bytes, and C writes at
    // most that many there, NUL-terminating whatever it writes and keeping no
    // pointer to it.
    let ret = unsafe { ffi::av_strerror(errnum, dst.cast(), buf.len()) };
    // Every path in C terminates a non-empty buffer; a missing NUL would be a
    // C defect, read as an empty description rather than past the buffer.
    let text = CStr::from_bytes_until_nul(buf).unwrap_or(c"");
    if ret < 0 { Err(text) } else { Ok(text) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// `AVERROR(ENOMEM)`.
    const ENOMEM: i32 = -12;
    /// `AVERROR_EOF`: `-MKTAG('E','O','F',' ')`.
    const EOF: i32 = -(i32::from_le_bytes(*b"EOF "));

    #[test]
    fn describes_ffmpeg_and_system_codes() {
        let mut buf = [0xAAu8; 64];
        assert_eq!(describe(EOF, &mut buf), Ok(c"End of file"));
        assert_eq!(describe(ENOMEM, &mut buf), Ok(c"Cannot allocate memory"));
    }

    #[test]
    fn unknown_codes_get_the_generic_text() {
        let mut buf = [0u8; 64];
        assert_eq!(
            describe(-0x7777_7777, &mut buf),
            Err(c"Error number -2004318071 occurred")
        );
    }

    #[test]
    fn truncates_to_the_buffer() {
        let mut buf = [0xAAu8; 4];
        assert_eq!(describe(EOF, &mut buf), Ok(c"End"));
        let mut one = [0xAAu8; 1];
        assert_eq!(describe(EOF, &mut one), Ok(c""));
    }

    #[test]
    fn system_text_that_does_not_fit_is_the_generic_text() {
        let mut buf = [0xAAu8; 8];
        assert_eq!(describe(ENOMEM, &mut buf), Err(c"Error n"));
    }

    #[test]
    fn empty_buffer_keeps_the_status() {
        assert_eq!(describe(EOF, &mut []), Ok(c""));
        assert_eq!(describe(ENOMEM, &mut []), Err(c""));
        assert_eq!(describe(-0x7777_7777, &mut []), Err(c""));
    }
}
