//! UB tests of `libavutil/mathematics.h`'s safe [`Rounding`] API: every
//! `u32` edge through the conversions, the flag algebra and the formatting,
//! and the values shared across threads.

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mathematics::{InvalidRounding, Rounding};

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
        match Rounding::try_from(ffi::AVRounding(raw)) {
            Ok(r) => {
                assert_eq!(r.as_raw().0, raw);
                assert_eq!(Rounding::from_raw(r.into()), Some(r));
                assert!(METHODS.contains(&r.method()));
                assert_eq!(r.passes_min_max(), raw & 8192 != 0);
                assert!(!format!("{r:?}").is_empty());
            }
            Err(e) => {
                assert_eq!(e, InvalidRounding(raw));
                assert!(Rounding::from_raw(ffi::AVRounding(raw)).is_none());
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
            .map(|_| s.spawn(move || shared.iter().map(|r| r.method().as_raw().0).sum::<u32>()))
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
    use ffmpeg::libavutil::mathematics::{bessel_i0, compare_mod};

    #[test]
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
