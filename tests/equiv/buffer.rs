//! Pools built by the safe constructors behave as pools C builds itself:
//! the same buffers handed out, recycled and counted after every step of a
//! request/reference/release sequence, the same allocator calls, and the
//! allocator state released at the same point.
//!
//! Both sides are driven through C's `av_buffer_pool_get` and the raw
//! `AVBufferRef` routines until the pool's `get` has a safe wrapper; the Rust
//! side reaches its pool through the owner's raw seam, and its allocators
//! return safe `UninitBuffer`s.

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox, BufferReference};
use ffmpeg::libavutil::ffi;

/// One step of a sequence; indices pick a held reference modulo their count.
#[derive(Clone, Copy, Debug)]
enum Op {
    Get,
    Ref(usize),
    Unref(usize),
}

/// The ops of a deterministic xorshift sequence, requests weighted up.
fn ops(seed: u64, len: usize) -> Vec<Op> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let k = (x >> 8) as usize;
            match x % 7 {
                0..=2 => Op::Get,
                3 => Op::Ref(k),
                _ => Op::Unref(k),
            }
        })
        .collect()
}

/// What C reports for one held reference: the first-seen index of its data
/// (so recycling shows as a repeated index), its size, count and
/// writability.
type Held = (usize, usize, i32, i32);

/// One side's pool and the references taken from it.
struct Side {
    pool: *mut ffi::AVBufferPool,
    held: Vec<*mut ffi::AVBufferRef>,
    seen: Vec<*mut u8>,
}

impl Side {
    fn new(pool: *mut ffi::AVBufferPool) -> Self {
        assert!(!pool.is_null());
        Self {
            pool,
            held: Vec::new(),
            seen: Vec::new(),
        }
    }

    /// Runs `op`; `Get` reports whether C returned a reference.
    fn step(&mut self, op: Op) -> bool {
        match op {
            Op::Get => {
                // SAFETY: the pool is live until `finish`.
                let buf = unsafe { ffi::av_buffer_pool_get(self.pool) };
                if buf.is_null() {
                    return false;
                }
                // SAFETY: a fresh, live reference.
                let data = unsafe { (*buf).data };
                if !self.seen.contains(&data) {
                    self.seen.push(data);
                }
                self.held.push(buf);
                true
            }
            Op::Ref(k) if !self.held.is_empty() => {
                let src = self.held[k % self.held.len()];
                // SAFETY: `src` is a live reference this side holds.
                let buf = unsafe { ffi::av_buffer_ref(src) };
                assert!(!buf.is_null());
                self.held.push(buf);
                true
            }
            Op::Unref(k) if !self.held.is_empty() => {
                let mut buf = self.held.swap_remove(k % self.held.len());
                // SAFETY: a reference this side holds, released once.
                unsafe { ffi::av_buffer_unref(&mut buf) };
                true
            }
            _ => true,
        }
    }

    fn snapshot(&self) -> Vec<Held> {
        self.held
            .iter()
            .map(|&buf| {
                // SAFETY: every held reference is live.
                unsafe {
                    let index = self.seen.iter().position(|&d| d == (*buf).data).unwrap();
                    (
                        index,
                        (*buf).size,
                        ffi::av_buffer_get_ref_count(buf),
                        ffi::av_buffer_is_writable(buf),
                    )
                }
            })
            .collect()
    }

    /// Writes every byte each held reference describes.
    fn fill(&self) {
        for &buf in &self.held {
            // SAFETY: a live pool reference describes `size` bytes; writes to
            // a shared one race with nothing in this single-threaded test.
            unsafe { ptr::write_bytes((*buf).data, 0xa5, (*buf).size) };
        }
    }
}

/// Drives both sides through `ops` and compares after every step; `observe`
/// compares whatever else the caller tracks (allocator calls, releases).
fn run(rust: &mut Side, c: &mut Side, ops: &[Op], observe: &mut dyn FnMut(&str)) {
    for (i, &op) in ops.iter().enumerate() {
        assert_eq!(rust.step(op), c.step(op), "step {i}: {op:?}");
        assert_eq!(rust.snapshot(), c.snapshot(), "step {i}: {op:?}");
        assert_eq!(rust.seen.len(), c.seen.len(), "step {i}: {op:?}");
        observe(&format!("step {i}: {op:?}"));
    }
    rust.fill();
    c.fill();
}

/// Releases both pools' owners with outstanding buffers, then the buffers.
fn finish(rust: &mut Side, owner: BufferPoolBox, c: &mut Side, observe: &mut dyn FnMut(&str)) {
    drop(owner);
    // SAFETY: C's own pool reference, released once; the slot is NULLed.
    unsafe { ffi::av_buffer_pool_uninit(&mut c.pool) };
    assert!(c.pool.is_null());
    observe("after the owners");
    assert_eq!(rust.snapshot(), c.snapshot());
    rust.fill();
    c.fill();
    while !rust.held.is_empty() {
        rust.step(Op::Unref(0));
        c.step(Op::Unref(0));
        assert_eq!(rust.snapshot(), c.snapshot());
        observe("draining");
    }
}

const SIZES: [usize; 5] = [0, 1, 7, 64, 4096];

#[test]
fn default_pools_hand_out_recycle_and_count_as_c() {
    for size in SIZES {
        for seed in 1..=6 {
            let owner = BufferPool::new(size).unwrap();
            let mut rust = Side::new(owner.as_c_ptr());
            // SAFETY: a NULL allocator selects av_buffer_alloc.
            let mut c = Side::new(unsafe { ffi::av_buffer_pool_init(size, None) });
            let ops = ops(seed * 0x9e37_79b9 + size as u64, 120);
            run(&mut rust, &mut c, &ops, &mut |_| {});
            finish(&mut rust, owner, &mut c, &mut |_| {});
        }
    }
}

#[test]
fn owners_released_first_and_last_as_c() {
    for size in SIZES {
        // Released with nothing outstanding, with only free buffers, and with
        // free and outstanding ones.
        for gets in [0, 3] {
            for keep in [0, 1, 3] {
                let owner = BufferPool::new(size).unwrap();
                let mut rust = Side::new(owner.as_c_ptr());
                // SAFETY: as above.
                let mut c = Side::new(unsafe { ffi::av_buffer_pool_init(size, None) });
                let mut seq = vec![Op::Get; gets];
                seq.extend((keep.min(gets)..gets).map(|_| Op::Unref(0)));
                run(&mut rust, &mut c, &seq, &mut |_| {});
                finish(&mut rust, owner, &mut c, &mut |_| {});
            }
        }
    }
}

/// C's `init2` allocator state: calls, `pool_free` calls, and a NULL period.
#[repr(C)]
struct CState {
    calls: AtomicUsize,
    freed: AtomicUsize,
    fail_every: usize,
}

unsafe extern "C" fn c_alloc2(opaque: *mut c_void, size: usize) -> *mut ffi::AVBufferRef {
    // SAFETY: `opaque` is the test's live `CState`, kept until after the pool
    // is freed.
    let state = unsafe { &*opaque.cast::<CState>() };
    let n = state.calls.fetch_add(1, Ordering::SeqCst) + 1;
    if state.fail_every != 0 && n % state.fail_every == 0 {
        return ptr::null_mut();
    }
    // SAFETY: no preconditions.
    unsafe { ffi::av_buffer_alloc(size) }
}

unsafe extern "C" fn c_pool_free(opaque: *mut c_void) {
    // SAFETY: as in `c_alloc2`.
    unsafe { &*opaque.cast::<CState>() }
        .freed
        .fetch_add(1, Ordering::SeqCst);
}

/// Counts the drops of the Rust allocator state.
struct DropCount(Arc<AtomicUsize>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn allocator_pools_call_and_release_their_state_as_c_init2() {
    for size in [0, 16, 1000] {
        for fail_every in [0, 1, 2, 3] {
            for seed in 1..=4 {
                let calls = Arc::new(AtomicUsize::new(0));
                let drops = Arc::new(AtomicUsize::new(0));
                let (seen, guard) = (calls.clone(), DropCount(drops.clone()));
                // SAFETY: each buffer holds `size` bytes from its start; the
                // pool is never touched.
                let owner = unsafe {
                    BufferPool::with_allocator(size, move |n| {
                        let _ = &guard;
                        let k = seen.fetch_add(1, Ordering::SeqCst) + 1;
                        if fail_every != 0 && k % fail_every == 0 {
                            None
                        } else {
                            BufferReference::alloc(n)
                        }
                    })
                }
                .unwrap();
                let state = Box::into_raw(Box::new(CState {
                    calls: AtomicUsize::new(0),
                    freed: AtomicUsize::new(0),
                    fail_every,
                }));
                // SAFETY: `state` outlives the pool (freed below, after
                // `c_pool_free` ran); the callbacks match init2's contract.
                let c_pool = unsafe {
                    ffi::av_buffer_pool_init2(size, state.cast(), Some(c_alloc2), Some(c_pool_free))
                };
                let mut rust = Side::new(owner.as_c_ptr());
                let mut c = Side::new(c_pool);
                let mut observe = |at: &str| {
                    // SAFETY: `state` is live until the end of this iteration.
                    let s = unsafe { &*state };
                    assert_eq!(
                        calls.load(Ordering::SeqCst),
                        s.calls.load(Ordering::SeqCst),
                        "{at}"
                    );
                    assert_eq!(
                        drops.load(Ordering::SeqCst),
                        s.freed.load(Ordering::SeqCst),
                        "{at}"
                    );
                };
                let ops = ops(seed * 7919 + fail_every as u64, 90);
                run(&mut rust, &mut c, &ops, &mut observe);
                finish(&mut rust, owner, &mut c, &mut observe);
                assert_eq!(drops.load(Ordering::SeqCst), 1);
                // SAFETY: both pools are freed; `state` is ours again.
                drop(unsafe { Box::from_raw(state) });
            }
        }
    }
}

/// `av_buffer_pool_init`'s custom allocator, counted (only this test uses it).
static INIT_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn c_alloc(size: usize) -> *mut ffi::AVBufferRef {
    INIT_CALLS.fetch_add(1, Ordering::SeqCst);
    // SAFETY: no preconditions.
    unsafe { ffi::av_buffer_alloc(size) }
}

#[test]
fn stateless_allocators_as_c_init() {
    for size in [1, 512] {
        INIT_CALLS.store(0, Ordering::SeqCst);
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        // SAFETY: as in `allocator_pools_call_and_release_their_state_as_c_init2`.
        let owner = unsafe {
            BufferPool::with_allocator(size, move |n| {
                seen.fetch_add(1, Ordering::SeqCst);
                BufferReference::alloc(n)
            })
        }
        .unwrap();
        // SAFETY: `c_alloc` returns NULL or a sole, writable `size`-byte
        // reference.
        let mut c = Side::new(unsafe { ffi::av_buffer_pool_init(size, Some(c_alloc)) });
        let mut rust = Side::new(owner.as_c_ptr());
        let mut observe = |at: &str| {
            assert_eq!(
                calls.load(Ordering::SeqCst),
                INIT_CALLS.load(Ordering::SeqCst),
                "{at}"
            );
        };
        run(&mut rust, &mut c, &ops(size as u64 + 3, 100), &mut observe);
        finish(&mut rust, owner, &mut c, &mut observe);
    }
}

/// Buffer-frame wave 0, symbol batch 5: `av_buffer_default_free`'s release
/// against the C callback.
mod default_release {
    use core::ptr;

    use ffmpeg::libavutil::buffer::default_free;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

    #[test]
    fn default_free_releases_as_the_c_callback() {
        for len in [0usize, 1, 7, 64, 4096] {
            // SAFETY: an `av_malloc` block of `len` bytes (or NULL, for 0 on a
            // failing allocation), released once by the default callback, which
            // ignores its opaque.
            unsafe {
                let data = ffi::av_malloc(len).cast::<u8>();
                ffi::av_buffer_default_free(ptr::null_mut(), data);
            }
            default_free(alloc_array_zeroed::<u8>(len).unwrap_or_else(HeapVec::empty));
        }
        // NULL data is a no-op on both sides.
        // SAFETY: `av_free(NULL)` does nothing.
        unsafe { ffi::av_buffer_default_free(ptr::null_mut(), ptr::null_mut()) };
        default_free(HeapVec::<u8>::empty());
    }
}

/// Buffer-frame wave 1, batch 0: `BufferReference` owners against raw C
/// references built the same way (`av_buffer_alloc` + memset,
/// `av_buffer_allocz`, `av_buffer_create` with the default or a counting
/// release, read-only or not), driven through the same random sequence of
/// clones, releases, narrowings, writes and replacements, and compared after
/// every step: which buffer each reference counts and where its window
/// starts, its bytes, its buffer's count and writability, and how many
/// releases ran.
mod reference {
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use ffmpeg::libavutil::buffer::{BufferReference, BufferReferenceBox, UninitBuffer};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapVec;

    #[derive(Clone, Copy, Debug)]
    enum Kind {
        Alloc(u8),
        AllocZeroed,
        Heap,
        Owner,
        ReadOnly,
    }

    /// One step; indices pick a held reference modulo their count.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        New(Kind, usize),
        Clone(usize),
        Drop(usize),
        Advance(usize, usize),
        Truncate(usize, usize),
        Write(usize, u8),
        Refill(usize, u8),
        Replace(usize, usize),
    }

    const SIZES: [usize; 5] = [0, 1, 5, 64, 300];

    fn xorshift(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }

    fn ops(seed: u64, len: usize) -> Vec<Op> {
        let mut x = seed | 1;
        (0..len)
            .map(|_| {
                let r = xorshift(&mut x);
                let (a, b) = ((r >> 16) as usize, (r >> 40) as usize);
                match r % 11 {
                    0 | 1 => {
                        let kind = match a % 5 {
                            0 => Kind::Alloc(b as u8),
                            1 => Kind::AllocZeroed,
                            2 => Kind::Heap,
                            3 => Kind::Owner,
                            _ => Kind::ReadOnly,
                        };
                        Op::New(kind, SIZES[b % SIZES.len()])
                    }
                    2 | 3 => Op::Clone(a),
                    4 => Op::Drop(a),
                    5 => Op::Advance(a, b),
                    6 => Op::Truncate(a, b),
                    7 => Op::Write(a, b as u8),
                    8 => Op::Refill(a, b as u8),
                    _ => Op::Replace(a, b),
                }
            })
            .collect()
    }

    /// `len` bytes that differ from one `New` to the next.
    fn pattern(len: usize, seed: usize) -> Vec<u8> {
        let mut v = (0..=255u8)
            .collect::<Vec<_>>()
            .repeat(len.div_ceil(256) + 1);
        v.drain(..seed % 256);
        v.truncate(len);
        v
    }

    /// What C reports for one reference: the index of the buffer it counts
    /// (in creation order), its window's offset from that buffer's first
    /// data pointer, its bytes, the buffer's count and writability.
    type Seen = (usize, usize, Vec<u8>, c_int, c_int);

    /// The buffers a side created, newest last, with their first data.
    #[derive(Default)]
    struct Buffers(Vec<(*mut ffi::AVBuffer, *mut u8)>);

    impl Buffers {
        /// Reads `r` as C sees it.
        ///
        /// # Safety
        ///
        /// `r` must be a live reference.
        unsafe fn see(&self, r: *const ffi::AVBufferRef) -> Seen {
            // SAFETY: the caller passes a live reference, whose window holds
            // `size` initialised bytes.
            unsafe {
                let (buffer, data, size) = ((*r).buffer, (*r).data, (*r).size);
                let k = self.0.iter().rposition(|&(b, _)| b == buffer).unwrap();
                let bytes = if size == 0 {
                    Vec::new()
                } else {
                    core::slice::from_raw_parts(data, size).to_vec()
                };
                (
                    k,
                    (data as usize).wrapping_sub(self.0[k].1 as usize),
                    bytes,
                    ffi::av_buffer_get_ref_count(r),
                    ffi::av_buffer_is_writable(r),
                )
            }
        }

        /// Records the buffer `r` counts as the newest.
        ///
        /// # Safety
        ///
        /// `r` must be a live reference.
        unsafe fn record(&mut self, r: *const ffi::AVBufferRef) {
            // SAFETY: as in `see`.
            self.0.push(unsafe { ((*r).buffer, (*r).data) });
        }
    }

    /// The Rust side's bytes, counting their releases.
    struct Counted {
        bytes: Vec<u8>,
        frees: Arc<AtomicUsize>,
    }

    impl AsMut<[u8]> for Counted {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.bytes
        }
    }

    impl AsRef<[u8]> for Counted {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            self.frees.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The C side's release: frees the av_malloc copy, counting.
    unsafe extern "C" fn c_counted_free(opaque: *mut c_void, data: *mut u8) {
        // SAFETY: `opaque` is the test's live counter; `data` is the block
        // `c_new` allocated for this buffer.
        unsafe {
            (*opaque.cast::<AtomicUsize>()).fetch_add(1, Ordering::SeqCst);
            ffi::av_free(data.cast());
        }
    }

    struct RustSide {
        held: Vec<BufferReferenceBox>,
        buffers: Buffers,
        frees: Arc<AtomicUsize>,
    }

    struct CSide {
        held: Vec<*mut ffi::AVBufferRef>,
        buffers: Buffers,
        frees: Arc<AtomicUsize>,
    }

    impl RustSide {
        fn new_ref(&self, kind: Kind, size: usize, seed: usize) -> BufferReferenceBox {
            let counted = || Counted {
                bytes: pattern(size, seed),
                frees: self.frees.clone(),
            };
            match kind {
                Kind::Alloc(byte) => BufferReference::alloc(size).unwrap().fill(byte),
                Kind::AllocZeroed => BufferReference::alloc_zeroed(size).unwrap(),
                Kind::Heap => {
                    let heap =
                        HeapVec::from_slice(&pattern(size, seed)).unwrap_or_else(HeapVec::empty);
                    BufferReference::from_heap_vec(heap).unwrap()
                }
                Kind::Owner => BufferReference::from_owner(counted())
                    .map_err(drop)
                    .unwrap(),
                Kind::ReadOnly => BufferReference::from_read_only(counted())
                    .map_err(drop)
                    .unwrap(),
            }
        }

        /// Runs `op`; reports what a write found.
        fn step(&mut self, op: Op, seed: usize) -> Option<bool> {
            let n = self.held.len();
            match op {
                Op::New(kind, size) => {
                    let r = self.new_ref(kind, size, seed);
                    // SAFETY: a live reference.
                    unsafe { self.buffers.record(r.as_c_ptr()) };
                    self.held.push(r);
                }
                _ if n == 0 => {}
                Op::Clone(k) => {
                    let r = self.held[k % n].try_clone().unwrap();
                    self.held.push(r);
                }
                Op::Drop(k) => drop(self.held.swap_remove(k % n)),
                Op::Advance(k, by) => {
                    let mut m = self.held[k % n].as_mut();
                    let by = by % (m.as_ref().len() + 1);
                    m.advance(by);
                }
                Op::Truncate(k, to) => {
                    let mut m = self.held[k % n].as_mut();
                    let to = to % (m.as_ref().len() + 2);
                    m.truncate(to);
                }
                Op::Write(k, byte) => {
                    let mut m = self.held[k % n].as_mut();
                    let bytes = m.data_mut();
                    let wrote = bytes.is_some();
                    if let Some(bytes) = bytes {
                        bytes.copy_from_slice(&vec![byte; bytes.len()]);
                    }
                    return Some(wrote);
                }
                Op::Refill(k, byte) => {
                    let r = self.held.remove(k % n);
                    let (r, wrote) = match UninitBuffer::try_from(r) {
                        Ok(u) => (u.fill(byte), true),
                        Err(r) => (r, false),
                    };
                    self.held.insert(k % n, r);
                    return Some(wrote);
                }
                Op::Replace(k, j) => {
                    let with = self.held[j % n].try_clone().unwrap();
                    self.held[k % n].as_mut().set(with);
                }
            }
            None
        }

        fn snapshot(&self) -> Vec<Seen> {
            let see = |r: &BufferReferenceBox| {
                // SAFETY: every held reference is live.
                unsafe { self.buffers.see(r.as_c_ptr()) }
            };
            self.held.iter().map(see).collect()
        }
    }

    impl CSide {
        fn new_ref(&self, kind: Kind, size: usize, seed: usize) -> *mut ffi::AVBufferRef {
            let copy = |flags: c_int, counted: bool| {
                let p = pattern(size, seed);
                // SAFETY: a fresh av_malloc block of `size` bytes takes the
                // pattern; C owns it from the av_buffer_create on.
                unsafe {
                    let data = ffi::av_malloc(size).cast::<u8>();
                    assert!(!data.is_null());
                    ptr::copy_nonoverlapping(p.as_ptr(), data, size);
                    let (free, opaque) = if counted {
                        let opaque = Arc::as_ptr(&self.frees).cast_mut().cast::<c_void>();
                        (Some(c_counted_free as unsafe extern "C" fn(_, _)), opaque)
                    } else {
                        (None, ptr::null_mut())
                    };
                    ffi::av_buffer_create(data, size, free, opaque, flags)
                }
            };
            let r = match kind {
                Kind::Alloc(byte) => {
                    // SAFETY: a fresh reference to `size` writable bytes.
                    unsafe {
                        let r = ffi::av_buffer_alloc(size);
                        assert!(!r.is_null());
                        ptr::write_bytes((*r).data, byte, size);
                        r
                    }
                }
                // SAFETY: no preconditions.
                Kind::AllocZeroed => unsafe { ffi::av_buffer_allocz(size) },
                Kind::Heap => copy(0, false),
                Kind::Owner => copy(0, true),
                Kind::ReadOnly => copy(ffi::AV_BUFFER_FLAG_READONLY as c_int, true),
            };
            assert!(!r.is_null());
            r
        }

        fn step(&mut self, op: Op, seed: usize) -> Option<bool> {
            let n = self.held.len();
            // SAFETY: every held reference is live and this side's own; each
            // is released once, by `Drop`, `Replace` or `finish`.
            unsafe {
                match op {
                    Op::New(kind, size) => {
                        let r = self.new_ref(kind, size, seed);
                        self.buffers.record(r);
                        self.held.push(r);
                    }
                    _ if n == 0 => {}
                    Op::Clone(k) => {
                        let r = ffi::av_buffer_ref(self.held[k % n]);
                        assert!(!r.is_null());
                        self.held.push(r);
                    }
                    Op::Drop(k) => {
                        let mut r = self.held.swap_remove(k % n);
                        ffi::av_buffer_unref(&mut r);
                    }
                    Op::Advance(k, by) => {
                        let r = self.held[k % n];
                        let by = by % ((*r).size + 1);
                        (*r).data = (*r).data.wrapping_add(by);
                        (*r).size -= by;
                    }
                    Op::Truncate(k, to) => {
                        let r = self.held[k % n];
                        let to = to % ((*r).size + 2);
                        if to < (*r).size {
                            (*r).size = to;
                        }
                    }
                    Op::Write(k, byte) | Op::Refill(k, byte) => {
                        let r = self.held[k % n];
                        let wrote = ffi::av_buffer_is_writable(r) != 0;
                        if wrote {
                            ptr::write_bytes((*r).data, byte, (*r).size);
                        }
                        return Some(wrote);
                    }
                    Op::Replace(k, j) => {
                        let with = ffi::av_buffer_ref(self.held[j % n]);
                        assert!(!with.is_null());
                        ffi::av_buffer_unref(&mut self.held[k % n]);
                        self.held[k % n] = with;
                    }
                }
            }
            None
        }

        fn snapshot(&self) -> Vec<Seen> {
            let see = |&r: &*mut ffi::AVBufferRef| {
                // SAFETY: every held reference is live.
                unsafe { self.buffers.see(r) }
            };
            self.held.iter().map(see).collect()
        }

        fn finish(&mut self) {
            for mut r in self.held.drain(..) {
                // SAFETY: this side's live reference, released once.
                unsafe { ffi::av_buffer_unref(&mut r) };
            }
        }
    }

    fn sides() -> (RustSide, CSide) {
        (
            RustSide {
                held: Vec::new(),
                buffers: Buffers::default(),
                frees: Arc::new(AtomicUsize::new(0)),
            },
            CSide {
                held: Vec::new(),
                buffers: Buffers::default(),
                frees: Arc::new(AtomicUsize::new(0)),
            },
        )
    }

    #[test]
    fn single_references_read_as_c_from_every_constructor() {
        for size in SIZES {
            for kind in [
                Kind::Alloc(0),
                Kind::Alloc(0xff),
                Kind::AllocZeroed,
                Kind::Heap,
                Kind::Owner,
                Kind::ReadOnly,
            ] {
                let (mut rust, mut c) = sides();
                let op = Op::New(kind, size);
                rust.step(op, size);
                c.step(op, size);
                assert_eq!(rust.snapshot(), c.snapshot(), "{op:?}");
                drop(rust.held.pop());
                c.finish();
                let frees = (
                    rust.frees.load(Ordering::SeqCst),
                    c.frees.load(Ordering::SeqCst),
                );
                assert_eq!(frees.0, frees.1, "{op:?}");
            }
        }
    }

    #[test]
    fn sequences_count_write_narrow_and_release_as_c() {
        // Writes that found the reference writable, and that did not.
        let mut outcomes = [0usize; 2];
        for seed in 1..=12u64 {
            let (mut rust, mut c) = sides();
            for (i, op) in ops(seed * 0x9e37_79b9, 160).into_iter().enumerate() {
                let wrote = rust.step(op, i);
                assert_eq!(wrote, c.step(op, i), "seed {seed} step {i}: {op:?}");
                if let Some(w) = wrote {
                    outcomes[usize::from(w)] += 1;
                }
                assert_eq!(
                    rust.snapshot(),
                    c.snapshot(),
                    "seed {seed} step {i}: {op:?}"
                );
                assert_eq!(
                    rust.frees.load(Ordering::SeqCst),
                    c.frees.load(Ordering::SeqCst),
                    "seed {seed} step {i}: {op:?}"
                );
            }
            rust.held.clear();
            c.finish();
            assert_eq!(
                rust.frees.load(Ordering::SeqCst),
                c.frees.load(Ordering::SeqCst)
            );
        }
        assert!(outcomes[0] > 20 && outcomes[1] > 20, "{outcomes:?}");
    }
}
