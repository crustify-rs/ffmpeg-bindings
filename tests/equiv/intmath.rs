//! Equivalence of `libavutil/intmath.c`'s safe [`log2`] and [`log2_16bit`] with the raw C
//! functions, on every edge and every 16-bit value.
#![expect(
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::c_int;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::intmath::{log2, log2_16bit};

use super::common_h::u32_edges;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log2_matches_av_log2() {
    for v in u32_edges() {
        // SAFETY: `av_log2` takes its argument by value.
        let c = unsafe { ffi::av_log2(v) };
        assert_eq!(log2(v) as c_int, c, "{v:#x}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log2_16bit_matches_av_log2_16bit_on_every_value() {
    for v in 0..=u16::MAX {
        // SAFETY: `av_log2_16bit` takes its argument by value; it is below
        // 2^16.
        let c = unsafe { ffi::av_log2_16bit(v.into()) };
        assert_eq!(log2_16bit(v) as c_int, c, "{v:#x}");
    }
}
