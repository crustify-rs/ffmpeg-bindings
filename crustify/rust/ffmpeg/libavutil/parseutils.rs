//! Safe wrappers over `libavutil/parseutils.h`.
//!
//! The parsers read a NUL-terminated string and return what C stores in its
//! out-parameters, or C's `AVERROR` code: [`parse_color`] (and its prefix and
//! logging-context forms) reads a color (`av_parse_color`),
//! [`parse_video_size`] a frame size (`av_parse_video_size`), and
//! [`parse_date`] and [`parse_duration`] the two modes of `av_parse_time`.
//! None of them keeps a borrow of its input.

use core::ffi::{CStr, c_int, c_void};
use core::ptr;

use super::ffi;

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL as c_int);

/// The bytes C's `av_parse_color` skips as a hex marker before copying the
/// color: 1 for a leading `#`, 2 for a leading `0x` (lowercase only).
fn hex_offset(color: &[u8]) -> usize {
    match color {
        [b'#', ..] => 1,
        [b'0', b'x', ..] => 2,
        _ => 0,
    }
}

/// A `len` at or above which every `slen` is equivalent: C copies
/// `min(slen - hex_offset + 1, 128)` bytes, NUL included, so any `slen` of at
/// least `127 + hex_offset` copies the same 127.
const COLOR_SLEN_CAP: usize = 256;

/// The `slen` C's `av_parse_color` gets for the first `len` bytes of
/// `color`, or `None` when the prefix ends inside the hex marker.
///
/// C does not handle such a prefix: with `slen == hex_offset - 1` it copies
/// nothing and parses an uninitialised stack buffer (UB), and with a smaller
/// `slen` its size computation wraps to 128 and it parses past the prefix.
/// A large `len` is capped at [`COLOR_SLEN_CAP`], which also keeps C's
/// `slen - hex_offset + 1` from overflowing at `INT_MAX`.
fn color_slen(color: &[u8], len: usize) -> Option<c_int> {
    if len < hex_offset(color) {
        return None;
    }
    Some(len.min(COLOR_SLEN_CAP) as c_int)
}

/// The alpha specifier C's `av_parse_color` reads for `slen` (at least
/// `hex_offset(color)`): the bytes after the first `@` among the up to 127
/// it copies after the hex marker, or `None` without an `@`.
fn alpha_suffix(color: &[u8], slen: c_int) -> Option<&[u8]> {
    let start = hex_offset(color);
    let copied = (slen as usize - start).min(127);
    let color = &color[start..color.len().min(start + copied)];
    let at = color.iter().position(|&b| b == b'@')?;
    Some(&color[at + 1..])
}

/// Whether glibc's `strtod` reads all of `s` as a NaN: after C-locale
/// whitespace and an optional sign, `nan` in any case, optionally followed
/// by a parenthesised run of ASCII letters, digits and `_`.
fn is_nan_spelling(s: &[u8]) -> bool {
    let s = match s.iter().position(|&b| !matches!(b, b' ' | b'\t'..=b'\r')) {
        Some(i) => &s[i..],
        None => return false,
    };
    let s = s
        .strip_prefix(b"+")
        .or_else(|| s.strip_prefix(b"-"))
        .unwrap_or(s);
    let Some((nan, rest)) = s.split_at_checked(3) else {
        return false;
    };
    if !nan.eq_ignore_ascii_case(b"nan") {
        return false;
    }
    match rest {
        [] => true,
        [b'(', inner @ .., b')'] => inner
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_'),
        _ => false,
    }
}

/// The `slen` for `av_parse_color` of the first `len` bytes of `color`, or
/// `AVERROR(EINVAL)` for what C does not handle: a prefix inside the hex
/// marker (see [`color_slen`]), and an alpha specifier `strtod` reads as
/// NaN, which C range-checks with comparisons NaN fails and then converts to
/// a byte (UB).
fn checked_slen(color: &CStr, len: usize) -> Result<c_int, i32> {
    let bytes = color.to_bytes();
    let slen = color_slen(bytes, len).ok_or(AVERROR_EINVAL)?;
    if alpha_suffix(bytes, slen).is_some_and(is_nan_spelling) {
        return Err(AVERROR_EINVAL);
    }
    Ok(slen)
}

/// Wraps: av_parse_color
///
/// The red, green, blue and alpha bytes `color` names, in that order.
///
/// `color` is a color name from FFmpeg's table (matched regardless of ASCII
/// case, enumerated by `av_get_known_color_name`), or `RRGGBB` or `RRGGBBAA`
/// in hexadecimal, optionally after `0x` or `#`; `random` or `bikeshed` pick
/// a random color, alpha included. The alpha byte defaults to 255, and a
/// trailing `@` followed by `0x` and a hexadecimal byte, or by a decimal
/// opacity in `[0.0, 1.0]` (scaled by 255 and truncated), replaces it. Only
/// the first 127 bytes after the hex marker are read.
///
/// Fails with `AVERROR(EINVAL)` otherwise, after logging why at
/// `AV_LOG_ERROR` without a context. An opacity `strtod` reads as NaN
/// (`red@nan`), which C would convert to a byte (UB), fails with
/// `AVERROR(EINVAL)` before C runs, without logging.
pub fn parse_color(color: &CStr) -> Result<[u8; 4], i32> {
    parse_color_prefix(color, usize::MAX)
}

/// Wraps: av_parse_color
///
/// As [`parse_color`], reading the color from the first `len` bytes of
/// `color` only (the whole string when it is shorter); an invalid alpha
/// specifier is still logged with the whole string.
///
/// A prefix that ends inside the `#` or `0x` marker fails with
/// `AVERROR(EINVAL)` before C runs, without logging: C copies no color for
/// such a prefix and reads an uninitialised buffer, or reads past the prefix.
pub fn parse_color_prefix(color: &CStr, len: usize) -> Result<[u8; 4], i32> {
    let slen = checked_slen(color, len)?;
    let mut rgba = [0u8; 4];
    let log_ctx = ptr::null_mut();
    // SAFETY: `rgba` is a writable 4-byte array and `color` a live
    // NUL-terminated string C only reads (whole, for the log message);
    // `checked_slen` keeps `slen` within `hex_offset..=COLOR_SLEN_CAP`, so C
    // copies at least its NUL and `slen - hex_offset + 1` cannot overflow,
    // and the alpha C converts is not NaN; a NULL log context is "no
    // context".
    let ret = unsafe { ffi::av_parse_color(rgba.as_mut_ptr(), color.as_ptr(), slen, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(rgba) }
}

/// Wraps: av_parse_color
///
/// As [`parse_color`], or with `Some(len)` as [`parse_color_prefix`], logging
/// a failure with `log_ctx` as the context.
///
/// `log_ctx` stays a raw seam while logging contexts have no safe handle:
/// pass NULL for no context.
///
/// # Safety
///
/// `log_ctx` must be NULL or point to a live struct whose first member is a
/// `const AVClass *` (NULL, or a valid class) that `av_log` and the installed
/// log callback may read during the call.
pub unsafe fn parse_color_with_context(
    color: &CStr,
    len: Option<usize>,
    log_ctx: *mut c_void,
) -> Result<[u8; 4], i32> {
    let slen = checked_slen(color, len.unwrap_or(usize::MAX))?;
    let mut rgba = [0u8; 4];
    // SAFETY: as for `parse_color_prefix`, and the caller vouches for
    // `log_ctx`.
    let ret = unsafe { ffi::av_parse_color(rgba.as_mut_ptr(), color.as_ptr(), slen, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(rgba) }
}

/// Wraps: av_parse_video_size
///
/// The width and height, both positive, that `size` gives: a named size
/// (`vga`, `hd1080`, `4k`, ...; case-sensitive) or `<width>x<height>`, two
/// decimal integers as `strtol` reads them (leading whitespace and a sign
/// allowed) separated by any one byte. C narrows each integer to `int`
/// modulo 2^32, so `4294967297x1` is `(1, 1)`.
///
/// Fails with `AVERROR(EINVAL)` on trailing bytes or a dimension that is not
/// positive.
pub fn parse_video_size(size: &CStr) -> Result<(i32, i32), i32> {
    let (mut width, mut height) = (0, 0);
    // SAFETY: `width` and `height` are writable locals, and `size` a live
    // NUL-terminated string that C only reads, up to its NUL.
    let ret = unsafe { ffi::av_parse_video_size(&mut width, &mut height, size.as_ptr()) };
    if ret < 0 {
        Err(ret)
    } else {
        Ok((width, height))
    }
}

/// `av_parse_time` of `timestr` in the given mode.
fn parse_time(timestr: &CStr, duration: bool) -> Result<i64, i32> {
    let mut timeval = 0;
    // SAFETY: `timeval` is a writable local and `timestr` a live
    // NUL-terminated string that C only reads, up to its NUL.
    let ret = unsafe { ffi::av_parse_time(&mut timeval, timestr.as_ptr(), c_int::from(duration)) };
    if ret < 0 { Err(ret) } else { Ok(timeval) }
}

/// Wraps: av_parse_time
///
/// The microseconds since 1970-01-01 00:00:00 UTC of the date `date`:
///
/// ```text
/// [{YYYY-MM-DD|YYYYMMDD}[T|t| ]]{HH:MM:SS|HHMMSS}[.m...][Z|z|{+|-}HH[[:]MM]]
/// now
/// ```
///
/// The time is local unless `Z` or a UTC offset follows it (an offset needs
/// the date part); a missing date is today's, and `now` (any case) is the
/// current time. Up to six fractional digits count.
///
/// Fails with `AVERROR(EINVAL)` on a malformed date, and `AVERROR(ERANGE)`
/// when the microseconds overflow an `i64`.
pub fn parse_date(date: &CStr) -> Result<i64, i32> {
    parse_time(date, false)
}

/// Wraps: av_parse_time
///
/// The microseconds the duration `duration` spans:
///
/// ```text
/// [-][HH:]MM:SS[.m...][s|ms|us]
/// [-]S+[.m...][s|ms|us]
/// ```
///
/// with up to 9999 hours, minutes and seconds below 60, or any `i64` count
/// of seconds (or of the unit its suffix names); up to six fractional digits
/// count (three after `ms`, none after `us`).
///
/// Fails with `AVERROR(EINVAL)` on a malformed duration, and
/// `AVERROR(ERANGE)` when it overflows an `i64` of microseconds.
pub fn parse_duration(duration: &CStr) -> Result<i64, i32> {
    parse_time(duration, true)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn hex_offset_matches_only_lowercase_markers() {
        assert_eq!(hex_offset(b""), 0);
        assert_eq!(hex_offset(b"#"), 1);
        assert_eq!(hex_offset(b"#ff0000"), 1);
        assert_eq!(hex_offset(b"0"), 0);
        assert_eq!(hex_offset(b"0x"), 2);
        assert_eq!(hex_offset(b"0xff0000"), 2);
        assert_eq!(hex_offset(b"0Xff0000"), 0);
        assert_eq!(hex_offset(b"x0"), 0);
        assert_eq!(hex_offset(b"red"), 0);
    }

    #[test]
    fn color_slen_refuses_prefixes_inside_the_marker_and_caps_the_rest() {
        assert_eq!(color_slen(b"#ff0000", 0), None);
        assert_eq!(color_slen(b"#ff0000", 1), Some(1));
        assert_eq!(color_slen(b"0xff0000", 0), None);
        assert_eq!(color_slen(b"0xff0000", 1), None);
        assert_eq!(color_slen(b"0xff0000", 2), Some(2));
        assert_eq!(color_slen(b"red", 0), Some(0));
        assert_eq!(color_slen(b"red", 3), Some(3));
        assert_eq!(color_slen(b"red", 255), Some(255));
        assert_eq!(color_slen(b"red", 256), Some(256));
        assert_eq!(color_slen(b"red", 257), Some(256));
        assert_eq!(color_slen(b"red", usize::MAX), Some(256));
        assert_eq!(color_slen(b"", usize::MAX), Some(256));
    }

    #[test]
    fn the_cap_is_past_every_copy_c_makes() {
        // C copies `min(slen - hex_offset + 1, 128)` bytes: at the cap the
        // first operand is above 128 for every marker.
        for offset in 0..=2 {
            assert!(COLOR_SLEN_CAP - offset + 1 > 128);
        }
        const { assert!(COLOR_SLEN_CAP < c_int::MAX as usize) };
    }

    #[test]
    fn alpha_suffix_follows_the_first_at_within_the_copied_bytes() {
        assert_eq!(alpha_suffix(b"red", 256), None);
        assert_eq!(alpha_suffix(b"red@0.5", 256), Some(&b"0.5"[..]));
        assert_eq!(alpha_suffix(b"red@0.5@1", 256), Some(&b"0.5@1"[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 256), Some(&b"nan"[..]));
        assert_eq!(alpha_suffix(b"0xff0000@", 256), Some(&b""[..]));
        // The prefix ends before the `@`, or within the specifier.
        assert_eq!(alpha_suffix(b"red@nan", 3), None);
        assert_eq!(alpha_suffix(b"red@nan", 4), Some(&b""[..]));
        assert_eq!(alpha_suffix(b"red@nan", 6), Some(&b"na"[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 8), Some(&b""[..]));
        assert_eq!(alpha_suffix(b"#ff0000@nan", 11), Some(&b"nan"[..]));
        // C copies at most 127 bytes after the marker.
        let mut long = vec![b'a'; 125];
        long.extend_from_slice(b"@nan");
        assert_eq!(alpha_suffix(&long, 256), Some(&b"n"[..]));
        let mut long = vec![b'#'; 128];
        long.extend_from_slice(b"@nan");
        assert_eq!(alpha_suffix(&long, 256), None);
    }

    #[test]
    fn is_nan_spelling_examples() {
        for s in [
            &b"nan"[..],
            b"NaN",
            b"-nan",
            b"+NAN",
            b"\x0b nan",
            b"nan()",
            b"nan(a_1)",
        ] {
            assert!(is_nan_spelling(s), "{s:?}");
        }
        for s in [
            &b""[..],
            b"na",
            b"nanx",
            b"nan ",
            b"nan(",
            b"nan(-)",
            b"+-nan",
            b"0x1",
            b"inf",
        ] {
            assert!(!is_nan_spelling(s), "{s:?}");
        }
    }

    unsafe extern "C" {
        fn strtod(s: *const core::ffi::c_char, end: *mut *mut core::ffi::c_char) -> f64;
    }

    /// Whether libc's `strtod` reads all of `s` as a NaN.
    fn c_reads_nan(s: &[u8]) -> bool {
        let s = std::ffi::CString::new(s).unwrap();
        let mut end = ptr::null_mut();
        // SAFETY: `s` is a live NUL-terminated string and `end` a writable
        // local, which `strtod` points into `s`.
        let d = unsafe { strtod(s.as_ptr(), &mut end) };
        // SAFETY: `end` points into `s`, at or before its NUL.
        d.is_nan() && unsafe { *end } == 0
    }

    #[test]
    fn is_nan_spelling_matches_libc_strtod() {
        let fixed: [&[u8]; 28] = [
            b"nan",
            b"NaN",
            b"NAN",
            b"+nan",
            b"-nan",
            b" nan",
            b"\t\n\x0b\x0c\r nan",
            b"nan()",
            b"nan(1)",
            b"nan(abc_XYZ09)",
            b"nan(",
            b"nan)",
            b"nan(a-b)",
            b"nan(a b)",
            b"nanx",
            b"na",
            b"",
            b" ",
            b"+-nan",
            b"--nan",
            b"0.5",
            b"inf",
            b"nan ",
            b"nan(0x1f)",
            b"\xa0nan",
            b"nan\xff",
            b"1e999",
            b"0xnan",
        ];
        for s in fixed {
            assert_eq!(
                is_nan_spelling(s),
                c_reads_nan(s),
                "{:?}",
                s.escape_ascii().to_string()
            );
        }
        // Every string of up to four bytes over an alphabet that reaches
        // each branch.
        let alphabet = b" \x0b+-nNaA()_1x.";
        let mut s = Vec::new();
        for len in 0..=4u32 {
            for mut i in 0..alphabet.len().pow(len) {
                s.clear();
                for _ in 0..len {
                    s.push(alphabet[i % alphabet.len()]);
                    i /= alphabet.len();
                }
                assert_eq!(
                    is_nan_spelling(&s),
                    c_reads_nan(&s),
                    "{:?}",
                    s.escape_ascii().to_string()
                );
            }
        }
    }

    /// The corrected behaviour: a prefix that ends inside the marker fails
    /// instead of reading an uninitialised buffer (`#`, length 0) or the
    /// bytes after the prefix (`0x`, length 0, which C parses as red).
    #[test]
    fn prefixes_inside_the_marker_fail_before_c() {
        for (color, len) in [(c"#ff0000", 0), (c"0xff0000", 0), (c"0xff0000", 1)] {
            assert_eq!(
                parse_color_prefix(color, len),
                Err(AVERROR_EINVAL),
                "{color:?} {len}"
            );
            // SAFETY: a NULL log context is "no context".
            let ret = unsafe { parse_color_with_context(color, Some(len), ptr::null_mut()) };
            assert_eq!(ret, Err(AVERROR_EINVAL), "{color:?} {len}");
        }
        // The marker alone is a valid, empty, prefix.
        assert_eq!(parse_color_prefix(c"#ff0000", 1), Err(AVERROR_EINVAL));
        assert_eq!(parse_color_prefix(c"0xff0000", 2), Err(AVERROR_EINVAL));
        assert_eq!(parse_color_prefix(c"#ff0000", 7), Ok([255, 0, 0, 255]));
        assert_eq!(parse_color_prefix(c"0xff0000", 8), Ok([255, 0, 0, 255]));
    }

    /// The corrected behaviour: a NaN opacity fails instead of reaching C's
    /// conversion of NaN to a byte (UBSan: `parseutils.c:433`).
    #[test]
    fn nan_opacities_fail_before_c() {
        for color in [
            c"red@nan",
            c"#ff0000@NaN",
            c"0xff000080@-nan",
            c"red@ nan(1)",
            c"random@nan",
        ] {
            assert_eq!(parse_color(color), Err(AVERROR_EINVAL), "{color:?}");
            let len = color.count_bytes();
            assert_eq!(
                parse_color_prefix(color, len),
                Err(AVERROR_EINVAL),
                "{color:?}"
            );
        }
        // Cut off before the specifier, the color is valid.
        assert_eq!(parse_color_prefix(c"red@nan", 3), Ok([255, 0, 0, 255]));
        // A NaN spelling C rejects itself still reaches C.
        assert_eq!(parse_color(c"red@nanx"), Err(AVERROR_EINVAL));
        assert_eq!(parse_color(c"red@0.5"), Ok([255, 0, 0, 127]));
    }

    #[test]
    fn parse_time_selects_the_mode() {
        assert_eq!(parse_time(c"1.5", true), Ok(1_500_000));
        assert_eq!(parse_time(c"1.5", false), Err(AVERROR_EINVAL));
        assert_eq!(parse_time(c"1970-01-01T00:00:01Z", false), Ok(1_000_000));
        assert_eq!(
            parse_time(c"1970-01-01T00:00:01Z", true),
            Err(AVERROR_EINVAL)
        );
    }
}
