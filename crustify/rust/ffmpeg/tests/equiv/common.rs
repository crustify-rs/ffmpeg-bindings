//! Equivalence of `libavutil/common.h`'s [`zero_extend`] with
//! `av_zero_extend_c`.

use ffmpeg::libavutil::common::zero_extend;
use ffmpeg::libavutil::ffi;

#[test]
fn zero_extend_matches_c_at_every_defined_position() {
    let values = [
        0,
        1,
        0x7f,
        0x80,
        0xdead_beef,
        0x8000_0000,
        i32::MAX as u32,
        u32::MAX,
    ];
    for a in values {
        for p in 0..=31 {
            // SAFETY: a value-only function; `p` is within `0..=31`.
            let c = unsafe { ffi::av_zero_extend_c(a, p) };
            assert_eq!(zero_extend(a, p), Some(c), "{a:#x} {p}");
        }
    }
}

#[test]
fn chained_zero_extends_agree_after_every_step() {
    let (mut mine, mut theirs) = (u32::MAX, u32::MAX);
    for p in [31, 24, 24, 17, 8, 3, 0] {
        mine = zero_extend(mine, p).unwrap();
        // SAFETY: as above.
        theirs = unsafe { ffi::av_zero_extend_c(theirs, p) };
        assert_eq!(mine, theirs, "{p}");
    }
}
