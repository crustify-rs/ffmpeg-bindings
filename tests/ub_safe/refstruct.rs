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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// Buffer-frame wave 1, symbol batch 1: objects with a release closure and
/// typed pools, through every owner form, drop order, flag set and callback
/// subset; releases and callbacks on other threads, concurrently; callbacks
/// that re-enter their pool; entries outliving their pool; and allocation
/// failures, which drop the closures and state unrun.
mod releases_and_typed_pools {
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;

    use ffmpeg::libavutil::csp::ColorPrimariesDescriptor;
    use ffmpeg::libavutil::intfloat::{IntFloat64, IntFloat64Mut};
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::refstruct::{
        ConstRefStructArc, PoolCallbacks, PoolFlags, RefStructArc, RefStructBox,
        TypedRefStructPool, alloc_with_release, into_const, is_exclusive, replace, to_const,
    };

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

    /// An object whose release counts its runs and records what it saw.
    fn counted(runs: &Arc<AtomicUsize>, seen: &Arc<AtomicU64>) -> RefStructBox<IntFloat64> {
        let (runs, seen) = (Arc::clone(runs), Arc::clone(seen));
        alloc_with_release::<IntFloat64, _>(move |mut last| {
            runs.fetch_add(1, Ordering::SeqCst);
            seen.store(last.as_ref().bits(), Ordering::SeqCst);
            // The release may write the payload: nothing reads it after.
            last.set_bits(u64::MAX);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn release_runs_once_through_every_owner_form() {
        let runs = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(AtomicU64::new(0));

        // The sole owner, dropped at once.
        drop(counted(&runs, &seen));
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        // Shared, `const` and replaced references, the last one released
        // through each form in turn.
        for last in 0..4 {
            let before = runs.load(Ordering::SeqCst);
            let mut b = counted(&runs, &seen);
            b.as_mut().set_bits(100 + last);
            let a = RefStructArc::from(b);
            let c: ConstRefStructArc<IntFloat64> = to_const(&a);
            let mut slot = Some(a.clone());
            let k = into_const(a);
            assert!(!is_exclusive(&k));
            let survivor = match last {
                0 => {
                    drop((c, k));
                    replace(&mut slot, None);
                    None
                }
                1 => {
                    drop((c, slot.take()));
                    Some(k)
                }
                2 => {
                    drop((k, slot.take()));
                    Some(c)
                }
                _ => {
                    let a = slot.take().unwrap();
                    drop((c, k));
                    let mut b = a.try_into_box().expect("sole");
                    b.as_mut().set_bits(200);
                    drop(b);
                    None
                }
            };
            if let Some(s) = survivor {
                assert_eq!(runs.load(Ordering::SeqCst), before);
                assert_eq!(s.as_ref().bits(), 100 + last);
                drop(s);
            }
            assert_eq!(runs.load(Ordering::SeqCst), before + 1);
            let expected = if last == 3 { 200 } else { 100 + last };
            assert_eq!(seen.load(Ordering::SeqCst), expected);
        }
        assert_eq!(Arc::strong_count(&runs), 1, "every release was dropped");
    }

    /// The release of a 64-byte payload writes its last field, on the thread
    /// that drops the last of many references spread over threads.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn release_runs_on_the_thread_dropping_the_last_reference() {
        for round in 0..16 {
            let (tx, rx) = std::sync::mpsc::channel();
            let mut b = alloc_with_release::<ColorPrimariesDescriptor, _>(move |mut last| {
                let den = last.as_ref().white_point().y().den();
                last.white_point_mut().set_y(Rational::new(0, 1));
                tx.send((thread::current().id(), den)).unwrap();
            })
            .unwrap();
            b.as_mut()
                .white_point_mut()
                .set_y(Rational::new(1, 9 + round));
            let a = RefStructArc::from(b);
            let barrier = Arc::new(Barrier::new(4));
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let mine = a.clone();
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        assert_eq!(mine.as_ref().white_point().y().num(), 1);
                        let more = mine.clone();
                        drop(mine);
                        drop(more);
                        thread::current().id()
                    })
                })
                .collect();
            drop(a);
            let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
            let (released_on, den) = rx.recv().unwrap();
            assert!(ids.contains(&released_on) || released_on == thread::current().id());
            assert_eq!(den, 9 + round);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn typed_pool_entries_are_valid_under_every_flag_set() {
        for flags in every_flag_set() {
            let pool = TypedRefStructPool::<ColorPrimariesDescriptor>::new(flags).unwrap();
            let mut held = Vec::new();
            for round in 0..3 {
                for i in 0..4 {
                    let mut e = pool.get().unwrap();
                    let d = e.as_ref().white_point().x().den();
                    // New entries are zeroed whatever the flags; reused
                    // ones hold what was left, or zeroes again.
                    assert!(d == 0 || (7..=10).contains(&d), "{flags:?} {d}");
                    e.as_mut().white_point_mut().set_x(Rational::new(1, 7 + i));
                    held.push(e);
                }
                if round < 2 {
                    held.truncate(1);
                }
            }
            let zero = flags.contains(PoolFlags::ZERO_EVERY_TIME);
            drop(held);
            let e = pool.get().unwrap();
            assert_eq!(e.as_ref().white_point().x().den() == 0, zero, "{flags:?}");
        }
    }

    /// Entries outlive the pool: dropping its last reference frees only the
    /// entries it holds; each one returned later is freed then, and the
    /// last of them frees the pool and drops its state.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn entries_outlive_their_pool_in_every_order() {
        for order in 0..3 {
            let freed = Arc::new(AtomicUsize::new(0));
            let state = Arc::clone(&freed);
            let callbacks = PoolCallbacks::<IntFloat64, _>::new(state)
                .reset(|_, mut e| e.set_bits(0))
                .free_entry(|freed, _| {
                    freed.fetch_add(1, Ordering::SeqCst);
                });
            let pool = TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks).unwrap();
            let clone = pool.clone();
            assert!(TypedRefStructPool::ptr_eq(&pool, &clone));
            let mut entries: Vec<_> = (0..4).map(|_| pool.get().unwrap()).collect();
            drop(entries.pop());
            drop(entries.pop());
            match order {
                0 => drop((pool, clone)),
                1 => drop((clone, pool)),
                _ => {
                    let shared = RefStructArc::from(entries.pop().unwrap());
                    drop((pool, clone));
                    let again = shared.clone();
                    drop(shared);
                    assert_eq!(again.as_ref().bits(), 0);
                    drop(again);
                }
            }
            let left = entries.len();
            assert_eq!(freed.load(Ordering::SeqCst), 4 - left);
            for (i, mut e) in entries.into_iter().enumerate() {
                e.as_mut().set_bits(i as u64);
                drop(e);
            }
            assert_eq!(freed.load(Ordering::SeqCst), 4);
            assert_eq!(Arc::strong_count(&freed), 1, "the pool dropped its state");
        }
    }

    /// The shared state of the callbacks below.
    #[derive(Default)]
    struct Counts {
        inits: AtomicUsize,
        resets: AtomicUsize,
        frees: AtomicUsize,
        fail: AtomicBool,
    }

    fn init(c: &Arc<Counts>, mut e: IntFloat64Mut<'_>) -> bool {
        assert_eq!(e.as_ref().bits(), 0, "init sees a zeroed entry");
        c.inits.fetch_add(1, Ordering::SeqCst);
        e.set_bits(1);
        !c.fail.load(Ordering::SeqCst)
    }

    fn reset(c: &Arc<Counts>, mut e: IntFloat64Mut<'_>) {
        c.resets.fetch_add(1, Ordering::SeqCst);
        e.set_bits(2);
    }

    fn free_entry(c: &Arc<Counts>, e: IntFloat64Mut<'_>) {
        // Reads the entry C is about to free, whatever it holds.
        std::hint::black_box(e.as_ref().bits());
        c.frees.fetch_add(1, Ordering::SeqCst);
    }

    fn callbacks(c: &Arc<Counts>, mask: u8) -> PoolCallbacks<IntFloat64, Arc<Counts>> {
        let mut cbs = PoolCallbacks::new(Arc::clone(c));
        if mask & 1 != 0 {
            cbs = cbs.init(init);
        }
        if mask & 2 != 0 {
            cbs = cbs.reset(reset);
        }
        if mask & 4 != 0 {
            cbs = cbs.free_entry(free_entry);
        }
        cbs
    }

    /// Init failures under every flag set and callback subset: `get` is
    /// `None`, the entry is freed (LSan reports otherwise), and the error
    /// callbacks C selects run on it.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failing_init_under_every_flag_set_and_callback_subset() {
        for flags in every_flag_set() {
            for mask in 0..8u8 {
                let counts = Arc::new(Counts::default());
                let pool =
                    TypedRefStructPool::with_callbacks(flags, callbacks(&counts, mask)).unwrap();
                let ok = pool.get().unwrap();
                counts.fail.store(true, Ordering::SeqCst);
                let failed = pool.get();
                assert_eq!(failed.is_none(), mask & 1 != 0, "{flags:?} {mask}");
                drop(failed);
                counts.fail.store(false, Ordering::SeqCst);
                drop(ok);
                let reused = pool.get().unwrap();
                drop((pool, reused));
                let resets = counts.resets.load(Ordering::SeqCst);
                let frees = counts.frees.load(Ordering::SeqCst);
                let on_error = |flag, bit| mask & 1 != 0 && mask & bit != 0 && flags.contains(flag);
                let reset_on_error = usize::from(on_error(PoolFlags::RESET_ON_INIT_ERROR, 2));
                let free_on_error = usize::from(on_error(PoolFlags::FREE_ON_INIT_ERROR, 4));
                // Without an init callback nothing fails: the second get
                // is a second entry, returned (reset) and freed like the
                // first.
                let second = usize::from(mask & 1 == 0);
                if mask & 2 != 0 {
                    assert_eq!(resets, 2 + second + reset_on_error, "{flags:?} {mask}");
                }
                if mask & 4 != 0 {
                    assert_eq!(frees, 1 + second + free_on_error, "{flags:?} {mask}");
                }
                assert_eq!(Arc::strong_count(&counts), 1);
            }
        }
    }

    /// `init` sees zeroes under `ZERO_EVERY_TIME`, where C does not zero a
    /// new entry, even when the allocator hands back dirty memory.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn init_sees_zeroed_entries_where_c_leaves_them_dirty() {
        for flags in [
            PoolFlags::ZERO_EVERY_TIME,
            PoolFlags::NO_ZEROING | PoolFlags::ZERO_EVERY_TIME,
            PoolFlags::NO_ZEROING,
        ] {
            for _ in 0..8 {
                // Dirty blocks of an entry's size for av_malloc to reuse.
                let dirty: Vec<_> = (0..32)
                    .map(|_| {
                        let mut b = RefStructBox::<IntFloat64>::new_zeroed().unwrap();
                        b.as_mut().set_bits(u64::MAX);
                        b
                    })
                    .collect();
                drop(dirty);
                let counts = Arc::new(Counts::default());
                let pool =
                    TypedRefStructPool::with_callbacks(flags, callbacks(&counts, 1)).unwrap();
                let entries: Vec<_> = (0..32).map(|_| pool.get().unwrap()).collect();
                let zeroed = flags.contains(PoolFlags::ZERO_EVERY_TIME);
                assert!(entries.iter().all(|e| (e.as_ref().bits() == 0) == zeroed));
                let plain = TypedRefStructPool::<IntFloat64>::new(flags).unwrap();
                assert!(
                    (0..32)
                        .map(|_| plain.get().unwrap())
                        .all(|e| e.as_ref().bits() == 0)
                );
            }
        }
    }

    /// Callbacks that get from and return to their own pool while C runs
    /// them: neither holds the pool's lock. The state's pool reference is a
    /// cycle, broken before the end so the pool is freed (LSan checks).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn callbacks_reenter_their_pool() {
        #[derive(Default)]
        struct Reentrant {
            pool: Mutex<Option<TypedRefStructPool<IntFloat64>>>,
            depth: AtomicUsize,
            nested: AtomicUsize,
        }
        fn reenter(s: &Arc<Reentrant>) {
            if s.depth.fetch_add(1, Ordering::SeqCst) == 0 {
                let pool = s.pool.lock().unwrap().clone();
                if let Some(pool) = pool {
                    let e = pool.get();
                    s.nested
                        .fetch_add(usize::from(e.is_some()), Ordering::SeqCst);
                    drop(e);
                }
            }
            s.depth.fetch_sub(1, Ordering::SeqCst);
        }
        let state = Arc::new(Reentrant::default());
        let callbacks = PoolCallbacks::<IntFloat64, _>::new(Arc::clone(&state))
            .init(|s, _| {
                reenter(s);
                true
            })
            .reset(|s, _| reenter(s));
        let pool = TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks).unwrap();
        *state.pool.lock().unwrap() = Some(pool.clone());
        for _ in 0..8 {
            let a = pool.get().unwrap();
            let b = pool.get().unwrap();
            drop(a);
            drop(b);
        }
        assert!(state.nested.load(Ordering::SeqCst) >= 8);
        state.pool.lock().unwrap().take();
        drop(pool);
        assert_eq!(Arc::strong_count(&state), 1, "the pool was freed");
    }

    /// Pool references and entries spread over threads: concurrent gets,
    /// writes, returns on other threads than the getter's, and the pool's
    /// last reference dropped while entries are out (TSan checks the C
    /// synchronization and the shared state).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_gets_and_returns_across_threads() {
        let counts = Arc::new(Counts::default());
        let pool =
            TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks(&counts, 7)).unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<RefStructBox<IntFloat64>>();
        let barrier = Arc::new(Barrier::new(5));
        let getters: Vec<_> = (0..4)
            .map(|t| {
                let pool = pool.clone();
                let tx = tx.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    for i in 0..200u64 {
                        let mut e = pool.get().unwrap();
                        assert!(matches!(e.as_ref().bits(), 1 | 2));
                        e.as_mut().set_bits(1000 * t + i + 3);
                        if i % 3 == 0 {
                            tx.send(e).unwrap();
                        } else {
                            let shared = RefStructArc::from(e);
                            let other = shared.clone();
                            drop(shared);
                            assert_eq!(other.as_ref().bits(), 1000 * t + i + 3);
                        }
                    }
                })
            })
            .collect();
        drop(tx);
        let returner = thread::spawn(move || {
            barrier.wait();
            rx.into_iter()
                .map(|e| assert!(e.as_ref().bits() >= 3))
                .count()
        });
        drop(pool);
        for g in getters {
            g.join().unwrap();
        }
        assert_eq!(returner.join().unwrap(), 4 * 67);
        let inits = counts.inits.load(Ordering::SeqCst);
        assert_eq!(counts.frees.load(Ordering::SeqCst), inits);
        assert_eq!(counts.resets.load(Ordering::SeqCst), 800);
        assert_eq!(Arc::strong_count(&counts), 1);
    }

    /// C's writes to an entry (the reset callback through C's pointer, then
    /// `ZERO_EVERY_TIME`'s memset) between Rust views of it through the
    /// boxes `get` hands out, and the release's write through C's pointer
    /// after Rust reborrows of the payload: BSan's foreign-write checks.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn foreign_writes_between_rust_views_of_entries() {
        let counts = Arc::new(Counts::default());
        let pool =
            TypedRefStructPool::with_callbacks(PoolFlags::ZERO_EVERY_TIME, callbacks(&counts, 3))
                .unwrap();
        for i in 1..16u64 {
            let mut e = pool.get().unwrap();
            assert_eq!(e.as_ref().bits(), 0);
            e.as_mut().set_bits(i);
            let r = e.as_ref();
            assert_eq!(r.bits(), i);
            let a = RefStructArc::from(e);
            let b = a.clone();
            assert_eq!(a.as_ref().bits(), b.as_ref().bits());
            drop(a);
            drop(b);
        }
        let seen = Arc::new(AtomicU64::new(0));
        let runs = Arc::new(AtomicUsize::new(0));
        let mut b = counted(&runs, &seen);
        for i in 0..8 {
            b.as_mut().set_bits(i);
            assert_eq!(b.as_ref().bits(), i);
        }
        drop(b);
        assert_eq!(seen.load(Ordering::SeqCst), 7);
    }

    /// Every way to hold a typed pool: fallible and infallible clones, the
    /// shared handle, identity and `Debug`, each outliving the original.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_references_clone_compare_and_format() {
        let pool = TypedRefStructPool::<IntFloat64>::new(PoolFlags::NONE).unwrap();
        let other = TypedRefStructPool::<IntFloat64>::new(PoolFlags::NONE).unwrap();
        let tried = pool.try_clone().expect("64-bit counts never refuse");
        let cloned = tried.clone();
        assert!(TypedRefStructPool::ptr_eq(&pool, &tried));
        assert!(TypedRefStructPool::ptr_eq(&cloned, &pool));
        assert!(!TypedRefStructPool::ptr_eq(&pool, &other));
        let shown = format!("{pool:?}");
        assert!(shown.starts_with("TypedRefStructPool("), "{shown}");
        assert_eq!(shown, format!("{cloned:?}"));
        drop(pool);
        {
            let handle = tried.as_ref();
            let copy = handle;
            std::hint::black_box((handle, copy));
        }
        let mut e = tried.get().unwrap();
        e.as_mut().set_bits(3);
        drop((tried, cloned));
        assert_eq!(e.as_ref().bits(), 3);
    }

    /// Under an `av_max_alloc` limit of zero nothing is allocated: the
    /// constructors fail and drop their closure and state unrun, and a pool
    /// still hands out the entries it holds. The limit is process-wide, so
    /// this runs alone in a child process.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocation_failure_drops_closures_and_state_unrun() {
        const ISOLATED: &str = "CRUSTIFY_UB_SAFE_REFSTRUCT_RELEASE_ALLOC_FAILURE_CHILD";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "refstruct::releases_and_typed_pools::allocation_failure_drops_closures_and_state_unrun",
                    "--test-threads=1",
                ])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let counts = Arc::new(Counts::default());
        let pool =
            TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks(&counts, 7)).unwrap();
        drop(pool.get().unwrap());
        let runs = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(AtomicU64::new(0));
        ffmpeg::libavutil::mem::set_max_alloc(0);
        let (r, s) = (Arc::clone(&runs), Arc::clone(&seen));
        let none = alloc_with_release::<IntFloat64, _>(move |_| {
            r.fetch_add(1, Ordering::SeqCst);
            drop(s);
        });
        assert!(none.is_none());
        assert_eq!(
            (Arc::strong_count(&runs), runs.load(Ordering::SeqCst)),
            (1, 0)
        );
        assert!(TypedRefStructPool::<IntFloat64>::new(PoolFlags::NONE).is_none());
        assert!(
            TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks(&counts, 7)).is_none()
        );
        assert_eq!(
            Arc::strong_count(&counts),
            2,
            "the failed pool's state was dropped"
        );
        assert!(RefStructBox::<IntFloat64>::new_uninit().is_none());
        // The returned entry is reused without an allocation; a second one
        // needs one.
        let reused = pool.get().expect("a free entry");
        assert!(pool.get().is_none());
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        drop((reused, pool));
        assert_eq!(Arc::strong_count(&counts), 1);
    }
}

/// Typed pools torn down from inside their own callbacks (buffer-frame wave 1
/// symbol review): a reset that drops the pool's last reference, so C runs
/// the pool's uninit, and with it the entry-freeing callbacks, nested in
/// `reset_with`; an entry-freeing callback that releases other entries of
/// the same pool during that uninit's flush; and an init that releases the
/// state's pool reference and entries while C is getting one. Each callback
/// reads the shared state again after the nested teardown has returned, so a
/// state dropped too early is a use after free (ASan), and the state's drop
/// is checked to come after every callback.
mod pool_teardown_in_callbacks {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use ffmpeg::libavutil::intfloat::IntFloat64;
    use ffmpeg::libavutil::refstruct::{
        PoolCallbacks, PoolFlags, RefStructBox, TypedRefStructPool,
    };

    /// What the callbacks share: a pool reference and entries they may drop,
    /// and the log of every callback, written after any nested teardown.
    #[derive(Default)]
    struct Owner {
        pool: Mutex<Option<TypedRefStructPool<IntFloat64>>>,
        entries: Mutex<Vec<RefStructBox<IntFloat64>>>,
        log: Mutex<Vec<(&'static str, u64)>>,
        /// Set when the pool drops its state, which must follow every
        /// callback.
        dropped: AtomicBool,
    }

    /// The pool's state, C's opaque: its drop is C's `free_cb`.
    struct State(Arc<Owner>);

    impl Drop for State {
        fn drop(&mut self) {
            self.0.dropped.store(true, Ordering::SeqCst);
        }
    }

    /// Records `what` on `bits`, checking the state is still alive.
    fn record(s: &State, what: &'static str, bits: u64) {
        assert!(
            !s.0.dropped.load(Ordering::SeqCst),
            "{what} after the state"
        );
        s.0.log.lock().unwrap().push((what, bits));
    }

    /// Drops what the state holds outside its locks: the nested callbacks
    /// lock them again.
    fn release_held(s: &State) {
        let pool = s.0.pool.lock().unwrap().take();
        let entries = std::mem::take(&mut *s.0.entries.lock().unwrap());
        drop(entries);
        drop(pool);
    }

    fn get(pool: &TypedRefStructPool<IntFloat64>, bits: u64) -> RefStructBox<IntFloat64> {
        let mut e = pool.get().unwrap();
        e.as_mut().set_bits(bits);
        e
    }

    fn freed(owner: &Owner) -> bool {
        owner.dropped.load(Ordering::SeqCst)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reset_drops_the_last_pool_reference() {
        for flags in [PoolFlags::NONE, PoolFlags::ZERO_EVERY_TIME] {
            let owner = Arc::new(Owner::default());
            let callbacks = PoolCallbacks::<IntFloat64, _>::new(State(Arc::clone(&owner)))
                .reset(|s, e| {
                    let bits = e.as_ref().bits();
                    release_held(s);
                    record(s, "reset", bits);
                })
                .free_entry(|s, e| record(s, "free_entry", e.as_ref().bits()));
            let pool = TypedRefStructPool::with_callbacks(flags, callbacks).unwrap();
            let out = get(&pool, 1);
            // Returned while nothing is held: C keeps it for reuse.
            drop(get(&pool, 2));
            *owner.pool.lock().unwrap() = Some(pool);
            // The reset drops the pool's last reference: C frees the kept
            // entry inside it, and this one when the reset returns.
            drop(out);
            assert!(freed(&owner), "the pool was freed");
            assert_eq!(
                *owner.log.lock().unwrap(),
                [
                    ("reset", 2),
                    ("free_entry", 2),
                    ("reset", 1),
                    ("free_entry", 1)
                ],
                "{flags:?}"
            );
            assert_eq!(Arc::strong_count(&owner), 1);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn free_entry_releases_entries_of_its_own_pool() {
        let owner = Arc::new(Owner::default());
        let callbacks = PoolCallbacks::<IntFloat64, _>::new(State(Arc::clone(&owner)))
            .reset(|s, e| record(s, "reset", e.as_ref().bits()))
            .free_entry(|s, e| {
                let bits = e.as_ref().bits();
                release_held(s);
                record(s, "free_entry", bits);
            });
        let pool = TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks).unwrap();
        let held: Vec<_> = (10..13).map(|b| get(&pool, b)).collect();
        let kept = [get(&pool, 1), get(&pool, 2)];
        drop(kept);
        owner.log.lock().unwrap().clear();
        *owner.entries.lock().unwrap() = held;
        // The pool's last reference: C's uninit frees the two kept entries,
        // and the first entry-freeing callback releases the three held ones,
        // each reset and freed inside it.
        drop(pool);
        assert!(freed(&owner), "the pool was freed");
        let log = owner.log.lock().unwrap().clone();
        assert_eq!(log.len(), 2 + 3 * 2, "{log:?}");
        assert_eq!(log.iter().filter(|(w, _)| *w == "reset").count(), 3);
        let mut freed: Vec<_> = log
            .iter()
            .filter(|(w, _)| *w == "free_entry")
            .map(|&(_, b)| b)
            .collect();
        freed.sort_unstable();
        assert_eq!(freed, [1, 2, 10, 11, 12]);
        assert_eq!(Arc::strong_count(&owner), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn init_releases_pool_references_and_entries() {
        let owner = Arc::new(Owner::default());
        let callbacks = PoolCallbacks::<IntFloat64, _>::new(State(Arc::clone(&owner)))
            .init(|s, mut e| {
                release_held(s);
                record(s, "init", 0);
                e.set_bits(7);
                true
            })
            .reset(|s, e| record(s, "reset", e.as_ref().bits()))
            .free_entry(|s, e| record(s, "free_entry", e.as_ref().bits()));
        let pool = TypedRefStructPool::with_callbacks(PoolFlags::NONE, callbacks).unwrap();
        let first = get(&pool, 3);
        *owner.entries.lock().unwrap() = vec![first];
        *owner.pool.lock().unwrap() = Some(pool.clone());
        // The free list is empty, so C allocates and inits a new entry; the
        // init returns the held entry and drops the state's pool reference
        // while the caller's keeps the pool.
        let second = pool.get().unwrap();
        assert_eq!(second.as_ref().bits(), 7);
        drop(pool);
        assert!(!freed(&owner), "an entry is still out");
        drop(second);
        assert!(freed(&owner), "the pool was freed");
        assert_eq!(
            *owner.log.lock().unwrap(),
            [
                ("init", 0),
                ("reset", 3),
                ("init", 0),
                ("free_entry", 3),
                ("reset", 7),
                ("free_entry", 7)
            ]
        );
        assert_eq!(Arc::strong_count(&owner), 1);
    }
}
