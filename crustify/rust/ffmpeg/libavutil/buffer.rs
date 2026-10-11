//! Safe wrappers over `libavutil/buffer.h`.
//!
//! Two layouts here are opaque to the public API, so neither has field
//! accessors:
//!
//! - [`Buffer`] (`AVBuffer`) is a reference-counted data buffer. Nothing owns
//!   one directly: each `AVBufferRef` holds a counted reference, and the last
//!   one to go runs the buffer's data release callback and frees the body. A
//!   [`BufferRef`] handle is therefore always borrowed from a reference, and
//!   its one public operation is identity ([`BufferRef::ptr_eq`]), which tells
//!   whether two references share a data buffer. The `BufferRef` and
//!   `BufferMut` names are this layout's handles, so `AVBufferRef`'s own
//!   wrapper is [`BufferReference`].
//! - [`BufferPool`] (`AVBufferPool`) recycles equally sized buffers. The
//!   caller's pool is a sole owner, [`BufferPoolBox`], released with
//!   `av_buffer_pool_uninit`; buffers taken from the pool keep its body alive
//!   after that, so they never borrow from the box.
//!
//! [`default_free`] is the release an `AVBuffer` applies to its data when it
//! was created without a free callback (`av_buffer_default_free`): the data
//! is an `av_malloc`-family block, a [`HeapVec`], freed with `av_free`.
//!
//! [`BufferReference`] is `AVBufferRef`, one reference to a buffer: a counted
//! share of it and a window into its bytes. The caller owns the reference as
//! a [`BufferReferenceBox`], which `clone` shares with another reference to
//! the same buffer and whose drop releases one, the data going with the last.
//! Its bytes are read through [`BufferReferenceRef::data`], and written
//! through [`BufferReferenceMut::data_mut`] only while the reference is the
//! buffer's only one and not read-only, as C's convention has it. A new
//! buffer of uninitialised bytes is an [`UninitBuffer`] until filled.

use core::ffi::{c_int, c_uint, c_void};
use core::fmt;
use core::mem::MaybeUninit;
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use core::slice;

use ffibox::{CBox, CDrop, CDupClone, define_ctype};

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
    /// withheld: nothing owns a body as such, and the references that
    /// release one ([`BufferReference`]) carry their own proof.
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
    /// Each buffer `alloc` returns passes to the pool, as the only reference
    /// to a writable buffer ([`UninitBuffer`]); `None` fails the request.
    ///
    /// State that cannot move to another thread is rejected:
    ///
    /// ```compile_fail,E0277
    /// use ffmpeg::libavutil::buffer::BufferPool;
    /// let local = std::rc::Rc::new(0);
    /// // SAFETY: the allocator returns no buffer.
    /// let _ = unsafe {
    ///     BufferPool::with_allocator(8, move |_| {
    ///         let _ = &local;
    ///         None
    ///     })
    /// };
    /// ```
    ///
    /// and so is state borrowed from the caller, which the pool can outlive:
    ///
    /// ```compile_fail,E0597
    /// use ffmpeg::libavutil::buffer::BufferPool;
    /// let calls = std::sync::atomic::AtomicUsize::new(0);
    /// // SAFETY: the allocator returns no buffer.
    /// let _ = unsafe {
    ///     BufferPool::with_allocator(8, |_| {
    ///         calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ///         None
    ///     })
    /// };
    /// ```
    ///
    /// # Safety
    ///
    /// Each buffer `alloc` returns must hold at least `size` bytes from the
    /// start of its buffer's own data, whatever part the reference itself
    /// describes: C recycles the buffer from its start with the pool's size.
    /// A buffer from [`BufferReference::alloc`] of at least `size` bytes
    /// does; one whose window was advanced past its start may not.
    ///
    /// Neither `alloc` nor the release of a buffer it returns (the drop of a
    /// [`from_owner`](BufferReference::from_owner) owner) may call into this
    /// same pool: requesting one of its buffers, releasing the last reference
    /// to one, or dropping its owner all lock the pool mutex. C holds that
    /// mutex during every `alloc` call, and while dropping the owner hands
    /// the pool's free buffers to their releases; the pool's final release
    /// runs those as it frees the pool.
    pub unsafe fn with_allocator<F>(size: usize, alloc: F) -> Option<BufferPoolBox>
    where
        F: FnMut(usize) -> Option<UninitBuffer> + Send + 'static,
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

/// `alloc2` for [`BufferPool::with_allocator`]: runs the boxed allocator and
/// hands its buffer's reference to the pool.
unsafe extern "C" fn allocate_with<F>(state: *mut c_void, size: usize) -> *mut ffi::AVBufferRef
where
    F: FnMut(usize) -> Option<UninitBuffer> + Send + 'static,
{
    // SAFETY: `state` is the pool's live `Box<F>`, dropped only by
    // `drop_allocator` after every call; C makes each call under the pool
    // mutex, so no other `&mut F` exists, and the contract of
    // `with_allocator` forbids calling into this pool from `alloc`.
    let alloc = unsafe { &mut *state.cast::<F>() };
    alloc(size).map_or(ptr::null_mut(), UninitBuffer::into_c)
}

/// `pool_free` for [`BufferPool::with_allocator`]: drops the boxed allocator.
unsafe extern "C" fn drop_allocator<F>(state: *mut c_void)
where
    F: FnMut(usize) -> Option<UninitBuffer> + Send + 'static,
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

define_ctype!(
    /// Wraps: AVBufferRef
    ///
    /// One reference to a [`Buffer`]: one of its counted references, and a
    /// window of [`len`](BufferReferenceRef::len) bytes into its data. Its
    /// getters are on [`BufferReferenceRef`] (`as_ref()`), its setters on
    /// [`BufferReferenceMut`] (`as_mut()`).
    ///
    /// The struct itself belongs to one holder, a [`BufferReferenceBox`] (C
    /// keeps its size out of the ABI, so it is never held by value); the
    /// buffer and its bytes are shared by every reference to it. Cloning a box
    /// adds a reference to the same buffer (`av_buffer_ref`), and the last one
    /// dropped releases the data. Two references to one buffer may describe
    /// different parts of it.
    ///
    /// Every reference a handle reaches holds one count of a live buffer and
    /// describes `len` initialised bytes inside that buffer's data (a NULL
    /// `data` only when `len` is 0), and its buffer's data release may run on
    /// any thread. Bytes that are not yet initialised are held by an
    /// [`UninitBuffer`] instead. The bytes are written only through a
    /// reference that is the buffer's only one and not read-only
    /// ([`data_mut`](BufferReferenceMut::data_mut)), as C's own convention
    /// has it.
    ///
    /// `Send` and `Sync`: C counts references atomically, and the bytes are
    /// written only while a single reference exists.
    BufferReference,
    BufferReferenceRef,
    BufferReferenceMut,
    ffi::AVBufferRef
);

// SAFETY: a reference is three words owned by its one holder. What it shares
// is the buffer, whose count C changes atomically (buffer.h: referencing and
// unreferencing are thread-safe), and whose data release the type's invariant
// makes callable on any thread: `av_free` for `av_malloc` data, or the drop of
// a `Send` Rust owner (`release_owner`).
unsafe impl Send for BufferReference {}
// SAFETY: through a shared handle, Rust reads the struct and its bytes, and C
// reads the struct and updates or loads the count atomically (`av_buffer_ref`,
// `av_buffer_get_ref_count`, `av_buffer_is_writable`). The bytes are written
// only through an exclusive handle whose buffer has a single reference, so
// not while another reference, or a shared handle to this one, exists.
unsafe impl Sync for BufferReference {}
// SAFETY: as for `Sync for BufferReference`: copies of a shared handle only
// read, and C's routines over a shared reference are thread-safe.
unsafe impl Send for BufferReferenceRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for BufferReferenceRef<'_> {}
// SAFETY: as for `Send for BufferReference`: the handle is the only access to
// the struct, and every write it makes to the shared bytes is checked sole.
unsafe impl Send for BufferReferenceMut<'_> {}
// SAFETY: `&BufferReferenceMut` reaches the reference only through `as_ref`,
// a shared handle; the setters need `&mut`.
unsafe impl Sync for BufferReferenceMut<'_> {}

/// Wraps: av_buffer_unref
/// Handwritten
///
/// Releases a [`BufferReferenceBox`]'s reference: C frees the struct and drops
/// one count of its buffer, releasing the data with the last one. Its copy
/// adds a reference to the same buffer (`av_buffer_ref`). `impl_cdrop!` and
/// `impl_cdupclone!` would need raw-pointer adapters only to pass the
/// reference's slot and to check the count first; the trait methods hold
/// both.
///
/// Stateless: C frees the struct with `av_free` on any thread, and the
/// buffer's data release may run on any thread by [`BufferReference`]'s
/// invariant.
#[derive(Clone, Copy, Debug, Default)]
pub struct BufferUnref;

// SAFETY: av_buffer_unref releases exactly the one reference a box holds: it
// av_frees the struct (NULLing only this local slot) and decrements the
// buffer's count with acq_rel ordering, running the data release and freeing
// the body on the last one. It reads nothing of the box afterwards.
unsafe impl CDrop<BufferReference> for BufferUnref {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<BufferReference>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVBufferRef>();
        // SAFETY: the caller hands over the box's sole claim on a live
        // reference; C keeps no pointer to the slot.
        unsafe { ffi::av_buffer_unref(&mut slot) }
    }
}

/// The count of references at which [`BufferUnref`] refuses another one.
///
/// C counts an `AVBuffer`'s references in an unchecked `atomic_uint`, and a
/// safe `mem::forget(reference.clone())` leaks a count that nothing will
/// release: 2^32 of them would wrap it, so a later release frees the buffer
/// under live references. Half the range leaves the other half for clones
/// that pass the check concurrently on other threads, at most one per thread.
const MAX_REFERENCES: c_uint = c_uint::MAX / 2;

/// Whether a buffer counting `count` references may take another, `count`
/// being what `av_buffer_get_ref_count` returns: C converts the unsigned
/// count to `int`, which converts back to it.
fn may_add_reference(count: c_int) -> bool {
    (count as c_uint) < MAX_REFERENCES
}

/// Wraps: av_buffer_ref
///
/// A new reference to the same buffer, the same window of the same bytes.
// SAFETY: a non-null result is a fresh av_mallocz'd struct, independent of
// `ptr` and released by `c_drop`. What the two references share is the
// buffer, which C treats as immutable while it has more than one reference
// (`av_buffer_is_writable`, the check `data_mut` makes) except for its atomic
// count, so writing through either box never reaches the other's bytes. NULL
// means C failed, or the count would near wrapping (`MAX_REFERENCES`).
unsafe impl CDupClone<BufferReference> for BufferUnref {
    #[inline]
    unsafe fn c_dup(&self, ptr: NonNull<BufferReference>) -> Option<NonNull<BufferReference>> {
        // SAFETY: the caller passes a live reference, borrowed for the call.
        let src = unsafe { BufferReferenceRef::from_ptr(ptr.as_ptr().cast()) }?;
        NonNull::new(src.try_clone()?.into_raw())
    }
}

/// One reference to a buffer, owned: the reference is released when the box
/// drops, and the buffer's data with its last reference. `clone` adds a
/// reference to the same buffer (`av_buffer_ref`), and aborts the process
/// where C cannot allocate one, or the buffer's count would near wrapping;
/// `try_clone` returns `None` there. `Send` and `Sync` with
/// [`BufferReference`].
///
/// Built by [`BufferReference`]'s constructors, never allocated bare:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::buffer::BufferReferenceBox;
/// let _ = BufferReferenceBox::new();
/// ```
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::buffer::BufferReferenceBox;
/// let _ = BufferReferenceBox::new_zeroed();
/// ```
///
/// Its bytes are borrowed from the box, so they cannot outlive it:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::buffer::BufferReference;
/// let buf = BufferReference::alloc_zeroed(8).unwrap();
/// let bytes = buf.as_ref().data();
/// drop(buf);
/// assert_eq!(bytes[0], 0);
/// ```
///
/// and a writable view excludes every other use of the reference, cloning
/// included:
///
/// ```compile_fail,E0502
/// use ffmpeg::libavutil::buffer::BufferReference;
/// let mut buf = BufferReference::alloc_zeroed(8).unwrap();
/// let mut handle = buf.as_mut();
/// let bytes = handle.data_mut().unwrap();
/// let copy = buf.clone();
/// bytes[0] = 1;
/// ```
pub type BufferReferenceBox = CBox<BufferReference, BufferUnref>;

impl BufferReference {
    /// Wraps: av_buffer_alloc
    ///
    /// A reference to a new buffer of `size` uninitialised `av_malloc` bytes,
    /// which C frees with `av_free`; `None` when C cannot allocate them (more
    /// than the [`set_max_alloc`](super::mem::set_max_alloc) limit, or out of
    /// memory). Fill it to get a [`BufferReferenceBox`]; for zeroed bytes in
    /// one step, see [`alloc_zeroed`](Self::alloc_zeroed).
    #[must_use]
    pub fn alloc(size: usize) -> Option<UninitBuffer> {
        // SAFETY: av_buffer_alloc takes any size and returns NULL or a fresh,
        // sole and writable reference to `size` av_malloc bytes, released by
        // av_buffer_unref. Its bytes are uninitialised, so the box goes
        // straight into an `UninitBuffer`, which never lends its handles.
        unsafe { BufferReferenceBox::from_c(ffi::av_buffer_alloc(size)) }.map(UninitBuffer)
    }

    /// Wraps: av_buffer_allocz
    ///
    /// A reference to a new buffer of `size` zeroed `av_malloc` bytes, which C
    /// frees with `av_free`; `None` when C cannot allocate them.
    #[must_use]
    pub fn alloc_zeroed(size: usize) -> Option<BufferReferenceBox> {
        // SAFETY: av_buffer_allocz returns NULL or a fresh, sole reference to
        // `size` zeroed av_malloc bytes, released by av_buffer_unref with
        // av_free, on any thread.
        unsafe { BufferReferenceBox::from_c(ffi::av_buffer_allocz(size)) }
    }

    /// Wraps: av_buffer_create
    ///
    /// A reference to a new, writable buffer over `data`, which it then owns:
    /// C frees the block with `av_free` once the last reference is released
    /// (`av_buffer_default_free`). An empty array makes an empty buffer. When
    /// C cannot allocate the buffer, `data` comes back untouched.
    pub fn from_heap_vec(data: HeapVec<u8>) -> Result<BufferReferenceBox, HeapVec<u8>> {
        let (ptr, len) = data.into_raw_parts();
        // SAFETY: `ptr` is NULL with `len` 0, or the `len` initialised bytes
        // of the av_malloc block whose claim `into_raw_parts` gave up; a NULL
        // free selects av_buffer_default_free (av_free), which runs on any
        // thread. C stores the block or leaves it untouched.
        let buf = unsafe { ffi::av_buffer_create(ptr, len, None, ptr::null_mut(), 0) };
        // SAFETY: NULL, or a fresh reference with the type's invariant.
        unsafe { BufferReferenceBox::from_c(buf) }.ok_or_else(|| {
            // SAFETY: C failed and kept nothing: the block is ours again,
            // with the claim `into_raw_parts` gave up.
            unsafe { HeapVec::from_raw_parts_or_empty(ptr, len) }.unwrap_or_else(HeapVec::empty)
        })
    }

    /// Wraps: av_buffer_create
    ///
    /// A reference to a new, writable buffer over the bytes of `owner`, which
    /// the buffer then owns: `owner` is dropped once the last reference is
    /// released, on the thread releasing it, hence `Send`. A panic in its
    /// `Drop` aborts the process. When C cannot allocate the buffer, `owner`
    /// comes back.
    ///
    /// `as_mut` is called once, and the bytes it returns are the buffer's for
    /// as long as `owner` lives, so they must not move or change size while
    /// it is left alone, as for a `Vec<u8>`, a `Box<[u8]>` or an array.
    pub fn from_owner<T>(owner: T) -> Result<BufferReferenceBox, T>
    where
        T: AsMut<[u8]> + Send + 'static,
    {
        let owner = Box::into_raw(Box::new(owner));
        // SAFETY: `owner` is the live box just leaked; nothing else uses it
        // until `release_owner` or the failure path reclaims it, so these
        // bytes stay exclusively the buffer's.
        let bytes = unsafe { (*owner).as_mut() };
        let (data, len) = (bytes.as_mut_ptr(), bytes.len());
        let (free, opaque) = (Some(release_owner::<T> as ReleaseFn), owner.cast());
        // SAFETY: `data` addresses `len` initialised, writable bytes that
        // live, unmoved, as long as the leaked `owner`, which C keeps as the
        // opaque of `release_owner::<T>`, or neither when it fails.
        let buf = unsafe { ffi::av_buffer_create(data, len, free, opaque, 0) };
        // SAFETY: NULL, or a fresh sole reference to initialised bytes whose
        // release drops a `Send` owner, on any thread.
        unsafe { BufferReferenceBox::from_c(buf) }.ok_or_else(|| {
            // SAFETY: C failed before storing `owner`, so the box is ours.
            *unsafe { Box::from_raw(owner) }
        })
    }

    /// Wraps: av_buffer_create
    ///
    /// A reference to a new, read-only buffer over the bytes of `owner`, which
    /// the buffer then owns, as [`from_owner`](Self::from_owner). The buffer
    /// carries C's `AV_BUFFER_FLAG_READONLY`, so no reference to it is ever
    /// writable and the bytes are never written, by Rust or C; anything that
    /// needs to write copies them first. `owner` may thus share its bytes,
    /// as an `Arc<[u8]>` or a `&'static [u8]` does.
    pub fn from_read_only<T>(owner: T) -> Result<BufferReferenceBox, T>
    where
        T: AsRef<[u8]> + Send + 'static,
    {
        let owner = Box::into_raw(Box::new(owner));
        // SAFETY: as in `from_owner`; the shared borrow suffices, as the
        // bytes are only read.
        let bytes = unsafe { (*owner).as_ref() };
        let (data, len) = (bytes.as_ptr().cast_mut(), bytes.len());
        let (free, opaque) = (Some(release_owner::<T> as ReleaseFn), owner.cast());
        let flags = ffi::AV_BUFFER_FLAG_READONLY as c_int;
        // SAFETY: as in `from_owner`, for bytes unwritten by anyone: the flag
        // makes every reference unwritable, so C never writes them either.
        let buf = unsafe { ffi::av_buffer_create(data, len, free, opaque, flags) };
        // SAFETY: as in `from_owner`.
        unsafe { BufferReferenceBox::from_c(buf) }.ok_or_else(|| {
            // SAFETY: C failed before storing `owner`, so the box is ours.
            *unsafe { Box::from_raw(owner) }
        })
    }
}

/// The signature of an `AVBuffer`'s data release.
type ReleaseFn = unsafe extern "C" fn(*mut c_void, *mut u8);

/// The data release of a buffer over a Rust owner: drops the `Box<T>` that
/// [`BufferReference::from_owner`] or
/// [`from_read_only`](BufferReference::from_read_only) passed as its opaque.
unsafe extern "C" fn release_owner<T>(opaque: *mut c_void, _data: *mut u8) {
    // SAFETY: C calls a buffer's release once, after its last reference is
    // gone (the acq_rel decrement orders every access to the bytes before
    // it), with the opaque the constructor stored: the leaked `Box<T>`.
    drop(unsafe { Box::from_raw(opaque.cast::<T>()) });
}

impl<'a> BufferReferenceRef<'a> {
    /// The `data` and `size` fields, an empty window as a dangling pointer
    /// and 0, so that every pair addresses a valid run.
    fn parts(&self) -> (NonNull<u8>, usize) {
        // SAFETY: reads of two fields projected through the shared handle's
        // pointer to a live, initialised `AVBufferRef`.
        let (data, len) = unsafe {
            (
                addr_of!((*self.as_ptr()).data).read(),
                addr_of!((*self.as_ptr()).size).read(),
            )
        };
        match NonNull::new(data) {
            Some(data) if len > 0 => (data, len),
            _ => (NonNull::dangling(), 0),
        }
    }

    /// Whether the buffer has this one reference and is not read-only, so
    /// its bytes are this reference's alone.
    fn is_sole_and_writable(&self) -> bool {
        // SAFETY: a live reference; C acquire-loads the count and reads the
        // buffer's flags, which are fixed since creation.
        unsafe { ffi::av_buffer_is_writable(self.as_ptr()) != 0 }
    }

    /// Field: AVBufferRef.data
    ///
    /// The bytes this reference describes, borrowed for as long as it lives.
    /// Nothing writes them meanwhile: a writer needs the only reference to
    /// the buffer, and this is one.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        let (data, len) = self.parts();
        // SAFETY: by the type's invariant, `data` addresses `len` initialised
        // bytes of the buffer (or is dangling for 0), which this reference
        // keeps alive for `'a`; they are written only through a sole
        // reference, which this one, held for `'a`, rules out.
        unsafe { slice::from_raw_parts(data.as_ptr(), len) }
    }

    /// Field: AVBufferRef.size
    ///
    /// The number of bytes this reference describes, which may be fewer than
    /// its buffer holds.
    #[must_use]
    pub fn len(&self) -> usize {
        // SAFETY: a read of the `size` field projected through the shared
        // handle's pointer to a live, initialised `AVBufferRef`.
        unsafe { addr_of!((*self.as_ptr()).size).read() }
    }

    /// Whether this reference describes no byte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Field: AVBufferRef.buffer
    ///
    /// The buffer this reference counts, whose identity tells whether two
    /// references share their data ([`BufferRef::ptr_eq`]).
    #[must_use]
    pub fn buffer(&self) -> BufferRef<'a> {
        // SAFETY: a read of the `buffer` pointer projected through the shared
        // handle's pointer to a live `AVBufferRef`.
        let buffer = unsafe { addr_of!((*self.as_ptr()).buffer).read() };
        // SAFETY: by the type's invariant, the pointer addresses the live
        // buffer this reference counts, kept alive by it for `'a`.
        unsafe { BufferRef::from_ptr(buffer) }.expect("a reference counts a buffer")
    }

    /// Field: AVBufferRef.buffer
    ///
    /// A new, owned reference to the same buffer, describing the same bytes
    /// (`av_buffer_ref`, as [`BufferReferenceBox`]'s `clone`); `None` when C
    /// cannot allocate it, or the buffer's count would near wrapping.
    #[must_use]
    pub fn try_clone(&self) -> Option<BufferReferenceBox> {
        // SAFETY: a live reference; C atomically loads its buffer's count.
        if !may_add_reference(unsafe { ffi::av_buffer_get_ref_count(self.as_ptr()) }) {
            return None;
        }
        // SAFETY: a live reference; C copies the struct into a fresh
        // av_mallocz block and adds one count to the buffer, or returns NULL
        // with the count untouched.
        let copy = unsafe { ffi::av_buffer_ref(self.as_ptr()) };
        // SAFETY: NULL, or a fresh reference to this one's buffer and bytes,
        // which keeps the type's invariant.
        unsafe { BufferReferenceBox::from_c(copy) }
    }
}

impl fmt::Debug for BufferReferenceRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferReference")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

impl BufferReferenceMut<'_> {
    /// Field: AVBufferRef.data
    ///
    /// The bytes this reference describes, writable, when the buffer has no
    /// other reference and is not read-only (`av_buffer_is_writable`); `None`
    /// otherwise. Cloning the reference makes it unwritable until the clone
    /// is gone.
    #[must_use]
    pub fn data_mut(&mut self) -> Option<&mut [u8]> {
        let shared = self.as_ref();
        if !shared.is_sole_and_writable() {
            return None;
        }
        let (data, len) = shared.parts();
        // SAFETY: by the type's invariant, `data` addresses `len` initialised
        // bytes of the buffer (or is dangling for 0), writable as it is not
        // read-only. This is its only reference (the acquire load ordered
        // every released one before us), and the exclusive borrow of `self`
        // stops it being cloned while the slice lives, so nothing else reads
        // or writes them.
        Some(unsafe { slice::from_raw_parts_mut(data.as_ptr(), len) })
    }

    /// Field: AVBufferRef.data
    ///
    /// Drops the first `n` bytes from the window this reference describes,
    /// as C callers do by advancing `data`; the buffer and other references
    /// to it are unchanged.
    ///
    /// # Panics
    ///
    /// When `n` exceeds [`len`](BufferReferenceRef::len).
    pub fn advance(&mut self, n: usize) {
        let (data, len) = self.as_ref().parts();
        assert!(n <= len, "cannot advance a {len}-byte reference by {n}");
        if n == 0 {
            return;
        }
        let p = self.as_mut_ptr();
        // SAFETY: writes of two fields projected through the exclusive
        // handle's pointer to a live `AVBufferRef`. A window of `len > 0`
        // bytes starts at its real `data`, and the new one is its tail, at
        // most one past its end, so it keeps the type's invariant.
        unsafe {
            addr_of_mut!((*p).data).write(data.as_ptr().add(n));
            addr_of_mut!((*p).size).write(len - n);
        }
    }

    /// Field: AVBufferRef.size
    ///
    /// Shortens the window this reference describes to its first `len`
    /// bytes, as C callers do by lowering `size`; nothing changes when it is
    /// no longer than that already.
    pub fn truncate(&mut self, len: usize) {
        if len < self.as_ref().len() {
            // SAFETY: a write of the `size` field projected through the
            // exclusive handle's pointer to a live `AVBufferRef`; the shorter
            // window keeps the type's invariant.
            unsafe { addr_of_mut!((*self.as_mut_ptr()).size).write(len) }
        }
    }

    /// Field: AVBufferRef.buffer
    ///
    /// Makes this the reference `with` was, buffer and window, and returns
    /// the reference this was, as an owner. The struct C may know this
    /// reference by stays where it is; only the contents move.
    #[must_use = "dropping the old reference releases it; use `set` for that"]
    pub fn replace(&mut self, with: BufferReferenceBox) -> BufferReferenceBox {
        // SAFETY: swaps two live `AVBufferRef`s, this exclusively borrowed one
        // and the one `with` owns; each holds one count of its buffer, which
        // moves with it, so both keep the type's invariant. Both structs are
        // av_malloc blocks, released alike by av_buffer_unref.
        unsafe { ptr::swap(self.as_mut_ptr(), with.as_c_ptr()) };
        with
    }

    /// Field: AVBufferRef.buffer
    ///
    /// Makes this the reference `with` was, and releases the reference this
    /// was, as [`replace`](Self::replace) and a drop.
    pub fn set(&mut self, with: BufferReferenceBox) {
        drop(self.replace(with));
    }
}

/// Wraps: AVBufferRef
/// Handwritten
///
/// A reference to a buffer whose bytes may not be initialised yet: what
/// [`BufferReference::alloc`] returns, and what a pool allocator hands over
/// ([`BufferPool::with_allocator`]). It is the buffer's only reference, not
/// read-only, and lends no handle, so its bytes are this owner's alone, seen
/// as [`MaybeUninit<u8>`]. [`fill`](Self::fill),
/// [`copy_from_slice`](Self::copy_from_slice) or
/// [`assume_init`](Self::assume_init) make it a [`BufferReferenceBox`].
///
/// A hand-written owner over a [`BufferReferenceBox`], released by its
/// [`BufferUnref`] policy: a second policy on the box would still give it the
/// box's handles, whose getters read the bytes as initialised.
///
/// It lends no handle, and has no clone, which would share its bytes:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::buffer::BufferReference;
/// let buf = BufferReference::alloc(8).unwrap();
/// let _ = buf.as_ref().data();
/// ```
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::buffer::BufferReference;
/// let buf = BufferReference::alloc(8).unwrap();
/// let _ = buf.clone();
/// ```
///
/// and its uninitialised view ends when it is filled:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::buffer::BufferReference;
/// let mut buf = BufferReference::alloc(8).unwrap();
/// let view = buf.as_mut_slice();
/// let filled = buf.fill(0);
/// view[0] = core::mem::MaybeUninit::new(1);
/// ```
///
/// `Send` and `Sync` with [`BufferReference`].
pub struct UninitBuffer(BufferReferenceBox);

impl UninitBuffer {
    /// The number of bytes the reference describes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.as_ref().len()
    }

    /// Whether the reference describes no byte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bytes, which may be uninitialised.
    #[must_use]
    pub fn as_slice(&self) -> &[MaybeUninit<u8>] {
        let (data, len) = self.0.as_ref().parts();
        // SAFETY: `data` addresses the reference's `len` bytes (or is
        // dangling for 0), which live as long as it does; `MaybeUninit`
        // admits any contents, and nothing writes them while `self` is
        // borrowed, as this is their only owner.
        unsafe { slice::from_raw_parts(data.cast::<MaybeUninit<u8>>().as_ptr(), len) }
    }

    /// The bytes, writable, which may be uninitialised.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        let (data, len) = self.0.as_ref().parts();
        // SAFETY: as in `as_slice`; the buffer has this one reference and is
        // not read-only, so its bytes are writable and `&mut self` makes
        // their view exclusive.
        unsafe { slice::from_raw_parts_mut(data.cast::<MaybeUninit<u8>>().as_ptr(), len) }
    }

    /// Sets every byte to `byte`.
    pub fn fill(mut self, byte: u8) -> BufferReferenceBox {
        let bytes = self.as_mut_slice();
        // SAFETY: writes the whole exclusively borrowed run, one operation.
        unsafe { ptr::write_bytes(bytes.as_mut_ptr(), byte, bytes.len()) };
        // SAFETY: every byte was written just above.
        unsafe { self.assume_init() }
    }

    /// Copies `src` into the bytes.
    ///
    /// # Panics
    ///
    /// When `src` is not exactly [`len`](Self::len) bytes long.
    pub fn copy_from_slice(mut self, src: &[u8]) -> BufferReferenceBox {
        let bytes = self.as_mut_slice();
        assert_eq!(
            bytes.len(),
            src.len(),
            "source length differs from the buffer's"
        );
        // SAFETY: `src` and the exclusively borrowed run are both `len` bytes
        // long and cannot overlap, as `src` is borrowed shared and the run
        // exclusively.
        unsafe { ptr::copy_nonoverlapping(src.as_ptr(), bytes.as_mut_ptr().cast(), src.len()) };
        // SAFETY: every byte was written just above.
        unsafe { self.assume_init() }
    }

    /// The reference, its bytes taken as initialised.
    ///
    /// # Safety
    ///
    /// Every byte must have been written, through
    /// [`as_mut_slice`](Self::as_mut_slice) or otherwise.
    pub unsafe fn assume_init(self) -> BufferReferenceBox {
        self.0
    }

    /// Gives the reference to C, for a pool allocator's return.
    pub(crate) fn into_c(self) -> *mut ffi::AVBufferRef {
        self.0.into_c()
    }
}

/// The reference, its bytes from then on seen as possibly uninitialised,
/// when it is its buffer's only one and not read-only; otherwise it comes
/// back.
impl TryFrom<BufferReferenceBox> for UninitBuffer {
    type Error = BufferReferenceBox;

    fn try_from(buf: BufferReferenceBox) -> Result<Self, Self::Error> {
        if buf.as_ref().is_sole_and_writable() {
            Ok(Self(buf))
        } else {
            Err(buf)
        }
    }
}

impl fmt::Debug for UninitBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UninitBuffer")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
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
            F: FnMut(usize) -> Option<UninitBuffer> + Send + 'static,
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
                None
            },
            3,
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // A zero-sized state, never called.
        drive(|_| None, 0);
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
        // SAFETY: each buffer holds `size` bytes from its start; the
        // allocator never touches the pool.
        let pool = unsafe {
            BufferPool::with_allocator(24, move |size| {
                let _ = &guard;
                seen.fetch_add(1, Ordering::SeqCst);
                BufferReference::alloc(size)
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
            // would return no buffer.
            let pool = unsafe {
                BufferPool::with_allocator(64, move |_| {
                    let _ = &guard;
                    None
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
            // SAFETY: each buffer holds 8 bytes from its start; the
            // allocator never touches the pool.
            let pool = unsafe {
                BufferPool::with_allocator(8, move |size| {
                    let _ = &guard;
                    seen.fetch_add(1, Ordering::SeqCst);
                    BufferReference::alloc(size)
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

    #[test]
    #[cfg_attr(miri, ignore)]
    fn recycled_entry_stays_free_when_its_reference_cannot_be_allocated() {
        const FLAG: &str = "CRUSTIFY_BUFFER_POOL_RECYCLE_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            let calls = Arc::new(AtomicUsize::new(0));
            let seen = calls.clone();
            // SAFETY: each buffer holds 8 bytes from its start; the
            // allocator never touches the pool.
            let pool = unsafe {
                BufferPool::with_allocator(8, move |size| {
                    seen.fetch_add(1, Ordering::SeqCst);
                    BufferReference::alloc(size)
                })
            }
            .unwrap();
            let first = get(&pool);
            assert!(!first.is_null());
            // SAFETY: a live reference of this test's.
            let data = unsafe { (*first).data };
            unref(first);
            // Reusing the free entry needs only a new 24-byte AVBufferRef,
            // which a 16-byte limit refuses: C fails the request and keeps
            // the entry listed, without calling the allocator.
            // SAFETY: this isolated child runs only this exact test, so no
            // concurrent C allocator reads or writes the process-wide limit.
            unsafe { ffi::av_max_alloc(16) };
            assert!(get(&pool).is_null());
            // SAFETY: as above; C's default limit.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            let again = get(&pool);
            assert!(!again.is_null());
            // SAFETY: a live reference of this test's.
            assert_eq!(unsafe { (*again).data }, data);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            unref(again);
            drop(pool);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::buffer::unit_tests::recycled_entry_stays_free_when_its_reference_cannot_be_allocated",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// Buffer-frame wave 1, batch 0: `BufferReference`'s internal plumbing.
    ///
    /// Bytes that count the drops of their owner.
    struct Owner {
        bytes: Vec<u8>,
        _drops: DropCount,
    }

    impl AsMut<[u8]> for Owner {
        fn as_mut(&mut self) -> &mut [u8] {
            &mut self.bytes
        }
    }

    #[test]
    fn reference_owner_sizes_and_threading() {
        use core::mem::size_of;
        assert_eq!(size_of::<BufferReferenceRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<BufferReferenceMut<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<BufferReferenceBox>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<UninitBuffer>>(), size_of::<usize>());
        fn send_sync<T: Send + Sync>() {}
        send_sync::<BufferReferenceBox>();
        send_sync::<UninitBuffer>();
        send_sync::<BufferReferenceRef<'_>>();
        send_sync::<BufferReferenceMut<'_>>();
    }

    #[test]
    fn reference_limit_leaves_half_the_count_for_racing_clones() {
        for count in [0, 1, 2, i32::MAX - 1] {
            assert!(may_add_reference(count), "{count}");
        }
        // `MAX_REFERENCES` is `i32::MAX` as unsigned; counts past `INT_MAX`
        // come back from C as negative `int`s.
        assert_eq!(MAX_REFERENCES, i32::MAX as c_uint);
        for count in [i32::MAX, i32::MIN, -2, -1] {
            assert!(!may_add_reference(count), "{count}");
        }
    }

    #[test]
    fn owner_release_drops_the_boxed_owner_once() {
        let drops = Arc::new(AtomicUsize::new(0));
        let owner = Box::into_raw(Box::new(DropCount(drops.clone())));
        // SAFETY: the leaked box's one release, as C makes it.
        unsafe { release_owner::<DropCount>(owner.cast(), ptr::null_mut()) };
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        // A zero-sized owner: a dangling box, dropped without freeing.
        let empty = Box::into_raw(Box::new([0u8; 0]));
        // SAFETY: as above.
        unsafe { release_owner::<[u8; 0]>(empty.cast(), ptr::null_mut()) };
    }

    /// The window getters and narrowing setters over a Rust-built reference
    /// to Rust bytes, so Miri checks their slices: none of them calls C.
    #[test]
    fn reference_window_views_over_rust_storage() {
        let mut bytes: Vec<u8> = (0..32).collect();
        let base = bytes.as_mut_ptr();
        // Never dereferenced by these paths, only compared.
        let body = NonNull::<ffi::AVBuffer>::dangling().as_ptr();
        let mut raw = ffi::AVBufferRef {
            buffer: body,
            data: base,
            size: 32,
        };
        // SAFETY: `raw` describes 32 initialised bytes of `bytes`, both live
        // and used only through this handle until its last use below.
        let mut m = unsafe { BufferReferenceMut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(m.as_ref().data(), &(0..32).collect::<Vec<u8>>()[..]);
        assert_eq!(m.as_ref().buffer().as_ptr(), body.cast_const());
        m.advance(5);
        m.truncate(40);
        m.truncate(10);
        assert_eq!(m.as_ref().data(), &[5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
        m.advance(10);
        assert!(m.as_ref().is_empty() && m.as_ref().data().is_empty());
        assert_eq!(raw.data, base.wrapping_add(15));
        assert_eq!(raw.size, 0);
        // A NULL, empty window stays NULL.
        let mut empty = ffi::AVBufferRef {
            buffer: body,
            data: ptr::null_mut(),
            size: 0,
        };
        // SAFETY: an empty window over no bytes.
        let mut m = unsafe { BufferReferenceMut::from_ptr(&raw mut empty) }.unwrap();
        m.advance(0);
        m.truncate(0);
        assert!(m.as_ref().data().is_empty());
        assert!(empty.data.is_null());
        bytes[0] = 1;
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn reference_adoption_round_trips_and_rejects_null() {
        let buf = BufferReference::alloc(32).unwrap();
        let raw = buf.into_c();
        // SAFETY: the sole reference `into_c` gave up, adopted once; its bytes
        // are written before any is read.
        let buf = unsafe { BufferReferenceBox::from_c(raw) }.unwrap();
        let mut buf = UninitBuffer::try_from(buf).unwrap().fill(7);
        let raw = buf.as_mut().as_mut_ptr();
        assert_eq!(raw, buf.as_c_ptr());
        let back = buf.into_c();
        assert_eq!(back, raw);
        // SAFETY: as above, for an initialised reference.
        let buf = unsafe { BufferReferenceBox::from_c(back) }.unwrap();
        assert_eq!(buf.as_ref().data(), &[7; 32][..]);
        // SAFETY: NULL is rejected without being adopted.
        assert!(unsafe { BufferReferenceBox::from_c(ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn uninit_conversion_takes_only_sole_writable_references() {
        let sole = BufferReference::alloc_zeroed(4).unwrap();
        let copy = sole.clone();
        let sole = UninitBuffer::try_from(sole).unwrap_err();
        let copy = UninitBuffer::try_from(copy).unwrap_err();
        drop(copy);
        let mut uninit = UninitBuffer::try_from(sole).unwrap();
        assert_eq!(uninit.as_mut_slice().len(), 4);
        let read_only = BufferReference::from_read_only(&[1u8, 2][..]).unwrap();
        let read_only = UninitBuffer::try_from(read_only).unwrap_err();
        assert_eq!(read_only.as_ref().data(), &[1, 2]);
        assert_eq!(uninit.copy_from_slice(&[9; 4]).as_ref().data(), &[9; 4]);
    }

    /// The constructors' and the copy's failure paths: inputs come back
    /// untouched and unreleased, counts are unchanged.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn reference_failures_return_their_inputs() {
        const FLAG: &str = "CRUSTIFY_BUFFER_REFERENCE_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            let drops = Arc::new(AtomicUsize::new(0));
            let heap = HeapVec::from_slice(&[3u8; 40]).unwrap();
            let heap_data = heap.as_ptr();
            let shared = BufferReference::alloc_zeroed(8).unwrap();
            // A 40-byte limit admits 40 data bytes but not the 48-byte
            // AVBuffer, so C frees the data it allocated and fails.
            // SAFETY: this isolated child runs only this exact test, so no
            // concurrent C allocator reads or writes the process-wide limit.
            unsafe { ffi::av_max_alloc(40) };
            assert!(BufferReference::alloc(40).is_none());
            assert!(BufferReference::alloc(41).is_none());
            assert!(BufferReference::alloc_zeroed(16).is_none());
            let heap = BufferReference::from_heap_vec(heap).unwrap_err();
            assert_eq!(
                (heap.as_ptr(), heap.as_slice()),
                (heap_data, &[3u8; 40][..])
            );
            let owner = Owner {
                bytes: vec![5u8; 3],
                _drops: DropCount(drops.clone()),
            };
            let owner = BufferReference::from_owner(owner).map(drop).unwrap_err();
            assert_eq!(owner.bytes, [5; 3]);
            let read_only = BufferReference::from_read_only(Box::<[u8]>::from(&[6u8][..]))
                .map(drop)
                .unwrap_err();
            assert_eq!(*read_only, [6]);
            // The 24-byte copy fits a 40-byte limit; a 16-byte one does not,
            // and the count stays.
            assert!(shared.try_clone().is_some());
            // SAFETY: as above.
            unsafe { ffi::av_max_alloc(16) };
            assert!(shared.try_clone().is_none());
            assert!(shared.as_ref().try_clone().is_none());
            // SAFETY: a live reference.
            let count = unsafe { ffi::av_buffer_get_ref_count(shared.as_c_ptr()) };
            assert_eq!(count, 1);
            // SAFETY: as above; C's default limit.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            drop(owner);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            let heap = BufferReference::from_heap_vec(heap).unwrap();
            assert_eq!(heap.as_ref().data(), &[3u8; 40][..]);
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::buffer::unit_tests::reference_failures_return_their_inputs",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
