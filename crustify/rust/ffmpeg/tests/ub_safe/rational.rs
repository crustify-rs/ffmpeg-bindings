//! UB tests for `libavutil/rational.h`'s `Rational`, through the safe API
//! only: inline values, `av_malloc`-family owners and arrays of them, every
//! handle form, copies outliving their source, and the asserted `Send` /
//! `Sync`.

use std::thread;

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};
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
