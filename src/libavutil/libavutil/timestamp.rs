//! Safe wrappers over `libavutil/timestamp.h`.

use core::ffi::CStr;

use super::ffi;
use super::rational::{Rational, RationalRef};

/// Wraps: `av_ts_make_string`
///
/// Writes `ts` into `buf` as decimal text, or `NOPTS` for `AV_NOPTS_VALUE`
/// (`i64::MIN`), and returns that text, borrowed from `buf`.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::ffi;
/// use ffmpeg_bindings::libavutil::libavutil::timestamp::make_string;
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

/// Wraps: `av_ts_make_time_string2`
///
/// Writes the time `ts` ticks of `time_base` stand for, in seconds, into
/// `buf` and returns that text, borrowed from `buf`; `AV_NOPTS_VALUE`
/// (`i64::MIN`) is `NOPTS`.
///
/// The value `ts * num / den`, computed as a `double`, is printed with six
/// decimals, or with six significant digits when its magnitude is below 1,
/// and loses its trailing zeros and a bare trailing `.`. A zero denominator
/// gives `inf`, `-inf` or, for `0/0` and `0 * inf`, a NaN as `printf` spells
/// it: `-nan` on x86-64 (C's trimming cuts a positive `nan` to `n`).
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::ffi;
/// use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
/// use ffmpeg_bindings::libavutil::libavutil::timestamp::make_time_string;
/// let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
/// assert_eq!(
///     make_time_string(&mut buf, 135_000, Rational::new(1, 90_000)),
///     c"1.5"
/// );
/// assert_eq!(
///     make_time_string(&mut buf, 3, Rational::new(1, 1000)),
///     c"0.003"
/// );
/// assert_eq!(make_time_string(&mut buf, -1, Rational::new(1, 0)), c"-inf");
/// assert_eq!(
///     make_time_string(&mut buf, i64::MIN, Rational::new(1, 1)),
///     c"NOPTS"
/// );
/// ```
pub fn make_time_string(
    buf: &mut [u8; ffi::AV_TS_MAX_STRING_SIZE as usize],
    ts: i64,
    time_base: Rational,
) -> &CStr {
    // SAFETY: `buf` is `AV_TS_MAX_STRING_SIZE` writable bytes, the bound
    // `av_ts_make_time_string2` gives `snprintf` and its NUL store (at most
    // `buf[31]`); `time_base` is passed by value and any pair of `int`s is
    // accepted; the returned pointer is `buf`, which C does not retain.
    unsafe { ffi::av_ts_make_time_string2(buf.as_mut_ptr().cast(), ts, time_base.into_raw()) };
    // C NUL-terminated its output within `buf`.
    CStr::from_bytes_until_nul(buf).unwrap_or_default()
}

/// Wraps: `av_ts_make_time_string`
///
/// [`make_time_string`], with the time base read through a shared handle, as
/// C reads it through a `const AVRational *`: a time base stored in another
/// object (a field of a wrapped C struct) is passed in place, without first
/// copying it out. Writes the same text into `buf` and returns it, borrowed
/// from `buf`.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::ffi;
/// use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
/// use ffmpeg_bindings::libavutil::libavutil::timestamp::make_time_string_by_ref;
/// let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
/// let tb = Rational::new(1, 90_000);
/// assert_eq!(
///     make_time_string_by_ref(&mut buf, 135_000, tb.as_ref()),
///     c"1.5"
/// );
/// assert_eq!(
///     make_time_string_by_ref(&mut buf, i64::MIN, tb.as_ref()),
///     c"NOPTS"
/// );
/// ```
pub fn make_time_string_by_ref<'a>(
    buf: &'a mut [u8; ffi::AV_TS_MAX_STRING_SIZE as usize],
    ts: i64,
    tb: RationalRef<'_>,
) -> &'a CStr {
    // SAFETY: `buf` is `AV_TS_MAX_STRING_SIZE` writable bytes, the bound
    // `av_ts_make_time_string2` gives `snprintf`, and its trimming indexes
    // below that bound (`snprintf` writes at least one character); `tb`
    // points to a live `AVRational` C reads once, by value. The returned
    // pointer is `buf`.
    unsafe { ffi::av_ts_make_time_string(buf.as_mut_ptr().cast(), ts, tb.as_ptr()) };
    // C NUL-terminated its output within `buf`.
    CStr::from_bytes_until_nul(buf).unwrap_or_default()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_string_overwrites_a_dirty_buffer() {
        let mut buf = [b'x'; ffi::AV_TS_MAX_STRING_SIZE as usize];
        assert_eq!(make_string(&mut buf, 0), c"0");
        assert_eq!(&buf[..3], b"0\0x");
        assert_eq!(make_string(&mut buf, i64::MAX), c"9223372036854775807");
        assert_eq!(make_string(&mut buf, i64::MIN + 1), c"-9223372036854775807");
        assert_eq!(make_string(&mut buf, i64::MIN), c"NOPTS");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_time_string_overwrites_a_dirty_buffer() {
        let mut buf = [b'x'; ffi::AV_TS_MAX_STRING_SIZE as usize];
        let ms = Rational::new(1, 1000);
        assert_eq!(make_time_string(&mut buf, 1500, ms), c"1.5");
        // `snprintf` wrote "1.500000"; the trim stored a NUL after "1.5".
        assert_eq!(&buf[..9], b"1.5\x000000\0");
        assert_eq!(buf[9], b'x');
        assert_eq!(make_time_string(&mut buf, 0, ms), c"0");
        assert_eq!(make_time_string(&mut buf, i64::MIN, ms), c"NOPTS");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_time_string_trims_as_documented() {
        let mut buf = [0; ffi::AV_TS_MAX_STRING_SIZE as usize];
        let cases: [(i64, i32, i32, &CStr); 12] = [
            (2, 3, 1, c"6"),
            (-5, 1, 10, c"-0.5"),
            (1, 1, 3, c"0.333333"),
            (-2, 1, 3, c"-0.666667"),
            (1, 1, 10_000_000, c"0.0000001"),
            (1, 1, i32::MAX, c"0.000000000465661"),
            (123_456_789, 1, 1000, c"123456.789"),
            (i64::MAX, 1, 1, c"9223372036854775808"),
            (i64::MAX, i32::MAX, 1, c"19807040619342712361531211776"),
            (i64::MIN + 1, i32::MIN, 1, c"19807040628566084398385987584"),
            (1, 1, 0, c"inf"),
            (-1, 1, 0, c"-inf"),
        ];
        for (ts, num, den, text) in cases {
            assert_eq!(
                make_time_string(&mut buf, ts, Rational::new(num, den)),
                text,
                "{ts} {num}/{den}"
            );
        }
        for (ts, num) in [(0, 0), (1, 0), (0, 1)] {
            let nan = make_time_string(&mut buf, ts, Rational::new(num, 0)).to_bytes();
            assert!(nan.ends_with(b"nan") || nan == b"n", "{nan:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_time_string_by_ref_trims_seconds() {
        let mut buf = [b'x'; ffi::AV_TS_MAX_STRING_SIZE as usize];
        let ms = Rational::new(1, 1000);
        assert_eq!(make_time_string_by_ref(&mut buf, 1500, ms.as_ref()), c"1.5");
        assert_eq!(make_time_string_by_ref(&mut buf, 0, ms.as_ref()), c"0");
        assert_eq!(
            make_time_string_by_ref(&mut buf, -1, ms.as_ref()),
            c"-0.001"
        );
        assert_eq!(
            make_time_string_by_ref(&mut buf, 3, Rational::new(1, 3).as_ref()),
            c"1"
        );
        assert_eq!(
            make_time_string_by_ref(&mut buf, i64::MIN, ms.as_ref()),
            c"NOPTS"
        );
        assert_eq!(
            make_time_string_by_ref(&mut buf, 1, Rational::new(1, 0).as_ref()),
            c"inf"
        );
        assert_eq!(
            make_time_string_by_ref(&mut buf, -1, Rational::new(1, 0).as_ref()),
            c"-inf"
        );
    }
}
