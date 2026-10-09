//! UB tests for `libavutil/rational.h`'s `Rational`, through the safe API
//! only: inline values, `av_malloc`-family owners and arrays of them, every
//! handle form, copies outliving their source, and the asserted `Send` /
//! `Sync`.

use std::thread;

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapBox, HeapVec, alloc_array_from_fn, alloc_cell_array_zeroed};
use ffmpeg::libavutil::rational::{Rational, RationalMut, RationalRef};

/// Pairs at every edge: zero and infinite forms, both signs on either part,
/// and the `int` extremes, whose negation or division would overflow.
const PAIRS: [(i32, i32); 10] = [
    (0, 0),
    (1, 0),
    (-1, 0),
    (0, -1),
    (i32::MIN, -1),
    (-1, i32::MIN),
    (i32::MAX, i32::MIN),
    (i32::MIN, i32::MIN),
    (30000, 1001),
    (-6, 4),
];

fn parts(r: RationalRef<'_>) -> (i32, i32) {
    (r.num(), r.den())
}

/// `num / den` built without a C call, so the tests using it also run under
/// Miri.
fn q(num: i32, den: i32) -> Rational {
    Rational::from_raw(ffi::AVRational { num, den })
}

#[test]
fn every_pair_builds_and_reads_back() {
    for (num, den) in PAIRS {
        let q = Rational::new(num, den);
        assert_eq!(parts(q.as_ref()), (num, den));
        let raw = Rational::from_raw(ffi::AVRational { num, den });
        assert_eq!(parts(raw.as_ref()), (num, den));
        let back: ffi::AVRational = raw.into();
        assert_eq!((back.num, back.den), (num, den));
        assert!(!format!("{q:?}").is_empty());
    }
}

#[test]
fn shared_handles_copy_and_coexist() {
    let v = q(1, 3);
    let a = v.as_ref();
    let b = a;
    let c = v.as_ref();
    assert_eq!((a.num(), b.den(), parts(c)), (1, 3, (1, 3)));
}

fn reset(mut m: RationalMut<'_>) {
    m.set(Rational::zeroed());
}

#[test]
fn exclusive_handle_reborrows_without_aliasing() {
    let mut v = q(9, 9);
    let mut m = v.as_mut();
    reset(m.as_mut());
    assert_eq!(parts(m.as_ref()), (0, 0));
    m.set_num(i32::MIN);
    let shared = m.as_ref();
    assert_eq!(shared.num(), i32::MIN);
    m.set_den(i32::MIN);
    assert_eq!(parts(v.as_ref()), (i32::MIN, i32::MIN));
}

#[test]
fn copies_outlive_and_are_independent_of_their_source() {
    let copy = {
        let mut boxed = HeapBox::<Rational>::new_zeroed().unwrap();
        boxed.as_mut().set(Rational::new(5, 7));
        let copy = boxed.as_ref().get();
        boxed.as_mut().set_num(0);
        copy
        // `boxed` is freed here; `copy` holds its own bytes.
    };
    assert_eq!(parts(copy.as_ref()), (5, 7));
    let mut b = copy;
    b.as_mut().set_den(-1);
    #[allow(clippy::clone_on_copy)] // exercises the explicit `Clone` impl
    let c = Clone::clone(&b);
    assert_eq!((parts(copy.as_ref()), parts(c.as_ref())), ((5, 7), (5, -1)));
}

#[test]
fn heap_box_owner_lives_reads_writes_clones_and_frees() {
    let mut a = HeapBox::<Rational>::new_zeroed().unwrap();
    for (num, den) in PAIRS {
        a.as_mut().set_num(num);
        a.as_mut().set_den(den);
        assert_eq!(parts(a.as_ref()), (num, den));
    }
    let b = a.clone();
    let c = b.try_clone().unwrap();
    // The source may go first; the copies own their own blocks.
    drop(a);
    assert_eq!(parts(b.as_ref()), (-6, 4));
    drop(b);
    assert_eq!(parts(c.as_ref()), (-6, 4));
    drop(HeapBox::<Rational>::new_uninit().unwrap());
}

#[test]
fn heap_array_handles_stay_in_bounds() {
    let mut a = alloc_cell_array_zeroed::<Rational>(PAIRS.len()).unwrap();
    assert!(a.as_handles().iter().all(|h| parts(h) == (0, 0)));
    {
        let mut handles = a.as_handles_mut();
        for (i, (num, den)) in PAIRS.into_iter().enumerate() {
            handles.get_mut(i).unwrap().set(Rational::new(num, den));
        }
        assert!(handles.get_mut(PAIRS.len()).is_none());
    }
    let read: Vec<(i32, i32)> = a.as_handles().iter().map(parts).collect();
    assert_eq!(read, PAIRS);
    assert!(a.as_handles().get(PAIRS.len()).is_none());
    let b = a.clone();
    drop(a);
    assert_eq!(b.as_handles().iter().map(parts).collect::<Vec<_>>(), PAIRS);
    drop(alloc_cell_array_zeroed::<Rational>(0).unwrap());
}

#[test]
fn values_and_owners_move_across_threads() {
    let q = Rational::new(24000, 1001);
    let b = HeapBox::<Rational>::new_zeroed().unwrap();
    let (q, b) = thread::spawn(move || {
        let mut b = b;
        b.as_mut().set(q);
        (q, b)
    })
    .join()
    .unwrap();
    assert_eq!(parts(b.as_ref()), parts(q.as_ref()));
    // Freed on another thread than the one that allocated it.
    thread::spawn(move || drop(b)).join().unwrap();
}

#[test]
fn shared_handles_read_concurrently() {
    let q = Rational::new(1, 90000);
    let boxed = HeapBox::<Rational>::new_zeroed().unwrap();
    let (qr, br) = (q.as_ref(), boxed.as_ref());
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || {
                for _ in 0..1000 {
                    assert_eq!(parts(qr), (1, 90000));
                    assert_eq!(parts(br.get().as_ref()), (0, 0));
                }
            });
        }
    });
}

#[test]
fn exclusive_handles_write_on_other_threads() {
    let mut values = [Rational::zeroed(); 4];
    let mut boxed = HeapBox::<Rational>::new_zeroed().unwrap();
    thread::scope(|s| {
        for (i, v) in values.iter_mut().enumerate() {
            let mut m = v.as_mut();
            s.spawn(move || m.set_num(i as i32));
        }
        let mut m = boxed.as_mut();
        s.spawn(move || m.set_den(i32::MIN));
    });
    let nums: Vec<i32> = values.iter().map(|v| v.as_ref().num()).collect();
    assert_eq!(nums, [0, 1, 2, 3]);
    assert_eq!(parts(boxed.as_ref()), (0, i32::MIN));
}

#[test]
fn a_shared_exclusive_handle_only_reads() {
    let mut v = q(3, 4);
    let m = v.as_mut();
    let m = &m;
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(move || assert_eq!(parts(m.as_ref()), (3, 4)));
        }
    });
}

#[test]
fn inline_handles_cross_threads() {
    let mut values = [q(0, 0); 4];
    thread::scope(|s| {
        for (i, v) in values.iter_mut().enumerate() {
            let mut m = v.as_mut();
            s.spawn(move || {
                m.set(q(i as i32, -(i as i32)));
                m.set_den(i32::MIN);
            });
        }
    });
    let shared: Vec<RationalRef<'_>> = values.iter().map(Rational::as_ref).collect();
    thread::scope(|s| {
        for (i, &r) in shared.iter().enumerate() {
            s.spawn(move || assert_eq!(parts(r.get().as_ref()), (i as i32, i32::MIN)));
        }
    });
    let moved = thread::spawn(move || values).join().unwrap();
    assert_eq!(parts(moved[3].as_ref()), (3, i32::MIN));
}

#[test]
fn rust_slices_copy_into_heap_arrays_and_back() {
    let values: Vec<Rational> = PAIRS.iter().map(|&(num, den)| q(num, den)).collect();
    // A Rust slice copied into an `av_malloc` block (`av_memdup`).
    let a = HeapVec::from_slice(&values).unwrap();
    assert_eq!(a.len(), PAIRS.len());
    assert_eq!(a.as_handles().iter().map(parts).collect::<Vec<_>>(), PAIRS);
    // A borrowed run copied element by element into its own block, which
    // outlives the source.
    let run = a.as_handles();
    let b = alloc_array_from_fn(run.len(), |i| run.get(i).unwrap().get()).unwrap();
    drop(a);
    assert_eq!(b.as_handles().iter().map(parts).collect::<Vec<_>>(), PAIRS);
    // Built element by element into uninitialised storage.
    let c = alloc_array_from_fn(PAIRS.len(), |i| Rational::new(PAIRS[i].0, PAIRS[i].1)).unwrap();
    assert_eq!(c.as_handles().iter().map(parts).collect::<Vec<_>>(), PAIRS);
    // Moved to another thread and freed there.
    let (b, c) = thread::spawn(move || (b, c)).join().unwrap();
    assert_eq!(
        parts(b.as_handles().get(9).unwrap()),
        parts(c.as_handles().get(9).unwrap())
    );
    thread::spawn(move || drop((b, c))).join().unwrap();
    // Empty runs: nothing to copy, nothing to free.
    drop(HeapVec::<Rational>::from_slice(&[]).unwrap());
    drop(alloc_array_from_fn(0, |_| Rational::zeroed()).unwrap());
}

#[test]
fn exclusive_array_views_split_and_iterate_without_aliasing() {
    let mut a = alloc_cell_array_zeroed::<Rational>(PAIRS.len()).unwrap();
    let mut all = a.as_handles_mut();
    assert!(all.split_at_mut(PAIRS.len() + 1).is_none());
    assert!(all.slice_mut(..=PAIRS.len()).is_none());
    {
        // Disjoint halves written together, through a reborrow and a copy.
        let (mut lo, mut hi) = all.split_at_mut(4).unwrap();
        for (i, mut h) in lo.iter_mut().enumerate() {
            h.set(q(i as i32, 1));
        }
        for (i, mut h) in hi.iter_mut().enumerate() {
            h.set_den(-(i as i32) - 1);
        }
        hi.get_mut(0).unwrap().set_num(i32::MIN);
        assert_eq!(parts(lo.as_ref().get(3).unwrap()), (3, 1));
        assert_eq!(parts(hi.as_ref().get(0).unwrap()), (i32::MIN, -1));
    }
    {
        let mut tail = all.slice_mut(8..).unwrap();
        assert_eq!(tail.as_ref().iter().count(), 2);
        tail.as_mut().get_mut(1).unwrap().set_num(7);
        tail.into_mut(0).unwrap().set(q(5, 5));
    }
    let last = all.into_mut(PAIRS.len() - 1).unwrap();
    assert_eq!(parts(last.as_ref()), (7, -6));
    let read: Vec<(i32, i32)> = a.as_handles().iter().map(parts).collect();
    assert_eq!(
        read,
        [
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1),
            (i32::MIN, -1),
            (0, -2),
            (0, -3),
            (0, -4),
            (5, 5),
            (7, -6),
        ]
    );
}

// ---- av_reduce ----

/// UB tests of `libavutil/rational.h`'s [`reduce`]: the arguments that
/// overflow in C (`INT64_MIN` terms, negative or out-of-`int` bounds) are
/// refused before the call, and every other extreme reduces without
/// overflow, across threads.
mod reduce {
    use ffmpeg::libavutil::rational::reduce;

    const TERMS: [i64; 11] = [
        i64::MIN,
        i64::MIN + 1,
        -(1 << 31),
        -1,
        0,
        1,
        i32::MAX as i64,
        1 << 31,
        1 << 32,
        i64::MAX - 1,
        i64::MAX,
    ];

    const BOUNDS: [i32; 7] = [i32::MIN, -1, 0, 1, 1 << 16, i32::MAX - 1, i32::MAX];

    #[test]
    fn every_extreme_is_refused_or_reduced_in_range() {
        for num in TERMS {
            for den in TERMS {
                for max in BOUNDS {
                    let r = reduce(num, den, max);
                    let refused = max < 0 || num == i64::MIN || den == i64::MIN;
                    assert_eq!(r.is_none(), refused, "{num}/{den} within {max}");
                    if let Some(r) = r {
                        // A zero bound still yields terms of up to 1, as in C.
                        let bound = max.max(1);
                        assert!(
                            r.value.as_ref().num().unsigned_abs() <= bound as u32
                                && r.value.as_ref().den() <= bound,
                            "{r:?}"
                        );
                        assert!(r.value.as_ref().den() >= 0);
                    }
                }
            }
        }
    }

    #[test]
    fn concurrent_reductions_do_not_race() {
        std::thread::scope(|scope| {
            for t in 0..8i64 {
                scope.spawn(move || {
                    for i in 1..2000 {
                        let r = reduce(i * 7 + t, i64::MAX - i, i32::MAX).unwrap();
                        assert!(r.value.as_ref().den() >= 0);
                    }
                });
            }
        });
    }
}

/// Arithmetic, comparison and conversion (core wave 1, batch 2) on every edge
/// pair and on pseudo-random ones, including those C overflows on unless the
/// wrapper steps around them, here and on other threads.
mod arithmetic {
    use core::cmp::Ordering;
    use std::thread;

    use ffmpeg::libavutil::rational::Rational;

    /// Each part's edges: zero, ones, the extremes and their neighbours.
    const PARTS: [i32; 12] = [
        0,
        1,
        -1,
        2,
        3,
        1001,
        i32::MAX,
        i32::MAX - 1,
        i32::MIN,
        i32::MIN + 1,
        1 << 30,
        -(1 << 30),
    ];

    fn edges() -> Vec<Rational> {
        PARTS
            .iter()
            .flat_map(|&n| PARTS.iter().map(move |&d| Rational::new(n, d)))
            .collect()
    }

    /// A deterministic xorshift stream of rationals, half of them edges.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn part(&mut self) -> i32 {
            match self.next() % 3 {
                0 => PARTS[(self.next() % PARTS.len() as u64) as usize],
                1 => self.next() as i32,
                _ => (self.next() % 2001) as i32 - 1000,
            }
        }

        fn q(&mut self) -> Rational {
            Rational::new(self.part(), self.part())
        }
    }

    #[test]
    fn every_edge_pair_computes() {
        let all = edges();
        let mut sum = 0i64;
        for &a in &all {
            for &b in &all {
                for r in [a + b, a - b, a * b, a / b] {
                    sum += i64::from(r.as_ref().num() ^ r.as_ref().den());
                }
                let _ = a.partial_cmp(&b);
                assert_eq!(a == b, b == a);
                let _ = a.gcd(b, i32::MAX);
                let _ = a.gcd(b, i32::MIN);
            }
        }
        assert_ne!(sum, 0);
    }

    #[test]
    fn every_edge_converts() {
        for a in edges() {
            let _ = a.recip();
            let _ = a.to_f64();
            if let Some(bits) = a.to_f32_bits() {
                // The value is never subnormal or out of `f32`'s range.
                let f = f32::from_bits(bits);
                assert!(f.is_nan() || f.is_infinite() || f == 0.0 || f.is_normal());
            }
            for max in [0, 1, i32::MAX] {
                let _ = Rational::from_f64(a.to_f64(), max);
            }
            assert!(Rational::from_f64(a.to_f64(), -1).is_none());
        }
        for d in [
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            5e-324,
            2_147_483_650.0,
            -2_147_483_650.999,
        ] {
            let _ = Rational::from_f64(d, i32::MAX);
        }
    }

    #[test]
    fn random_triples_compare_distances() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let all = edges();
        for i in 0..20_000 {
            let (q, a, b) = (rng.q(), rng.q(), rng.q());
            let _ = q.cmp_distance(a, b);
            let e = all[i % all.len()];
            let _ = e.cmp_distance(e, e);
            let _ = q.cmp_distance(e, a);
        }
    }

    #[test]
    fn random_lists_find_the_nearest() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for _ in 0..5_000 {
            let len = (rng.next() % 9) as usize;
            let list: Vec<Rational> = (0..len).map(|_| rng.q()).collect();
            let q = rng.q();
            if let Some(i) = q.nearest(&list) {
                assert!(i < list.len());
                assert!(list[..=i].iter().all(|c| c.as_ref().den() != 0));
            }
        }
        let min = Rational::new(1, i32::MIN);
        assert!(Rational::new(1, 2).nearest(&[min; 64]).is_none());
        assert!(Rational::new(1, 2).nearest(&[]).is_none());
    }

    #[test]
    fn overflowing_operands_are_stepped_around() {
        let m = Rational::new(i32::MIN, i32::MIN);
        let two = m + m;
        assert_eq!((two.as_ref().num(), two.as_ref().den()), (2, 1));
        let z = Rational::new(0, 0);
        for q in edges() {
            assert_eq!(q.cmp_distance(Rational::new(1, 2), z), None);
            assert_eq!(
                q.cmp_distance(Rational::new(1, i32::MIN), Rational::new(-1, i32::MIN)),
                None
            );
        }
        assert_eq!(
            Rational::new(-1, 2).to_f32_bits(),
            Some((-0.5f32).to_bits())
        );
        assert_eq!(Rational::new(1, 0).gcd(Rational::new(0, 0), i32::MAX), None);
        assert_eq!(z.partial_cmp(&z), None::<Ordering>);
    }

    #[test]
    fn operations_run_on_many_threads_at_once() {
        let workers: Vec<_> = (0..8u64)
            .map(|t| {
                thread::spawn(move || {
                    let mut rng = Rng(0x1234_5678 + t);
                    let mut acc = Rational::new(0, 1);
                    for _ in 0..2_000 {
                        let (a, b) = (rng.q(), rng.q());
                        acc = acc + a * b - a / b;
                        let _ = acc.cmp_distance(a, b);
                        let _ = acc.to_f32_bits();
                        let _ = acc.nearest(&[a, b]);
                    }
                    acc.to_f64()
                })
            })
            .collect();
        for w in workers {
            let _ = w.join().unwrap();
        }
    }
}
