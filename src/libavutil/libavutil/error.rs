//! Safe wrappers over `libavutil/error.h`.
//!
//! [`describe`] writes the description of an `AVERROR` code into a caller
//! buffer, and [`make_error_string`] does the same without the status, as
//! `av_err2str` uses it.

use core::ffi::CStr;

use super::ffi;

/// Wraps: `av_strerror`
///
/// - `Ok` with FFmpeg's own description, or the system's for an `AVERROR(errno)` code;
/// - `Err` with the generic "Error number N occurred" when no description is found or the system's
///   does not fit `buf`.
///
/// An empty `buf` has no room for even the terminator: C writes nothing, and
/// the description is `c""`, with the status C reports for a zero-sized
/// buffer. `AV_ERROR_MAX_STRING_SIZE` (64) bytes hold every FFmpeg
/// description.
///
/// # Errors
///
/// Writes a description of the `AVERROR` code `errnum` into `buf` and returns it, NUL-terminated
/// and truncated to fit:
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

/// The buffer size `av_err2str` uses, which fits every description FFmpeg
/// defines itself (`AV_ERROR_MAX_STRING_SIZE`).
pub const MAX_STRING_SIZE: usize = ffi::AV_ERROR_MAX_STRING_SIZE as usize;

/// Wraps: `av_make_error_string`
///
/// Writes the description of the `AVERROR` code `errnum` into `buf`, and
/// returns it, as [`describe`] does without the status: FFmpeg's own text for its codes, the
/// system's for a negated `errno`, and `Error number <errnum> occurred` for anything else,
/// truncated to fit with its NUL. An empty `buf` yields an empty string.
/// [`MAX_STRING_SIZE`] bytes hold every FFmpeg description untruncated.
pub fn make_error_string(errnum: i32, buf: &mut [u8]) -> &CStr {
    if buf.is_empty() {
        return Default::default();
    }
    // SAFETY: `buf` is valid for writes of `buf.len()` bytes, which bounds
    // every write `av_strerror` makes and includes the NUL it always writes
    // into a non-empty buffer. The returned pointer is `buf`'s own.
    unsafe { ffi::av_make_error_string(buf.as_mut_ptr().cast(), buf.len(), errnum) };
    // C NUL-terminates a non-empty buffer on every path; an unterminated one
    // would be a broken libc, read as no description at all.
    CStr::from_bytes_until_nul(buf).unwrap_or_default()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// `AVERROR(ENOMEM)`.
    const ENOMEM: i32 = -12;
    /// `AVERROR_EOF`: `-MKTAG('E','O','F',' ')`.
    const EOF: i32 = -(i32::from_le_bytes(*b"EOF "));

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describes_ffmpeg_and_system_codes() {
        let mut buf = [0xAAu8; 64];
        assert_eq!(describe(EOF, &mut buf), Ok(c"End of file"));
        assert_eq!(describe(ENOMEM, &mut buf), Ok(c"Cannot allocate memory"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn unknown_codes_get_the_generic_text() {
        let mut buf = [0u8; 64];
        assert_eq!(
            describe(-0x7777_7777, &mut buf),
            Err(c"Error number -2004318071 occurred")
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn truncates_to_the_buffer() {
        let mut buf = [0xAAu8; 4];
        assert_eq!(describe(EOF, &mut buf), Ok(c"End"));
        let mut one = [0xAAu8; 1];
        assert_eq!(describe(EOF, &mut one), Ok(c""));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn system_text_that_does_not_fit_is_the_generic_text() {
        let mut buf = [0xAAu8; 8];
        assert_eq!(describe(ENOMEM, &mut buf), Err(c"Error n"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_buffer_keeps_the_status() {
        assert_eq!(describe(EOF, &mut []), Ok(c""));
        assert_eq!(describe(ENOMEM, &mut []), Err(c""));
        assert_eq!(describe(-0x7777_7777, &mut []), Err(c""));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_error_string_describes_ffmpeg_codes() {
        let mut buf = [0xAA; MAX_STRING_SIZE];
        assert_eq!(make_error_string(EOF, &mut buf), c"End of file");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_error_string_truncates_to_the_buffer() {
        let mut buf = [0xAA; 4];
        assert_eq!(make_error_string(EOF, &mut buf), c"End");
        let mut one = [0xAA; 1];
        assert_eq!(make_error_string(EOF, &mut one), c"");
    }

    #[test]
    fn make_error_string_into_an_empty_buffer_is_empty() {
        assert_eq!(make_error_string(EOF, &mut []), c"");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_error_string_reports_unknown_codes_by_number() {
        let mut buf = [0; MAX_STRING_SIZE];
        let s = make_error_string(i32::MIN, &mut buf).to_str().unwrap();
        assert_eq!(s, format!("Error number {} occurred", i32::MIN));
    }
}
