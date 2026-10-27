//! Equivalence of `libavutil/refstruct.h`'s wrappers with C on
//! independently owned inputs.
#![expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::needless_for_each,
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

/// Buffer-frame wave 0, type batch 3: `AVRefStructOpaque` as C's union
/// builders and consumers see it, and `AVRefStructPool`'s lifecycle against a
/// pool C allocates and counts itself.
mod pool_and_opaque {
    use core::ffi::c_void;
    use core::ptr;
    use std::cell::{Cell, RefCell};
    use std::sync::Barrier;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::refstruct::{
        PoolFlags, RefStructOpaque, RefStructPool, RefStructPoolArc,
    };

    /// Addresses at the edges of a pointer; C only copies them.
    const ADDRS: [usize; 6] = [0, 1, 8, 0x1000, usize::MAX >> 1, usize::MAX & !7];

    const SIZE: usize = 48;

    /// Every flag combination a pool without callbacks can see.
    fn flag_sets() -> Vec<PoolFlags> {
        let all = [
            PoolFlags::NO_ZEROING,
            PoolFlags::RESET_ON_INIT_ERROR,
            PoolFlags::FREE_ON_INIT_ERROR,
            PoolFlags::ZERO_EVERY_TIME,
        ];
        (0..16u32)
            .map(|m| {
                all.iter()
                    .enumerate()
                    .filter(|(i, _)| m & (1 << i) != 0)
                    .fold(PoolFlags::NONE, |f, (_, &x)| f | x)
            })
            .collect()
    }

    fn exclusive(p: *const ffi::AVRefStructPool) -> bool {
        // SAFETY: `p` is a live pool whose reference the caller holds; C
        // reads its RefStruct header's count.
        unsafe { ffi::av_refstruct_exclusive(p.cast()) == 1 }
    }

    fn c_alloc(flags: PoolFlags) -> *mut ffi::AVRefStructPool {
        // SAFETY: no callbacks; any size and flags.
        let p = unsafe { ffi::av_refstruct_pool_alloc(SIZE, flags.bits()) };
        assert!(!p.is_null());
        p
    }

    fn c_ref(p: *mut ffi::AVRefStructPool) -> *mut ffi::AVRefStructPool {
        // SAFETY: the caller holds a reference to the live pool `p`.
        unsafe { ffi::av_refstruct_ref(p.cast()) }.cast()
    }

    fn c_uninit(p: *mut ffi::AVRefStructPool) {
        let mut slot = p;
        // SAFETY: releases one reference the caller holds.
        unsafe { ffi::av_refstruct_pool_uninit(&raw mut slot) };
        assert!(slot.is_null());
    }

    fn get(p: *mut ffi::AVRefStructPool) -> *mut u8 {
        // SAFETY: the caller holds a reference to the live pool, so it is
        // not uninitialised yet.
        let e = unsafe { ffi::av_refstruct_pool_get(p) }.cast::<u8>();
        assert!(!e.is_null());
        e
    }

    fn put(e: *mut u8) {
        let mut slot = e.cast::<c_void>();
        // SAFETY: `e` is one reference to a live entry the caller got.
        unsafe { ffi::av_refstruct_unref((&raw mut slot).cast()) };
        assert!(slot.is_null());
    }

    fn fill(e: *mut u8, byte: u8) {
        // SAFETY: an entry is `SIZE` writable bytes the caller holds.
        unsafe { ptr::write_bytes(e, byte, SIZE) };
    }

    fn bytes(e: *mut u8) -> Vec<u8> {
        let mut v = vec![0u8; SIZE];
        // SAFETY: an entry is `SIZE` bytes the caller holds, initialised
        // by C's zeroing or by `fill`.
        unsafe { ptr::copy_nonoverlapping(e, v.as_mut_ptr(), SIZE) };
        v
    }

    /// What the entries of a pool show through one get/fill/return/get
    /// cycle: whether a fresh entry is zeroed (only read where C zeroes
    /// it), whether the returned entry is reused, and what it holds then.
    fn entry_cycle(
        p: *mut ffi::AVRefStructPool,
        flags: PoolFlags,
    ) -> (Option<bool>, bool, Vec<u8>) {
        let first = get(p);
        let zeroes =
            !flags.contains(PoolFlags::NO_ZEROING) || flags.contains(PoolFlags::ZERO_EVERY_TIME);
        let fresh = zeroes.then(|| bytes(first).iter().all(|&b| b == 0));
        fill(first, 0xAB);
        put(first);
        let second = get(p);
        let reused = second == first;
        let seen = bytes(second);
        put(second);
        (fresh, reused, seen)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_entries_follow_the_flags_as_c() {
        for flags in flag_sets() {
            let rust = RefStructPool::new(SIZE, flags).expect("pool");
            let c = c_alloc(flags);
            let ours = entry_cycle(rust.as_c_ptr(), flags);
            let theirs = entry_cycle(c, flags);
            assert_eq!(ours, theirs, "flags {flags:?}");
            drop(rust);
            c_uninit(c);
        }
    }

    /// The counted references after every step of the same sequence, as
    /// `av_refstruct_exclusive` reports them on each side.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reference_counts_follow_c_step_by_step() {
        let rust = RefStructPool::new(SIZE, PoolFlags::NONE).expect("pool");
        let c = c_alloc(PoolFlags::NONE);
        assert_eq!(exclusive(rust.as_c_ptr()), exclusive(c));
        let mut ours: Vec<RefStructPoolArc> = vec![rust];
        let mut theirs = vec![c];
        // Grow (1) and shrink (0): the count wanders between one and five.
        let plan = [1, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1];
        for (step, &push) in plan.iter().enumerate() {
            if push == 1 {
                ours.push(ours[0].clone());
                theirs.push(c_ref(theirs[0]));
            } else {
                drop(ours.pop());
                c_uninit(theirs.pop().unwrap());
            }
            assert_eq!(ours.len(), theirs.len());
            assert_eq!(
                exclusive(ours[0].as_c_ptr()),
                exclusive(theirs[0]),
                "step {step}"
            );
            assert!(ours.iter().all(|a| RefStructPoolArc::ptr_eq(a, &ours[0])));
        }
        // Release from the front: the first reference is not special.
        while ours.len() > 1 {
            drop(ours.remove(0));
            c_uninit(theirs.remove(0));
            assert_eq!(exclusive(ours[0].as_c_ptr()), exclusive(theirs[0]));
        }
        drop(ours);
        c_uninit(theirs.pop().unwrap());
    }

    /// Entries outlive the pool's last reference on both sides and stay
    /// usable; the last one returned frees the pool (`LSan` otherwise reports
    /// it, `ASan` any use after free).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn entries_outlive_the_last_reference_as_in_c() {
        let rust = RefStructPool::new(SIZE, PoolFlags::ZERO_EVERY_TIME).expect("pool");
        let c = c_alloc(PoolFlags::ZERO_EVERY_TIME);
        let ours: Vec<_> = (0..4).map(|_| get(rust.as_c_ptr())).collect();
        let theirs: Vec<_> = (0..4).map(|_| get(c)).collect();
        let spare = rust.clone();
        drop(rust);
        drop(spare);
        c_uninit(c);
        for (i, (&a, &b)) in ours.iter().zip(&theirs).enumerate() {
            fill(a, i as u8);
            fill(b, i as u8);
            assert_eq!(bytes(a), bytes(b));
        }
        ours.into_iter().for_each(put);
        theirs.into_iter().for_each(put);
    }

    /// Clones on other threads get and return entries while the owning
    /// thread drops its reference first (`TSan` checks the hand-offs).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn references_and_entries_cross_threads() {
        const THREADS: usize = 6;
        let pool = RefStructPool::new(SIZE, PoolFlags::NONE).expect("pool");
        let start = Barrier::new(THREADS);
        thread::scope(|s| {
            for t in 0..THREADS {
                let mine = pool.clone();
                let start = &start;
                s.spawn(move || {
                    start.wait();
                    for round in 0..50 {
                        let e = get(mine.as_c_ptr());
                        fill(e, (t + round) as u8);
                        assert!(bytes(e).iter().all(|&b| b == (t + round) as u8));
                        let again = mine.clone();
                        put(e);
                        drop(again);
                    }
                    // Keep one entry past this thread's last reference.
                    let kept = get(mine.as_c_ptr());
                    drop(mine);
                    put(kept);
                });
            }
            drop(pool);
        });
    }

    thread_local! {
        /// The opaques the callbacks below received, in order.
        static SEEN: RefCell<Vec<(&'static str, usize, usize)>> = const { RefCell::new(Vec::new()) };
    }

    fn seen(tag: &'static str, opaque: ffi::AVRefStructOpaque) {
        let o = RefStructOpaque::from_raw(opaque);
        let (nc, c) = (o.as_ref().mut_ptr().addr(), o.as_ref().const_ptr().addr());
        SEEN.with(|s| s.borrow_mut().push((tag, nc, c)));
    }

    fn take_seen() -> Vec<(&'static str, usize, usize)> {
        SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()))
    }

    unsafe extern "C" fn free_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) {
        seen("free", opaque);
    }

    unsafe extern "C" fn init_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) -> i32 {
        seen("init", opaque);
        0
    }

    unsafe extern "C" fn reset_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) {
        seen("reset", opaque);
    }

    unsafe extern "C" fn free_entry_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) {
        seen("free_entry", opaque);
    }

    unsafe extern "C" fn pool_free_cb(opaque: ffi::AVRefStructOpaque) {
        seen("pool_free", opaque);
    }

    /// One object through `av_refstruct_alloc_ext_c` with `opaque`, released
    /// at once: its free callback reports what C handed back.
    fn object_with(opaque: RefStructOpaque) -> Vec<(&'static str, usize, usize)> {
        // SAFETY: a callback that only records its opaque; C never
        // dereferences the opaque.
        let obj = unsafe { ffi::av_refstruct_alloc_ext_c(16, 0, opaque.into_raw(), Some(free_cb)) };
        assert!(!obj.is_null());
        put(obj.cast());
        take_seen()
    }

    /// The same through the static inline `av_refstruct_alloc_ext`, which
    /// builds `{ .nc = opaque }` in C.
    fn c_object_with(opaque: *mut c_void) -> Vec<(&'static str, usize, usize)> {
        // SAFETY: as in `object_with`.
        let obj = unsafe { ffi::av_refstruct_alloc_ext(16, 0, opaque, Some(free_cb)) };
        assert!(!obj.is_null());
        put(obj.cast());
        take_seen()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn objects_hand_back_the_opaque_c_builds() {
        for a in ADDRS {
            let reference = c_object_with(ptr::without_provenance_mut(a));
            assert_eq!(reference, [("free", a, a)]);
            assert_eq!(
                object_with(RefStructOpaque::new(ptr::without_provenance_mut(a))),
                reference
            );
            assert_eq!(
                object_with(RefStructOpaque::from_const(ptr::without_provenance(a))),
                reference
            );
            let mut set = RefStructOpaque::default();
            set.as_mut().set_const_ptr(ptr::without_provenance(a));
            assert_eq!(object_with(set), reference);
        }
    }

    /// One pool's whole life with every callback: a get, a reuse, the
    /// uninit and the last entry's return, each callback's opaque recorded.
    fn pool_life(p: *mut ffi::AVRefStructPool) -> Vec<(&'static str, usize, usize)> {
        let e = get(p);
        put(e);
        let e = get(p);
        c_uninit(p);
        put(e);
        take_seen()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_callbacks_receive_the_opaque_c_builds() {
        for a in ADDRS {
            // SAFETY: callbacks that only record their opaque, called on
            // this thread only.
            let c = unsafe {
                ffi::av_refstruct_pool_alloc_ext(
                    SIZE,
                    0,
                    ptr::without_provenance_mut(a),
                    Some(init_cb),
                    Some(reset_cb),
                    Some(free_entry_cb),
                    Some(pool_free_cb),
                )
            };
            assert!(!c.is_null());
            let reference = pool_life(c);
            assert_eq!(
                reference.iter().map(|e| e.0).collect::<Vec<_>>(),
                ["init", "reset", "reset", "free_entry", "pool_free"]
            );
            assert!(reference.iter().all(|&(_, nc, c)| nc == a && c == a));
            for opaque in [
                RefStructOpaque::new(ptr::without_provenance_mut(a)),
                RefStructOpaque::from_const(ptr::without_provenance(a)),
            ] {
                // SAFETY: as above.
                let p = unsafe {
                    ffi::av_refstruct_pool_alloc_ext_c(
                        SIZE,
                        0,
                        opaque.into_raw(),
                        Some(init_cb),
                        Some(reset_cb),
                        Some(free_entry_cb),
                        Some(pool_free_cb),
                    )
                };
                assert!(!p.is_null());
                assert_eq!(pool_life(p), reference);
            }
        }
    }

    unsafe extern "C" fn failing_init_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) -> i32 {
        seen("init", opaque);
        -1
    }

    /// A pool whose init callback fails, with every combination of the
    /// error flags: what each pool's callbacks see, and that no entry comes
    /// out, from `PoolFlags` and a Rust opaque as from C's own.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failing_init_follows_the_error_flags_as_c() {
        let errors = [
            PoolFlags::NONE,
            PoolFlags::RESET_ON_INIT_ERROR,
            PoolFlags::FREE_ON_INIT_ERROR,
            PoolFlags::RESET_ON_INIT_ERROR | PoolFlags::FREE_ON_INIT_ERROR,
        ];
        for flags in errors {
            let a = 0x2000usize;
            let life = |p: *mut ffi::AVRefStructPool| {
                // SAFETY: the caller holds the pool's one reference.
                let e = unsafe { ffi::av_refstruct_pool_get(p) };
                let none = e.is_null();
                c_uninit(p);
                (none, take_seen())
            };
            // SAFETY: callbacks that only record their opaque, called on
            // this thread only.
            let c = unsafe {
                ffi::av_refstruct_pool_alloc_ext(
                    SIZE,
                    flags.bits(),
                    ptr::without_provenance_mut(a),
                    Some(failing_init_cb),
                    Some(reset_cb),
                    Some(free_entry_cb),
                    Some(pool_free_cb),
                )
            };
            assert!(!c.is_null());
            let reference = life(c);
            assert!(reference.0);
            // SAFETY: as above.
            let p = unsafe {
                ffi::av_refstruct_pool_alloc_ext_c(
                    SIZE,
                    flags.bits(),
                    RefStructOpaque::new(ptr::without_provenance_mut(a)).into_raw(),
                    Some(failing_init_cb),
                    Some(reset_cb),
                    Some(free_entry_cb),
                    Some(pool_free_cb),
                )
            };
            assert!(!p.is_null());
            assert_eq!(life(p), reference, "flags {flags:?}");
        }
    }

    /// Set in the child process an isolated test re-runs itself in.
    const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

    /// Under an `av_max_alloc` limit below a pool's own allocation, the
    /// constructor fails as C's does; under one between the pool's and an
    /// entry's, both pools build and refuse entries. The limit is
    /// process-global, so this runs in a child process running nothing else.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocation_failures_match_c() {
        const NAME: &str = "refstruct::pool_and_opaque::allocation_failures_match_c";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }
        // The pool is a RefStruct object: its struct after a 64-byte (or
        // larger) header, so 64 bytes cannot hold it, while 4096 can but
        // cannot hold an entry of 8192 bytes.
        for (limit, size) in [(64, SIZE), (4096, 8192)] {
            // SAFETY: `av_max_alloc` takes a size by value; nothing else
            // runs in this process.
            unsafe { ffi::av_max_alloc(limit) };
            // SAFETY: no callbacks; any size and flags.
            let c = unsafe { ffi::av_refstruct_pool_alloc(size, 0) };
            let rust = RefStructPool::new(size, PoolFlags::NONE);
            assert_eq!(rust.is_some(), !c.is_null(), "limit {limit}");
            if let Some(pool) = rust {
                // SAFETY: both pools are live; C fails the entry's
                // allocation and returns NULL.
                let (ours, theirs) = (
                    unsafe { ffi::av_refstruct_pool_get(pool.as_c_ptr()) },
                    unsafe { ffi::av_refstruct_pool_get(c) },
                );
                assert!(ours.is_null() && theirs.is_null());
                c_uninit(c);
            } else {
                assert_eq!(limit, 64);
            }
        }
        // SAFETY: as above; the default limit again.
        unsafe { ffi::av_max_alloc(i32::MAX as usize) };
        assert!(RefStructPool::new(SIZE, PoolFlags::NONE).is_some());
    }

    unsafe extern "C" fn bump_cb(opaque: ffi::AVRefStructOpaque, _obj: *mut c_void) {
        let slot = RefStructOpaque::from_raw(opaque)
            .as_ref()
            .mut_ptr()
            .cast::<u64>();
        // SAFETY: as the comment below states, for this part of the operation.
        let slot_read = unsafe { slot.read() };
        // SAFETY: the test below passes `Cell::as_ptr` of a live
        // `Cell<u64>`, which no Rust access touches during C's call.
        unsafe { slot.write(slot_read + 1) };
    }

    /// C keeps a pointer to Rust memory in an object's header while Rust
    /// reads and writes the pointee through its own shared borrows, then
    /// writes through it from the free callback: the retained pointer stays
    /// valid across those reborrows (`BSan`) because it points into a `Cell`.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn retained_opaque_survives_rust_reborrows() {
        let counter = Cell::new(40u64);
        let opaque = RefStructOpaque::new(counter.as_ptr().cast());
        // SAFETY: a callback writing through the opaque, which outlives the
        // object (released below, in this scope).
        let obj = unsafe { ffi::av_refstruct_alloc_ext_c(8, 0, opaque.into_raw(), Some(bump_cb)) };
        assert!(!obj.is_null());
        counter.set(counter.get() + 1);
        let view = &counter;
        assert_eq!(view.get(), 41);
        let mut copy = opaque;
        copy.as_mut().set_mut_ptr(ptr::null_mut());
        assert_ne!(copy, opaque);
        // SAFETY: a second reference to the object, released below.
        let second = unsafe { ffi::av_refstruct_ref(obj) };
        put(second.cast());
        assert_eq!(counter.get(), 41);
        put(obj.cast());
        assert_eq!(view.get(), 42);
    }

    /// C writes into pool entries (its zeroing on every get) between Rust
    /// views of the same bytes; each view is taken fresh after C's call.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn c_rewrites_entries_between_rust_views() {
        let pool = RefStructPool::new(SIZE, PoolFlags::ZERO_EVERY_TIME).expect("pool");
        let e = get(pool.as_c_ptr());
        {
            // SAFETY: the entry is `SIZE` bytes this test holds, zeroed by
            // C, with no other view alive.
            let view = unsafe { core::slice::from_raw_parts_mut(e, SIZE) };
            assert!(view.iter().all(|&b| b == 0));
            view.fill(0x5A);
        }
        put(e);
        let again = get(pool.as_c_ptr());
        assert_eq!(again, e);
        {
            // SAFETY: as above, after C zeroed the reused entry again.
            let view = unsafe { core::slice::from_raw_parts(again, SIZE) };
            assert_eq!(view, [0u8; SIZE]);
        }
        put(again);
    }
}

/// Buffer-frame wave 0, symbol batch 5: counted references against raw C
/// object pointers driven through the same C routines, on independently
/// allocated objects. After every step of a sequence, both sides must agree
/// on which slots hold a reference, which hold the same object, which hold
/// the only reference, and what each payload reads.
mod counted_references {
    use core::ffi::c_void;
    use core::mem::size_of;
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::csp::ColorPrimariesDescriptor;
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::intfloat_h::IntFloat64;
    use ffmpeg_bindings::libavutil::libavutil::refstruct::{
        ConstRefStructArc, RefStructArc, into_const, is_exclusive, replace, to_const,
    };

    const SLOTS: usize = 6;

    /// C's side: one `void *` per slot, NULL when empty; the first `SLOTS` are
    /// written through, the rest are `const` references.
    struct CSide([*mut c_void; 2 * SLOTS]);

    impl CSide {
        fn unref(&mut self, i: usize) {
            // SAFETY: the slot holds NULL or a reference this side owns.
            unsafe { ffi::av_refstruct_unref((&raw mut self.0[i]).cast()) }
        }

        fn exclusive(&self, i: usize) -> bool {
            // SAFETY: a non-NULL slot holds a live object's reference.
            !self.0[i].is_null() && unsafe { ffi::av_refstruct_exclusive(self.0[i]) } == 1
        }

        fn bits(&self, i: usize) -> Option<u64> {
            // SAFETY: as above; the payload is 8 bytes, at least 16-aligned.
            (!self.0[i].is_null()).then(|| unsafe { self.0[i].cast::<u64>().read() })
        }
    }

    impl Drop for CSide {
        fn drop(&mut self) {
            for i in 0..self.0.len() {
                self.unref(i);
            }
        }
    }

    /// Rust's side, slot for slot.
    struct RustSide {
        refs: [Option<RefStructArc<IntFloat64>>; SLOTS],
        consts: [Option<ConstRefStructArc<IntFloat64>>; SLOTS],
    }

    impl RustSide {
        fn ptr(&self, i: usize) -> *const IntFloat64 {
            let p = if i < SLOTS {
                self.refs[i].as_ref().map(|a| a.as_ptr())
            } else {
                self.consts[i - SLOTS].as_ref().map(|a| a.as_ptr())
            };
            p.map_or(ptr::null(), |p| p.cast_const())
        }

        fn exclusive(&self, i: usize) -> bool {
            if i < SLOTS {
                self.refs[i].as_ref().is_some_and(is_exclusive)
            } else {
                self.consts[i - SLOTS].as_ref().is_some_and(is_exclusive)
            }
        }

        fn bits(&self, i: usize) -> Option<u64> {
            if i < SLOTS {
                self.refs[i].as_ref().map(|a| a.as_ref().bits())
            } else {
                self.consts[i - SLOTS].as_ref().map(|a| a.as_ref().bits())
            }
        }
    }

    fn assert_same_state(step: usize, c: &CSide, r: &RustSide) {
        for i in 0..2 * SLOTS {
            assert_eq!(
                c.0[i].is_null(),
                r.ptr(i).is_null(),
                "step {step}: slot {i} presence"
            );
            assert_eq!(
                c.exclusive(i),
                r.exclusive(i),
                "step {step}: slot {i} exclusivity"
            );
            assert_eq!(c.bits(i), r.bits(i), "step {step}: slot {i} payload");
            for j in 0..2 * SLOTS {
                let same_c = !c.0[i].is_null() && c.0[i] == c.0[j];
                let same_r = !r.ptr(i).is_null() && r.ptr(i) == r.ptr(j);
                assert_eq!(same_c, same_r, "step {step}: slots {i} and {j} identity");
            }
        }
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// One step on both sides; each arm is the C sequence, then the Rust call it
    /// corresponds to.
    fn step(rng: &mut XorShift, c: &mut CSide, r: &mut RustSide) {
        let (i, j) = (rng.below(SLOTS), rng.below(SLOTS));
        match rng.below(10) {
            // A fresh zeroed object.
            0 => {
                c.unref(i);
                // SAFETY: an allocation by size.
                c.0[i] = unsafe { ffi::av_refstruct_allocz(8) };
                assert!(!c.0[i].is_null());
                r.refs[i] = Some(RefStructArc::new_zeroed().unwrap());
            }
            // A new reference to slot j's object (av_refstruct_ref), replacing slot i's.
            1 => {
                if !c.0[j].is_null() {
                    // SAFETY: slot j holds a live object's reference.
                    let new = unsafe { ffi::av_refstruct_ref(c.0[j]) };
                    c.unref(i);
                    c.0[i] = new;
                }
                if let Some(src) = &r.refs[j] {
                    r.refs[i] = Some(src.clone());
                }
            }
            // Release (av_refstruct_unref).
            2 => {
                c.unref(i);
                r.refs[i] = None;
            }
            3 => {
                c.unref(SLOTS + i);
                r.consts[i] = None;
            }
            // av_refstruct_replace between written-through slots.
            4 => {
                let src = c.0[j];
                // SAFETY: slot i holds NULL or an owned reference, slot j NULL
                // or a live object.
                unsafe { ffi::av_refstruct_replace((&raw mut c.0[i]).cast(), src) };
                if i == j {
                    let src = r.refs[j].clone();
                    replace(&mut r.refs[i], src.as_ref());
                } else {
                    let src = r.refs[j].take();
                    replace(&mut r.refs[i], src.as_ref());
                    r.refs[j] = src;
                }
            }
            // ... and between `const` slots.
            5 => {
                let src = c.0[SLOTS + j];
                // SAFETY: as above.
                unsafe { ffi::av_refstruct_replace((&raw mut c.0[SLOTS + i]).cast(), src) };
                let src = r.consts[j].clone();
                replace(&mut r.consts[i], src.as_ref());
            }
            // A `const` reference to a written-through slot's object
            // (av_refstruct_ref_c).
            6 => {
                if !c.0[j].is_null() {
                    // SAFETY: slot j holds a live object's reference.
                    let new = unsafe { ffi::av_refstruct_ref_c(c.0[j]) };
                    c.unref(SLOTS + i);
                    c.0[SLOTS + i] = new.cast_mut();
                }
                if let Some(src) = &r.refs[j] {
                    r.consts[i] = Some(to_const(src));
                }
            }
            // A `const` copy of a `const` reference.
            7 => {
                if !c.0[SLOTS + j].is_null() {
                    // SAFETY: as above.
                    let new = unsafe { ffi::av_refstruct_ref_c(c.0[SLOTS + j]) };
                    c.unref(SLOTS + i);
                    c.0[SLOTS + i] = new.cast_mut();
                }
                if let Some(src) = &r.consts[j] {
                    r.consts[i] = Some(src.clone());
                }
            }
            // A reference moved into a `const` slot.
            8 => {
                if !c.0[j].is_null() {
                    c.unref(SLOTS + i);
                    c.0[SLOTS + i] = core::mem::replace(&mut c.0[j], ptr::null_mut());
                }
                if let Some(src) = r.refs[j].take() {
                    r.consts[i] = Some(into_const(src));
                }
            }
            // A write, where the reference is the only one.
            _ => {
                let v = rng.next();
                let c_wrote = c.exclusive(i);
                if c_wrote {
                    // SAFETY: the only reference to a live 8-byte payload.
                    unsafe { c.0[i].cast::<u64>().write(v) };
                }
                let wrote = r.refs[i]
                    .as_mut()
                    .and_then(|a| a.get_mut())
                    .map(|mut m| m.set_bits(v));
                assert_eq!(wrote.is_some(), c_wrote);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reference_sequences_follow_c() {
        // BSan retags every handle and slot the state checks touch: a few
        // seeds there take minutes, all of them hours.
        let bsan = std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan");
        let (seeds, steps) = if bsan { (2u64, 100) } else { (64, 400) };
        for seed in 1..=seeds {
            let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let mut c = CSide([ptr::null_mut(); 2 * SLOTS]);
            let mut r = RustSide {
                refs: Default::default(),
                consts: Default::default(),
            };
            for n in 0..steps {
                step(&mut rng, &mut c, &mut r);
                assert_same_state(n, &c, &r);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn zeroed_payloads_match_c_byte_for_byte() {
        let size = size_of::<ffi::AVColorPrimariesDesc>();
        // For the unsafe operations below: an allocation by size, then a read of its `size` zeroed
        // bytes and the release of its one reference.
        let c_bytes = {
            // SAFETY: as the comment above this block states.
            let mut obj = unsafe { ffi::av_refstruct_allocz(size) };
            assert!(!obj.is_null());
            // SAFETY: as the comment above this block states.
            let bytes = unsafe { core::slice::from_raw_parts(obj.cast::<u8>(), size).to_vec() };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_refstruct_exclusive(obj), 1) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_refstruct_unref((&raw mut obj).cast()) };
            assert!(obj.is_null());
            bytes
        };
        let a = RefStructArc::<ColorPrimariesDescriptor>::new_zeroed().unwrap();
        let raw = a.as_ref().get().into_raw();
        // SAFETY: a plain `AVColorPrimariesDesc` value viewed as its bytes.
        let r_bytes = unsafe { core::slice::from_raw_parts((&raw const raw).cast::<u8>(), size) };
        assert_eq!(c_bytes, r_bytes);
        assert!(is_exclusive(&a));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replace_of_the_last_reference_releases_like_c() {
        // C: the slot's object goes when replaced by another, as the Rust slot's.
        // For the unsafe operations below: two fresh objects, each reference released exactly once.
        {
            // SAFETY: as the comment above this block states.
            let mut x = unsafe { ffi::av_refstruct_allocz(8) };
            // SAFETY: as the comment above this block states.
            let mut y = unsafe { ffi::av_refstruct_allocz(8) };
            // SAFETY: as the comment above this block states.
            let mut slot = unsafe { ffi::av_refstruct_ref(y) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_refstruct_unref((&raw mut y).cast()) };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_refstruct_exclusive(slot), 1) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_refstruct_replace((&raw mut slot).cast(), x) };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_refstruct_exclusive(x), 0) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_refstruct_replace((&raw mut slot).cast(), ptr::null()) };
            assert!(slot.is_null());
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(ffi::av_refstruct_exclusive(x), 1) };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_refstruct_unref((&raw mut x).cast()) };
        }
        let x = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
        let mut slot = Some(RefStructArc::<IntFloat64>::new_zeroed().unwrap());
        assert!(is_exclusive(slot.as_ref().unwrap()));
        replace(&mut slot, Some(&x));
        assert!(!is_exclusive(&x));
        replace(&mut slot, None);
        assert!(slot.is_none());
        assert!(is_exclusive(&x));
    }
}

/// Buffer-frame wave 1, symbol batch 1: objects with a release closure
/// against `av_refstruct_alloc_ext` with a C callback, and typed pools against
/// C pools of the same size, flags and callbacks, step by step. Both sides
/// log every callback with the payload it saw; after each step the logs, the
/// gets' outcomes and every held payload must agree.
///
/// Where C leaves a new entry's bytes unspecified (`NO_ZEROING`, and
/// `ZERO_EVERY_TIME` before `init`), the typed pool zeroes them: the init
/// callback's view of a new entry is masked there, and so is a new entry C
/// hands out unzeroed, which is told apart from a reused one by the free list
/// the sequence tracks.
mod releases_and_typed_pools {
    use core::ffi::c_void;
    use core::ptr;
    use std::cell::{Cell, RefCell};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::intfloat_h::{IntFloat64, IntFloat64Mut};
    use ffmpeg_bindings::libavutil::libavutil::refstruct::{
        PoolCallbacks, PoolFlags, RefStructArc, RefStructBox, TypedRefStructPool,
        alloc_with_release, is_exclusive,
    };

    type Log = Vec<(&'static str, u64)>;

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    thread_local! {
        /// What the C callbacks saw, on the thread running the sequence.
        static C_LOG: RefCell<Log> = const { RefCell::new(Vec::new()) };
        /// Whether the C init callback fails.
        static C_FAIL: Cell<bool> = const { Cell::new(false) };
        /// The id the C init callback writes next.
        static C_NEXT: Cell<u64> = const { Cell::new(1) };
        /// What the sequences exercised: callbacks by tag, failed and
        /// successful gets.
        static SEEN: RefCell<std::collections::BTreeMap<&'static str, usize>> =
            const { RefCell::new(std::collections::BTreeMap::new()) };
    }

    fn count(what: &'static str) {
        SEEN.with(|s| *s.borrow_mut().entry(what).or_default() += 1);
    }

    fn c_log(tag: &'static str, value: u64) {
        C_LOG.with(|l| l.borrow_mut().push((tag, value)));
    }

    fn read(obj: *mut c_void) -> u64 {
        // SAFETY: every object here is an 8-byte payload the caller holds,
        // initialised before it is read (unzeroed ones are only read by the
        // init callbacks, whose value is masked, as bytes).
        unsafe { obj.cast::<u64>().read() }
    }

    fn write(obj: *mut c_void, value: u64) {
        // SAFETY: an 8-byte payload the caller holds.
        unsafe { obj.cast::<u64>().write(value) };
    }

    fn unref(obj: *mut c_void) {
        let mut slot = obj;
        // SAFETY: one counted reference the caller holds.
        unsafe { ffi::av_refstruct_unref((&raw mut slot).cast()) };
    }

    unsafe extern "C" fn c_release(_: ffi::AVRefStructOpaque, obj: *mut c_void) {
        c_log("release", read(obj));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn release_runs_where_cs_free_callback_runs() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for _ in 0..300 {
            let refs = 1 + rng.below(5);
            let value = rng.next();

            // SAFETY: an 8-byte zeroed object whose callback only logs.
            let c = unsafe { ffi::av_refstruct_alloc_ext(8, 0, ptr::null_mut(), Some(c_release)) };
            assert!(!c.is_null());
            let rust_log = Arc::new(Mutex::new(Log::new()));
            let log = Arc::clone(&rust_log);
            let mut b = alloc_with_release::<IntFloat64, _>(move |last| {
                log.lock().unwrap().push(("release", last.as_ref().bits()));
            })
            .unwrap();
            assert_eq!((read(c), b.as_ref().bits()), (0, 0));
            write(c, value);
            b.as_mut().set_bits(value);

            let mut c_refs = vec![c];
            let mut r_refs = vec![RefStructArc::from(b)];
            for _ in 1..refs {
                // SAFETY: `c` is live while `c_refs` holds a reference.
                c_refs.push(unsafe { ffi::av_refstruct_ref(c) });
                r_refs.push(r_refs[0].clone());
            }
            while !c_refs.is_empty() {
                let i = rng.below(c_refs.len());
                if rng.below(2) == 0 {
                    // A write through a sole reference before releasing it.
                    let v = rng.next();
                    if let Some(mut m) = r_refs[i].get_mut() {
                        m.set_bits(v);
                        write(c_refs[i], v);
                    }
                }
                unref(c_refs.swap_remove(i));
                drop(r_refs.swap_remove(i));
                let theirs = C_LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
                let ours = std::mem::take(&mut *rust_log.lock().unwrap());
                assert_eq!(ours, theirs);
                if let Some(r) = r_refs.first() {
                    // SAFETY: `c` is live while `c_refs` holds a reference.
                    let c_sole = unsafe { ffi::av_refstruct_exclusive(c_refs[0]) } == 1;
                    assert_eq!(is_exclusive(r), c_sole);
                    assert_eq!(r.as_ref().bits(), read(c_refs[0]));
                }
            }
            assert_eq!(Arc::strong_count(&rust_log), 1, "the release was dropped");
        }
    }

    /// The Rust callbacks' shared state; dropping it is the pool's `free_cb`.
    struct Shared {
        log: Mutex<Log>,
        fail: AtomicBool,
        next: AtomicU64,
    }

    struct State(Arc<Shared>);

    impl Drop for State {
        fn drop(&mut self) {
            self.0.log.lock().unwrap().push(("pool_free", 0));
        }
    }

    unsafe extern "C" fn c_init(_: ffi::AVRefStructOpaque, obj: *mut c_void) -> i32 {
        c_log("init", read(obj));
        write(obj, C_NEXT.with(|n| n.replace(n.get() + 1)));
        if C_FAIL.with(Cell::get) { -22 } else { 0 }
    }

    unsafe extern "C" fn c_reset(_: ffi::AVRefStructOpaque, obj: *mut c_void) {
        let v = read(obj);
        c_log("reset", v);
        write(obj, v ^ 0xFFFF_0000);
    }

    unsafe extern "C" fn c_free_entry(_: ffi::AVRefStructOpaque, obj: *mut c_void) {
        c_log("free_entry", read(obj));
    }

    unsafe extern "C" fn c_pool_free(_: ffi::AVRefStructOpaque) {
        c_log("pool_free", 0);
    }

    fn r_init(s: &State, mut e: IntFloat64Mut<'_>) -> bool {
        s.0.log.lock().unwrap().push(("init", e.as_ref().bits()));
        e.set_bits(s.0.next.fetch_add(1, Ordering::Relaxed));
        !s.0.fail.load(Ordering::Relaxed)
    }

    fn r_reset(s: &State, mut e: IntFloat64Mut<'_>) {
        let v = e.as_ref().bits();
        s.0.log.lock().unwrap().push(("reset", v));
        e.set_bits(v ^ 0xFFFF_0000);
    }

    fn r_free_entry(s: &State, e: IntFloat64Mut<'_>) {
        s.0.log
            .lock()
            .unwrap()
            .push(("free_entry", e.as_ref().bits()));
    }

    const SLOTS: usize = 4;

    /// The pool constructors compared: `Plain` is `TypedRefStructPool::new`
    /// against `av_refstruct_pool_alloc`; `Callbacks(mask)` is
    /// `with_callbacks` with the init (1), reset (2) and entry-freeing (4)
    /// callbacks of `mask` against `av_refstruct_pool_alloc_ext` with the
    /// same C callbacks and a pool-freeing one.
    #[derive(Clone, Copy, Debug)]
    enum Kind {
        Plain,
        Callbacks(u8),
    }

    struct Run {
        flags: PoolFlags,
        kind: Kind,
        c_pool: *mut ffi::AVRefStructPool,
        c_slots: [*mut c_void; SLOTS],
        /// The C entries returned to the pool while it is alive, in order.
        c_free: Vec<*mut c_void>,
        r_pool: Option<TypedRefStructPool<IntFloat64>>,
        r_slots: [Option<RefStructBox<IntFloat64>>; SLOTS],
        shared: Arc<Shared>,
    }

    impl Run {
        fn new(flags: PoolFlags, kind: Kind) -> Self {
            C_NEXT.with(|n| n.set(1));
            C_FAIL.with(|f| f.set(false));
            let shared = Arc::new(Shared {
                log: Mutex::new(Vec::new()),
                fail: AtomicBool::new(false),
                next: AtomicU64::new(1),
            });
            let (c_pool, r_pool) = match kind {
                Kind::Plain => (
                    // SAFETY: no callbacks; any size and flags.
                    unsafe { ffi::av_refstruct_pool_alloc(8, flags.bits()) },
                    TypedRefStructPool::<IntFloat64>::new(flags),
                ),
                Kind::Callbacks(mask) => {
                    let has = |bit: u8| mask & bit != 0;
                    let mut cbs = PoolCallbacks::new(State(Arc::clone(&shared)));
                    if has(1) {
                        cbs = cbs.init(r_init);
                    }
                    if has(2) {
                        cbs = cbs.reset(r_reset);
                    }
                    if has(4) {
                        cbs = cbs.free_entry(r_free_entry);
                    }
                    // SAFETY: callbacks that only touch this thread's
                    // thread-locals and the 8-byte entries they are given;
                    // every call happens on this thread.
                    let c = unsafe {
                        ffi::av_refstruct_pool_alloc_ext(
                            8,
                            flags.bits(),
                            ptr::null_mut(),
                            if has(1) { Some(c_init) } else { None },
                            if has(2) { Some(c_reset) } else { None },
                            if has(4) { Some(c_free_entry) } else { None },
                            Some(c_pool_free),
                        )
                    };
                    (c, TypedRefStructPool::with_callbacks(flags, cbs))
                }
            };
            assert!(!c_pool.is_null() && r_pool.is_some());
            Self {
                flags,
                kind,
                c_pool,
                c_slots: [ptr::null_mut(); SLOTS],
                c_free: Vec::new(),
                r_pool,
                r_slots: Default::default(),
                shared,
            }
        }

        fn has_init(&self) -> bool {
            matches!(self.kind, Kind::Callbacks(m) if m & 1 != 0)
        }

        /// Whether C may hand `init` an entry it has not zeroed.
        fn init_sees_garbage(&self) -> bool {
            self.flags.contains(PoolFlags::NO_ZEROING)
                || self.flags.contains(PoolFlags::ZERO_EVERY_TIME)
        }

        fn get(&mut self, i: usize) {
            // SAFETY: the C pool is live: the run releases it only after
            // its last get.
            let c = unsafe { ffi::av_refstruct_pool_get(self.c_pool) }.cast::<c_void>();
            let r = self.r_pool.as_ref().unwrap().get();
            assert_eq!(
                r.is_some(),
                !c.is_null(),
                "{:?} {:?}",
                self.flags,
                self.kind
            );
            if c.is_null() {
                count("failed get");
                return;
            }
            count("get");
            let reused = match self.c_free.iter().rposition(|&e| e == c) {
                Some(at) => {
                    self.c_free.remove(at);
                    true
                }
                None => false,
            };
            let c_unzeroed = self.flags.contains(PoolFlags::NO_ZEROING)
                && !self.flags.contains(PoolFlags::ZERO_EVERY_TIME)
                && !self.has_init();
            if !reused && c_unzeroed {
                // Unspecified in C; the typed pool hands out zeroes.
                write(c, 0);
            }
            self.c_slots[i] = c;
            self.r_slots[i] = r;
        }

        fn write(&mut self, i: usize, v: u64) {
            write(self.c_slots[i], v);
            self.r_slots[i].as_mut().unwrap().as_mut().set_bits(v);
        }

        fn put(&mut self, i: usize) {
            let c = std::mem::replace(&mut self.c_slots[i], ptr::null_mut());
            if !self.c_pool.is_null() {
                self.c_free.push(c);
            }
            unref(c);
            self.r_slots[i] = None;
        }

        fn release_pool(&mut self) {
            let mut slot = std::mem::replace(&mut self.c_pool, ptr::null_mut());
            // SAFETY: the run's one reference to the live C pool.
            unsafe { ffi::av_refstruct_pool_uninit(&raw mut slot) };
            self.c_free.clear();
            self.r_pool = None;
        }

        fn set_fail(&mut self, fail: bool) {
            C_FAIL.with(|f| f.set(fail));
            self.shared.fail.store(fail, Ordering::Relaxed);
        }

        fn check(&self, step: usize) {
            let mask = |log: Log| -> Log {
                log.into_iter()
                    .map(|(t, v)| {
                        (
                            t,
                            if t == "init" && self.init_sees_garbage() {
                                0
                            } else {
                                v
                            },
                        )
                    })
                    .collect()
            };
            let theirs = mask(C_LOG.with(|l| std::mem::take(&mut *l.borrow_mut())));
            let ours = mask(std::mem::take(&mut *self.shared.log.lock().unwrap()));
            theirs.iter().for_each(|&(t, _)| count(t));
            let at = (step, self.flags, self.kind);
            assert_eq!(ours, theirs, "{at:?}");
            for (c, r) in self.c_slots.iter().zip(&self.r_slots) {
                assert_eq!(
                    r.as_ref().map(|r| r.as_ref().bits()),
                    (!c.is_null()).then(|| read(*c)),
                    "{at:?}"
                );
            }
        }
    }

    impl Drop for Run {
        fn drop(&mut self) {
            for i in 0..SLOTS {
                if !self.c_slots[i].is_null() {
                    self.put(i);
                }
            }
            if !self.c_pool.is_null() {
                self.release_pool();
            }
        }
    }

    fn flag_sets() -> impl Iterator<Item = PoolFlags> {
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

    fn kinds() -> impl Iterator<Item = Kind> {
        core::iter::once(Kind::Plain).chain((0..8).map(Kind::Callbacks))
    }

    /// Random get / write / return / fail-toggle sequences, then the pool's
    /// release with entries out, then their return, for every flag set and
    /// every callback subset.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn typed_pools_follow_c_pools_step_by_step() {
        let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
        for flags in flag_sets() {
            for kind in kinds() {
                for _ in 0..4 {
                    let mut run = Run::new(flags, kind);
                    let steps = 20 + rng.below(40);
                    for step in 0..steps {
                        let i = rng.below(SLOTS);
                        let live = !run.c_pool.is_null();
                        match rng.below(10) {
                            0..=3 if live && run.c_slots[i].is_null() => run.get(i),
                            4..=5 if !run.c_slots[i].is_null() => run.write(i, rng.next()),
                            6..=7 if !run.c_slots[i].is_null() => run.put(i),
                            8 => run.set_fail(rng.below(3) == 0),
                            9 if live && step > steps / 2 => run.release_pool(),
                            _ => {}
                        }
                        run.check(step);
                    }
                    for i in 0..SLOTS {
                        if !run.c_slots[i].is_null() {
                            run.put(i);
                            run.check(steps + i);
                        }
                    }
                    if !run.c_pool.is_null() {
                        run.release_pool();
                    }
                    run.check(steps + SLOTS);
                    drop(run);
                }
            }
        }
        let seen = SEEN.with(|s| s.take());
        for what in [
            "get",
            "failed get",
            "init",
            "reset",
            "free_entry",
            "pool_free",
        ] {
            assert!(
                seen.get(what).copied().unwrap_or(0) >= 50,
                "{what}: {seen:?}"
            );
        }
    }

    /// A typed pool's reuse order and payloads through a fixed sequence,
    /// spelled out: the most recently returned entry comes back first, as
    /// it was left, or zeroed under `ZERO_EVERY_TIME`.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reuse_order_and_contents_match_c() {
        for flags in [PoolFlags::NONE, PoolFlags::ZERO_EVERY_TIME] {
            let mut run = Run::new(flags, Kind::Plain);
            for i in 0..3 {
                run.get(i);
                run.write(i, 10 + i as u64);
            }
            run.check(0);
            run.put(0);
            run.put(2);
            run.get(3);
            run.get(0);
            run.check(1);
            let expected = if flags.contains(PoolFlags::ZERO_EVERY_TIME) {
                [0, 0]
            } else {
                [12, 10]
            };
            assert_eq!(
                [3, 0].map(|i| run.r_slots[i].as_ref().unwrap().as_ref().bits()),
                expected
            );
        }
    }
}
