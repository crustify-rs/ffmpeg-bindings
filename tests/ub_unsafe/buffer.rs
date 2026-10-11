//! UB tests for `libavutil/buffer.h`'s unsafe `BufferPool::with_allocator`,
//! each allocator within its `# Safety` contract: it returns NULL or a sole,
//! writable reference whose buffer holds at least the pool's size, releasable
//! on any thread, and never requests a buffer from its own pool.
//!
//! Buffers are requested through C's `av_buffer_pool_get` on the owner's raw
//! seam until `AVBufferRef` has a safe owner.

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

use ffmpeg::libavutil::buffer::{BufferPool, BufferPoolBox};
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
        // SAFETY: the box keeps the pool alive; av_buffer_pool_get locks it.
        let buf = unsafe { ffi::av_buffer_pool_get(pool.as_c_ptr()) };
        (!buf.is_null()).then_some(Self(buf))
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
fn default_alloc(size: usize) -> *mut ffi::AVBufferRef {
    // SAFETY: no preconditions; NULL or a sole, writable `size`-byte buffer.
    unsafe { ffi::av_buffer_alloc(size) }
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
    // SAFETY: each call returns NULL or a fresh sole reference to
    // `size + extra` writable av_malloc bytes, freeable on any thread, and
    // never touches the pool.
    unsafe {
        BufferPool::with_allocator(size, move |n| {
            let _ = &guard;
            calls.fetch_add(1, Ordering::SeqCst);
            ffi::av_buffer_alloc(n + extra)
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
    // SAFETY: every other call returns NULL, the rest a sole 32-byte
    // av_malloc reference; the pool is never touched.
    let pool = unsafe {
        BufferPool::with_allocator(32, move |size| {
            let _ = &guard;
            n += 1;
            if n % 2 == 1 {
                ptr::null_mut()
            } else {
                ffi::av_buffer_alloc(size)
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
    // SAFETY: `default_alloc` returns NULL or a sole, writable 48-byte buffer.
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
    // SAFETY: each call returns NULL or a sole, writable 40-byte av_malloc
    // reference, freeable on any thread; the pool is never touched.
    let pool = unsafe {
        BufferPool::with_allocator(40, move |size| {
            log.sizes.push(size);
            seen.fetch_add(1, Ordering::Relaxed);
            ffi::av_buffer_alloc(size)
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

/// `av_buffer_create` frees, counted per opaque.
unsafe extern "C" fn counted_free(opaque: *mut c_void, data: *mut u8) {
    // SAFETY: `opaque` is the test's static `AtomicUsize`; `data` came from
    // av_malloc in `custom_buffers_keep_their_opaque_and_free`.
    unsafe {
        (*opaque.cast::<AtomicUsize>()).fetch_add(1, Ordering::SeqCst);
        ffi::av_free(data.cast());
    }
}

#[test]
fn custom_buffers_keep_their_opaque_and_free() {
    // Only this test frees through `counted_free` with this counter.
    static FREES: AtomicUsize = AtomicUsize::new(0);
    let frees = &FREES;
    let opaque = ptr::from_ref(frees).cast_mut().cast::<c_void>() as usize;
    // SAFETY: each call wraps `size` fresh av_malloc bytes in a sole,
    // writable reference whose free (`counted_free`, thread-safe) releases
    // them; the counter it reaches is 'static.
    let pool = unsafe {
        BufferPool::with_allocator(24, move |size| {
            let data = ffi::av_malloc(size).cast::<u8>();
            if data.is_null() {
                return ptr::null_mut();
            }
            let buf =
                ffi::av_buffer_create(data, size, Some(counted_free), opaque as *mut c_void, 0);
            if buf.is_null() {
                ffi::av_free(data.cast());
            }
            buf
        })
    }
    .unwrap();
    let (a, b) = (Owned::get(&pool).unwrap(), Owned::get(&pool).unwrap());
    for buf in [&a, &b] {
        // SAFETY: a live reference from this pool.
        let original = unsafe { ffi::av_buffer_pool_buffer_get_opaque(buf.0) };
        assert_eq!(original as usize, opaque);
        buf.fill(5);
    }
    drop(a);
    assert_eq!(frees.load(Ordering::SeqCst), 0);
    let recycled = Owned::get(&pool).unwrap();
    // SAFETY: as above.
    let original = unsafe { ffi::av_buffer_pool_buffer_get_opaque(recycled.0) };
    assert_eq!(original as usize, opaque);
    drop(pool);
    drop(recycled);
    drop(b);
    // Both entries went back through their own free when the pool went.
    assert_eq!(frees.load(Ordering::SeqCst), 2);
}
