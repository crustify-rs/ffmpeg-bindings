//! UB tests for `libavutil/intmath.c`'s safe [`log2_16bit`], through the safe API only:
//! every value of its domain.

use ffmpeg_bindings::libavutil::libavutil::intmath::log2_16bit;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log2_16bit_takes_every_value_of_its_domain() {
    for v in 0..=u16::MAX {
        assert!(log2_16bit(v) < 16);
    }
}
