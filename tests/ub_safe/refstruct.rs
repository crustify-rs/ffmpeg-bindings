//! UB tests of `libavutil/refstruct.h`'s safe wrappers.

/// Buffer-frame wave 0, type batch 3: `RefStructOpaque` values and
/// `RefStructPoolArc` owners, across sizes, flags, drop orders and threads.
mod pool_and_opaque {
    use std::collections::HashSet;
    use std::sync::Barrier;
    use std::thread;

    use ffmpeg::libavutil::refstruct::{
        PoolFlags, RefStructOpaque, RefStructPool, RefStructPoolArc,
    };

    const ADDRS: [usize; 6] = [0, 1, 8, 0x1000, usize::MAX >> 1, usize::MAX];

    /// Sizes C only stores until an entry is requested: none of them is
    /// allocated by the pool's constructor.
    const SIZES: [usize; 5] = [0, 1, 64, 1 << 20, usize::MAX];

    fn every_flag_set() -> impl Iterator<Item = PoolFlags> {
        let all = [
            PoolFlags::NO_ZEROING,
            PoolFlags::RESET_ON_INIT_ERROR,
            PoolFlags::FREE_ON_INIT_ERROR,
            PoolFlags::ZERO_EVERY_TIME,
        ];
        (0..16u32).map(move |m| {
            all.iter()
                .enumerate()
                .filter(|(i, _)| m & (1 << i) != 0)
                .fold(PoolFlags::NONE, |f, (_, &x)| f | x)
        })
    }

    #[test]
    fn every_size_and_flag_set_builds_and_releases() {
        for size in SIZES {
            for flags in every_flag_set() {
                let a = RefStructPool::new(size, flags).expect("pool");
                let b = a.clone();
                let _shared = (a.as_ref(), b.as_ref());
                drop(a);
                drop(b);
            }
        }
    }

    /// Every drop order of four references, the constructor's included.
    #[test]
    fn every_drop_order() {
        let orders = [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]];
        for order in orders {
            let first = RefStructPool::new(16, PoolFlags::NONE).expect("pool");
            let mut refs: Vec<Option<RefStructPoolArc>> = vec![
                Some(first.clone()),
                Some(first.clone()),
                Some(first.clone()),
                Some(first),
            ];
            for i in order {
                let gone = refs[i].take();
                assert!(gone.is_some());
                for r in refs.iter().flatten() {
                    let _ = r.as_ref();
                }
            }
        }
    }

    /// The owner never yields its exclusive handle, alone or shared.
    #[test]
    fn owner_never_hands_out_the_exclusive_handle() {
        let mut a = RefStructPool::new(8, PoolFlags::ZERO_EVERY_TIME).expect("pool");
        assert!(a.get_mut().is_none());
        let mut b = a.clone();
        assert!(a.get_mut().is_none());
        assert!(b.get_mut().is_none());
        let b = b
            .try_into_box()
            .expect_err("a pool reference is never sole");
        drop(a);
        assert!(b.try_into_box().is_err());
    }

    /// Clones made, moved and dropped on many threads at once, the first
    /// owner dropped while they run, and shared handles read concurrently.
    #[test]
    fn references_cross_threads() {
        const THREADS: usize = 8;
        let pool = RefStructPool::new(32, PoolFlags::NONE).expect("pool");
        let start = Barrier::new(THREADS + 1);
        thread::scope(|s| {
            for _ in 0..THREADS {
                let mine = pool.clone();
                let start = &start;
                s.spawn(move || {
                    start.wait();
                    let mut held = Vec::new();
                    for round in 0..200 {
                        held.push(mine.clone());
                        if round % 3 == 0 {
                            drop(held.swap_remove(0));
                        }
                    }
                    assert!(held.iter().all(|h| RefStructPoolArc::ptr_eq(h, &mine)));
                    drop(mine);
                    held
                });
            }
            let shared = pool.as_ref();
            s.spawn(move || {
                let _ = shared;
            });
            start.wait();
        });
        drop(pool);
    }

    /// The last reference is dropped on a thread other than the creator's.
    #[test]
    fn last_reference_released_elsewhere() {
        let pool = RefStructPool::new(4, PoolFlags::NO_ZEROING).expect("pool");
        let other = pool.clone();
        drop(pool);
        thread::spawn(move || drop(other)).join().unwrap();
    }

    #[test]
    fn opaque_values_round_trip_and_cross_threads() {
        let mut values: Vec<RefStructOpaque> = Vec::new();
        for a in ADDRS {
            let m = RefStructOpaque::new(std::ptr::without_provenance_mut(a));
            let c = RefStructOpaque::from_const(std::ptr::without_provenance(a));
            assert_eq!(m, c);
            assert_eq!(m.as_ref().mut_ptr(), c.as_ref().mut_ptr());
            let mut v = RefStructOpaque::default();
            {
                let mut h = v.as_mut();
                h.set_mut_ptr(std::ptr::without_provenance_mut(a ^ 1));
                h.set_const_ptr(std::ptr::without_provenance(a));
                assert_eq!(h.as_ref().const_ptr().addr(), a);
            }
            assert_eq!(v, m);
            assert_eq!(RefStructOpaque::from_raw(v.into_raw()), v);
            values.push(v);
        }
        let set: HashSet<RefStructOpaque> = values.iter().copied().collect();
        assert_eq!(set.len(), ADDRS.len());
        let shown = format!("{values:?}");
        let back = thread::spawn(move || {
            assert_eq!(format!("{values:?}"), shown);
            values
        })
        .join()
        .unwrap();
        thread::scope(|s| {
            for v in &back {
                let r = v.as_ref();
                s.spawn(move || r.const_ptr().addr());
            }
        });
    }

    /// The handles' lifetimes keep the value alive and exclusive.
    #[test]
    fn opaque_handles_reborrow() {
        let mut v = RefStructOpaque::new(std::ptr::without_provenance_mut(0x40));
        let mut m = v.as_mut();
        let mut again = m.as_mut();
        again.set_mut_ptr(std::ptr::without_provenance_mut(0x80));
        let shared = m.as_ref();
        assert_eq!(shared.mut_ptr().addr(), 0x80);
        assert_eq!(v.as_ref().const_ptr().addr(), 0x80);
    }
}

/// Buffer-frame wave 0, symbol batch 5: counted references to RefStruct
/// objects through every owner form, the `const` references, slot
/// replacement and cross-thread counting.
mod counted_references {
    use std::sync::{Arc, Barrier};
    use std::thread;

    use ffmpeg::libavutil::csp::{CieXy, ColorPrimariesDescriptor};
    use ffmpeg::libavutil::intfloat::IntFloat64;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::refstruct::{
        ConstRefStructArc, RefStructArc, RefStructBox, into_const, is_exclusive, replace, to_const,
    };

    #[test]
    fn zeroed_payloads_are_written_and_read_end_to_end() {
        let mut a = RefStructArc::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        assert_eq!(a.as_ref().primaries().b().y().den(), 0);
        {
            let mut m = a.get_mut().unwrap();
            // The last field of the 64-byte payload.
            m.primaries_mut()
                .set_b(CieXy::new(Rational::new(1, 2), Rational::new(1, 3)));
            m.white_point_mut().set_x(Rational::new(5, 7));
        }
        assert_eq!(a.as_ref().primaries().b().y().den(), 3);
        assert_eq!(a.as_ref().white_point().x().num(), 5);

        let mut b = RefStructBox::<Rational>::new_zeroed().unwrap();
        assert_eq!((b.as_ref().num(), b.as_ref().den()), (0, 0));
        b.as_mut().set(Rational::new(-3, 4));
        let shared = RefStructArc::from(b);
        assert_eq!(shared.as_ref().den(), 4);
    }

    #[test]
    fn clones_share_one_object_and_outlive_the_first_reference() {
        let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        a.get_mut().unwrap().set_bits(0x0123_4567_89ab_cdef);
        let clones: Vec<_> = (0..16).map(|_| a.clone()).collect();
        assert!(clones.iter().all(|c| RefStructArc::ptr_eq(c, &a)));
        // The parent goes first.
        drop(a);
        for c in &clones {
            assert_eq!(c.as_ref().bits(), 0x0123_4567_89ab_cdef);
        }
        let mut last = clones.into_iter().next_back().unwrap();
        last.get_mut().unwrap().set_float(1.5);
        assert_eq!(last.as_ref().float(), 1.5);
    }

    #[test]
    fn exclusive_handle_only_for_the_sole_reference() {
        let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        assert!(is_exclusive(&a));
        let b = a.clone();
        assert!(!is_exclusive(&a) && !is_exclusive(&b));
        assert!(a.get_mut().is_none());
        let c = to_const(&b);
        drop(b);
        assert!(a.get_mut().is_none());
        drop(c);
        assert!(is_exclusive(&a));
        a.get_mut().unwrap().set_bits(9);
        assert_eq!(a.as_ref().bits(), 9);
    }

    #[test]
    fn sole_references_convert_to_and_from_the_box() {
        let a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        let b = a.clone();
        let a = a.try_into_box().unwrap_err();
        drop(b);
        let mut boxed = a.try_into_box().unwrap();
        boxed.as_mut().set_bits(42);
        let a = RefStructArc::from(boxed);
        let c = into_const(a);
        assert_eq!(c.as_ref().bits(), 42);
    }

    #[test]
    fn const_references_never_hand_out_the_exclusive_handle() {
        let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        a.get_mut().unwrap().set_bits(5);
        let mut c = to_const(&a);
        let mut d = c.clone();
        assert!(c.get_mut().is_none() && d.get_mut().is_none());
        drop(a);
        drop(c);
        // `d` is the only reference left, and still `const`.
        assert!(is_exclusive(&d));
        assert!(d.get_mut().is_none());
        assert!(d.try_into_box().is_err());

        let a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        let mut only = into_const(a);
        assert!(only.get_mut().is_none());
        assert_eq!(only.as_ref().bits(), 0);
    }

    #[test]
    fn replace_covers_every_slot_transition() {
        let mut x = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        x.get_mut().unwrap().set_bits(1);
        let mut y = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        y.get_mut().unwrap().set_bits(2);

        let mut slot = None;
        replace(&mut slot, None);
        assert!(slot.is_none());
        replace(&mut slot, Some(&x));
        assert!(!is_exclusive(&x));
        replace(&mut slot, Some(&x));
        assert!(RefStructArc::ptr_eq(slot.as_ref().unwrap(), &x));
        replace(&mut slot, Some(&y));
        assert!(is_exclusive(&x));
        assert_eq!(slot.as_ref().unwrap().as_ref().bits(), 2);
        // The slot holds the last reference to `y`'s object: replacing it frees
        // the object while `x`'s lives on.
        drop(y);
        replace(&mut slot, Some(&x));
        assert_eq!(slot.as_ref().unwrap().as_ref().bits(), 1);
        // A slot replaced by a clone of its own reference.
        let same = slot.clone();
        replace(&mut slot, same.as_ref());
        drop(same);
        replace(&mut slot, None);
        assert!(slot.is_none() && is_exclusive(&x));

        let c = to_const(&x);
        let mut cslot: Option<ConstRefStructArc<IntFloat64>> = None;
        replace(&mut cslot, Some(&c));
        drop(c);
        drop(x);
        assert!(is_exclusive(cslot.as_ref().unwrap()));
        assert_eq!(cslot.as_ref().unwrap().as_ref().bits(), 1);
        replace(&mut cslot, None);
    }

    #[test]
    fn references_are_counted_across_threads() {
        let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        a.get_mut().unwrap().set_bits(77);
        let c = to_const(&a);
        let start = Arc::new(Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|i| {
                let (a, c, start) = (a.clone(), c.clone(), start.clone());
                thread::spawn(move || {
                    start.wait();
                    let mut slot = None;
                    for n in 0..500 {
                        let b = a.clone();
                        assert_eq!(b.as_ref().bits(), 77);
                        if (n + i) % 2 == 0 {
                            replace(&mut slot, Some(&b));
                        } else {
                            replace(&mut slot, None);
                        }
                        let d = c.clone();
                        assert_eq!(d.as_ref().bits(), 77);
                        let _ = is_exclusive(&d);
                    }
                    // The worker's references are dropped on this thread.
                    (a, slot)
                })
            })
            .collect();
        let mut kept: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(a.get_mut().is_none());
        kept.clear();
        drop(c);
        a.get_mut().unwrap().set_bits(78);
        assert_eq!(a.as_ref().bits(), 78);
    }

    #[test]
    fn last_release_on_another_thread_happens_before_the_exclusive_write() {
        for _ in 0..64 {
            let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
            a.get_mut().unwrap().set_bits(1);
            let b = a.clone();
            let reader = thread::spawn(move || {
                // Read the payload, then release: C's acq_rel decrement orders
                // the read before the write the main thread makes once
                // `get_mut` observes the count at 1.
                assert_eq!(b.as_ref().bits(), 1);
                drop(b);
            });
            loop {
                if let Some(mut m) = a.get_mut() {
                    m.set_bits(2);
                    break;
                }
                std::hint::spin_loop();
            }
            reader.join().unwrap();
            assert_eq!(a.as_ref().bits(), 2);
        }
    }

    #[test]
    fn objects_freed_on_the_thread_holding_the_last_reference() {
        let objects: Vec<_> = (0..32)
            .map(|i| {
                let mut a = RefStructArc::<Rational>::new_zeroed().unwrap();
                a.get_mut().unwrap().set(Rational::new(i, 1));
                a
            })
            .collect();
        let copies: Vec<_> = objects.iter().map(to_const).collect();
        let t = thread::spawn(move || {
            for (i, c) in copies.iter().enumerate() {
                assert_eq!(c.as_ref().num(), i as i32);
            }
            drop(copies);
        });
        drop(objects);
        t.join().unwrap();
    }

    #[test]
    fn allocation_failure_returns_no_owner() {
        // Isolate the process-wide allocator limit from every other test.
        const ISOLATED: &str = "CRUSTIFY_UB_SAFE_REFSTRUCT_ALLOC_FAILURE_CHILD";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "refstruct::counted_references::allocation_failure_returns_no_owner",
                    "--test-threads=1",
                ])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let kept = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        ffmpeg::libavutil::mem::set_max_alloc(0);
        assert!(RefStructArc::<IntFloat64>::new_zeroed().is_none());
        assert!(RefStructBox::<ColorPrimariesDescriptor>::new_zeroed().is_none());
        // Counting allocates nothing.
        let more = kept.clone();
        let c = to_const(&more);
        let mut slot = None;
        replace(&mut slot, Some(&kept));
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        drop((more, c, slot));
        assert!(is_exclusive(&kept));
        drop(RefStructArc::<IntFloat64>::new_zeroed().unwrap());
    }

    /// Rust reads and writes of the payload interleaved with C's header-only
    /// count updates through retained pointers, across reborrows of the handles.
    #[test]
    fn foreign_count_updates_across_payload_reborrows() {
        let mut a = RefStructArc::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        for i in 1..8 {
            {
                let mut m = a.get_mut().unwrap();
                m.white_point_mut().set_y(Rational::new(i, 9));
                let w = m.as_ref().white_point();
                assert_eq!(w.y().num(), i);
            }
            let b = a.clone();
            let c = to_const(&b);
            assert_eq!(c.as_ref().white_point().y().num(), i);
            let mut slot = Some(c);
            replace(&mut slot, None);
            drop(b);
            let r = a.as_ref();
            assert_eq!(r.white_point().y().den(), 9);
            assert!(is_exclusive(&a));
        }
    }
}
