//! Equivalence of `libavutil/mathematics.h`'s [`Rounding`] with the C
//! `enum AVRounding` values `av_rescale_rnd` accepts and acts on.

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mathematics::Rounding;

/// `av_rescale_rnd` in C.
fn rescale(a: i64, b: i64, c: i64, rnd: ffi::AVRounding) -> i64 {
    // SAFETY: `av_rescale_rnd` takes integers by value, reads no memory, and
    // rejects any argument it cannot rescale with `INT64_MIN`.
    unsafe { ffi::av_rescale_rnd(a, b, c, rnd) }
}

/// Each rounding C accepts, spelled with C's constants, beside the same
/// rounding built through the Rust API.
fn roundings() -> Vec<(ffi::AVRounding, Rounding)> {
    use ffi::AVRounding as C;
    let methods = [
        (C::AV_ROUND_ZERO, Rounding::ZERO),
        (C::AV_ROUND_INF, Rounding::INF),
        (C::AV_ROUND_DOWN, Rounding::DOWN),
        (C::AV_ROUND_UP, Rounding::UP),
        (C::AV_ROUND_NEAR_INF, Rounding::NEAR_INF),
    ];
    let flagged = methods.map(|(c, r)| (C(c.0 | C::AV_ROUND_PASS_MINMAX.0), r.pass_min_max()));
    methods.into_iter().chain(flagged).collect()
}

/// Every `a` sign and magnitude class `av_rescale_rnd` branches on.
const A: [i64; 14] = [
    i64::MIN,
    -i64::MAX,
    -(1 << 40) - 1,
    -7,
    -1,
    0,
    1,
    5,
    7,
    i32::MAX as i64,
    i32::MAX as i64 + 1,
    (1 << 40) + 3,
    i64::MAX - 1,
    i64::MAX,
];

/// `(b, c)` pairs across the 32-bit fast path and the 128-bit slow path.
const BC: [(i64, i64); 8] = [
    (1, 1),
    (1, 2),
    (2, 3),
    (3, 7),
    (1000, 3),
    (i32::MAX as i64, 3),
    (i32::MAX as i64 + 1, 7),
    (3, i64::MAX),
];

#[test]
fn rust_values_are_the_c_values() {
    for (c, r) in roundings() {
        assert_eq!(r.as_raw(), c);
        assert_eq!(ffi::AVRounding::from(r), c);
        assert_eq!(Rounding::try_from(c), Ok(r));
    }
}

#[test]
fn rust_accepts_exactly_what_c_accepts() {
    // Every value up to past the flag, then the high bits.
    let raws = (0..=8192 + 64).chain([
        16384,
        16384 | 3,
        16384 | 8192,
        1 << 31,
        (1 << 31) | 8192 | 3,
        u32::MAX - 8192,
        u32::MAX,
    ]);
    for raw in raws {
        let c_accepts = rescale(1, 1, 1, ffi::AVRounding(raw)) != i64::MIN;
        let rust_accepts = Rounding::try_from(ffi::AVRounding(raw)).is_ok();
        assert_eq!(rust_accepts, c_accepts, "AVRounding({raw:#x})");
        // A rejection with the flag holds even for the values it would pass.
        if !c_accepts {
            assert_eq!(rescale(i64::MAX, 1, 1, ffi::AVRounding(raw)), i64::MIN);
        }
    }
}

#[test]
fn rescales_agree_on_every_input() {
    for (c, r) in roundings() {
        for &(b, cc) in &BC {
            for a in A {
                assert_eq!(
                    rescale(a, b, cc, r.into()),
                    rescale(a, b, cc, c),
                    "{r:?} {a} {b}/{cc}"
                );
            }
        }
    }
}

#[test]
fn chained_rescales_agree_after_every_step() {
    for (c, r) in roundings() {
        let (mut via_c, mut via_rust) = (987_654_321_i64, 987_654_321_i64);
        for &(b, cc) in BC.iter().cycle().take(40) {
            via_c = rescale(via_c, b, cc, c);
            via_rust = rescale(via_rust, b, cc, r.as_raw());
            assert_eq!(via_rust, via_c, "{r:?} after {b}/{cc}");
        }
    }
}

#[test]
fn pass_min_max_is_what_c_passes_through() {
    for (_, r) in roundings() {
        for extreme in [i64::MIN, i64::MAX] {
            let passed = rescale(extreme, 1, 2, r.into()) == extreme;
            assert_eq!(r.passes_min_max(), passed, "{r:?} on {extreme}");
        }
    }
}

#[test]
fn method_rounds_as_c_without_the_flag() {
    for (c, r) in roundings() {
        let method = r.method();
        assert!(!method.passes_min_max());
        for &(b, cc) in &BC {
            // Off the extremes, the flag changes nothing in C.
            for a in &A[1..A.len() - 1] {
                assert_eq!(
                    rescale(*a, b, cc, method.into()),
                    rescale(*a, b, cc, c),
                    "{r:?} {a} {b}/{cc}"
                );
            }
        }
    }
}

#[test]
fn methods_round_halves_as_c_documents() {
    // 5/2 and -5/2 under each method, as in `mathematics.h`.
    let expect = [
        (Rounding::ZERO, 2, -2),
        (Rounding::INF, 3, -3),
        (Rounding::DOWN, 2, -3),
        (Rounding::UP, 3, -2),
        (Rounding::NEAR_INF, 3, -3),
    ];
    for (r, pos, neg) in expect {
        assert_eq!(
            (rescale(5, 1, 2, r.into()), rescale(-5, 1, 2, r.into())),
            (pos, neg),
            "{r:?}"
        );
    }
}

/// `compare_mod` and `bessel_i0` against `av_compare_mod` and `av_bessel_i0`.
mod numeric {
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mathematics::{bessel_i0, compare_mod};

    /// Operands around zero, the wrap point of each width, and the limits.
    const OPERANDS: [u64; 12] = [
        0,
        1,
        2,
        0x7f,
        0x80,
        0xffff,
        0x1_0000,
        0x8000_0000,
        u32::MAX as u64,
        1 << 63,
        u64::MAX - 1,
        u64::MAX,
    ];

    #[test]
    fn compare_mod_matches_c_for_every_modulus_shape() {
        // Every power of two, then zero and moduli that are not powers of two,
        // which C accepts with a defined result.
        let moduli = (0..64).map(|b| 1u64 << b).chain([0, 3, 10, u64::MAX]);
        for m in moduli {
            for a in OPERANDS {
                for b in OPERANDS {
                    // SAFETY: `av_compare_mod` takes and returns integers by
                    // value.
                    let c = unsafe { ffi::av_compare_mod(a, b, m) };
                    assert_eq!(compare_mod(a, b, m), c, "{a:#x} {b:#x} {m:#x}");
                }
            }
        }
    }

    #[test]
    fn bessel_i0_matches_c_bit_for_bit() {
        let mut xs = vec![
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            1e-300,
            15.0,
            15.0 + f64::EPSILON * 16.0,
            700.0,
            710.0,
            1e308,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ];
        // Both approximation intervals, densely.
        xs.extend((0..=400).map(|i| f64::from(i) * 0.1 - 20.0));
        for x in xs {
            // SAFETY: `av_bessel_i0` takes and returns a double by value.
            let c = unsafe { ffi::av_bessel_i0(x) };
            let r = bessel_i0(x);
            assert!(
                r.to_bits() == c.to_bits() || (r.is_nan() && c.is_nan()),
                "{x}: {r} != {c}"
            );
        }
    }
}
