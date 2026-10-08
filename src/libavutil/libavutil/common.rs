//! Safe wrappers over `libavutil/common.h`.

use super::ffi;

/// Wraps: av_zero_extend_c
///
/// `a` with every bit from position `p` up cleared, keeping its `p` low bits
/// (`av_zero_extend`). `None` when `p > 31`, for which C's shift is
/// undefined.
pub fn zero_extend(a: u32, p: u32) -> Option<u32> {
    if p > 31 {
        return None;
    }
    // SAFETY: `av_zero_extend_c` computes on its arguments only, and `p` is
    // within the `0..=31` its `1U << p` requires.
    Some(unsafe { ffi::av_zero_extend_c(a, p) })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn zero_extend_keeps_the_low_bits() {
        assert_eq!(zero_extend(u32::MAX, 0), Some(0));
        assert_eq!(zero_extend(u32::MAX, 1), Some(1));
        assert_eq!(zero_extend(0xdead_beef, 16), Some(0xbeef));
        assert_eq!(zero_extend(u32::MAX, 31), Some(i32::MAX as u32));
        assert_eq!(zero_extend(0, 31), Some(0));
    }

    #[test]
    fn zero_extend_rejects_wide_positions_before_c() {
        for p in [32, 33, 64, u32::MAX] {
            assert_eq!(zero_extend(u32::MAX, p), None);
        }
    }
}
