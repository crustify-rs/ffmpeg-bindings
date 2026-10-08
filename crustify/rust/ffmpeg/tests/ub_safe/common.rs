//! UB tests for `libavutil/common.h`'s [`zero_extend`], through the safe API
//! only: no position reaches C's undefined shift.

use ffmpeg::libavutil::common::zero_extend;

#[test]
fn every_position_is_defined() {
    for p in (0..=100).chain([u32::MAX - 1, u32::MAX]) {
        for a in [0, 1, u32::MAX] {
            assert_eq!(zero_extend(a, p).is_some(), p <= 31);
        }
    }
}
