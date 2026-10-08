//! Equivalence of `libavutil/common.h`'s safe clamps, [`zero_extend`] and
//! `ceil_log2` with the raw C functions, on every edge their branches test.

use core::ffi::c_int;

use ffmpeg::libavutil::common::{
    ceil_log2, clamp_f32, clamp_f64, clamp_i32, clamp_i64, clamp_to_i8, clamp_to_i16, clamp_to_i32,
    clamp_to_signed_pow2, clamp_to_u8, clamp_to_u16, clamp_to_unsigned_pow2, zero_extend,
};
use ffmpeg::libavutil::ffi;

/// `i32` values around zero, every power of two and the type limits.
fn ints() -> Vec<i32> {
    let mut v = vec![0, 1, -1, i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX - 1];
    for b in 0..31 {
        let p = 1i32 << b;
        v.extend([p - 1, p, p + 1, -p - 1, -p, -p + 1]);
    }
    v
}

/// `i64` values around zero, every power of two and the type limits.
fn longs() -> Vec<i64> {
    let mut v = vec![0, 1, -1, i64::MIN, i64::MIN + 1, i64::MAX, i64::MAX - 1];
    for b in 0..63 {
        let p = 1i64 << b;
        v.extend([p - 1, p, p + 1, -p - 1, -p, -p + 1]);
    }
    v
}

/// Floats across signs, zeros, infinities, NaN and subnormals.
const FLOATS: [f64; 13] = [
    0.0,
    -0.0,
    0.5,
    -0.5,
    1.0,
    -1.0,
    1e300,
    -1e300,
    f64::MIN_POSITIVE / 2.0,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    f64::MAX,
];

/// Whether two floats are the same value, NaN equal to NaN and the zeros
/// told apart.
fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

#[test]
fn bounded_integer_clamps_match_c() {
    let bounds = [(0, 10), (-10, 10), (10, 0), (i32::MIN, i32::MAX), (5, 5)];
    for a in ints() {
        for (lo, hi) in bounds {
            // SAFETY: `av_clip_c` takes and returns integers by value.
            assert_eq!(clamp_i32(a, lo, hi), unsafe { ffi::av_clip_c(a, lo, hi) });
        }
    }
    let bounds = [(0, 10), (-1 << 40, 1 << 40), (10, 0), (i64::MIN, i64::MAX)];
    for a in longs() {
        for (lo, hi) in bounds {
            // SAFETY: `av_clip64_c` takes and returns integers by value.
            assert_eq!(clamp_i64(a, lo, hi), unsafe { ffi::av_clip64_c(a, lo, hi) });
        }
    }
}

#[test]
fn saturations_match_c() {
    for a in ints() {
        // SAFETY: each takes and returns integers by value.
        unsafe {
            assert_eq!(clamp_to_u8(a), ffi::av_clip_uint8_c(a), "{a}");
            assert_eq!(clamp_to_i8(a), ffi::av_clip_int8_c(a), "{a}");
            assert_eq!(clamp_to_u16(a), ffi::av_clip_uint16_c(a), "{a}");
            assert_eq!(clamp_to_i16(a), ffi::av_clip_int16_c(a), "{a}");
        }
    }
    for a in longs() {
        // SAFETY: `av_clipl_int32_c` takes and returns integers by value.
        assert_eq!(clamp_to_i32(a), unsafe { ffi::av_clipl_int32_c(a) }, "{a}");
    }
}

#[test]
fn power_of_two_clamps_match_c_at_every_defined_position() {
    for p in 0..=31u32 {
        for a in ints() {
            // SAFETY: both take and return integers by value; `p <= 31` keeps
            // their shifts defined.
            let (s, u) = unsafe {
                (
                    ffi::av_clip_intp2_c(a, p as c_int),
                    ffi::av_clip_uintp2_c(a, p as c_int),
                )
            };
            assert_eq!(clamp_to_signed_pow2(a, p), Some(s), "a = {a}, p = {p}");
            assert_eq!(clamp_to_unsigned_pow2(a, p), Some(u), "a = {a}, p = {p}");
        }
    }
}

#[test]
fn float_clamps_match_c() {
    for a in FLOATS {
        for lo in FLOATS {
            for hi in FLOATS {
                // SAFETY: `av_clipd_c` takes and returns doubles by value.
                let c = unsafe { ffi::av_clipd_c(a, lo, hi) };
                assert!(same(clamp_f64(a, lo, hi), c), "{a} {lo} {hi}");
                let (a, lo, hi) = (a as f32, lo as f32, hi as f32);
                // SAFETY: `av_clipf_c` takes and returns floats by value.
                let c = unsafe { ffi::av_clipf_c(a, lo, hi) };
                let r = clamp_f32(a, lo, hi);
                assert!(same(r.into(), c.into()), "{a} {lo} {hi}");
            }
        }
    }
}

#[test]
fn ceil_log2_matches_c_and_av_log2() {
    for x in ints() {
        // SAFETY: both take and return integers by value.
        let (c, log2) = unsafe {
            (
                ffi::av_ceil_log2_c(x),
                ffi::av_log2((x as u32).wrapping_sub(1) << 1),
            )
        };
        assert_eq!(ceil_log2(x), c, "{x}");
        assert_eq!(c, log2, "{x}");
        if x >= 2 {
            // The exact ceiling, from the bit width of `x - 1`.
            assert_eq!(c as u32, 32 - (x as u32 - 1).leading_zeros(), "{x}");
        }
    }
}

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
