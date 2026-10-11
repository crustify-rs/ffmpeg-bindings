//! Pools built by the safe constructors behave as pools C builds itself:
//! the same buffers handed out, recycled and counted after every step of a
//! request/reference/release sequence, the same allocator calls, and the
//! allocator state released at the same point.
//!
//! Both sides are driven through C's `av_buffer_pool_get` and the
//! `AVBufferRef` routines until `AVBufferRef` has a safe owner; the Rust
//! side reaches its pool through the owner's raw seam.

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox};
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
                // SAFETY: each call returns NULL or a fresh sole, writable
                // `size`-byte av_malloc reference; the pool is never touched.
                let owner = unsafe {
                    BufferPool::with_allocator(size, move |n| {
                        let _ = &guard;
                        let k = seen.fetch_add(1, Ordering::SeqCst) + 1;
                        if fail_every != 0 && k % fail_every == 0 {
                            ptr::null_mut()
                        } else {
                            ffi::av_buffer_alloc(n)
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
                ffi::av_buffer_alloc(n)
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
