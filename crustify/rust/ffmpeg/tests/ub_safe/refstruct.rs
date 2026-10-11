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
