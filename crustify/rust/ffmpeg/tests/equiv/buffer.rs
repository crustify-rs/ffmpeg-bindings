//! Pools built by the safe constructors behave as pools C builds itself:
//! the same buffers handed out, recycled and counted after every step of a
//! request/reference/release sequence, the same allocator calls, and the
//! allocator state released at the same point.
//!
//! The Rust side takes its buffers through the pool's safe `get` (since
//! buffer-frame wave 2), and its allocators return safe `UninitBuffer`s;
//! both sides then hold, clone and release them as raw `AVBufferRef`s.

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

/// One side's pool and the references taken from it: the Rust side's
/// owner, through which it requests buffers, or C's raw pool.
struct Side {
    owner: Option<BufferPoolBox>,
    pool: *mut ffi::AVBufferPool,
    held: Vec<*mut ffi::AVBufferRef>,
    seen: Vec<*mut u8>,
}

impl Side {
    fn new(pool: *mut ffi::AVBufferPool) -> Self {
        assert!(!pool.is_null());
        Self {
            owner: None,
            pool,
            held: Vec::new(),
            seen: Vec::new(),
        }
    }

    fn rust(owner: BufferPoolBox) -> Self {
        Self {
            owner: Some(owner),
            ..Self::new(ptr::NonNull::dangling().as_ptr())
        }
    }

    /// Runs `op`; `Get` reports whether C returned a reference.
    fn step(&mut self, op: Op) -> bool {
        match op {
            Op::Get => {
                let buf = match &self.owner {
                    // The bytes are written whole before they are read.
                    Some(owner) => owner
                        .as_ref()
                        .get()
                        .map_or(ptr::null_mut(), |u| u.fill(0x5a).into_c()),
                    // SAFETY: the pool is live until `finish`.
                    None => unsafe { ffi::av_buffer_pool_get(self.pool) },
                };
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
fn finish(rust: &mut Side, c: &mut Side, observe: &mut dyn FnMut(&str)) {
    drop(rust.owner.take().unwrap());
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
#[cfg_attr(miri, ignore = "calls C")]
fn default_pools_hand_out_recycle_and_count_as_c() {
    for size in SIZES {
        for seed in 1..=6 {
            let owner = BufferPool::new(size).unwrap();
            let mut rust = Side::rust(owner);
            // SAFETY: a NULL allocator selects av_buffer_alloc.
            let mut c = Side::new(unsafe { ffi::av_buffer_pool_init(size, None) });
            let ops = ops(seed * 0x9e37_79b9 + size as u64, 120);
            run(&mut rust, &mut c, &ops, &mut |_| {});
            finish(&mut rust, &mut c, &mut |_| {});
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn owners_released_first_and_last_as_c() {
    for size in SIZES {
        // Released with nothing outstanding, with only free buffers, and with
        // free and outstanding ones.
        for gets in [0, 3] {
            for keep in [0, 1, 3] {
                let owner = BufferPool::new(size).unwrap();
                let mut rust = Side::rust(owner);
                // SAFETY: as above.
                let mut c = Side::new(unsafe { ffi::av_buffer_pool_init(size, None) });
                let mut seq = vec![Op::Get; gets];
                seq.extend((keep.min(gets)..gets).map(|_| Op::Unref(0)));
                run(&mut rust, &mut c, &seq, &mut |_| {});
                finish(&mut rust, &mut c, &mut |_| {});
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
#[cfg_attr(miri, ignore = "calls C")]
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
                let mut rust = Side::rust(owner);
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
                finish(&mut rust, &mut c, &mut observe);
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
#[cfg_attr(miri, ignore = "calls C")]
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
        let mut rust = Side::rust(owner);
        let mut observe = |at: &str| {
            assert_eq!(
                calls.load(Ordering::SeqCst),
                INIT_CALLS.load(Ordering::SeqCst),
                "{at}"
            );
        };
        run(&mut rust, &mut c, &ops(size as u64 + 3, 100), &mut observe);
        finish(&mut rust, &mut c, &mut observe);
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
    #[cfg_attr(miri, ignore = "calls C")]
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
                        let mut wrote = ffi::av_buffer_is_writable(r) != 0;
                        // `UninitBuffer::try_from` also refuses a buffer with
                        // an opaque, whose release may read the bytes.
                        if matches!(op, Op::Refill(..)) {
                            wrote &= ffi::av_buffer_get_opaque(r).is_null();
                        }
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

/// Buffer-frame wave 2, batch 1: references from the safe constructors and
/// `alloc_resizable`, against raw C references built the same way, driven
/// through the same random sequence of clones, releases and narrowings and
/// of the mutators: `make_mut` (`av_buffer_make_writable`), `resize`
/// (`av_buffer_realloc`), `assign` and its slot form
/// (`av_buffer_replace`), each followed by a write of the bytes it made
/// writable or new. After every step both sides compare each reference's
/// buffer (by creation order, a resized buffer keeping its place) and window
/// offset, bytes, count, writability and whether it has an opaque, the
/// status each mutator returned, and how many owners were released.
///
/// A NULL window (`from_heap_vec` of an empty array) is left out: C copies
/// it with `memcpy` from NULL, UB the wrappers avoid (`buffer.rs`'s
/// `null_windows_are_copied_without_cs_memcpy`).
mod mutation {
    use core::ffi::{c_int, c_void};
    use core::ptr;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use ffmpeg::libavutil::buffer::{self, BufferReference, BufferReferenceBox};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapVec;

    #[derive(Clone, Copy, Debug)]
    enum Kind {
        AllocZeroed,
        Heap,
        Owner,
        ReadOnly,
        Resizable(u8),
    }

    /// One step; indices pick a held reference modulo their count.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        New(Kind, usize),
        Clone(usize),
        Drop(usize),
        Advance(usize, usize),
        MakeMut(usize, u8),
        Resize(usize, usize, u8),
        Assign(usize, usize),
        AssignSlot(usize, Option<usize>),
        AssignNew(usize),
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
                match r % 13 {
                    0 | 1 => {
                        let kind = match a % 5 {
                            0 => Kind::AllocZeroed,
                            1 => Kind::Heap,
                            2 => Kind::Owner,
                            3 => Kind::ReadOnly,
                            _ => Kind::Resizable(b as u8),
                        };
                        Op::New(kind, SIZES[b % SIZES.len()])
                    }
                    2 | 3 => Op::Clone(a),
                    4 => Op::Drop(a),
                    5 => Op::Advance(a, b),
                    6 | 7 => Op::MakeMut(a, b as u8),
                    8 | 9 => Op::Resize(a, [0, 1, 7, 64, 301, 700][b % 6], (b >> 8) as u8),
                    // Neighbours are often clones, sharing a buffer.
                    10 => Op::Assign(a, if b % 2 == 0 { b } else { a + 1 }),
                    11 => Op::AssignSlot(a, (b % 4 != 0).then_some(b)),
                    _ => Op::AssignNew(a),
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

    /// What one side reports for a reference: the index of the buffer it
    /// counts, its window's offset from that buffer's data, its bytes, the
    /// buffer's count and writability, and whether it has an opaque.
    type Seen = (usize, usize, Vec<u8>, c_int, bool, bool);

    /// The buffers a side created, newest last, with their data.
    #[derive(Default)]
    struct Buffers(Vec<(*mut ffi::AVBuffer, *mut u8)>);

    impl Buffers {
        fn index(&self, r: *const ffi::AVBufferRef) -> usize {
            // SAFETY: callers pass live references.
            let buffer = unsafe { (*r).buffer };
            self.0.iter().rposition(|&(b, _)| b == buffer).unwrap()
        }

        /// `r`'s buffer, its window and the bytes in it.
        ///
        /// # Safety
        ///
        /// `r` must be a live reference to initialised bytes.
        unsafe fn window(&self, r: *const ffi::AVBufferRef) -> (usize, usize, Vec<u8>) {
            let k = self.index(r);
            // SAFETY: the caller passes a live reference, whose window holds
            // `size` initialised bytes.
            unsafe {
                let (data, size) = ((*r).data, (*r).size);
                let bytes = if size == 0 {
                    Vec::new()
                } else {
                    core::slice::from_raw_parts(data, size).to_vec()
                };
                (k, (data as usize).wrapping_sub(self.0[k].1 as usize), bytes)
            }
        }

        /// Records `r`'s buffer as the newest, or as resized in place.
        ///
        /// # Safety
        ///
        /// `r` must be a live reference.
        unsafe fn track(
            &mut self,
            r: *const ffi::AVBufferRef,
            before: (*mut ffi::AVBuffer, *mut u8),
        ) {
            // SAFETY: the caller passes a live reference.
            let (buffer, data) = unsafe { ((*r).buffer, (*r).data) };
            if buffer != before.0 {
                self.0.push((buffer, data));
            } else if data != before.1 {
                // Reallocated in place, which needs a window at its start.
                let k = self.index(r);
                self.0[k].1 = data;
            }
        }
    }

    /// The parts of `r` that `Buffers::track` compares.
    ///
    /// # Safety
    ///
    /// `r` must be a live reference.
    unsafe fn parts(r: *const ffi::AVBufferRef) -> (*mut ffi::AVBuffer, *mut u8) {
        // SAFETY: as above.
        unsafe { ((*r).buffer, (*r).data) }
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
        // `CSide::new_ref` allocated for this buffer.
        unsafe {
            (*opaque.cast::<AtomicUsize>()).fetch_add(1, Ordering::SeqCst);
            ffi::av_free(data.cast());
        }
    }

    #[derive(Default)]
    struct RustSide {
        held: Vec<BufferReferenceBox>,
        buffers: Buffers,
        frees: Arc<AtomicUsize>,
    }

    #[derive(Default)]
    struct CSide {
        held: Vec<*mut ffi::AVBufferRef>,
        buffers: Buffers,
        frees: Arc<AtomicUsize>,
        /// Copies into a new buffer, resizes in place, and assignments
        /// within one buffer.
        paths: [usize; 3],
    }

    impl RustSide {
        fn new_ref(&self, kind: Kind, size: usize, seed: usize) -> BufferReferenceBox {
            let counted = || Counted {
                bytes: pattern(size, seed),
                frees: self.frees.clone(),
            };
            match kind {
                Kind::AllocZeroed => BufferReference::alloc_zeroed(size).unwrap(),
                Kind::Heap => BufferReference::from_heap_vec(
                    HeapVec::from_slice(&pattern(size, seed)).unwrap(),
                )
                .unwrap(),
                Kind::Owner => BufferReference::from_owner(counted())
                    .map_err(drop)
                    .unwrap(),
                Kind::ReadOnly => BufferReference::from_read_only(counted())
                    .map_err(drop)
                    .unwrap(),
                Kind::Resizable(byte) => BufferReference::alloc_resizable(size).unwrap().fill(byte),
            }
        }

        /// Runs `op`; reports what a mutator returned.
        fn step(&mut self, op: Op, seed: usize) -> Option<c_int> {
            let n = self.held.len();
            let code = |r: Result<(), c_int>| Some(r.err().unwrap_or(0));
            let before = |r: &BufferReferenceBox| {
                // SAFETY: a live reference.
                unsafe { parts(r.as_c_ptr()) }
            };
            match op {
                Op::New(kind, size) => {
                    let r = self.new_ref(kind, size, seed);
                    // SAFETY: a live reference.
                    unsafe {
                        self.buffers
                            .track(r.as_c_ptr(), (ptr::null_mut(), ptr::null_mut()))
                    };
                    self.held.push(r);
                }
                Op::AssignNew(j) if n > 0 => {
                    let mut slot = None;
                    let ret = buffer::assign(&mut slot, Some(self.held[j % n].as_ref()));
                    self.held.extend(slot);
                    return code(ret);
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
                Op::MakeMut(k, byte) => {
                    let r = &mut self.held[k % n];
                    let was = before(r);
                    let ret = r.as_mut().make_mut().map(|bytes| bytes.fill(byte));
                    // SAFETY: a live reference.
                    unsafe { self.buffers.track(r.as_c_ptr(), was) };
                    return code(ret);
                }
                Op::Resize(k, len, byte) => {
                    let r = &mut self.held[k % n];
                    let was = before(r);
                    let ret = r.as_mut().resize(len, byte);
                    // SAFETY: a live reference.
                    unsafe { self.buffers.track(r.as_c_ptr(), was) };
                    return code(ret);
                }
                Op::Assign(k, j) => {
                    let (k, j) = (k % n, j % n);
                    if k == j {
                        return Some(0);
                    }
                    let (lo, hi) = self.held.split_at_mut(k.max(j));
                    let (dst, src) = if k < j {
                        (&mut lo[k], &hi[0])
                    } else {
                        (&mut hi[0], &lo[j])
                    };
                    return code(dst.as_mut().assign(src.as_ref()));
                }
                Op::AssignSlot(k, j) => {
                    let k = k % n;
                    if j.is_some_and(|j| j % n == k) {
                        return Some(0);
                    }
                    let mut slot = Some(self.held.remove(k));
                    let src = j.map(|j| self.held[(j % n) - usize::from(j % n > k)].as_ref());
                    let ret = buffer::assign(&mut slot, src);
                    if let Some(r) = slot {
                        self.held.insert(k, r);
                    }
                    return code(ret);
                }
                Op::AssignNew(_) => unreachable!(),
            }
            None
        }

        fn snapshot(&self) -> Vec<Seen> {
            let see = |r: &BufferReferenceBox| {
                // SAFETY: every held reference is live, its bytes initialised.
                let (k, offset, _) = unsafe { self.buffers.window(r.as_c_ptr()) };
                let r = r.as_ref();
                let count = r.ref_count() as c_int;
                (
                    k,
                    offset,
                    r.data().to_vec(),
                    count,
                    r.is_writable(),
                    r.opaque().is_null(),
                )
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
                // SAFETY: no preconditions.
                Kind::AllocZeroed => unsafe { ffi::av_buffer_allocz(size) },
                Kind::Heap => copy(0, false),
                Kind::Owner => copy(0, true),
                Kind::ReadOnly => copy(ffi::AV_BUFFER_FLAG_READONLY as c_int, true),
                Kind::Resizable(byte) => {
                    let mut r = ptr::null_mut();
                    // SAFETY: an empty slot gets a fresh reference to `size`
                    // writable bytes, filled here.
                    unsafe {
                        assert_eq!(ffi::av_buffer_realloc(&mut r, size), 0);
                        ptr::write_bytes((*r).data, byte, size);
                    }
                    r
                }
            };
            assert!(!r.is_null());
            r
        }

        fn step(&mut self, op: Op, seed: usize) -> Option<c_int> {
            let n = self.held.len();
            // SAFETY: every held reference is live and this side's own; each
            // is released once, by `Drop`, an assignment of NULL or `finish`.
            unsafe {
                match op {
                    Op::New(kind, size) => {
                        let r = self.new_ref(kind, size, seed);
                        self.buffers.track(r, (ptr::null_mut(), ptr::null_mut()));
                        self.held.push(r);
                    }
                    Op::AssignNew(j) if n > 0 => {
                        let mut r = ptr::null_mut();
                        let ret = ffi::av_buffer_replace(&mut r, self.held[j % n]);
                        if !r.is_null() {
                            self.held.push(r);
                        }
                        return Some(ret);
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
                    Op::MakeMut(k, byte) => {
                        let was = parts(self.held[k % n]);
                        let ret = ffi::av_buffer_make_writable(&mut self.held[k % n]);
                        let r = self.held[k % n];
                        if ret == 0 {
                            ptr::write_bytes((*r).data, byte, (*r).size);
                        }
                        self.paths[0] += usize::from((*r).buffer != was.0);
                        self.buffers.track(r, was);
                        return Some(ret);
                    }
                    Op::Resize(k, len, byte) => {
                        let was = parts(self.held[k % n]);
                        let old = (*self.held[k % n]).size;
                        let ret = ffi::av_buffer_realloc(&mut self.held[k % n], len);
                        let r = self.held[k % n];
                        if ret == 0 && len > old {
                            ptr::write_bytes((*r).data.add(old), byte, len - old);
                        }
                        self.paths[0] += usize::from((*r).buffer != was.0);
                        self.paths[1] += usize::from((*r).buffer == was.0 && len != old);
                        self.buffers.track(r, was);
                        return Some(ret);
                    }
                    Op::Assign(k, j) => {
                        let (k, j) = (k % n, j % n);
                        if k == j {
                            return Some(0);
                        }
                        self.paths[2] +=
                            usize::from((*self.held[k]).buffer == (*self.held[j]).buffer);
                        return Some(ffi::av_buffer_replace(&mut self.held[k], self.held[j]));
                    }
                    Op::AssignSlot(k, j) => {
                        let k = k % n;
                        if j.is_some_and(|j| j % n == k) {
                            return Some(0);
                        }
                        let src = j.map_or(ptr::null(), |j| self.held[j % n].cast_const());
                        let ret = ffi::av_buffer_replace(&mut self.held[k], src);
                        if self.held[k].is_null() {
                            self.held.remove(k);
                        }
                        return Some(ret);
                    }
                    Op::AssignNew(_) => unreachable!(),
                }
            }
            None
        }

        fn snapshot(&self) -> Vec<Seen> {
            let see = |&r: &*mut ffi::AVBufferRef| {
                // SAFETY: every held reference is live, its bytes initialised.
                unsafe {
                    let (k, offset, bytes) = self.buffers.window(r);
                    (
                        k,
                        offset,
                        bytes,
                        ffi::av_buffer_get_ref_count(r),
                        ffi::av_buffer_is_writable(r) != 0,
                        ffi::av_buffer_get_opaque(r).is_null(),
                    )
                }
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

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn sequences_mutate_count_and_release_as_c() {
        // Mutators run, and C's copies, in-place resizes and same-buffer
        // assignments, so that the sequences demonstrably reach every path.
        let (mut mutated, mut paths) = (0usize, [0usize; 3]);
        let seeds = seeds();
        let count = seeds.clone().count();
        for seed in seeds {
            let (mut rust, mut c) = (RustSide::default(), CSide::default());
            for (i, op) in ops(seed * 0x9e37_79b9, 200).into_iter().enumerate() {
                let op = match op {
                    // `from_heap_vec` of an empty array is a NULL window.
                    Op::New(Kind::Heap, 0) => Op::New(Kind::AllocZeroed, 0),
                    op => op,
                };
                let ret = rust.step(op, i);
                assert_eq!(ret, c.step(op, i), "seed {seed} step {i}: {op:?}");
                mutated += usize::from(ret.is_some());
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
                assert_eq!(
                    rust.buffers.0.len(),
                    c.buffers.0.len(),
                    "seed {seed} step {i}"
                );
            }
            rust.held.clear();
            c.finish();
            assert_eq!(
                rust.frees.load(Ordering::SeqCst),
                c.frees.load(Ordering::SeqCst)
            );
            paths.iter_mut().zip(c.paths).for_each(|(p, n)| *p += n);
        }
        assert!(mutated > 50 * count, "{mutated}");
        assert!(paths.iter().all(|&n| n > count * 10 / 16), "{paths:?}");
    }

    /// The seeds `sequences_mutate_count_and_release_as_c` runs: 1 to 16,
    /// or only 2 under BorrowSanitizer, which takes about 13 minutes per
    /// seed there (all 16 ran past the gate's two-hour limit). Seed 2 alone
    /// reaches all three of C's paths the test counts; seed 1 misses the
    /// same-buffer assignment.
    fn seeds() -> core::ops::RangeInclusive<u64> {
        if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            2..=2
        } else {
            1..=16
        }
    }
}

/// Buffer-frame wave 2, batch 1: `BufferPoolRef::get_with_opaque` against
/// C's `av_buffer_pool_get` + `av_buffer_pool_buffer_get_opaque`, for the
/// default allocator and for allocators whose buffers carry an opaque of
/// their own, over random request/release sequences: after every step both
/// sides compare which buffer each request recycled, its allocator's opaque
/// (by first-seen order, so recycling shows as a repeat), its length,
/// count, writability, and whether its own opaque (the pool's bookkeeping)
/// is set, and how many allocator buffers were released.
mod pool_get {
    use core::ffi::c_void;
    use core::mem::MaybeUninit;
    use core::ptr;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox, BufferReferenceBox, UninitBuffer};
    use ffmpeg::libavutil::ffi;

    /// The index of `p` in `seen`, appended when new; NULL is `None`.
    fn index<T: PartialEq + Copy>(seen: &mut Vec<T>, p: T, null: bool) -> Option<usize> {
        if null {
            return None;
        }
        Some(seen.iter().position(|&q| q == p).unwrap_or_else(|| {
            seen.push(p);
            seen.len() - 1
        }))
    }

    /// What one side reports for a request: its data's (0 for empty data,
    /// which Rust sees as a dangling address) and its allocator opaque's
    /// first-seen indices, its length, count, writability and whether it
    /// has an opaque of its own.
    type Got = (usize, Option<usize>, usize, i32, bool, bool);

    /// Bytes a Rust allocator's buffer owns, counting their releases.
    struct Counted(Vec<MaybeUninit<u8>>, Arc<AtomicUsize>);

    impl AsMut<[MaybeUninit<u8>]> for Counted {
        fn as_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            &mut self.0
        }
    }

    impl Drop for Counted {
        fn drop(&mut self) {
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// C's allocator state: releases of its buffers.
    struct CState {
        frees: AtomicUsize,
    }

    /// The C allocator buffers' release: frees the data, counting; the
    /// buffer's opaque is its data's address, so each one is distinct.
    unsafe extern "C" fn c_free(opaque: *mut c_void, data: *mut u8) {
        assert_eq!(opaque.cast::<u8>(), data);
        // SAFETY: `data` is the block `c_alloc2` allocated; the state is the
        // test's, live until after the pool is freed.
        unsafe {
            (*STATE.load(Ordering::SeqCst))
                .frees
                .fetch_add(1, Ordering::SeqCst);
            ffi::av_free(data.cast());
        }
    }

    /// The live `CState` of the running test (only one test uses it).
    static STATE: core::sync::atomic::AtomicPtr<CState> =
        core::sync::atomic::AtomicPtr::new(ptr::null_mut());

    unsafe extern "C" fn c_alloc2(_: *mut c_void, size: usize) -> *mut ffi::AVBufferRef {
        // SAFETY: a fresh block of `size` bytes, owned by the new buffer
        // (released by `c_free`), or freed here when C cannot create it.
        unsafe {
            let data = ffi::av_malloc(size).cast::<u8>();
            let buf = ffi::av_buffer_create(data, size, Some(c_free), data.cast(), 0);
            if buf.is_null() {
                ffi::av_free(data.cast());
            }
            buf
        }
    }

    struct Rust {
        pool: Option<BufferPoolBox>,
        held: Vec<BufferReferenceBox>,
        data: Vec<*const u8>,
        opaques: Vec<usize>,
    }

    struct C {
        pool: *mut ffi::AVBufferPool,
        held: Vec<*mut ffi::AVBufferRef>,
        data: Vec<*const u8>,
        opaques: Vec<usize>,
    }

    impl Rust {
        fn get(&mut self) -> Option<Got> {
            let (u, opaque): (UninitBuffer, _) = self.pool.as_ref()?.as_ref().get_with_opaque()?;
            let at = u.as_slice().as_ptr().cast::<u8>();
            let buf = u.fill(0x3c);
            let r = buf.as_ref();
            let got = (
                index(&mut self.data, at, r.is_empty()).unwrap_or(0),
                index(&mut self.opaques, opaque as usize, opaque.is_null()),
                r.len(),
                r.ref_count() as i32,
                r.is_writable(),
                r.opaque().is_null(),
            );
            self.held.push(buf);
            Some(got)
        }
    }

    impl C {
        fn get(&mut self) -> Option<Got> {
            // SAFETY: a live pool; a non-NULL result is a pool buffer of
            // `size` bytes, written whole before anything reads them.
            unsafe {
                let r = ffi::av_buffer_pool_get(self.pool);
                if r.is_null() {
                    return None;
                }
                let opaque = ffi::av_buffer_pool_buffer_get_opaque(r);
                ptr::write_bytes((*r).data, 0x3c, (*r).size);
                let got = (
                    index(&mut self.data, (*r).data.cast_const(), (*r).size == 0).unwrap_or(0),
                    index(&mut self.opaques, opaque as usize, opaque.is_null()),
                    (*r).size,
                    ffi::av_buffer_get_ref_count(r),
                    ffi::av_buffer_is_writable(r) != 0,
                    ffi::av_buffer_get_opaque(r).is_null(),
                );
                self.held.push(r);
                Some(got)
            }
        }

        fn drop_held(&mut self, k: usize) {
            let mut r = self.held.swap_remove(k);
            // SAFETY: this side's live reference, released once.
            unsafe { ffi::av_buffer_unref(&mut r) };
        }
    }

    fn run(
        rust_pool: BufferPoolBox,
        c_pool: *mut ffi::AVBufferPool,
        seed: u64,
        frees: &dyn Fn() -> (usize, usize),
    ) {
        let mut rust = Rust {
            pool: Some(rust_pool),
            held: Vec::new(),
            data: Vec::new(),
            opaques: Vec::new(),
        };
        let mut c = C {
            pool: c_pool,
            held: Vec::new(),
            data: Vec::new(),
            opaques: Vec::new(),
        };
        let mut x = seed | 1;
        for i in 0..150 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            if x % 5 < 3 || rust.held.is_empty() {
                assert_eq!(rust.get(), c.get(), "step {i}");
            } else {
                let k = (x >> 8) as usize % rust.held.len();
                drop(rust.held.swap_remove(k));
                c.drop_held(k);
            }
            let (r, cf) = frees();
            assert_eq!(r, cf, "step {i}");
        }
        drop(rust.pool.take());
        // SAFETY: C's own pool reference, released once.
        unsafe { ffi::av_buffer_pool_uninit(&mut c.pool) };
        assert_eq!(frees().0, frees().1);
        while !rust.held.is_empty() {
            drop(rust.held.pop());
            c.drop_held(c.held.len() - 1);
            assert_eq!(frees().0, frees().1);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn default_pools_get_as_c_with_null_opaques() {
        for size in [0, 3, 256] {
            for seed in 1..=4u64 {
                // SAFETY: a NULL allocator selects av_buffer_alloc.
                let c_pool = unsafe { ffi::av_buffer_pool_init(size, None) };
                assert!(!c_pool.is_null());
                run(
                    BufferPool::new(size).unwrap(),
                    c_pool,
                    seed * 31 + size as u64,
                    &|| (0, 0),
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn allocator_opaques_come_back_as_c() {
        let state = Box::into_raw(Box::new(CState {
            frees: AtomicUsize::new(0),
        }));
        STATE.store(state, Ordering::SeqCst);
        for size in [0, 3, 256] {
            for seed in 1..=4u64 {
                let rust_frees = Arc::new(AtomicUsize::new(0));
                let counter = rust_frees.clone();
                // SAFETY: each buffer owns exactly `size` bytes from its start,
                // and its release (`Counted`'s drop) never touches the pool.
                let rust_pool = unsafe {
                    BufferPool::with_allocator(size, move |n| {
                        let owner = Counted(vec![MaybeUninit::uninit(); n], counter.clone());
                        UninitBuffer::from_owner(owner).ok()
                    })
                }
                .unwrap();
                // SAFETY: `c_alloc2` returns NULL or a sole, writable
                // `size`-byte reference; it uses no pool opaque.
                let c_pool = unsafe {
                    ffi::av_buffer_pool_init2(size, ptr::null_mut(), Some(c_alloc2), None)
                };
                assert!(!c_pool.is_null());
                // SAFETY: the state lives until the end of the test.
                let base = unsafe { (*state).frees.load(Ordering::SeqCst) };
                let frees = || {
                    // SAFETY: as above.
                    let c = unsafe { (*state).frees.load(Ordering::SeqCst) };
                    (rust_frees.load(Ordering::SeqCst), c - base)
                };
                run(rust_pool, c_pool, seed * 17 + size as u64, &frees);
            }
        }
        STATE.store(ptr::null_mut(), Ordering::SeqCst);
        // SAFETY: every C buffer was released, and no pool is left.
        drop(unsafe { Box::from_raw(state) });
    }
}

/// Review of buffer-frame wave 2: resizing a pool's buffer, which the
/// wrappers copy themselves (`realloc_by_copy`) as the buffer has an opaque,
/// against C's `av_buffer_realloc` of a buffer from a pool C builds the same
/// way, where C takes its own copy path. Both sides compare the resized
/// window's bytes and length, its count, writability and opaque, whether it
/// left the pool's block, and whether the next request recycles that block,
/// for filled (`BufferReferenceMut::resize`) and uninitialised
/// (`UninitBuffer::resize`) buffers.
mod pool_resize {
    use core::ptr;

    use ffmpeg::libavutil::buffer::BufferPool;
    use ffmpeg::libavutil::ffi;

    /// The resized window's bytes, count, writability and whether it has an
    /// opaque; whether its data left the pool's block; whether the next
    /// request got that block back.
    type Seen = (Vec<u8>, i32, bool, bool, bool, bool);

    const SIZES: [usize; 4] = [0, 3, 64, 300];
    const LENS: [usize; 5] = [0, 1, 3, 64, 4096];

    fn rust(size: usize, len: usize, filled: bool) -> Seen {
        let pool = BufferPool::new(size).unwrap();
        let u = pool.as_ref().get().unwrap();
        let block = u.as_slice().as_ptr().cast::<u8>();
        let buf = if filled {
            let mut buf = u.fill(0x11);
            buf.as_mut().resize(len, 0x22).unwrap();
            buf
        } else {
            let mut u = u;
            u.resize(len).unwrap();
            u.fill(0x33)
        };
        let r = buf.as_ref();
        let moved = r.is_empty() || r.data().as_ptr() != block;
        let seen = (
            r.data().to_vec(),
            r.ref_count() as i32,
            r.is_writable(),
            r.opaque().is_null(),
        );
        let next = pool.as_ref().get().unwrap();
        let recycled = size == 0 || next.as_slice().as_ptr().cast::<u8>() == block;
        drop(next);
        (seen.0, seen.1, seen.2, seen.3, moved, recycled)
    }

    fn c(size: usize, len: usize, filled: bool) -> Seen {
        // SAFETY: a NULL allocator selects av_buffer_alloc; every reference
        // is live until its unref, and its window's bytes are written whole
        // before anything reads them.
        unsafe {
            let mut pool = ffi::av_buffer_pool_init(size, None);
            assert!(!pool.is_null());
            let mut r = ffi::av_buffer_pool_get(pool);
            assert!(!r.is_null());
            let block = (*r).data.cast_const();
            let old = (*r).size;
            if filled {
                ptr::write_bytes((*r).data, 0x11, old);
            }
            assert_eq!(ffi::av_buffer_realloc(&mut r, len), 0);
            if filled {
                if len > old {
                    ptr::write_bytes((*r).data.add(old), 0x22, len - old);
                }
            } else {
                ptr::write_bytes((*r).data, 0x33, len);
            }
            let bytes = core::slice::from_raw_parts((*r).data, (*r).size).to_vec();
            let moved = (*r).size == 0 || (*r).data.cast_const() != block;
            let seen = (
                bytes,
                ffi::av_buffer_get_ref_count(r),
                ffi::av_buffer_is_writable(r) != 0,
                ffi::av_buffer_get_opaque(r).is_null(),
            );
            let mut next = ffi::av_buffer_pool_get(pool);
            assert!(!next.is_null());
            let recycled = size == 0 || (*next).data.cast_const() == block;
            ffi::av_buffer_unref(&mut next);
            ffi::av_buffer_pool_uninit(&mut pool);
            ffi::av_buffer_unref(&mut r);
            (seen.0, seen.1, seen.2, seen.3, moved, recycled)
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pool_buffers_resize_by_copy_as_c() {
        for size in SIZES {
            for len in LENS {
                for filled in [true, false] {
                    let want = c(size, len, filled);
                    assert_eq!(rust(size, len, filled), want, "{size} -> {len}, {filled}");
                }
            }
        }
    }
}
