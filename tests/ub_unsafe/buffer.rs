//! UB tests for `libavutil/buffer.h`'s unsafe `BufferPool::with_allocator`,
//! each allocator within its `# Safety` contract: every buffer it returns
//! holds at least the pool's size from its start, and neither it nor those
//! buffers' releases call into its own pool.
//!
//! Buffers are requested through the pool's safe `get` and
//! `get_with_opaque` (buffer-frame wave 2), then held as raw references.

use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox, BufferReference, UninitBuffer};
use ffmpeg::libavutil::ffi;

/// Counts the drops of the allocator state it is moved into.
struct DropCount(Arc<AtomicUsize>);

impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A reference this test owns, released once.
struct Owned(*mut ffi::AVBufferRef);

// SAFETY: C reference counting is thread-safe; each `Owned` is the only Rust
// path to its reference.
unsafe impl Send for Owned {}

impl Owned {
    fn get(pool: &BufferPoolBox) -> Option<Self> {
        Self::get_with_opaque(pool).map(|(buf, _)| buf)
    }

    /// A buffer and its allocator's opaque; the bytes are written whole
    /// before anything reads them.
    fn get_with_opaque(pool: &BufferPoolBox) -> Option<(Self, *mut c_void)> {
        let (buf, opaque) = pool.as_ref().get_with_opaque()?;
        Some((Self(buf.fill(0).into_c()), opaque))
    }

    /// Writes and reads back every byte the reference describes, with whole
    /// slice operations (one BSan retag, not one per byte).
    fn fill(&self, byte: u8) {
        let size = self.size();
        // SAFETY: a live pool reference describes `size` writable bytes, and
        // this is its only user while the view lives.
        let bytes = unsafe {
            ptr::write_bytes(self.data(), byte, size);
            core::slice::from_raw_parts(self.data(), size)
        };
        assert!(bytes == vec![byte; size].as_slice());
    }

    fn size(&self) -> usize {
        // SAFETY: a live reference.
        unsafe { (*self.0).size }
    }

    fn data(&self) -> *mut u8 {
        // SAFETY: a live reference.
        unsafe { (*self.0).data }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: this owner's reference, released once.
        unsafe { ffi::av_buffer_unref(&mut self.0) };
    }
}

/// The default allocator as a fn item: a zero-sized allocator state.
fn default_alloc(size: usize) -> Option<UninitBuffer> {
    BufferReference::alloc(size)
}

/// Builds a counting pool over `av_buffer_alloc`, `extra` bytes larger than
/// the pool's size; the allocator state lives in `drops`' guard.
fn counting_pool(
    size: usize,
    extra: usize,
    calls: &Arc<AtomicUsize>,
    drops: &Arc<AtomicUsize>,
) -> BufferPoolBox {
    let (calls, guard) = (calls.clone(), DropCount(drops.clone()));
    // SAFETY: each buffer holds `size + extra` bytes from its start, and the
    // allocator never touches the pool.
    unsafe {
        BufferPool::with_allocator(size, move |n| {
            let _ = &guard;
            calls.fetch_add(1, Ordering::SeqCst);
            BufferReference::alloc(n + extra)
        })
    }
    .unwrap()
}

#[test]
fn allocator_state_outlives_the_owner_until_the_last_buffer_returns() {
    for size in [0, 1, 64, 4096] {
        for extra in [0, 3] {
            let (calls, drops) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
            let pool = counting_pool(size, extra, &calls, &drops);
            let held: Vec<Owned> = (0..4).map(|_| Owned::get(&pool).unwrap()).collect();
            assert_eq!(calls.load(Ordering::SeqCst), 4);
            drop(pool);
            for (i, buf) in held.iter().enumerate() {
                buf.fill(i as u8);
            }
            let mut held = held;
            while let Some(buf) = held.pop() {
                assert_eq!(drops.load(Ordering::SeqCst), 0);
                buf.fill(0xee);
                drop(buf);
            }
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
    }
}

#[test]
fn allocator_runs_only_for_an_empty_free_list_and_recycles_at_pool_size() {
    let (calls, drops) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    // The allocator's references describe `size + 5` bytes; recycled ones
    // describe the pool's `size`.
    let pool = counting_pool(16, 5, &calls, &drops);
    let first = Owned::get(&pool).unwrap();
    assert_eq!(first.size(), 21);
    let data = first.data();
    drop(first);
    let again = Owned::get(&pool).unwrap();
    assert_eq!((again.data(), again.size()), (data, 16));
    again.fill(0x11);
    let other = Owned::get(&pool).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    drop(other);
    drop(again);
    drop(pool);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn null_from_the_allocator_fails_only_that_request() {
    let drops = Arc::new(AtomicUsize::new(0));
    let guard = DropCount(drops.clone());
    let mut n = 0usize;
    // SAFETY: every other call returns no buffer, the rest a 32-byte one;
    // the pool is never touched.
    let pool = unsafe {
        BufferPool::with_allocator(32, move |size| {
            let _ = &guard;
            n += 1;
            if n % 2 == 1 {
                None
            } else {
                BufferReference::alloc(size)
            }
        })
    }
    .unwrap();
    assert!(Owned::get(&pool).is_none());
    let ok = Owned::get(&pool).unwrap();
    ok.fill(1);
    assert!(Owned::get(&pool).is_none());
    drop(ok);
    // Recycled: no allocator call, so no NULL.
    Owned::get(&pool).unwrap().fill(2);
    drop(pool);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn zero_sized_allocator_state() {
    // SAFETY: `default_alloc` returns 48-byte buffers and never touches the
    // pool.
    let pool = unsafe { BufferPool::with_allocator(48, default_alloc) }.unwrap();
    let a = Owned::get(&pool).unwrap();
    let b = Owned::get(&pool).unwrap();
    drop(pool);
    a.fill(3);
    drop(a);
    b.fill(4);
}

/// The allocator mutates plain (non-atomic) state: C's pool mutex is all that
/// orders its calls across threads.
struct Log {
    sizes: Vec<usize>,
    out: Arc<Mutex<Option<usize>>>,
}

impl Drop for Log {
    fn drop(&mut self) {
        *self.out.lock().unwrap() = Some(self.sizes.len());
    }
}

#[test]
fn concurrent_requests_serialize_the_allocator() {
    const THREADS: usize = 8;
    const ROUNDS: usize = 100;
    let out = Arc::new(Mutex::new(None));
    let mut log = Log {
        sizes: Vec::new(),
        out: out.clone(),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    // SAFETY: each buffer holds 40 bytes from its start; the pool is never
    // touched.
    let pool = unsafe {
        BufferPool::with_allocator(40, move |size| {
            log.sizes.push(size);
            seen.fetch_add(1, Ordering::Relaxed);
            BufferReference::alloc(size)
        })
    }
    .unwrap();
    let barrier = Barrier::new(THREADS);
    thread::scope(|s| {
        for t in 0..THREADS {
            let (pool, barrier) = (&pool, &barrier);
            s.spawn(move || {
                barrier.wait();
                let mut held = Vec::new();
                for i in 0..ROUNDS {
                    let buf = Owned::get(pool).unwrap();
                    buf.fill((t + i) as u8);
                    held.push(buf);
                    if i % 3 == 2 {
                        held.clear();
                    }
                }
            });
        }
    });
    let allocated = calls.load(Ordering::Relaxed);
    assert!((1..=THREADS * ROUNDS).contains(&allocated));
    drop(pool);
    assert_eq!(*out.lock().unwrap(), Some(allocated));
}

#[test]
fn owner_and_last_buffer_released_on_other_threads() {
    let (calls, drops) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let pool = counting_pool(256, 0, &calls, &drops);
    let bufs: Vec<Owned> = (0..3).map(|_| Owned::get(&pool).unwrap()).collect();
    thread::spawn(move || drop(pool)).join().unwrap();
    let handles: Vec<_> = bufs
        .into_iter()
        .map(|b| {
            thread::spawn(move || {
                b.fill(9);
                drop(b);
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

/// Bytes a buffer owns, counting their releases.
struct Counted {
    bytes: Vec<MaybeUninit<u8>>,
    frees: &'static AtomicUsize,
}

impl AsMut<[MaybeUninit<u8>]> for Counted {
    fn as_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.bytes
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.frees.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn custom_buffers_keep_their_opaque_and_release() {
    // Only this test releases a `Counted` with this counter.
    static FREES: AtomicUsize = AtomicUsize::new(0);
    // SAFETY: each buffer owns exactly `size` bytes from its start, and its
    // release (`Counted`'s drop) never touches the pool.
    let pool = unsafe {
        BufferPool::with_allocator(24, |size| {
            let owner = Counted {
                bytes: vec![MaybeUninit::uninit(); size],
                frees: &FREES,
            };
            UninitBuffer::from_owner(owner).ok()
        })
    }
    .unwrap();
    let (a, first) = Owned::get_with_opaque(&pool).unwrap();
    let (b, second) = Owned::get_with_opaque(&pool).unwrap();
    let opaques = [first, second];
    assert!(!opaques[0].is_null() && opaques[0] != opaques[1]);
    a.fill(5);
    b.fill(5);
    drop(a);
    assert_eq!(FREES.load(Ordering::SeqCst), 0);
    let (recycled, original) = Owned::get_with_opaque(&pool).unwrap();
    assert_eq!(original, opaques[0]);
    drop(pool);
    drop(recycled);
    drop(b);
    // Both entries went back through their own release when the pool went.
    assert_eq!(FREES.load(Ordering::SeqCst), 2);
}

/// The buffer of another pool that `ReleasesKept` returns, at most once.
static KEPT: Mutex<Option<Owned>> = Mutex::new(None);

/// Bytes whose release also returns `KEPT` to its own pool.
struct ReleasesKept(Vec<MaybeUninit<u8>>);

impl AsMut<[MaybeUninit<u8>]> for ReleasesKept {
    fn as_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        &mut self.0
    }
}

impl Drop for ReleasesKept {
    fn drop(&mut self) {
        drop(KEPT.lock().unwrap().take());
    }
}

/// The contract bars the releases of the allocator's buffers from
/// calling into their own pool, whose mutex C holds while dropping the owner
/// flushes them; another pool's buffers are theirs to release.
#[test]
fn free_callbacks_may_release_buffers_of_other_pools() {
    let other = BufferPool::new(32).unwrap();
    // SAFETY: each buffer owns exactly `size` bytes from its start, and its
    // release (`ReleasesKept`'s drop) calls into `other` only, never this
    // pool.
    let pool = unsafe {
        BufferPool::with_allocator(16, |size| {
            let owner = ReleasesKept(vec![MaybeUninit::uninit(); size]);
            UninitBuffer::from_owner(owner).ok()
        })
    }
    .unwrap();
    let (a, b) = (Owned::get(&pool).unwrap(), Owned::get(&pool).unwrap());
    drop(a);
    let kept = Owned::get(&other).unwrap();
    kept.fill(3);
    *KEPT.lock().unwrap() = Some(kept);
    // `a`'s entry is flushed under this pool's mutex; its free returns `kept`
    // to `other`, locking only that pool's mutex.
    thread::spawn(move || drop(pool)).join().unwrap();
    assert!(KEPT.lock().unwrap().is_none());
    // The last buffer frees the pool and, through `ReleasesKept`, its data.
    b.fill(4);
    drop(b);
    let again = Owned::get(&other).unwrap();
    again.fill(6);
    drop(other);
    drop(again);
}

/// Review of buffer-frame wave 2 (`ub_unsafe/pool-buffer-resized-in-place-
/// under-its-entry`): an allocator of resizable buffers keeps its contract,
/// yet C leaves a pool buffer it took over flagged reallocatable. Resizing
/// one, uninitialised or filled, larger or smaller, copies it instead of
/// `av_realloc`ing the block its pool entry records: the original returns to
/// the pool whole, is recycled at the same address and released once.
#[test]
fn resizable_allocator_buffers_resize_by_copy() {
    // SAFETY: every buffer holds exactly `size` bytes from its start (a
    // fresh resizable buffer's window is its data), and neither the
    // allocator nor its buffers' release (`av_free`) calls into the pool.
    let pool = unsafe { BufferPool::with_allocator(16, BufferReference::alloc_resizable) }.unwrap();
    for len in [1 << 20, 4, 0, 17] {
        let mut first = pool.as_ref().get().unwrap();
        let at = first.as_slice().as_ptr();
        first.resize(len).unwrap();
        assert_eq!(first.len(), len);
        assert_ne!(first.as_slice().as_ptr(), at);
        assert!(first.fill(1).as_ref().data() == vec![1; len].as_slice());
        let again = pool.as_ref().get().unwrap();
        assert_eq!(again.as_slice().as_ptr(), at);
        let mut filled = again.fill(3);
        filled.as_mut().resize(len, 4).unwrap();
        let mut want = vec![3; len.min(16)];
        want.resize(len, 4);
        assert!(filled.as_ref().data() == want.as_slice());
    }
    let last = pool.as_ref().get().unwrap();
    drop(pool);
    assert!(last.fill(5).as_ref().data() == [5; 16].as_slice());
}

/// Buffer-frame wave 1, batch 0: `UninitBuffer::assume_init`, each call
/// after every byte was written: through `as_mut_slice` whole or in pieces,
/// vacuously for an empty buffer, and by an earlier initialised owner the
/// buffer was converted from; then the bytes read back exactly (ASan sees a
/// read past them) and the result used as any reference.
mod uninit_assume_init {
    use core::mem::MaybeUninit;
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::buffer::{BufferReference, UninitBuffer};
    use ffmpeg::libavutil::mem::HeapVec;

    const SIZES: [usize; 5] = [0, 1, 15, 4096, 1 << 16];

    #[test]
    fn after_writing_the_whole_run() {
        for size in SIZES {
            let mut u = BufferReference::alloc(size).unwrap();
            // A 256-byte cycle repeated by whole-slice copies.
            let mut src = (0..=255u8).collect::<Vec<_>>().repeat(size.div_ceil(256));
            src.truncate(size);
            let dst = u.as_mut_slice();
            assert_eq!(dst.len(), size);
            // SAFETY: both runs are `size` bytes and distinct allocations; one
            // copy (one BSan retag, not one per byte).
            unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast(), size) };
            // SAFETY: every byte was written just above.
            let mut buf = unsafe { u.assume_init() };
            assert!(buf.as_ref().data() == src.as_slice());
            assert!(buf.as_mut().data_mut().is_some());
            let copy = buf.clone();
            thread::spawn(move || assert!(copy.as_ref().data() == src.as_slice()))
                .join()
                .unwrap();
        }
    }

    #[test]
    fn after_writing_in_pieces() {
        for size in SIZES {
            let mut u = BufferReference::alloc(size).unwrap();
            let (a, b) = u.as_mut_slice().split_at_mut(size / 3);
            let (b, c) = b.split_at_mut(b.len() / 2);
            // SAFETY: each piece is written whole, one operation each (one
            // BSan retag, not one per byte).
            unsafe {
                ptr::write_bytes(a.as_mut_ptr(), 0x11, a.len());
                ptr::write_bytes(c.as_mut_ptr(), 0x33, c.len());
                ptr::write_bytes(b.as_mut_ptr(), 0x22, b.len());
            }
            // SAFETY: the three pieces cover the whole run.
            let buf = unsafe { u.assume_init() };
            let data = buf.as_ref().data();
            assert_eq!(data.len(), size);
            let (x, y) = (size / 3, size / 3 + (size - size / 3) / 2);
            assert!(data[..x] == vec![0x11; x][..]);
            assert!(data[x..y] == vec![0x22; y - x][..]);
            assert!(data[y..] == vec![0x33; size - y][..]);
        }
    }

    #[test]
    fn vacuously_for_an_empty_buffer() {
        let u = BufferReference::alloc(0).unwrap();
        assert!(u.is_empty());
        // SAFETY: there is no byte to write.
        let mut buf = unsafe { u.assume_init() };
        assert!(buf.as_ref().data().is_empty());
        assert_eq!(buf.as_mut().data_mut().map(|d| d.len()), Some(0));
    }

    #[test]
    fn after_an_initialised_owner_wrote_them() {
        for size in SIZES {
            let bytes = vec![0x7e; size];
            for buf in [
                BufferReference::alloc_zeroed(size).unwrap(),
                BufferReference::from_heap_vec(HeapVec::from_slice(&bytes).unwrap()).unwrap(),
            ] {
                let want = buf.as_ref().data().to_vec();
                let u = UninitBuffer::try_from(buf).unwrap();
                // SAFETY: the owner it was converted from held initialised
                // bytes, and nothing wrote them since.
                let buf = unsafe { u.assume_init() };
                assert!(buf.as_ref().data() == want.as_slice());
            }
        }
    }

    /// Review of buffer-frame wave 2: an `UninitBuffer::from_owner` owner
    /// whose bytes it initialised itself.
    #[test]
    fn after_a_rust_owner_initialised_them() {
        for size in SIZES {
            let u = UninitBuffer::from_owner(vec![MaybeUninit::new(0x5d); size]).unwrap();
            // SAFETY: the owner's every byte was initialised, and nothing
            // wrote them since.
            let buf = unsafe { u.assume_init() };
            assert!(buf.as_ref().data() == vec![0x5d; size].as_slice());
            assert!(!buf.as_ref().opaque().is_null());
        }
    }
}
