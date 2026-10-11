//! Safe wrappers over `libavutil/eval.h`.
//!
//! [`parse_number`] reads a number with FFmpeg's SI, binary and `dB`
//! suffixes (`av_strtod`).

use core::ffi::{CStr, c_char};
use core::ptr;

use super::ffi;

/// Wraps: av_strtod
///
/// The number at the start of `numstr`, and the rest of `numstr` after it.
///
/// The number is what `strtod` reads (decimal or hexadecimal floats,
/// `inf`, `nan`), or, after `0x` or `0X`, a hexadecimal integer as `strtoul`
/// reads it. A suffix may follow: `dB` turns the value from decibels into a
/// ratio (`10^(d / 20)`); one of the SI prefixes `y z a f p n u m c d h k K M
/// G T P E Z Y` scales it by its power of 10, or of 1024 with an `i` after
/// it (`Ki`, `Mi`, ...); then a `B` multiplies it by 8.
///
/// When nothing parses (an empty string, only whitespace, no number), the
/// value is 0 and the rest is all of `numstr`.
///
/// ```
/// use ffmpeg::libavutil::eval::parse_number;
/// assert_eq!(parse_number(c"1.5KiB, more"), (12288.0, c", more"));
/// assert_eq!(parse_number(c"none"), (0.0, c"none"));
/// ```
pub fn parse_number(numstr: &CStr) -> (f64, &CStr) {
    let start = numstr.as_ptr();
    let mut tail: *mut c_char = ptr::null_mut();
    // SAFETY: `numstr` is a live NUL-terminated string that C only reads (a
    // suffix byte only after a non-NUL one), and `tail` a writable local, in
    // which C stores the address of the first byte of `numstr` it did not
    // consume.
    let value = unsafe { ffi::av_strtod(start, &mut tail) };
    let bytes = numstr.to_bytes_with_nul();
    let parsed = (tail as usize).wrapping_sub(start as usize);
    assert!(parsed < bytes.len(), "av_strtod left the string");
    let rest = CStr::from_bytes_with_nul(&bytes[parsed..]).expect("the suffix keeps the NUL");
    (value, rest)
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn suffixes_scale_the_value() {
        assert_eq!(parse_number(c"2k"), (2000.0, c""));
        assert_eq!(parse_number(c"2K"), (2000.0, c""));
        assert_eq!(parse_number(c"2Ki"), (2048.0, c""));
        assert_eq!(parse_number(c"1B"), (8.0, c""));
        assert_eq!(parse_number(c"1MiB"), (8.0 * 1024.0 * 1024.0, c""));
        // `ff_exp10` is not exact.
        let (ratio, rest) = parse_number(c"20dB");
        assert!((ratio - 10.0).abs() < 1e-12, "{ratio}");
        assert_eq!(rest, c"");
        assert_eq!(parse_number(c"0x10"), (16.0, c""));
        assert_eq!(parse_number(c"0X10M"), (16e6, c""));
        // A letter without an SI power stays in the rest.
        assert_eq!(parse_number(c"3q"), (3.0, c"q"));
        assert_eq!(parse_number(c"3i"), (3.0, c"i"));
    }

    #[test]
    fn nothing_parsed_returns_the_whole_string() {
        for s in [c"", c"   ", c"x", c"-", c"k", c"B", c"dB"] {
            assert_eq!(parse_number(s), (0.0, s), "{s:?}");
        }
        // `0x` without digits reads the `0` only.
        assert_eq!(parse_number(c"0x"), (0.0, c"x"));
        let (nan, rest) = parse_number(c"nan!");
        assert!(nan.is_nan());
        assert_eq!(rest, c"!");
    }

    /// The rest borrows `numstr` itself, at the byte C stopped at.
    #[test]
    fn the_rest_is_a_suffix_of_the_input() {
        let s = c"  -1.25e1Mi tail";
        let (value, rest) = parse_number(s);
        assert_eq!(value, -12.5 * 1024.0 * 1024.0);
        assert_eq!(rest, c" tail");
        assert_eq!(
            rest.as_ptr() as usize - s.as_ptr() as usize,
            s.count_bytes() - rest.count_bytes()
        );
    }
}
