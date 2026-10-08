//! Safe wrappers over `libavutil/common.h`.
//!
//! The clipping helpers are `static inline` C functions of plain numbers: each
//! wrapper calls the header's generic C body (`av_clip_c` and its siblings,
//! compiled into the `-sys` crate's shim), so every result, the edge cases
//! included, is C's.
//!
//! The bit-position helpers ([`clamp_to_signed_pow2`],
//! [`clamp_to_unsigned_pow2`], [`zero_extend`]) return `None` for a position
//! above 31, for which C's shifts are undefined, without calling C.
//!
//! The bounded clamps ([`clamp_i32`], [`clamp_i64`], [`clamp_f32`],
//! [`clamp_f64`]) differ from `Ord::clamp` and `f32::clamp` in two ways
//! inherited from C: they never panic, returning `min` for `a < min` and
//! otherwise `max` for `a > max` even when `min > max`, and the float ones
//! return `min` for a NaN `a`.

use core::ffi::c_int;

use super::ffi;

/// The largest bit position the power-of-two clamps accept: `1U << p` is
/// undefined in C beyond it.
const MAX_POW2: u32 = 31;

/// Wraps: av_clip_c
///
/// `a` clamped into `min..=max`: `min` when `a < min`, else `max` when
/// `a > max`, else `a`. Never panics, even when `min > max`.
#[inline]
pub fn clamp_i32(a: i32, min: i32, max: i32) -> i32 {
    // SAFETY: `av_clip_c` takes and returns integers by value and touches no
    // memory; it is defined for every input.
    unsafe { ffi::av_clip_c(a, min, max) }
}

/// Wraps: av_clip64_c
///
/// [`clamp_i32`] on 64-bit integers.
#[inline]
pub fn clamp_i64(a: i64, min: i64, max: i64) -> i64 {
    // SAFETY: `av_clip64_c` takes and returns integers by value and touches
    // no memory; it is defined for every input.
    unsafe { ffi::av_clip64_c(a, min, max) }
}

/// Wraps: av_clip_uint8_c
///
/// `a` saturated into `0..=255`.
#[inline]
pub fn clamp_to_u8(a: i32) -> u8 {
    // SAFETY: `av_clip_uint8_c` takes and returns integers by value and
    // touches no memory; it is defined for every input.
    unsafe { ffi::av_clip_uint8_c(a) }
}

/// Wraps: av_clip_int8_c
///
/// `a` saturated into `-128..=127`.
#[inline]
pub fn clamp_to_i8(a: i32) -> i8 {
    // SAFETY: `av_clip_int8_c` takes and returns integers by value and
    // touches no memory; it is defined for every input.
    unsafe { ffi::av_clip_int8_c(a) }
}

/// Wraps: av_clip_uint16_c
///
/// `a` saturated into `0..=65535`.
#[inline]
pub fn clamp_to_u16(a: i32) -> u16 {
    // SAFETY: `av_clip_uint16_c` takes and returns integers by value and
    // touches no memory; it is defined for every input.
    unsafe { ffi::av_clip_uint16_c(a) }
}

/// Wraps: av_clip_int16_c
///
/// `a` saturated into `-32768..=32767`.
#[inline]
pub fn clamp_to_i16(a: i32) -> i16 {
    // SAFETY: `av_clip_int16_c` takes and returns integers by value and
    // touches no memory; it is defined for every input.
    unsafe { ffi::av_clip_int16_c(a) }
}

/// Wraps: av_clipl_int32_c
///
/// `a` saturated into the `i32` range.
#[inline]
pub fn clamp_to_i32(a: i64) -> i32 {
    // SAFETY: `av_clipl_int32_c` takes and returns integers by value and
    // touches no memory; it is defined for every input.
    unsafe { ffi::av_clipl_int32_c(a) }
}

/// Wraps: av_clip_intp2_c
///
/// `a` saturated into `-(2^p)..=2^p - 1`, the range of a `p + 1`-bit signed
/// integer. `None` when `p > 31`, for which C's shifts are undefined.
#[inline]
pub fn clamp_to_signed_pow2(a: i32, p: u32) -> Option<i32> {
    if p > MAX_POW2 {
        return None;
    }
    // SAFETY: `av_clip_intp2_c` takes and returns integers by value and
    // touches no memory. `p <= 31` keeps `1U << p` and `2U << p` defined, and
    // at `p == 31` its mask is zero, so the `1 << p` that would overflow is
    // never evaluated.
    Some(unsafe { ffi::av_clip_intp2_c(a, p as c_int) })
}

/// Wraps: av_clip_uintp2_c
///
/// `a` saturated into `0..=2^p - 1`, the range of a `p`-bit unsigned integer.
/// `None` when `p > 31`, for which C's shift is undefined.
#[inline]
pub fn clamp_to_unsigned_pow2(a: i32, p: u32) -> Option<u32> {
    if p > MAX_POW2 {
        return None;
    }
    // SAFETY: `av_clip_uintp2_c` takes and returns integers by value and
    // touches no memory; `p <= 31` keeps its `1U << p` defined.
    Some(unsafe { ffi::av_clip_uintp2_c(a, p as c_int) })
}

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

/// Wraps: av_clipf_c
///
/// `a` clamped into `min..=max` as C's `FFMIN(FFMAX(a, min), max)`: `min` for
/// a NaN `a` or one below `min`, then `max` when that exceeds `max`. Never
/// panics, even when `min > max` or a bound is NaN.
#[inline]
pub fn clamp_f32(a: f32, min: f32, max: f32) -> f32 {
    // SAFETY: `av_clipf_c` takes and returns floats by value and touches no
    // memory; it is defined for every input, NaN included.
    unsafe { ffi::av_clipf_c(a, min, max) }
}

/// Wraps: av_clipd_c
///
/// [`clamp_f32`] on `f64`.
#[inline]
pub fn clamp_f64(a: f64, min: f64, max: f64) -> f64 {
    // SAFETY: `av_clipd_c` takes and returns floats by value and touches no
    // memory; it is defined for every input, NaN included.
    unsafe { ffi::av_clipd_c(a, min, max) }
}

/// Wraps: av_ceil_log2_c
///
/// `ceil(log2(x))` for `x >= 1`. C computes it as `av_log2((x - 1) << 1)` in
/// unsigned arithmetic, so `x <= 0` wraps around and yields 31.
#[inline]
pub fn ceil_log2(x: i32) -> i32 {
    // SAFETY: `av_ceil_log2_c` takes and returns integers by value and touches
    // no memory; its arithmetic is unsigned, defined for every input.
    unsafe { ffi::av_ceil_log2_c(x) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn bounded_clamps_pick_min_then_max() {
        assert_eq!(clamp_i32(5, 0, 10), 5);
        assert_eq!(clamp_i32(-5, 0, 10), 0);
        assert_eq!(clamp_i32(50, 0, 10), 10);
        assert_eq!(clamp_i64(i64::MIN, -1, 1), -1);
        assert_eq!(clamp_i64(i64::MAX, -1, 1), 1);
        // Inverted bounds: below `min` gives `min`, anything else `max`.
        assert_eq!(clamp_i32(5, 10, 0), 10);
        assert_eq!(clamp_i32(15, 10, 0), 0);
        assert_eq!(clamp_i64(5, 10, 0), 10);
        assert_eq!(clamp_i64(10, 10, 0), 0);
    }

    #[test]
    fn float_clamps_follow_ffmin_ffmax() {
        assert_eq!(clamp_f32(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clamp_f32(f32::NAN, -1.0, 1.0), -1.0);
        assert_eq!(clamp_f32(f32::NEG_INFINITY, -1.0, 1.0), -1.0);
        assert_eq!(clamp_f32(f32::INFINITY, -1.0, 1.0), 1.0);
        assert_eq!(clamp_f64(f64::NAN, -1.0, 1.0), -1.0);
        assert_eq!(clamp_f64(2.0, 3.0, 1.0), 1.0);
        assert!(clamp_f64(0.0, f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn saturations_cover_their_ranges() {
        assert_eq!(
            [i32::MIN, -1, 0, 255, 256, i32::MAX].map(clamp_to_u8),
            [0, 0, 0, 255, 255, 255]
        );
        assert_eq!(
            [i32::MIN, -129, -128, 127, 128].map(clamp_to_i8),
            [-128, -128, -128, 127, 127]
        );
        assert_eq!([-1, 65535, 65536].map(clamp_to_u16), [0, 65535, 65535]);
        assert_eq!(
            [-32769, -32768, 32767, 32768].map(clamp_to_i16),
            [-32768, -32768, 32767, 32767]
        );
        assert_eq!(
            [i64::MIN, i32::MIN as i64 - 1, 7, i32::MAX as i64 + 1].map(clamp_to_i32),
            [i32::MIN, i32::MIN, 7, i32::MAX]
        );
    }

    #[test]
    fn power_of_two_clamps_span_every_bit_position() {
        for p in 0..=MAX_POW2 {
            let hi = ((1i64 << p) - 1) as i32;
            let lo = (-(1i64 << p)) as i32;
            assert_eq!(clamp_to_signed_pow2(i32::MAX, p), Some(hi), "p = {p}");
            assert_eq!(clamp_to_signed_pow2(i32::MIN, p), Some(lo), "p = {p}");
            assert_eq!(clamp_to_signed_pow2(0, p), Some(0));
            assert_eq!(
                clamp_to_unsigned_pow2(i32::MAX, p),
                Some(hi as u32),
                "p = {p}"
            );
            assert_eq!(clamp_to_unsigned_pow2(-1, p), Some(0));
        }
    }

    #[test]
    fn power_of_two_clamps_reject_wide_positions_before_c() {
        for p in [32, 33, 64, u32::MAX] {
            assert_eq!(clamp_to_signed_pow2(1, p), None);
            assert_eq!(clamp_to_unsigned_pow2(1, p), None);
        }
    }

    #[test]
    fn ceil_log2_rounds_up_and_wraps_below_one() {
        assert_eq!(
            [1, 2, 3, 4, 5, 1 << 30, (1 << 30) + 1, i32::MAX].map(ceil_log2),
            [0, 1, 2, 2, 3, 30, 31, 31]
        );
        assert_eq!([0, -1, i32::MIN].map(ceil_log2), [31, 31, 31]);
    }

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
