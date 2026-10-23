//! UB tests for `libavutil/common.h`'s safe clamps, [`zero_extend`],
//! `ceil_log2` and bit-counting helpers, through the safe API only: every
//! input class the C branches test, every 16-bit value and bit boundary,
//! no bit position reaching C's undefined shifts, and concurrent calls.
#![expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::common_h::{
    ceil_log2, clamp_f32, clamp_f64, clamp_i32, clamp_i64, clamp_to_i8, clamp_to_i16, clamp_to_i32,
    clamp_to_signed_pow2, clamp_to_u8, clamp_to_u16, clamp_to_unsigned_pow2, parity, zero_extend,
};
use ffmpeg_bindings::libavutil::libavutil::intmath::{log2, log2_16bit};

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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn zero_extend_is_defined_at_every_position() {
    for p in (0..=100).chain([u32::MAX - 1, u32::MAX]) {
        for a in [0, 1, u32::MAX] {
            assert_eq!(zero_extend(a, p).is_some(), p <= 31);
        }
    }
}

// ---- count_ones and the saturating arithmetic ----

/// UB tests of `libavutil/common.h`'s native bit-count and saturating
/// arithmetic: every overflow edge, in debug builds where Rust's own
/// arithmetic overflow would panic, and across threads.
mod count_and_saturate {
    use ffmpeg_bindings::libavutil::libavutil::common_h::{
        count_ones, count_ones64, saturating_add_doubled32, saturating_add32, saturating_add64,
        saturating_sub_doubled32, saturating_sub32, saturating_sub64,
    };

    const I32: [i32; 7] = [i32::MIN, i32::MIN + 1, -1, 0, 1, i32::MAX - 1, i32::MAX];
    const I64: [i64; 7] = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];

    #[test]
    fn every_edge_pair_neither_panics_nor_wraps() {
        for a in I32 {
            for b in I32 {
                let wide = |x: i64| x.clamp(i32::MIN.into(), i32::MAX.into()) as i32;
                let (a64, b64) = (i64::from(a), i64::from(b));
                assert_eq!(saturating_add32(a, b), wide(a64 + b64));
                assert_eq!(saturating_sub32(a, b), wide(a64 - b64));
                assert_eq!(
                    saturating_add_doubled32(a, b),
                    wide(a64 + i64::from(wide(2 * b64)))
                );
                assert_eq!(
                    saturating_sub_doubled32(a, b),
                    wide(a64 - i64::from(wide(2 * b64)))
                );
            }
        }
        for a in I64 {
            for b in I64 {
                let wide = |x: i128| x.clamp(i64::MIN.into(), i64::MAX.into()) as i64;
                assert_eq!(saturating_add64(a, b), wide(i128::from(a) + i128::from(b)));
                assert_eq!(saturating_sub64(a, b), wide(i128::from(a) - i128::from(b)));
            }
        }
    }

    #[test]
    fn popcount_spans_the_whole_word() {
        for bit in 0..64 {
            assert_eq!(count_ones64(1 << bit), 1);
            assert_eq!(count_ones64(!(1u64 << bit)), 63);
        }
        for bit in 0..32 {
            assert_eq!(count_ones(1 << bit), 1);
        }
    }

    #[test]
    fn usable_in_const_and_across_threads() {
        const P: u32 = count_ones(0xF0F0);
        const S: i32 = saturating_add32(i32::MAX, 1);
        assert_eq!((P, S), (8, i32::MAX));
        let r = std::thread::spawn(|| saturating_sub64(i64::MIN, 1))
            .join()
            .unwrap();
        assert_eq!(r, i64::MIN);
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn log2_and_parity_take_every_bit_boundary() {
    for bit in 0..32 {
        for v in [(1u32 << bit) - 1, 1 << bit, (1 << bit) | 1, !(1u32 << bit)] {
            assert!(log2(v) < 32);
            let _ = parity(v);
        }
    }
    assert_eq!((log2(0), log2(u32::MAX)), (0, 31));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn helpers_run_concurrently() {
    let handles: Vec<_> = (0..4u32)
        .map(|t| {
            thread::spawn(move || {
                (0..10_000u32)
                    .map(|i| {
                        let v = i.wrapping_mul(0x9E37_79B9) ^ t;
                        log2(v) + log2_16bit(v as u16) + u32::from(parity(v))
                    })
                    .sum::<u32>()
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
