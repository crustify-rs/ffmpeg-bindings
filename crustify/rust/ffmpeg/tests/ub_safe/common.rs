//! UB tests for `libavutil/common.h`'s safe clamps, [`zero_extend`] and
//! `ceil_log2`, through the safe API only: every input class the C branches
//! test, and no bit position reaches C's undefined shifts.

use ffmpeg::libavutil::common::{
    ceil_log2, clamp_f32, clamp_f64, clamp_i32, clamp_i64, clamp_to_i8, clamp_to_i16, clamp_to_i32,
    clamp_to_signed_pow2, clamp_to_u8, clamp_to_u16, clamp_to_unsigned_pow2, zero_extend,
};

/// `i32` values around zero, every power of two and the type limits.
fn ints() -> impl Iterator<Item = i32> {
    (0..31)
        .flat_map(|b| {
            let p = 1i32 << b;
            [p - 1, p, p + 1, -p - 1, -p, -p + 1]
        })
        .chain([i32::MIN, i32::MIN + 1, i32::MAX, i32::MAX - 1])
}

#[test]
fn integer_clamps_stay_in_range_for_every_input() {
    for a in ints() {
        assert!((0..=255).contains(&i32::from(clamp_to_u8(a))));
        assert!((-128..=127).contains(&i32::from(clamp_to_i8(a))));
        assert!((0..=65535).contains(&i32::from(clamp_to_u16(a))));
        assert!((-32768..=32767).contains(&i32::from(clamp_to_i16(a))));
        assert_eq!(clamp_to_i32(i64::from(a)), a);
        assert_eq!(clamp_i32(a, i32::MIN, i32::MAX), a);
        // Inverted bounds never panic.
        let r = clamp_i32(a, 10, -10);
        assert!(r == 10 || r == -10);
        let r = clamp_i64(i64::from(a) << 32, 1, -1);
        assert!(r == 1 || r == -1);
        let _ = ceil_log2(a);
    }
    for a in [i64::MIN, i64::MAX, 0, i64::from(i32::MAX) + 1] {
        let _ = clamp_to_i32(a);
        assert_eq!(clamp_i64(a, i64::MIN, i64::MAX), a);
    }
}

#[test]
fn power_of_two_clamps_are_defined_at_every_accepted_position() {
    for p in 0..=31u32 {
        let hi = (1i64 << p) - 1;
        for a in ints() {
            let s = i64::from(clamp_to_signed_pow2(a, p).unwrap());
            assert!((-hi - 1..=hi).contains(&s), "a = {a}, p = {p}");
            let u = i64::from(clamp_to_unsigned_pow2(a, p).unwrap());
            assert!((0..=hi).contains(&u), "a = {a}, p = {p}");
        }
    }
}

#[test]
fn power_of_two_clamps_reject_positions_beyond_31() {
    for p in (32..=64).chain([u32::MAX / 2, u32::MAX]) {
        for a in [0, -1, i32::MIN, i32::MAX] {
            assert_eq!(clamp_to_signed_pow2(a, p), None, "{p}");
            assert_eq!(clamp_to_unsigned_pow2(a, p), None, "{p}");
        }
    }
}

#[test]
fn float_clamps_accept_every_special_value() {
    let specials = [
        0.0,
        -0.0,
        1.0,
        f64::MIN,
        f64::MAX,
        f64::MIN_POSITIVE / 4.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    for a in specials {
        for lo in specials {
            for hi in specials {
                let _ = clamp_f64(a, lo, hi);
                let _ = clamp_f32(a as f32, lo as f32, hi as f32);
            }
        }
    }
}

#[test]
fn zero_extend_is_defined_at_every_position() {
    for p in (0..=100).chain([u32::MAX - 1, u32::MAX]) {
        for a in [0, 1, u32::MAX] {
            assert_eq!(zero_extend(a, p).is_some(), p <= 31);
        }
    }
}
