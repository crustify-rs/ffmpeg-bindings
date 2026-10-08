//! Safe wrappers over `libavutil/timestamp.h`.

use core::ffi::CStr;

use super::ffi;

/// Wraps: av_ts_make_string
///
/// Writes `ts` into `buf` as decimal text, or `NOPTS` for `AV_NOPTS_VALUE`
/// (`i64::MIN`), and returns that text, borrowed from `buf`.
///
/// ```
/// use ffmpeg::libavutil::{ffi, timestamp::make_string};
/// let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
/// assert_eq!(make_string(&mut buf, -42), c"-42");
/// assert_eq!(make_string(&mut buf, i64::MIN), c"NOPTS");
/// ```
pub fn make_string(buf: &mut [u8; ffi::AV_TS_MAX_STRING_SIZE as usize], ts: i64) -> &CStr {
    // SAFETY: `buf` is `AV_TS_MAX_STRING_SIZE` writable bytes, the bound
    // `av_ts_make_string` gives `snprintf`; the returned pointer is `buf`.
    unsafe { ffi::av_ts_make_string(buf.as_mut_ptr().cast(), ts) };
    // `snprintf` NUL-terminated its output within `buf`.
    CStr::from_bytes_until_nul(buf).unwrap_or_default()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn make_string_overwrites_a_dirty_buffer() {
        let mut buf = [b'x'; ffi::AV_TS_MAX_STRING_SIZE as usize];
        assert_eq!(make_string(&mut buf, 0), c"0");
        assert_eq!(&buf[..3], b"0\0x");
        assert_eq!(make_string(&mut buf, i64::MAX), c"9223372036854775807");
        assert_eq!(make_string(&mut buf, i64::MIN + 1), c"-9223372036854775807");
        assert_eq!(make_string(&mut buf, i64::MIN), c"NOPTS");
    }
}
