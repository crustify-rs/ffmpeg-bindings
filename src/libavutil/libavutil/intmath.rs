//! Safe wrappers over `libavutil/intmath.c`: the highest set bit of an integer.

use core::ffi::c_int;

use super::ffi;

/// Converts a C result that the C contract keeps in `0..=31` to `u32`.
fn bit_index(r: c_int) -> u32 {
    debug_assert!((0..32).contains(&r), "bit index {r} out of range");
    r.unsigned_abs()
}

/// Wraps: `av_log2`
///
/// The index of the highest set bit of `v`, `floor(log2(v))`, with 0 for an
/// argument of 0 as for 1 (C's `v | 1`).
#[must_use]
pub fn log2(v: u32) -> u32 {
    // SAFETY: `av_log2` takes its argument by value and reads no memory.
    bit_index(unsafe { ffi::av_log2(v) })
}

/// Wraps: `av_log2_16bit`
///
/// [`log2`] of a 16-bit value. C declares an `unsigned` argument but defines
/// the result only below 2^16: its portable fallback indexes a 256-entry
/// table with the high byte, which wider values overrun, so the argument is a
/// `u16` here.
#[must_use]
pub fn log2_16bit(v: u16) -> u32 {
    // SAFETY: `av_log2_16bit` takes its argument by value; a value below 2^16
    // keeps every implementation's table lookup in bounds.
    bit_index(unsafe { ffi::av_log2_16bit(v.into()) })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn log2_is_the_highest_set_bit() {
        assert_eq!(log2(0), 0);
        assert_eq!(log2(1), 0);
        for bit in 0..32 {
            assert_eq!(log2(1 << bit), bit);
            assert_eq!(log2((1 << bit) | 1), bit);
        }
        assert_eq!(log2(u32::MAX), 31);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn log2_16bit_matches_log2_on_its_domain() {
        for v in [0, 1, 2, 3, 0xFF, 0x100, 0x7FFF, 0x8000, u16::MAX] {
            assert_eq!(log2_16bit(v), log2(v.into()), "{v:#x}");
        }
    }

    #[test]
    fn bit_index_widens_in_range_results() {
        assert_eq!(bit_index(0), 0);
        assert_eq!(bit_index(31), 31);
    }
}
