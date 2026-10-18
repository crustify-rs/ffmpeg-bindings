//! `BufferPool::with_allocator` with an allocator that keeps the contract as
//! the affected commit states it: every call returns a fresh, sole, writable
//! `av_buffer_create` reference of the pool's size whose free callback runs on
//! any thread, and the allocator itself never calls into the pool.
//!
//! The free callback of those buffers releases a reference to another buffer
//! of the same pool, which the program owns. C runs that callback from
//! `buffer_pool_flush` while `av_buffer_pool_uninit` holds the pool mutex, so
//! the release (`pool_release_buffer`) locks the same default mutex again on
//! the same thread: undefined in POSIX, a self-deadlock in glibc.
//!
//! ```console
//! cargo run --offline
//! ```
//!
//! The drop runs on a helper thread and the main thread gives it five
//! seconds: the program exits 1 when it deadlocks (the affected commit), 0 if
//! the drop returns.

use core::ffi::c_void;
use core::ptr;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ffmpeg::libavutil::buffer::BufferPool;
use ffmpeg::libavutil::ffi;

/// A buffer reference the program owns, kept for the free callback.
struct Owned(*mut ffi::AVBufferRef);

// SAFETY: C counts buffer references atomically, and `Owned` is the only path
// to its reference.
unsafe impl Send for Owned {}

/// The reference the free callback releases, at most once.
static KEPT: Mutex<Option<Owned>> = Mutex::new(None);

/// The free callback of every buffer the allocator returns: frees the data,
/// then releases the kept reference, if any.
unsafe extern "C" fn free_data(_opaque: *mut c_void, data: *mut u8) {
    // SAFETY: `data` is the `av_malloc` block the allocator handed to
    // `av_buffer_create`, released here once.
    unsafe { ffi::av_free(data.cast()) };
    if let Some(Owned(mut buf)) = KEPT.lock().unwrap().take() {
        eprintln!("free callback: releasing the kept buffer of the same pool");
        // SAFETY: the program owns this reference and releases it once.
        unsafe { ffi::av_buffer_unref(&mut buf) };
    }
}

/// A fresh, sole, writable `size`-byte reference, freed by `free_data`.
fn alloc(size: usize) -> *mut ffi::AVBufferRef {
    // SAFETY: no preconditions; NULL is checked.
    let data = unsafe { ffi::av_malloc(size) }.cast::<u8>();
    if data.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `data` is `size` bytes of av_malloc storage, now owned by the
    // new buffer and released by `free_data`.
    let buf = unsafe { ffi::av_buffer_create(data, size, Some(free_data), ptr::null_mut(), 0) };
    if buf.is_null() {
        // SAFETY: the buffer was not created, so `data` is still ours.
        unsafe { ffi::av_free(data.cast()) };
    }
    buf
}

fn main() {
    // SAFETY: as the affected commit's `# Safety` reads: each call returns NULL
    // or a fresh `av_buffer_create` reference (the only one to its buffer, no
    // copy kept) to `size` writable bytes, not read-only, whose free callback
    // (`av_free`, then the release of a reference the program owns) may run on
    // any thread at any time; `alloc` never calls into the pool.
    let pool = unsafe { BufferPool::with_allocator(16, alloc) }.expect("pool");
    let get = || {
        // SAFETY: the box keeps the pool alive; C locks it for the request.
        let buf = unsafe { ffi::av_buffer_pool_get(pool.as_c_ptr()) };
        assert!(!buf.is_null());
        Owned(buf)
    };
    // Two allocator buffers: `a` goes back to the free list, `b` is kept.
    let (Owned(mut a), b) = (get(), get());
    // SAFETY: the program owns `a` and releases it once.
    unsafe { ffi::av_buffer_unref(&mut a) };
    *KEPT.lock().unwrap() = Some(b);

    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        eprintln!("dropping the pool owner: av_buffer_pool_uninit flushes `a`");
        drop(pool);
        done.send(()).unwrap();
    });
    match finished.recv_timeout(Duration::from_secs(5)) {
        Ok(()) => eprintln!("the owner's drop returned"),
        Err(_) => {
            eprintln!(
                "deadlock: the free callback, run under the pool mutex by \
                 av_buffer_pool_uninit, locked it again to return the kept buffer"
            );
            std::process::exit(1);
        }
    }
}
