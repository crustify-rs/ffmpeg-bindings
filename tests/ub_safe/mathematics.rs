//! UB tests of `libavutil/mathematics.h`'s safe [`Rounding`] API: every
//! `u32` edge through the conversions, the flag algebra and the formatting,
//! and the values shared across threads.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]

use ffmpeg_bindings::libavutil::libavutil::mathematics::{InvalidRounding, Rounding};

const METHODS: [Rounding; 5] = [
    Rounding::ZERO,
    Rounding::INF,
    Rounding::DOWN,
    Rounding::UP,
    Rounding::NEAR_INF,
];

/// Values around every bit the validity check masks or keeps.
fn edge_values() -> impl Iterator<Item = u32> {
    (0..=16)
        .chain((8192 - 2)..=(8192 + 16))
        .chain((0..32).map(|bit| 1 << bit))
        .chain((0..32).map(|bit| u32::MAX >> bit))
        .chain([!8192, (1 << 31) | 8192 | 5])
}

#[test]
fn conversions_never_panic_and_round_trip() {
    for raw in edge_values() {
        match Rounding::try_from(raw) {
            Ok(r) => {
                assert_eq!(r.as_raw(), raw);
                assert_eq!(Rounding::from_raw(r.into()), Some(r));
                assert!(METHODS.contains(&r.method()));
                assert_eq!(r.passes_min_max(), raw & 8192 != 0);
                assert!(!format!("{r:?}").is_empty());
            }
            Err(e) => {
                assert_eq!(e, InvalidRounding(raw));
                assert!(Rounding::from_raw(raw).is_none());
                assert!(e.to_string().contains("invalid AVRounding"));
            }
        }
    }
}

#[test]
fn flag_algebra_stays_valid() {
    for r in METHODS {
        let chain = [
            r,
            r.pass_min_max(),
            r.pass_min_max().method(),
            r.method().pass_min_max(),
        ];
        for x in chain {
            assert_eq!(Rounding::from_raw(x.as_raw()), Some(x));
        }
    }
}

#[test]
fn roundings_cross_threads() {
    let all: Vec<Rounding> = METHODS
        .iter()
        .flat_map(|&r| [r, r.pass_min_max()])
        .collect();
    std::thread::scope(|s| {
        let shared = &all;
        let workers: Vec<_> = (0..4)
            .map(|_| s.spawn(move || shared.iter().map(|r| r.method().as_raw()).sum::<u32>()))
            .collect();
        for w in workers {
            assert_eq!(w.join().unwrap(), 2 * (1 + 2 + 3 + 5));
        }
    });
    let moved = std::thread::spawn(move || all.into_iter().filter(|r| r.passes_min_max()).count());
    assert_eq!(moved.join().unwrap(), 5);
}

/// `compare_mod` and `bessel_i0` on every edge, here and on other threads.
mod numeric {
    use ffmpeg_bindings::libavutil::libavutil::mathematics::{bessel_i0, compare_mod};

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn compare_mod_is_defined_for_every_modulus() {
        let edges = [0, 1, 2, 3, 1 << 31, 1 << 63, u64::MAX - 1, u64::MAX];
        for m in (0..64).map(|b| 1u64 << b).chain(edges) {
            for a in edges {
                for b in edges {
                    let c = compare_mod(a, b, m);
                    if m.is_power_of_two() && m > 1 {
                        // Within `1 - m / 2..=m / 2`, compared as `i128`.
                        let half = i128::from(m / 2);
                        assert!((1 - half..=half).contains(&i128::from(c)));
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bessel_i0_is_defined_on_every_float_class() {
        for x in [
            0.0,
            -0.0,
            f64::MIN_POSITIVE,
            15.0,
            -15.0,
            710.0,
            f64::MAX,
            f64::MIN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let y = bessel_i0(x);
            assert!(y.is_nan() || y >= 1.0, "{x}: {y}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn numeric_functions_run_concurrently() {
        std::thread::scope(|s| {
            for t in 0..4u32 {
                s.spawn(move || {
                    for i in 0..5_000u32 {
                        let x = f64::from(i) * 0.01 + f64::from(t);
                        assert!(bessel_i0(x) >= 1.0);
                        let _ = compare_mod(u64::from(i), u64::from(t), 1 << (i % 64));
                    }
                });
            }
        });
    }
}

/// `gcd` on every sign and extreme.
mod greatest_common_divisor {
    use ffmpeg_bindings::libavutil::libavutil::mathematics::gcd;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn gcd_takes_every_sign_and_extreme() {
        let edges = [
            0,
            1,
            -1,
            2,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            1 << 62,
            -(1 << 62),
            3 << 61,
        ];
        for a in edges {
            for b in edges {
                let g = gcd(a, b);
                if g != 0 && g != i64::MIN {
                    assert_eq!((a % g, b % g), (0, 0), "gcd({a}, {b}) = {g}");
                }
            }
        }
        assert_eq!(gcd(i64::MIN, i64::MIN), i64::MIN);
        assert_eq!(gcd(0, 0), 0);
    }
}

// ---- av_rescale ----

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn rescale_every_extreme_is_sound() {
    use ffmpeg_bindings::libavutil::libavutil::mathematics::rescale;
    let v = [
        i64::MIN,
        i64::MIN + 1,
        -(1 << 40),
        -1,
        0,
        1,
        i64::from(i32::MAX),
        1 << 40,
        i64::MAX,
    ];
    for a in v {
        for b in v {
            for c in v {
                if let Some(r) = rescale(a, b, c) {
                    assert_ne!(r, i64::MIN);
                }
            }
        }
    }
    let r = std::thread::spawn(|| rescale(i64::MAX, i64::MAX, 3))
        .join()
        .unwrap();
    assert_eq!(r, None);
}

/// Rescaling and timestamp arithmetic (core wave 1, batch 2) on every edge
/// and on pseudo-random arguments, including those C overflows on, divides by
/// zero on or aborts on unless the wrapper rejects them first, here and on
/// other threads.
mod timestamps {
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::mathematics::{
        Rounding, add_stable, compare_timestamps, rescale_delta, rescale_rational,
        rescale_rational_rounded, rescale_rounded,
    };
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

    use super::METHODS;

    const I64: [i64; 16] = [
        0,
        1,
        -1,
        2,
        i64::MAX,
        i64::MIN,
        i64::MAX - 1,
        i64::MIN + 1,
        1 << 62,
        -(1 << 62),
        (1 << 62) - 1,
        -(1 << 62) + 1,
        i32::MAX as i64,
        i32::MIN as i64,
        i32::MAX as i64 + 1,
        90_000,
    ];

    const PARTS: [i32; 9] = [
        0,
        1,
        -1,
        1000,
        44_100,
        i32::MAX,
        i32::MIN,
        i32::MIN + 1,
        1001,
    ];

    fn time_bases() -> Vec<Rational> {
        PARTS
            .iter()
            .flat_map(|&n| PARTS.iter().map(move |&d| Rational::new(n, d)))
            .collect()
    }

    fn roundings() -> Vec<Rounding> {
        METHODS
            .iter()
            .flat_map(|&r| [r, r.pass_min_max()])
            .collect()
    }

    /// A deterministic xorshift stream.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn pick<T: Copy>(&mut self, from: &[T]) -> T {
            from[(self.next() % from.len() as u64) as usize]
        }

        fn i64(&mut self) -> i64 {
            match self.next() % 3 {
                0 => self.pick(&I64),
                1 => self.next() as i64,
                _ => (self.next() % 200_001) as i64 - 100_000,
            }
        }

        fn tb(&mut self) -> Rational {
            match self.next() % 3 {
                0 => Rational::new(self.pick(&PARTS), self.pick(&PARTS)),
                1 => Rational::new(self.next() as i32, self.next() as i32),
                _ => Rational::new(
                    1 + (self.next() % 1000) as i32,
                    1 + (self.next() % 100_000) as i32,
                ),
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_edge_rescales() {
        for a in I64 {
            for b in I64 {
                for c in I64 {
                    for r in roundings() {
                        let _ = rescale_rounded(a, b, c, r);
                    }
                }
            }
        }
        let tbs = time_bases();
        for a in I64 {
            for &bq in &tbs {
                for &cq in &tbs {
                    let _ = rescale_rational(a, bq, cq);
                    let _ = rescale_rational_rounded(a, bq, cq, Rounding::UP.pass_min_max());
                    let _ = compare_timestamps(a, bq, a.wrapping_neg(), cq);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_delta_rescales_keep_their_state() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let durations = [0, 1, 1024, i32::MAX, -1, i32::MIN];
        for _ in 0..30_000 {
            let mut last = match rng.next() % 3 {
                0 => i64::MIN,
                1 => rng.pick(&I64),
                _ => rng.i64(),
            };
            let before = last;
            let (in_tb, fs_tb, out_tb) = (rng.tb(), rng.tb(), rng.tb());
            let (ts, duration) = (rng.i64(), rng.pick(&durations));
            let r = rescale_delta(in_tb, ts, fs_tb, duration, &mut last, out_tb);
            if ts == i64::MIN || duration < 0 {
                assert_eq!((r, last), (None, before));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn delta_rescales_of_one_stream_at_the_edges() {
        let fine = Rational::new(1, 48_000);
        let coarse = Rational::new(1, 25);
        for start in I64 {
            let mut last = start;
            for ts in I64 {
                for duration in [0, 1, 1920, i32::MAX] {
                    let _ = rescale_delta(coarse, ts, fine, duration, &mut last, fine);
                    let _ = rescale_delta(fine, ts, coarse, duration, &mut last, coarse);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn steps_c_overflows_on_are_rejected() {
        // Each of these overflows an `int64_t` addition or subtraction inside
        // `av_rescale_delta`; the wrapper answers `None` without calling it.
        let (one, half, fine) = (
            Rational::new(1, 1),
            Rational::new(1, 2),
            Rational::new(1, 1 << 20),
        );
        let cases = [
            (one, 1i64 << 42, fine, 1, fine),
            (one, (1 << 62) - 1, one, 1, half),
            (one, (1 << 62) - 1, half, 2, half),
            (one, 1 << 62, one, 1, half),
            (one, -(1 << 62), one, 1, half),
        ];
        for (in_tb, ts, fs_tb, duration, out_tb) in cases {
            let mut last = 0;
            assert_eq!(
                rescale_delta(in_tb, ts, fs_tb, duration, &mut last, out_tb),
                None
            );
            assert_eq!(last, 0);
        }
        let mut last = i64::MIN;
        assert_eq!(rescale_delta(one, i64::MAX, one, 1, &mut last, one), None);
        assert_eq!(last, i64::MIN);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_stable_additions() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let incs = [
            1,
            0,
            -1,
            2,
            1024,
            i64::from(i32::MAX),
            i64::from(i32::MIN),
            i64::MAX,
            i64::MIN,
        ];
        for _ in 0..30_000 {
            let inc = if rng.next().is_multiple_of(2) {
                rng.pick(&incs)
            } else {
                rng.i64()
            };
            let _ = add_stable(rng.tb(), rng.i64(), rng.tb(), inc);
        }
        let tbs = time_bases();
        for &ts_tb in &tbs {
            for &inc_tb in &tbs {
                for inc in incs {
                    let _ = add_stable(ts_tb, 1 << 40, inc_tb, inc);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn timestamps_on_many_threads_at_once() {
        let workers: Vec<_> = (0..8u64)
            .map(|t| {
                thread::spawn(move || {
                    let mut rng = Rng(0xDEAD_BEEF + t);
                    let mut last = i64::MIN;
                    let mut ts = 0;
                    let (sr, ms) = (Rational::new(1, 48_000), Rational::new(1, 1000));
                    for i in 0..2_000i64 {
                        let _ = rescale_delta(ms, i * 20, sr, 960, &mut last, sr);
                        ts = add_stable(ms, ts, sr, 960).unwrap_or(ts);
                        let _ = compare_timestamps(rng.i64(), rng.tb(), rng.i64(), rng.tb());
                        let _ = rescale_rational(rng.i64(), rng.tb(), rng.tb());
                    }
                    (last, ts)
                })
            })
            .collect();
        for w in workers {
            let (last, ts) = w.join().unwrap();
            assert_eq!(last, 2_000 * 960);
            assert_eq!(ts, 40_000);
        }
    }
}
