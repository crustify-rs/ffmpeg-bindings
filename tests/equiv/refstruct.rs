//! Equivalence of `libavutil/refstruct.h`'s wrappers with C on
//! independently owned inputs.

/// Buffer-frame wave 0, type batch 3: `AVRefStructOpaque` as C's union
/// builders and consumers see it, and `AVRefStructPool`'s lifecycle against a
/// pool C allocates and counts itself.
mod pool_and_opaque {
    use core::ffi::c_void;
    use core::ptr;
    use std::cell::{Cell, RefCell};
    use std::sync::Barrier;
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::refstruct::{
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
        unsafe { ffi::av_refstruct_pool_uninit(&mut slot) };
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
        fill(first, 0xab);
        put(first);
        let second = get(p);
        let reused = second == first;
        let seen = bytes(second);
        put(second);
        (fresh, reused, seen)
    }

    #[test]
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
    /// usable; the last one returned frees the pool (LSan otherwise reports
    /// it, ASan any use after free).
    #[test]
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
    /// thread drops its reference first (TSan checks the hand-offs).
    #[test]
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
                let (ours, theirs) = unsafe {
                    (
                        ffi::av_refstruct_pool_get(pool.as_c_ptr()),
                        ffi::av_refstruct_pool_get(c),
                    )
                };
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
        // SAFETY: the test below passes `Cell::as_ptr` of a live
        // `Cell<u64>`, which no Rust access touches during C's call.
        unsafe { slot.write(slot.read() + 1) };
    }

    /// C keeps a pointer to Rust memory in an object's header while Rust
    /// reads and writes the pointee through its own shared borrows, then
    /// writes through it from the free callback: the retained pointer stays
    /// valid across those reborrows (BSan) because it points into a `Cell`.
    #[test]
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
    fn c_rewrites_entries_between_rust_views() {
        let pool = RefStructPool::new(SIZE, PoolFlags::ZERO_EVERY_TIME).expect("pool");
        let e = get(pool.as_c_ptr());
        {
            // SAFETY: the entry is `SIZE` bytes this test holds, zeroed by
            // C, with no other view alive.
            let view = unsafe { core::slice::from_raw_parts_mut(e, SIZE) };
            assert!(view.iter().all(|&b| b == 0));
            view.fill(0x5a);
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

    use ffmpeg::libavutil::csp::ColorPrimariesDescriptor;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::intfloat::IntFloat64;
    use ffmpeg::libavutil::refstruct::{
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
    fn reference_sequences_follow_c() {
        // BSan retags every handle and slot the state checks touch: a few
        // seeds there take minutes, all of them hours.
        let bsan = std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan");
        let (seeds, steps) = if bsan { (2u64, 100) } else { (64, 400) };
        for seed in 1..=seeds {
            let mut rng = XorShift(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
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
    fn zeroed_payloads_match_c_byte_for_byte() {
        let size = size_of::<ffi::AVColorPrimariesDesc>();
        // SAFETY: an allocation by size, then a read of its `size` zeroed bytes
        // and the release of its one reference.
        let c_bytes = unsafe {
            let mut obj = ffi::av_refstruct_allocz(size);
            assert!(!obj.is_null());
            let bytes = core::slice::from_raw_parts(obj.cast::<u8>(), size).to_vec();
            assert_eq!(ffi::av_refstruct_exclusive(obj), 1);
            ffi::av_refstruct_unref((&raw mut obj).cast());
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
    fn replace_of_the_last_reference_releases_like_c() {
        // C: the slot's object goes when replaced by another, as the Rust slot's.
        // SAFETY: two fresh objects, each reference released exactly once.
        unsafe {
            let mut x = ffi::av_refstruct_allocz(8);
            let mut y = ffi::av_refstruct_allocz(8);
            let mut slot = ffi::av_refstruct_ref(y);
            ffi::av_refstruct_unref((&raw mut y).cast());
            assert_eq!(ffi::av_refstruct_exclusive(slot), 1);
            ffi::av_refstruct_replace((&raw mut slot).cast(), x);
            assert_eq!(ffi::av_refstruct_exclusive(x), 0);
            ffi::av_refstruct_replace((&raw mut slot).cast(), ptr::null());
            assert!(slot.is_null());
            assert_eq!(ffi::av_refstruct_exclusive(x), 1);
            ffi::av_refstruct_unref((&raw mut x).cast());
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
