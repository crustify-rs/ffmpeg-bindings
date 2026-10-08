//! Safe wrappers over `libavutil/bswap.h`.
//!
//! The byte swaps are `static inline` C functions of plain integers: each
//! wrapper calls the header's generic C body (compiled into the `-sys` crate's
//! shim), which equals the matching `swap_bytes` on every input.

use super::ffi;

/// Wraps: av_bswap16
///
/// `x` with its two bytes swapped, as [`u16::swap_bytes`].
#[inline]
pub fn swap16(x: u16) -> u16 {
    // SAFETY: `av_bswap16` takes and returns an integer by value and touches
    // no memory.
    unsafe { ffi::av_bswap16(x) }
}

/// Wraps: av_bswap32
///
/// `x` with its byte order reversed, as [`u32::swap_bytes`].
#[inline]
pub fn swap32(x: u32) -> u32 {
    // SAFETY: `av_bswap32` takes and returns an integer by value and touches
    // no memory.
    unsafe { ffi::av_bswap32(x) }
}

/// Wraps: av_bswap64
///
/// `x` with its byte order reversed, as [`u64::swap_bytes`].
#[inline]
pub fn swap64(x: u64) -> u64 {
    // SAFETY: `av_bswap64` takes and returns an integer by value and touches
    // no memory.
    unsafe { ffi::av_bswap64(x) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn swaps_reverse_the_bytes() {
        assert_eq!(swap16(0x1234), 0x3412);
        assert_eq!(swap32(0x1234_5678), 0x7856_3412);
        assert_eq!(swap64(0x0102_0304_0506_0708), 0x0807_0605_0403_0201);
    }

    #[test]
    fn swaps_are_involutions() {
        for x in [0, 1, 0x80, 0xff, 0x7fff, 0x8000, u16::MAX] {
            assert_eq!(swap16(swap16(x)), x);
        }
        for x in [0, 1, 0x8000_0000, 0xdead_beef, u32::MAX] {
            assert_eq!(swap32(swap32(x)), x);
        }
        for x in [0, 1, 1 << 63, 0xdead_beef_cafe_f00d, u64::MAX] {
            assert_eq!(swap64(swap64(x)), x);
        }
    }
}
