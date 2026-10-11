//! Safe wrappers over `libavutil/buffer.h`.
//!
//! Both layouts here are opaque to the public API, so neither has field
//! accessors:
//!
//! - [`Buffer`] (`AVBuffer`) is a reference-counted data buffer. Nothing owns
//!   one directly: each `AVBufferRef` holds a counted reference, and the last
//!   one to go runs the buffer's data release callback and frees the body. A
//!   [`BufferRef`] handle is therefore always borrowed from a reference, and
//!   its one public operation is identity ([`BufferRef::ptr_eq`]), which tells
//!   whether two references share a data buffer. The `BufferRef` and
//!   `BufferMut` names are this layout's handles, so `AVBufferRef`'s own
//!   wrapper needs another name.
//! - [`BufferPool`] (`AVBufferPool`) recycles equally sized buffers. The
//!   caller's pool is a sole owner, [`BufferPoolBox`], released with
//!   `av_buffer_pool_uninit`; buffers taken from the pool keep its body alive
//!   after that, so they never borrow from the box.
//!
//! [`default_free`] is the release an `AVBuffer` applies to its data when it
//! was created without a free callback (`av_buffer_default_free`): the data
//! is an `av_malloc`-family block, a [`HeapVec`], freed with `av_free`.

use core::ffi::c_void;
use core::ptr::{self, NonNull};

use ffibox::{CBox, CDrop, define_ctype};

use super::ffi;
use super::mem::HeapVec;

define_ctype!(
    /// Wraps: AVBuffer
    ///
    /// A reference-counted data buffer, reached only through the references
    /// that keep it alive: its handles borrow from one of them, and there is
    /// no owner of the body itself. C keeps the layout private.
    ///
    /// `Sync`, so shared handles cross threads: the count is atomic, and C
    /// writes the other fields only before the first reference is published
    /// or through a reference it has proven to be the only one. `Send` is
    /// withheld: no Rust path owns or releases a body yet.
    Buffer,
    BufferRef,
    BufferMut,
    ffi::AVBuffer
);

// The C readers of a shared AVBuffer are av_buffer_get_opaque,
// av_buffer_get_ref_count, av_buffer_is_writable and av_buffer_ref; buffer.h
// documents reference counting as thread-safe (atomic add, acq_rel subtract).
// SAFETY: those readers touch the atomic `refcount`, or fields fixed while
// more than one reference can reach the body: buffer_create,
// pool_alloc_buffer and av_buffer_pool_get write them before the first
// reference is out, and av_buffer_realloc rewrites data/size only for a
// writable, i.e. sole, reference. No `BufferRef` method writes.
unsafe impl Sync for Buffer {}
// SAFETY: as for `Sync for Buffer`: copies of a shared handle only compare
// addresses, and C's readers of a shared buffer are race-free.
unsafe impl Send for BufferRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for BufferRef<'_> {}
// SAFETY: `&BufferMut` reaches the buffer only through `as_ref`, a shared
// handle.
unsafe impl Sync for BufferMut<'_> {}

impl BufferRef<'_> {
    /// Whether both handles view the same data buffer, as C callers compare
    /// two `AVBufferRef.buffer` pointers. Two references to one buffer may
    /// still describe different parts of its data.
    #[must_use]
    pub fn ptr_eq(&self, other: &BufferRef<'_>) -> bool {
        ptr::eq(self.as_ptr(), other.as_ptr())
    }
}

define_ctype!(
    /// Wraps: AVBufferPool
    ///
    /// A pool of equally sized, reusable buffers, owned by the caller as a
    /// [`BufferPoolBox`]. C keeps the layout private.
    ///
    /// The caller's box is one counted reference to the pool, and every buffer
    /// handed out is another, so the pool, its allocator state and its free
    /// buffers live until the box is dropped *and* the last outstanding buffer
    /// is returned, on whichever thread that happens.
    ///
    /// `Send` and `Sync`: every C path that touches a pool after
    /// construction takes its mutex or uses its atomic count.
    BufferPool,
    BufferPoolRef,
    BufferPoolMut,
    ffi::AVBufferPool
);

// After av_buffer_pool_init/init2 publish a pool, C touches it only through
// av_buffer_pool_get, av_buffer_pool_uninit and its buffers' release callback
// (pool_release_buffer, on whichever thread drops a buffer's last reference).
// The default allocator, av_buffer_alloc, is thread-agnostic.
// SAFETY: each of those takes the pool mutex around the free list and the
// allocator call and counts references atomically; init alone writes the
// other fields. The last release destroys the unlocked mutex and frees the
// pool on its own thread, which pthread permits. `with_allocator` requires
// `Send` allocator state, whose calls C serializes under the mutex.
unsafe impl Send for BufferPool {}
// SAFETY: as for `Send`: a shared handle reaches only routines that lock the
// pool mutex or use its atomic count, and no `BufferPoolRef` method writes.
unsafe impl Sync for BufferPool {}
// SAFETY: as for `Send for BufferPool`: an exclusive handle reaches the same
// lock-protected routines as the owner.
unsafe impl Send for BufferPoolMut<'_> {}
// SAFETY: as for `Sync for BufferPool`.
unsafe impl Send for BufferPoolRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for BufferPoolRef<'_> {}
// SAFETY: `&BufferPoolMut` reaches the pool only through `as_ref`, a shared
// handle.
unsafe impl Sync for BufferPoolMut<'_> {}

/// Wraps: av_buffer_pool_uninit
/// Handwritten
///
/// Releases the caller's reference to a pool. `impl_cdrop!` would need a
/// raw-pointer adapter only to pass the pool's slot; the trait method holds
/// the slot itself.
///
/// The pool body is freed now, or later by its last outstanding buffer's
/// return; either way the pool's free buffers are released here.
#[derive(Clone, Copy, Debug, Default)]
pub struct BufferPoolUninit;

// SAFETY: av_buffer_pool_uninit releases exactly the one reference
// av_buffer_pool_init/init2 gave the caller: it flushes the free entries under
// the mutex and decrements the count, freeing the pool only when no buffer is
// outstanding. It never reads the box's storage after that decrement.
unsafe impl CDrop<BufferPool> for BufferPoolUninit {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<BufferPool>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVBufferPool>();
        // SAFETY: the caller hands over the box's sole claim on a live pool;
        // C NULLs only this local slot and keeps no pointer to it.
        unsafe { ffi::av_buffer_pool_uninit(&mut slot) }
    }
}

/// The caller's sole reference to a buffer pool, released by
/// [`BufferPoolUninit`]. Not `Clone`: C has no up-ref for the caller's
/// reference. `Send` and `Sync` with [`BufferPool`].
///
/// A pool is only built by its constructors, never allocated bare:
///
/// ```compile_fail
/// use ffmpeg::libavutil::buffer::BufferPoolBox;
/// let _ = BufferPoolBox::new();
/// ```
/// ```compile_fail
/// use ffmpeg::libavutil::buffer::BufferPoolBox;
/// let _ = BufferPoolBox::new_uninit();
/// ```
/// and it has no deep copy:
/// ```compile_fail
/// use ffmpeg::libavutil::buffer::BufferPool;
/// let pool = BufferPool::new(64).unwrap();
/// let _ = pool.clone();
/// ```
pub type BufferPoolBox = CBox<BufferPool, BufferPoolUninit>;

impl BufferPool {
    /// Wraps: av_buffer_pool_init
    ///
    /// A pool of `size`-byte buffers allocated with C's default allocator,
    /// `av_buffer_alloc`. Nothing is allocated for the buffers until one is
    /// requested, so any `size` succeeds here. `None` when C cannot allocate
    /// the pool or initialise its mutex.
    ///
    /// For a custom allocator, see [`with_allocator`](Self::with_allocator),
    /// which also covers `av_buffer_pool_init`'s non-NULL `alloc`.
    #[must_use]
    pub fn new(size: usize) -> Option<BufferPoolBox> {
        // SAFETY: a NULL allocator selects av_buffer_alloc; C returns NULL or
        // a fresh pool holding the one reference the box adopts and
        // BufferPoolUninit releases.
        unsafe { BufferPoolBox::from_c(ffi::av_buffer_pool_init(size, None)) }
    }

    /// Wraps: av_buffer_pool_init2
    ///
    /// A pool of `size`-byte buffers drawn from `alloc`, called with `size`
    /// whenever the pool has no free buffer. The pool owns `alloc`: it is
    /// dropped when the pool is finally freed, after this box is dropped and
    /// every outstanding buffer has been returned, on the thread returning
    /// the last one; or here, when C fails to build the pool (`None`).
    ///
    /// C calls `alloc` with its pool mutex held, from any thread requesting
    /// a buffer, one call at a time; hence `FnMut + Send`. A panic in
    /// `alloc` aborts the process.
    ///
    /// This replaces `av_buffer_pool_init`'s custom allocator too: a closure
    /// that ignores its state is that case.
    ///
    /// State that cannot move to another thread is rejected:
    ///
    /// ```compile_fail,E0277
    /// use ffmpeg::libavutil::buffer::BufferPool;
    /// let local = std::rc::Rc::new(0);
    /// // SAFETY: the allocator only returns NULL.
    /// let _ = unsafe {
    ///     BufferPool::with_allocator(8, move |_| {
    ///         let _ = &local;
    ///         core::ptr::null_mut()
    ///     })
    /// };
    /// ```
    ///
    /// and so is state borrowed from the caller, which the pool can outlive:
    ///
    /// ```compile_fail,E0597
    /// use ffmpeg::libavutil::buffer::BufferPool;
    /// let calls = std::sync::atomic::AtomicUsize::new(0);
    /// // SAFETY: the allocator only returns NULL.
    /// let _ = unsafe {
    ///     BufferPool::with_allocator(8, |_| {
    ///         calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ///         core::ptr::null_mut()
    ///     })
    /// };
    /// ```
    ///
    /// # Safety
    ///
    /// The `AVBufferRef` return is a raw seam until `AVBufferRef` has a safe
    /// owner. Each call of `alloc` must return NULL (the request fails) or a
    /// reference whose ownership passes to the pool, and that reference must
    /// - be the only reference to its buffer, now and later (no copy kept);
    /// - have a buffer whose own data is at least `size` writable bytes from
    ///   its start, whatever part the reference itself describes (C recycles
    ///   the buffer from its start with the pool's size), and not be marked
    ///   read-only, since recycled references are writable;
    /// - be releasable by its buffer's free callback on any thread, at any
    ///   time until the pool is freed.
    ///
    /// `alloc` must not call into this same pool: requesting one of its
    /// buffers, releasing the last reference to one, or dropping its owner
    /// all lock the pool mutex C holds during the call.
    pub unsafe fn with_allocator<F>(size: usize, alloc: F) -> Option<BufferPoolBox>
    where
        F: FnMut(usize) -> *mut ffi::AVBufferRef + Send + 'static,
    {
        let state = Box::into_raw(Box::new(alloc)).cast::<c_void>();
        // SAFETY: the trampolines are instantiated for this `F`, and `state`
        // is a leaked `Box<F>` that only they use: C passes it to every alloc
        // call and to pool_free, once, when the pool is freed.
        let pool = unsafe {
            ffi::av_buffer_pool_init2(
                size,
                state,
                Some(allocate_with::<F>),
                Some(drop_allocator::<F>),
            )
        };
        if pool.is_null() {
            // SAFETY: init2 failed before storing `state`, so C never calls
            // either trampoline and the box is still ours to drop.
            drop(unsafe { Box::from_raw(state.cast::<F>()) });
            return None;
        }
        // SAFETY: a fresh pool holding the one reference the box adopts and
        // BufferPoolUninit releases.
        unsafe { BufferPoolBox::from_c(pool) }
    }
}

/// `alloc2` for [`BufferPool::with_allocator`]: runs the boxed allocator.
unsafe extern "C" fn allocate_with<F>(state: *mut c_void, size: usize) -> *mut ffi::AVBufferRef
where
    F: FnMut(usize) -> *mut ffi::AVBufferRef + Send + 'static,
{
    // SAFETY: `state` is the pool's live `Box<F>`, dropped only by
    // `drop_allocator` after every call; C makes each call under the pool
    // mutex, so no other `&mut F` exists, and the contract of
    // `with_allocator` forbids calling into this pool from `alloc`.
    let alloc = unsafe { &mut *state.cast::<F>() };
    alloc(size)
}

/// `pool_free` for [`BufferPool::with_allocator`]: drops the boxed allocator.
unsafe extern "C" fn drop_allocator<F>(state: *mut c_void)
where
    F: FnMut(usize) -> *mut ffi::AVBufferRef + Send + 'static,
{
    // SAFETY: C calls pool_free once, after the last allocator call (the
    // last reference's acq_rel decrement orders them), with the `Box<F>`
    // leaked by `with_allocator`.
    drop(unsafe { Box::from_raw(state.cast::<F>()) });
}

/// Wraps: av_buffer_default_free
///
/// Releases `data` as an `AVBuffer` created without a free callback releases
/// its data: C frees the `av_malloc`-family block with `av_free` and ignores
/// its `opaque` argument. The elements are never dropped, as when `data`
/// drops; an empty array is a no-op.
pub fn default_free<T>(data: HeapVec<T>) {
    let (data, _) = data.into_raw_parts();
    // SAFETY: `data` is NULL or the `av_malloc`-family block whose claim
    // `into_raw_parts` gave up; C passes it to `av_free` once and never reads
    // the NULL opaque.
    unsafe { ffi::av_buffer_default_free(ptr::null_mut(), data.cast()) }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Counts the drops of a value it is moved into.
    struct DropCount(Arc<AtomicUsize>);

    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// A buffer from the pool, through C (no safe `get` exists yet).
    fn get(pool: &BufferPoolBox) -> *mut ffi::AVBufferRef {
        // SAFETY: the box keeps the pool alive; av_buffer_pool_get locks it.
        unsafe { ffi::av_buffer_pool_get(pool.as_ref().as_ptr().cast_mut()) }
    }

    fn unref(mut buf: *mut ffi::AVBufferRef) {
        // SAFETY: `buf` is a reference this test owns and never uses again.
        unsafe { ffi::av_buffer_unref(&mut buf) };
    }

    #[test]
    fn layout_and_handle_sizes() {
        use core::mem::size_of;
        assert_eq!(size_of::<BufferRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<BufferMut<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<BufferPoolRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<BufferPoolMut<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<BufferPoolBox>>(), size_of::<usize>());
        fn send_sync<T: Send + Sync>() {}
        send_sync::<BufferPoolBox>();
        send_sync::<BufferRef<'_>>();
    }

    #[test]
    fn trampolines_run_and_release_the_boxed_state() {
        /// Runs both trampolines over a leaked `Box<F>`, as C does.
        fn drive<F>(alloc: F, calls: usize)
        where
            F: FnMut(usize) -> *mut ffi::AVBufferRef + Send + 'static,
        {
            let state = Box::into_raw(Box::new(alloc)).cast::<c_void>();
            for size in 1..=calls {
                // SAFETY: `state` is the live `Box<F>`, used by nothing else
                // during the call.
                assert!(unsafe { allocate_with::<F>(state, size) }.is_null());
            }
            // SAFETY: the box's last use, as C's pool_free.
            unsafe { drop_allocator::<F>(state) };
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let guard = DropCount(drops.clone());
        let mut sizes = Vec::new();
        drive(
            move |size| {
                let _ = &guard;
                sizes.push(size);
                assert_eq!(sizes.len(), size);
                ptr::null_mut()
            },
            3,
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // A zero-sized state, never called.
        drive(|_| ptr::null_mut(), 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn buffer_identity_follows_shared_references() {
        // SAFETY: av_buffer_alloc has no preconditions; NULL is checked.
        let a = unsafe { ffi::av_buffer_alloc(16) };
        assert!(!a.is_null());
        // SAFETY: `a` is a live reference; av_buffer_ref only reads it.
        let b = unsafe { ffi::av_buffer_ref(a) };
        // SAFETY: as above.
        let c = unsafe { ffi::av_buffer_alloc(16) };
        assert!(!b.is_null() && !c.is_null());
        // SAFETY: each reference keeps its buffer alive until unref below,
        // after the handles' last use; `buffer` is a non-NULL body pointer.
        let (ha, hb, hc) = unsafe {
            (
                BufferRef::from_ptr((*a).buffer).unwrap(),
                BufferRef::from_ptr((*b).buffer).unwrap(),
                BufferRef::from_ptr((*c).buffer).unwrap(),
            )
        };
        assert!(ha.ptr_eq(&hb) && hb.ptr_eq(&ha) && ha.ptr_eq(&ha));
        assert!(!ha.ptr_eq(&hc) && !hc.ptr_eq(&hb));
        unref(a);
        unref(b);
        unref(c);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn owner_round_trip_and_null_adoption() {
        let pool = BufferPool::new(32).unwrap();
        let ptr = pool.into_c();
        // SAFETY: into_c handed back the sole pool reference, with no handle
        // alive; adoption restores its release policy.
        let pool = unsafe { BufferPoolBox::from_c(ptr) }.unwrap();
        let buf = get(&pool);
        assert!(!buf.is_null());
        drop(pool);
        unref(buf);
        // SAFETY: NULL is rejected without being adopted.
        assert!(unsafe { BufferPoolBox::from_c(ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn release_policy_frees_free_buffers_and_defers_outstanding_ones() {
        let pool = BufferPool::new(8).unwrap();
        let (a, b) = (get(&pool), get(&pool));
        unref(a);
        // `a` waits in the pool's free list, `b` is outstanding: dropping the
        // box flushes `a` and leaves the pool alive for `b`.
        drop(pool);
        // SAFETY: `b` is a live 8-byte reference of this test's.
        unsafe { ptr::write_bytes((*b).data, 0x5a, 8) };
        unref(b);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn allocator_state_is_dropped_once_after_the_last_buffer() {
        let drops = Arc::new(AtomicUsize::new(0));
        let guard = DropCount(drops.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        // SAFETY: av_buffer_alloc returns NULL or a sole, writable reference
        // to `size` bytes of av_malloc storage, freeable from any thread; the
        // allocator never touches the pool.
        let pool = unsafe {
            BufferPool::with_allocator(24, move |size| {
                let _ = &guard;
                seen.fetch_add(1, Ordering::SeqCst);
                ffi::av_buffer_alloc(size)
            })
        }
        .unwrap();
        let a = get(&pool);
        unref(a);
        let b = get(&pool);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(pool);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        unref(b);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn construction_failure_drops_the_allocator() {
        const FLAG: &str = "CRUSTIFY_BUFFER_POOL_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            // SAFETY: this isolated child runs only this exact test, so no
            // concurrent C allocator reads or writes the process-wide limit.
            // Rust's allocator does not use it.
            unsafe { ffi::av_max_alloc(1) };
            assert!(BufferPool::new(64).is_none());
            let drops = Arc::new(AtomicUsize::new(0));
            let guard = DropCount(drops.clone());
            // SAFETY: the allocator is never called (C fails first), and
            // would return NULL.
            let pool = unsafe {
                BufferPool::with_allocator(64, move |_| {
                    let _ = &guard;
                    ptr::null_mut()
                })
            };
            assert!(pool.is_none());
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::buffer::unit_tests::construction_failure_drops_the_allocator",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn entry_allocation_failure_releases_the_new_buffer() {
        const FLAG: &str = "CRUSTIFY_BUFFER_POOL_ENTRY_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            let drops = Arc::new(AtomicUsize::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            let (guard, seen) = (DropCount(drops.clone()), calls.clone());
            // SAFETY: av_buffer_alloc returns NULL or a sole, writable 8-byte
            // reference; the allocator never touches the pool.
            let pool = unsafe {
                BufferPool::with_allocator(8, move |size| {
                    let _ = &guard;
                    seen.fetch_add(1, Ordering::SeqCst);
                    ffi::av_buffer_alloc(size)
                })
            }
            .unwrap();
            // The 8 data bytes, the 48-byte AVBuffer and the 24-byte
            // AVBufferRef fit 64 bytes; the 88-byte BufferPoolEntry does not,
            // so C releases the allocator's fresh buffer and fails the request.
            // SAFETY: this isolated child runs only this exact test, so no
            // concurrent C allocator reads or writes the process-wide limit.
            unsafe { ffi::av_max_alloc(64) };
            assert!(get(&pool).is_null());
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            // SAFETY: as above; C's default limit.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            let buf = get(&pool);
            assert!(!buf.is_null());
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            drop(pool);
            unref(buf);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::buffer::unit_tests::entry_allocation_failure_releases_the_new_buffer",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
