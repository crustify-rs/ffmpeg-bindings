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

/// `av_gcd` against the safe `gcd`.
mod greatest_common_divisor {
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mathematics::gcd;

    /// Operands around `av_gcd`'s branches: zero, signs, powers of two (the
    /// trailing-zero shift), odd primes and composites, and the extremes.
    const GCD: [i64; 20] = [
        0,
        1,
        -1,
        2,
        -2,
        6,
        -9,
        12,
        17,
        1 << 20,
        3 << 40,
        -(5 << 33),
        1_000_000_007,
        600_851_475_143,
        i64::MAX,
        i64::MAX - 1,
        i64::MIN,
        i64::MIN + 1,
        i64::MIN / 2,
        -(1 << 62),
    ];

    #[test]
    fn gcd_matches_av_gcd_on_every_pair() {
        for a in GCD {
            for b in GCD {
                // SAFETY: `av_gcd` takes integers by value and reads no memory.
                let c = unsafe { ffi::av_gcd(a, b) };
                assert_eq!(gcd(a, b), c, "gcd({a}, {b})");
            }
        }
    }

    #[test]
    fn chained_gcds_agree_after_every_step() {
        let (mut rust, mut c) = (0i64, 0i64);
        for x in GCD.iter().rev().chain(&[360, 84, 18, 7]) {
            rust = gcd(rust, *x);
            // SAFETY: as above.
            c = unsafe { ffi::av_gcd(c, *x) };
            assert_eq!(rust, c, "after {x}");
        }
    }
}

// ---- av_rescale ----

#[test]
fn rescale_matches_av_rescale_on_every_class() {
    use ffmpeg::libavutil::mathematics::rescale as rescale_near;
    let bc = BC.into_iter().chain([
        (0, 1),
        (-1, 3),
        (5, 0),
        (5, -3),
        (i64::MAX, i64::MAX),
        (i64::MAX, 1),
    ]);
    for (b, c) in bc {
        for a in A {
            // SAFETY: `av_rescale` takes integers by value and reads no memory.
            let cr = unsafe { ffi::av_rescale(a, b, c) };
            let r = rescale_near(a, b, c);
            assert_eq!(r.unwrap_or(i64::MIN), cr, "{a} * {b} / {c}");
            // `None` is exactly C's rejection, and agrees with the general form.
            assert_eq!(r.is_none(), cr == i64::MIN);
            assert_eq!(
                cr,
                rescale(a, b, c, Rounding::NEAR_INF.into()),
                "{a} * {b} / {c}"
            );
        }
    }
}

/// Rescaling and timestamp arithmetic (core wave 1, batch 2): each safe
/// function against the C function on the same arguments, and the stateful
/// ones over call sequences compared after every step.
mod timestamps {
    use core::cmp::Ordering;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mathematics::{
        Rounding, add_stable, compare_timestamps, rescale_delta, rescale_rational,
        rescale_rational_rounded, rescale_rounded,
    };
    use ffmpeg::libavutil::rational::Rational;

    use super::{A, BC, rescale, roundings};

    /// Time bases from the usual to the degenerate: zero and negative parts,
    /// and the `int` extremes.
    const TBS: [(i32, i32); 12] = [
        (1, 1),
        (1, 1000),
        (1, 90000),
        (1, 44100),
        (1001, 30000),
        (1, 1_000_000),
        (3, 7),
        (0, 1),
        (1, 0),
        (-1, 25),
        (i32::MAX, 1),
        (1, i32::MAX),
    ];

    fn q(tb: (i32, i32)) -> Rational {
        Rational::new(tb.0, tb.1)
    }

    fn c(tb: (i32, i32)) -> ffi::AVRational {
        ffi::AVRational {
            num: tb.0,
            den: tb.1,
        }
    }

    /// What a safe rescale should say of C's result `r` for `a`: the value,
    /// or `None` for C's rejection, which is `INT64_MIN` except as the
    /// pass-through of `a == INT64_MIN`.
    fn expected(r: i64, a: i64, rnd: ffi::AVRounding, accepted: bool) -> Option<i64> {
        let pass = rnd.0 & ffi::AVRounding::AV_ROUND_PASS_MINMAX.0 != 0;
        (r != i64::MIN || (a == i64::MIN && pass && accepted)).then_some(r)
    }

    #[test]
    fn rescale_rounded_is_av_rescale_rnd() {
        let more_bc = BC
            .into_iter()
            .chain([(0, 1), (1, 0), (-1, 1), (1, -1), (i64::MAX, 1)]);
        for (b, c) in more_bc {
            for a in A {
                for (raw, rounding) in roundings() {
                    let r = rescale(a, b, c, raw);
                    let want = expected(r, a, raw, b >= 0 && c > 0);
                    assert_eq!(
                        rescale_rounded(a, b, c, rounding),
                        want,
                        "{a} {b} {c} {rounding:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn rescale_rational_is_av_rescale_q() {
        for bq in TBS {
            for cq in TBS {
                for a in A {
                    // SAFETY: both take integers and structs by value and
                    // handle every argument.
                    let r = unsafe { ffi::av_rescale_q(a, c(bq), c(cq)) };
                    assert_eq!(
                        rescale_rational(a, q(bq), q(cq)),
                        (r != i64::MIN).then_some(r),
                        "{a} {bq:?} {cq:?}"
                    );
                    let b = i64::from(bq.0) * i64::from(cq.1);
                    let d = i64::from(cq.0) * i64::from(bq.1);
                    for (raw, rounding) in roundings() {
                        // SAFETY: as above.
                        let r = unsafe { ffi::av_rescale_q_rnd(a, c(bq), c(cq), raw) };
                        assert_eq!(
                            rescale_rational_rounded(a, q(bq), q(cq), rounding),
                            expected(r, a, raw, b >= 0 && d > 0),
                            "{a} {bq:?} {cq:?} {rounding:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn compare_timestamps_is_av_compare_ts() {
        for ta in TBS {
            for tb in TBS {
                for a in A {
                    for b in [0, 1, -1, 90_000, i64::MAX, i64::MIN, a] {
                        // SAFETY: `av_compare_ts` takes its arguments by value
                        // and handles every one.
                        let r = unsafe { ffi::av_compare_ts(a, c(ta), b, c(tb)) };
                        assert_eq!(compare_timestamps(a, q(ta), b, q(tb)), r.cmp(&0));
                    }
                }
            }
        }
        assert_eq!(
            compare_timestamps(1, q((1, 1000)), 1000, q((1, 1_000_000))),
            Ordering::Equal
        );
    }

    /// One audio stream: input and sample time bases, packet stamps with
    /// jitter, durations, and output time base.
    struct Stream {
        in_tb: (i32, i32),
        fs_tb: (i32, i32),
        out_tb: (i32, i32),
        stamps: Vec<i64>,
        durations: Vec<i32>,
    }

    fn streams() -> Vec<Stream> {
        let jitter = [0, 1, -1, 0, 2, -2, 0, 0, 1, 5, -40, 0];
        vec![
            Stream {
                in_tb: (1, 1000),
                fs_tb: (1, 44100),
                out_tb: (1, 44100),
                stamps: (0..48)
                    .map(|i| i * 1024 * 1000 / 44100 + jitter[i as usize % 12])
                    .collect(),
                durations: vec![1024; 48],
            },
            Stream {
                in_tb: (1, 90000),
                fs_tb: (1, 48000),
                out_tb: (1, 48000),
                stamps: (0..48)
                    .map(|i| i * 1920 + jitter[i as usize % 12] * 7)
                    .collect(),
                durations: (0..48).map(|i| if i % 5 == 0 { 0 } else { 1024 }).collect(),
            },
            Stream {
                in_tb: (1, 25),
                fs_tb: (1, 48000),
                out_tb: (1, 1000),
                stamps: (0..48).map(|i| i + jitter[i as usize % 12] / 2).collect(),
                durations: vec![1920; 48],
            },
            Stream {
                in_tb: (1, 48000),
                fs_tb: (1, 48000),
                out_tb: (1, 1000),
                stamps: (0..48).map(|i| -(1 << 40) + i * 960).collect(),
                durations: vec![960; 48],
            },
            Stream {
                in_tb: (1, 1),
                fs_tb: (1, 4),
                out_tb: (1, 4),
                stamps: (0..48)
                    .map(|i| (1 << 58) + i * 3 + jitter[i as usize % 12] / 2)
                    .collect(),
                durations: vec![12; 48],
            },
        ]
    }

    #[test]
    fn rescale_delta_tracks_av_rescale_delta_over_streams() {
        for (n, s) in streams().into_iter().enumerate() {
            let (mut mine, mut theirs) = (i64::MIN, i64::MIN);
            for (i, (&ts, &dur)) in s.stamps.iter().zip(&s.durations).enumerate() {
                let r = rescale_delta(q(s.in_tb), ts, q(s.fs_tb), dur, &mut mine, q(s.out_tb));
                // SAFETY: `theirs` is a live state slot; the streams keep
                // every stamp off `AV_NOPTS_VALUE`, every duration
                // non-negative, and every step off C's overflows, which the
                // safe call above confirms by answering.
                let c = unsafe {
                    assert!(r.is_some(), "stream {n} step {i}");
                    ffi::av_rescale_delta(c(s.in_tb), ts, c(s.fs_tb), dur, &mut theirs, c(s.out_tb))
                };
                assert_eq!(r, Some(c), "stream {n} step {i}");
                assert_eq!(mine, theirs, "stream {n} step {i}");
            }
        }
    }

    #[test]
    fn unrepresentable_results_update_the_state_as_c_does() {
        // `2^61` ms is beyond `i64` in 1/90000 units, while the state in
        // seconds is not.
        let (ms, s, out) = ((1, 1000), (1, 1), (1, 90000));
        let (mut mine, mut theirs) = (i64::MIN, i64::MIN);
        for i in 0..4 {
            let ts = (1 << 61) + i * 1000;
            let r = rescale_delta(q(ms), ts, q(s), 1, &mut mine, q(out));
            // SAFETY: `theirs` is a live state slot; the stamp is not
            // `AV_NOPTS_VALUE`, the duration is non-negative, and C's state
            // update, about `2^61 / 1000 + 1`, stays far from overflowing.
            let c = unsafe { ffi::av_rescale_delta(c(ms), ts, c(s), 1, &mut theirs, c(out)) };
            assert_eq!(c, i64::MIN, "step {i}");
            assert_eq!((r, mine), (None, theirs), "step {i}");
        }
    }

    #[test]
    fn rescale_delta_restarts_from_a_reset_state() {
        let s = &streams()[0];
        let (mut mine, mut theirs) = (i64::MIN, i64::MIN);
        for (i, &ts) in s.stamps.iter().enumerate() {
            if i % 7 == 0 {
                (mine, theirs) = (i64::MIN, i64::MIN);
            }
            let r = rescale_delta(q(s.in_tb), ts, q(s.fs_tb), 1024, &mut mine, q(s.out_tb));
            // SAFETY: as in `rescale_delta_tracks_av_rescale_delta_over_streams`.
            let c = unsafe {
                ffi::av_rescale_delta(c(s.in_tb), ts, c(s.fs_tb), 1024, &mut theirs, c(s.out_tb))
            };
            assert_eq!((r, mine), (Some(c), theirs), "step {i}");
        }
    }

    #[test]
    fn add_stable_tracks_av_add_stable_over_sequences() {
        let cases = [
            ((1, 1000), (1, 44100), 1024),
            ((1, 90000), (1, 48000), 1),
            ((1, 90000), (1, 48000), 960),
            ((1, 25), (1, 1000), 33),
            ((1001, 30000), (1, 90000), 3003),
            ((1, 1), (1, 3), 1),
            ((1, 3), (1, 1), 2),
            ((1, 1000), (1, 1000), 7),
        ];
        for (ts_tb, inc_tb, inc) in cases {
            for start in [0, -5000, 1 << 40, i64::MAX - 100] {
                let (mut mine, mut theirs) = (start, start);
                for i in 0..200 {
                    let r = add_stable(q(ts_tb), mine, q(inc_tb), inc).unwrap();
                    // SAFETY: `av_add_stable` takes its arguments by value; the
                    // increments are positive and within `int`, and both time
                    // bases have nonzero parts, which keeps it off its
                    // overflows.
                    theirs = unsafe { ffi::av_add_stable(c(ts_tb), theirs, c(inc_tb), inc) };
                    mine = r;
                    assert_eq!(
                        mine, theirs,
                        "{ts_tb:?} {inc_tb:?} {inc} from {start}, step {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn add_stable_agrees_on_c_s_defined_edges() {
        for ts_tb in TBS {
            for inc_tb in TBS {
                for inc in [1, 2, -1, -3, 0, i64::from(i32::MAX), i64::from(i32::MIN)] {
                    for ts in A {
                        let Some(r) = add_stable(q(ts_tb), ts, q(inc_tb), inc) else {
                            continue;
                        };
                        // SAFETY: the safe call answered, so the arguments
                        // keep C off its overflows and divisions by zero.
                        let c = unsafe { ffi::av_add_stable(c(ts_tb), ts, c(inc_tb), inc) };
                        assert_eq!(r, c, "{ts_tb:?} {ts} {inc_tb:?} {inc}");
                    }
                }
            }
        }
    }

    #[test]
    fn pass_through_survives_a_round_trip() {
        let pass = Rounding::NEAR_INF.pass_min_max();
        let there = rescale_rational_rounded(i64::MIN, q((1, 1000)), q((1, 90000)), pass);
        assert_eq!(there, Some(i64::MIN));
        let back = rescale_rational_rounded(there.unwrap(), q((1, 90000)), q((1, 1000)), pass);
        // SAFETY: `av_rescale_q_rnd` takes its arguments by value.
        let c = unsafe {
            let raw = ffi::AVRounding(
                ffi::AVRounding::AV_ROUND_NEAR_INF.0 | ffi::AVRounding::AV_ROUND_PASS_MINMAX.0,
            );
            ffi::av_rescale_q_rnd(
                ffi::av_rescale_q_rnd(i64::MIN, c((1, 1000)), c((1, 90000)), raw),
                c((1, 90000)),
                c((1, 1000)),
                raw,
            )
        };
        assert_eq!(back, Some(c));
    }
}
