//! Safe wrappers over `libavutil/mem.h`.
//!
//! [`Heap`] is the lifecycle policy of the `av_malloc` family: every block
//! `av_malloc`, `av_mallocz`, `av_malloc_array`, `av_calloc` or `av_memdup`
//! returns is released by `av_free`. Owners name it as their policy:
//!
//! - one object: [`HeapBox<T>`], built by `HeapBox::<T>::new_zeroed()`
//!   (`av_mallocz`) or `HeapBox::<T>::new_uninit()` (`av_malloc`), and
//!   `Clone` (`av_memdup`) when `T: Copy`;
//! - an array: [`HeapVec<T>`], built by [`alloc_array`] (`av_malloc_array`),
//!   [`alloc_array_zeroed`] / [`alloc_cell_array_zeroed`] (`av_calloc`),
//!   [`alloc_array_from_fn`], or `HeapVec::from_slice` and `Clone`
//!   (`av_memdup`).
//!
//! The policy frees storage only. A wrapped type whose object owns
//! sub-allocations is released by its own destructor policy instead.
//!
//! [`StringHeap`] is the same allocator as a string policy: a NUL-terminated
//! string from `av_strdup`, `av_strndup` or any other `av_malloc`-family
//! routine is a [`HeapString`], built by [`string_from_c_str`] (`av_strdup`)
//! or [`string_from_bytes_until_nul`] (`av_strndup`), `Clone` through
//! `av_strdup`, and released by `av_free`.
//!
//! The `av_realloc` family resizes those arrays: [`realloc_array`] and
//! [`realloc_bytes`] hand the array back on failure, [`realloc_array_or_free`]
//! frees it, and [`realloc_array_in_place`] / [`realloc_bytes_in_place`]
//! resize the array in a slot, leaving it empty on failure.
//! [`resize_array_with`] resizes an initialised array, filling what it adds.
//! A resized block keeps `realloc`'s alignment, not `av_malloc`'s.
//! [`checked_size_mul`] is the overflow-checked size product they compute.
//!
//! Two growable owners keep C's in-place growth protocols:
//!
//! - [`DynArray<T>`] is the `av_dynarray` family's `(tab, nb)` pair, grown by
//!   [`push_or_clear`](DynArray::push_or_clear) (`av_dynarray2_add`) and, for
//!   pointer elements, [`push_ptr_or_clear`](DynArray::push_ptr_or_clear)
//!   (`av_dynarray_add`) and [`try_push_ptr`](DynArray::try_push_ptr)
//!   (`av_dynarray_add_nofree`), and released by `av_freep`;
//! - [`FastBuffer`] and [`ZeroedFastBuffer`] are the `(ptr, size)` pair of a
//!   buffer reused while large enough, grown by `av_fast_realloc`,
//!   `av_fast_malloc` and `av_fast_mallocz`, and released by `av_free`.
//!
//! [`set_max_alloc`] sets the process-wide size limit every one of these
//! allocations obeys, and [`copy_back_reference`] is the LZ77-style
//! overlapping copy within one buffer.

use core::ffi::{CStr, c_char, c_double, c_int, c_longlong, c_uint, c_void};
use core::fmt;
use core::marker::PhantomData;
use core::mem::{MaybeUninit, align_of, size_of};
use core::ptr::NonNull;

use ffibox::{
    CAlloc, CAllocZeroed, CBox, CCell, CDrop, CDupClone, CPlainElem, CSlice, CSliceMut, CStrBox,
    CVec, CZeroable, impl_cdupclone_str, impl_clenclone, impl_clendrop,
};

use super::ffi;

/// Alignment of every `av_malloc`-family block: `ALIGN` in `mem.c` is 64, 32
/// or 16 by the build's SIMD configuration, and never less than 16.
const ALIGN: usize = 16;

/// The `av_malloc` family's allocator and `av_free`, as one policy.
///
/// Thread-agnostic: a block may be freed on any thread, so the owners it
/// parameterizes cross threads on their elements' terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Heap;

/// One object in an `av_malloc`-family block, released by `av_free`.
pub type HeapBox<T> = CBox<T, Heap>;

/// An array in an `av_malloc`-family block, released by `av_free`.
pub type HeapVec<T> = CVec<T, Heap>;

/// Wraps: av_free
// SAFETY: `av_free` releases any `av_malloc`-family block exactly once and
// never reads it, so it is sound for storage before (`MaybeUninit<T>`) and
// after it holds a `T`. It releases the storage only: a `T` owning
// sub-allocations is adopted under its own destructor policy, as `CAlloc`
// leaves to the policy to document.
unsafe impl<T> CDrop<T> for Heap {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<T>) {
        // SAFETY: the caller hands over a live `av_malloc`-family block whose
        // claim it owns; `av_free` releases it once.
        unsafe { ffi::av_free(ptr.as_ptr().cast()) }
    }
}

/// Wraps: av_free
///
/// # Safety
///
/// `ptr` must be a live `av_malloc`-family block the caller owns.
unsafe fn free_len(ptr: *mut u8, _byte_len: usize) {
    // SAFETY: the caller hands over a live block it owns; `av_free` releases
    // the whole block whatever its length, touching none of it.
    unsafe { ffi::av_free(ptr.cast()) }
}
impl_clendrop!(Heap, free_len);

/// Wraps: av_memdup
///
/// # Safety
///
/// `ptr` must be valid for reads of `byte_len` bytes, from any allocator.
unsafe fn memdup(ptr: *mut u8, byte_len: usize) -> *mut u8 {
    // SAFETY: the caller guarantees `byte_len` readable bytes at `ptr`, which
    // `av_memdup` only reads. It returns a fresh `av_malloc(byte_len)` block,
    // aligned to `ALIGN` and released by `av_free`, or NULL on failure.
    unsafe { ffi::av_memdup(ptr.cast_const().cast(), byte_len) }.cast()
}
impl_clenclone!(Heap, memdup, align = ALIGN);

/// Wraps: av_memdup
///
/// The singleton form of the deep copy, giving [`HeapBox<T>`] its `Clone`. A
/// byte copy is a sound clone only of a `T` whose bytes own nothing, which
/// `T: Copy` asserts; a wrapped type without it is not cloneable here:
///
/// ```compile_fail,E0599
/// use ffibox::define_ctype;
/// use ffmpeg::libavutil::{ffi, mem::HeapBox};
/// define_ctype!(Rational, RationalRef, RationalMut, ffi::AVRational); // not `Copy`
/// fn copy(b: &HeapBox<Rational>) -> HeapBox<Rational> {
///     b.clone()
/// }
/// ```
// SAFETY: `av_memdup` returns a fresh `av_malloc(size_of::<T>())` block (one
// byte for a zero-sized `T`), aligned to `ALIGN`, which the const assertion
// bounds `align_of::<T>()` by, holding a byte copy of the source, or NULL on
// failure. `T: Copy` makes that copy an independent, valid `T` that shares no
// owned state with the source, and `CDrop` above releases it.
unsafe impl<T: CCell + Copy> CDupClone<T> for Heap {
    #[inline]
    unsafe fn c_dup(&self, ptr: NonNull<T>) -> Option<NonNull<T>> {
        const { assert!(align_of::<T>() <= ALIGN, "av_memdup cannot align T") };
        // SAFETY: the caller hands a live `T`, so `size_of::<T>()` bytes at
        // `ptr` are readable; `av_memdup` only reads them.
        let copy = unsafe { ffi::av_memdup(ptr.as_ptr().cast_const().cast(), size_of::<T>()) };
        NonNull::new(copy.cast())
    }
}

/// Wraps: av_malloc
// SAFETY: `av_malloc` returns a fresh, uniquely-owned block of `size_of::<T>()`
// bytes (one byte for a zero-sized `T`) aligned to `ALIGN`, which the const
// assertion bounds `align_of::<T>()` by, or NULL on failure; `CDrop` above
// frees it without reading it.
unsafe impl<T: CCell> CAlloc<T> for Heap {
    #[inline]
    fn c_alloc(&self) -> Option<NonNull<MaybeUninit<T>>> {
        const { assert!(align_of::<T>() <= ALIGN, "av_malloc cannot align T") };
        // SAFETY: `av_malloc` takes a size by value and has no preconditions.
        NonNull::new(unsafe { ffi::av_malloc(size_of::<T>()) }.cast())
    }
}

/// Wraps: av_mallocz
// SAFETY: as `CAlloc` above, with the block zero-filled, which `T: CZeroable`
// makes a valid `T`; `av_free` accepts the block whatever it holds.
unsafe impl<T: CZeroable> CAllocZeroed<T> for Heap {
    #[inline]
    fn c_alloc_zeroed(&self) -> Option<NonNull<T>> {
        const { assert!(align_of::<T>() <= ALIGN, "av_mallocz cannot align T") };
        // SAFETY: `av_mallocz` takes a size by value and has no preconditions.
        NonNull::new(unsafe { ffi::av_mallocz(size_of::<T>()) }.cast())
    }
}

/// Wraps: av_malloc_array
///
/// Uninitialised storage for `len` elements of `T`; `None` when
/// `len * size_of::<T>()` overflows, exceeds the `av_max_alloc` limit, or
/// cannot be allocated. Write the elements through `as_mut_slice`, or build an
/// initialised array with [`alloc_array_from_fn`] instead.
///
/// Types aligned beyond the allocator's 16-byte guarantee are rejected at
/// compile time:
///
/// ```compile_fail,E0080
/// #[repr(align(32))]
/// #[derive(Clone, Copy)]
/// struct Wide([u8; 32]);
/// let _ = ffmpeg::libavutil::mem::alloc_array::<Wide>(1);
/// ```
pub fn alloc_array<T>(len: usize) -> Option<HeapVec<MaybeUninit<T>>> {
    const { assert!(align_of::<T>() <= ALIGN, "av_malloc_array cannot align T") };
    // SAFETY: `av_malloc_array` takes its sizes by value and has no
    // preconditions.
    let ptr = unsafe { ffi::av_malloc_array(len, size_of::<T>()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned block of
    // `len * size_of::<T>()` bytes (the product does not overflow, or C would
    // have returned NULL), aligned for `T`, released by `av_free`; every bit
    // pattern is a valid `MaybeUninit<T>`.
    unsafe { HeapVec::from_raw_parts(ptr.cast(), len) }
}

/// An array of `len` elements, element `i` initialised to `f(i)`; `None` on
/// allocation failure, as [`alloc_array`].
///
/// The elements are never dropped: the array frees its storage only, so a `T`
/// owning resources leaks them, as it does if `f` panics part-way.
pub fn alloc_array_from_fn<T>(len: usize, mut f: impl FnMut(usize) -> T) -> Option<HeapVec<T>> {
    let mut array = alloc_array::<T>(len)?;
    for (i, slot) in array.as_mut_slice().iter_mut().enumerate() {
        slot.write(f(i));
    }
    // SAFETY: the loop wrote each of the `len` elements.
    Some(unsafe { array.assume_init() })
}

/// Wraps: av_calloc
///
/// `len` zeroed plain elements; `None` on failure, as [`alloc_array`]. Zero is
/// a valid value of every [`CPlainElem`]; a reference is not one:
///
/// ```compile_fail,E0277
/// let _ = ffmpeg::libavutil::mem::alloc_array_zeroed::<&u8>(1);
/// ```
pub fn alloc_array_zeroed<T: CPlainElem>(len: usize) -> Option<HeapVec<T>> {
    const { assert!(align_of::<T>() <= ALIGN, "av_calloc cannot align T") };
    // SAFETY: `av_calloc` takes its sizes by value and has no preconditions.
    let ptr = unsafe { ffi::av_calloc(len, size_of::<T>()) };
    // SAFETY: as `alloc_array`, with the block zero-filled, which
    // `T: CPlainElem` makes `len` valid `T`.
    unsafe { HeapVec::from_raw_parts(ptr.cast(), len) }
}

/// Wraps: av_calloc
///
/// `len` zeroed wrapped C objects, reached through `as_handles`; `None` on
/// failure, as [`alloc_array`]. Their destructor never runs: the array frees
/// its storage only.
pub fn alloc_cell_array_zeroed<T: CZeroable>(len: usize) -> Option<HeapVec<T>> {
    const { assert!(align_of::<T>() <= ALIGN, "av_calloc cannot align T") };
    // SAFETY: `av_calloc` takes its sizes by value and has no preconditions.
    let ptr = unsafe { ffi::av_calloc(len, size_of::<T>()) };
    // SAFETY: as `alloc_array`, with the block zero-filled, which
    // `T: CZeroable` makes `len` valid `T`.
    unsafe { HeapVec::from_raw_parts(ptr.cast(), len) }
}

/// The `av_malloc` family's string allocator and `av_free`, as one policy.
///
/// A separate policy from [`Heap`] because a C string clones by `strlen`
/// (`av_strdup`), not by its type's size. Thread-agnostic, as [`Heap`]: a
/// [`HeapString`] may be freed on any thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StringHeap;

/// An owned NUL-terminated string in an `av_malloc`-family block, released by
/// `av_free`.
pub type HeapString = CStrBox<StringHeap>;

/// Wraps: av_free
/// Handwritten
///
/// `impl_cdrop_str!` would need a raw-pointer adapter only to pass the string
/// to `av_free` as a `void *`; the trait method takes the `NonNull` directly.
// SAFETY: `av_free` releases any `av_malloc`- or `av_realloc`-family block,
// which every string this policy adopts or copies is, exactly once; a string
// owns no sub-allocations.
unsafe impl CDrop<c_char> for StringHeap {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<c_char>) {
        // SAFETY: the caller hands over a live `av_malloc`-family string whose
        // claim it owns; `av_free` releases it once.
        unsafe { ffi::av_free(ptr.as_ptr().cast()) }
    }
}

// The clone strategy: `av_strdup` copies the `strlen(s) + 1` bytes of a live
// string into a fresh `av_realloc` block, which `av_free` above releases, or
// returns NULL on failure. It only reads the source, and the copy shares
// nothing with it.
impl_cdupclone_str!(StringHeap, ffi::av_strdup);

/// Wraps: av_strdup
///
/// A copy of `s`; `None` when it cannot be allocated, which includes a string
/// whose `s.count_bytes() + 1` bytes exceed the `av_max_alloc` limit (`INT_MAX`
/// by default).
pub fn string_from_c_str(s: &CStr) -> Option<HeapString> {
    // SAFETY: `s` is a live NUL-terminated string, which `av_strdup` only
    // reads.
    let ptr = unsafe { ffi::av_strdup(s.as_ptr()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned, NUL-terminated
    // `av_realloc` block, which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

/// Wraps: av_strndup
///
/// A string of the bytes of `bytes` before its first NUL, or of all of them
/// when it has none, as `strndup` copies; `None` when it cannot be allocated,
/// which includes a copy of `n` bytes whose `n + 1` exceed the `av_max_alloc`
/// limit, as for [`string_from_c_str`]. Pass `s.as_bytes()` for a `&str`, and
/// a sub-slice to copy a prefix.
pub fn string_from_bytes_until_nul(bytes: &[u8]) -> Option<HeapString> {
    // An empty slice's pointer is dangling, which C's `memchr` and `memcpy`
    // may not receive even for a zero length.
    let src = if bytes.is_empty() {
        c"".as_ptr()
    } else {
        bytes.as_ptr().cast()
    };
    // SAFETY: `src` is valid for reads of `bytes.len()` bytes, which is all
    // `av_strndup` reads: it scans at most that many for a NUL and copies up
    // to it.
    let ptr = unsafe { ffi::av_strndup(src, bytes.len()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned `av_realloc` block,
    // NUL-terminated by `av_strndup`, which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

/// Alignment of every `av_realloc` block: `realloc`'s, which suits every
/// fundamental C type, so at least that of `double`, `long long` and a pointer.
const REALLOC_ALIGN: usize = {
    let (d, ll, p) = (
        align_of::<c_double>(),
        align_of::<c_longlong>(),
        align_of::<*const u8>(),
    );
    let m = if d > ll { d } else { ll };
    if m > p { m } else { p }
};

/// Handwritten
///
/// A dynamic array grown in place by the `av_dynarray` family, released by
/// `av_freep`.
///
/// C keeps the array as a `(tab, nb)` pair: `nb` elements in an `av_realloc`
/// block whose capacity is implicit, `nb` rounded up to a power of two, and
/// which an append doubles whenever `nb` is one. Only arrays grown from empty
/// by these appends keep that capacity, so a `DynArray` is built empty and
/// grown only through them; an arbitrary [`HeapVec`] cannot become one.
///
/// Elements are appended by value with [`push_or_clear`](Self::push_or_clear),
/// or, when they are pointers ([`DynPtr`]), with
/// [`push_ptr_or_clear`](Self::push_ptr_or_clear) or
/// [`try_push_ptr`](Self::try_push_ptr). Each is a `Copy` value, so neither a
/// failed append that frees the array nor dropping it leaves a destructor
/// unrun. Plain elements read out as a slice, wrapped C objects as handles;
/// [`into_vec`](Self::into_vec) gives the elements up as a [`HeapVec`].
///
/// Hand-written rather than a [`HeapVec`]: C reallocates and frees the `tab`
/// slot and writes the `nb` slot in place, and the implicit capacity is an
/// invariant a `HeapVec` cannot carry.
pub struct DynArray<T> {
    /// NULL, or an `av_realloc` block of at least `len` rounded up to a power
    /// of two elements, the first `len` initialised.
    tab: *mut T,
    /// `nb`, in `0..=INT_MAX`.
    len: c_int,
    _owns: PhantomData<T>,
}

// SAFETY: a `DynArray` owns its block and elements as a `Vec` does, and
// `av_freep` releases the block from any thread.
unsafe impl<T: Send> Send for DynArray<T> {}
// SAFETY: `&DynArray` reaches the elements only through shared views, as
// `&Vec` does.
unsafe impl<T: Sync> Sync for DynArray<T> {}

/// A pointer element of a [`DynArray`]: C's `av_dynarray_add` stores it as a
/// `void *` and never dereferences it, so the array neither owns nor borrows
/// the pointee.
///
/// Implemented for `*const U` and `*mut U`; sealed.
pub trait DynPtr: Copy + sealed::Sealed {}

mod sealed {
    use core::ffi::c_void;

    pub trait Sealed {
        /// The pointer as the `void *` C stores; read back as `Self`, the same
        /// address.
        fn into_void(self) -> *mut c_void;
    }
}

impl<U> sealed::Sealed for *const U {
    fn into_void(self) -> *mut c_void {
        self.cast_mut().cast()
    }
}
impl<U> DynPtr for *const U {}
impl<U> sealed::Sealed for *mut U {
    fn into_void(self) -> *mut c_void {
        self.cast()
    }
}
impl<U> DynPtr for *mut U {}

impl<T> DynArray<T> {
    /// An empty array, which owns no block.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tab: core::ptr::null_mut(),
            len: 0,
            _owns: PhantomData,
        }
    }

    /// The number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether the array holds no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The elements as a [`HeapVec`], released by `av_free`; the spare
    /// capacity goes with them, out of reach.
    #[must_use]
    pub fn into_vec(self) -> HeapVec<T> {
        let this = core::mem::ManuallyDrop::new(self);
        // SAFETY: `tab` is NULL with no elements, or an `av_realloc` block
        // this array owns, holding `len` initialised `T` aligned for `T` (as
        // the appends assert) within its size, which `av_free` releases
        // whatever its length; `ManuallyDrop` hands the claim over.
        unsafe { HeapVec::from_raw_parts_or_empty(this.tab, this.len()) }
            .expect("a dynarray is NULL only when empty")
    }

    /// The first element's address, dangling when the array is empty.
    fn data(&self) -> NonNull<T> {
        NonNull::new(self.tab).unwrap_or(NonNull::dangling())
    }
}

impl<T: CPlainElem> DynArray<T> {
    /// The elements as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: `data()` addresses `len` initialised, aligned `T` the array
        // owns exclusively (or is dangling with none), alive and unwritten
        // while `self` is borrowed.
        unsafe { core::slice::from_raw_parts(self.data().as_ptr(), self.len()) }
    }

    /// The elements as a mutable slice.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as `as_slice`, with the exclusive borrow of `self`.
        unsafe { core::slice::from_raw_parts_mut(self.data().as_ptr(), self.len()) }
    }
}

impl<T: CCell> DynArray<T> {
    /// The elements as shared handles.
    #[must_use]
    pub fn as_handles(&self) -> CSlice<'_, T> {
        // SAFETY: `data()` addresses `len` initialised, aligned `T` the array
        // owns, alive while `self` is borrowed.
        unsafe { CSlice::from_raw_parts(self.data(), self.len()) }
    }

    /// The elements as exclusive handles.
    #[must_use]
    pub fn as_handles_mut(&mut self) -> CSliceMut<'_, T> {
        // SAFETY: as `as_handles`, with the exclusive borrow of `self`.
        unsafe { CSliceMut::from_raw_parts(self.data(), self.len()) }
    }
}

impl<T: Copy> DynArray<T> {
    /// Wraps: av_dynarray2_add
    ///
    /// Appends a copy of `value`, doubling the block when the length is a
    /// power of two; `false` when that fails, which frees the array and
    /// empties it, as C does. It fails when `(len * 2) * size_of::<T>()`
    /// exceeds `INT_MAX` or the `av_max_alloc` limit, or the allocation fails.
    ///
    /// C copies the value's bytes into the block; its `NULL` form, which
    /// leaves the new element uninitialised for the caller to fill, has no
    /// safe counterpart. A zero-sized `T`, by which C would divide, and a `T`
    /// aligned beyond `realloc`'s guarantee are rejected at compile time:
    ///
    /// ```compile_fail,E0080
    /// let mut a = ffmpeg::libavutil::mem::DynArray::<()>::new();
    /// let _ = a.push_or_clear(());
    /// ```
    #[must_use = "a failed append empties the array"]
    pub fn push_or_clear(&mut self, value: T) -> bool {
        const {
            assert!(
                size_of::<T>() != 0,
                "av_dynarray2_add divides by the element size"
            )
        };
        const {
            assert!(
                align_of::<T>() <= REALLOC_ALIGN,
                "av_realloc cannot align T"
            )
        };
        // SAFETY: `tab` / `len` are a pair only this family has grown from
        // NULL / 0, so `tab` holds the capacity `av_dynarray2_add` assumes for
        // `len` elements of `size_of::<T>()` (non-zero) bytes. It reallocates
        // `tab` through its slot, copies the `size_of::<T>()` readable bytes of
        // `value` into the new element and increments `len`, or frees `tab`,
        // nulls the slot and zeroes `len`; it keeps no pointer to either slot.
        let slot = unsafe {
            ffi::av_dynarray2_add(
                (&raw mut self.tab).cast(),
                &mut self.len,
                size_of::<T>(),
                (&raw const value).cast(),
            )
        };
        !slot.is_null()
    }
}

impl<T: DynPtr> DynArray<T> {
    /// Wraps: av_dynarray_add
    ///
    /// Appends the pointer `elem`, doubling the block when the length is a
    /// power of two; `false` when that fails, which frees the array and
    /// empties it, as C does. The pointee is neither read nor owned.
    ///
    /// Only pointer elements go through `av_dynarray_add`'s `void *`:
    ///
    /// ```compile_fail,E0599
    /// let mut a = ffmpeg::libavutil::mem::DynArray::<usize>::new();
    /// let _ = a.push_ptr_or_clear(0);
    /// ```
    #[must_use = "a failed append empties the array"]
    pub fn push_ptr_or_clear(&mut self, elem: T) -> bool {
        // SAFETY: as `push_or_clear`, for `void *` elements, which `T` is
        // layout-compatible with (a thin pointer): `av_dynarray_add` stores
        // `elem` in the new slot without dereferencing it, or frees `tab`,
        // nulls the slot and zeroes `len`.
        unsafe {
            ffi::av_dynarray_add((&raw mut self.tab).cast(), &mut self.len, elem.into_void());
        }
        self.len != 0
    }

    /// Wraps: av_dynarray_add_nofree
    ///
    /// Appends the pointer `elem`, as
    /// [`push_ptr_or_clear`](Self::push_ptr_or_clear); on failure the array
    /// is left as it was and the `AVERROR(ENOMEM)` code is returned.
    pub fn try_push_ptr(&mut self, elem: T) -> Result<(), c_int> {
        // SAFETY: as `push_ptr_or_clear`; on failure `av_dynarray_add_nofree`
        // writes neither slot, and the old block stays the array's.
        let ret = unsafe {
            ffi::av_dynarray_add_nofree((&raw mut self.tab).cast(), &mut self.len, elem.into_void())
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

/// Wraps: av_freep
/// Handwritten
///
/// The release strategy of a [`DynArray`]: `av_freep` on the `tab` slot, as C
/// releases one. Elements are `Copy` and have nothing to drop.
impl<T> Drop for DynArray<T> {
    fn drop(&mut self) {
        // SAFETY: the slot holds NULL or an `av_realloc` block this array
        // owns; `av_freep` frees it once and nulls the slot.
        unsafe { ffi::av_freep((&raw mut self.tab).cast()) }
    }
}

impl<T> Default for DynArray<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Debug for DynArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynArray")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

/// The `(ptr, size)` of an `av_fast` buffer, from `buf`, which gives it up.
fn fast_parts<T>(buf: &mut HeapVec<T>) -> (*mut T, c_uint) {
    let (ptr, len) = core::mem::take(buf).into_raw_parts();
    let size = c_uint::try_from(len).expect("an av_fast buffer fits unsigned int");
    (ptr, size)
}

/// A reusable byte buffer, regrown by `av_fast_realloc` and `av_fast_malloc`
/// only when too small, and released by `av_free`.
///
/// C keeps it as a `(ptr, size)` pair, `size` the block's whole length in
/// bytes, at most `UINT_MAX`. A regrowth allocates `min_size + min_size / 16
/// + 32` bytes (capped by the `av_max_alloc` limit) so that slowly growing
/// requests rarely reallocate. The bytes start uninitialised: write them
/// through [`as_mut_slice`](Self::as_mut_slice). For a buffer whose new bytes
/// are zeroed, use [`ZeroedFastBuffer`].
#[derive(Debug, Default)]
pub struct FastBuffer(HeapVec<MaybeUninit<u8>>);

impl FastBuffer {
    /// An empty buffer, which owns no block.
    #[must_use]
    pub fn new() -> Self {
        Self(HeapVec::empty())
    }

    /// The buffer's size in bytes: what C records as its `size`.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.0.len()
    }

    /// The whole buffer.
    #[must_use]
    pub fn as_slice(&self) -> &[MaybeUninit<u8>] {
        self.0.as_slice()
    }

    /// The whole buffer, writable.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [MaybeUninit<u8>] {
        self.0.as_mut_slice()
    }

    /// The block as a [`HeapVec`] of its whole size.
    #[must_use]
    pub fn into_vec(self) -> HeapVec<MaybeUninit<u8>> {
        self.0
    }

    /// Wraps: av_fast_realloc
    ///
    /// Makes the buffer at least `min_size` bytes, keeping its contents: a
    /// buffer already large enough is left alone, a smaller one is
    /// reallocated, its bytes moved. `false` when that fails, which includes a
    /// `min_size` above `UINT_MAX` or the `av_max_alloc` limit; the buffer is
    /// then left as it was. (C reports a size of 0 there, while the old block
    /// stays allocated for the caller to free; the buffer keeps it instead.)
    #[must_use = "a failed regrowth leaves the buffer too small"]
    pub fn reserve(&mut self, min_size: usize) -> bool {
        let (ptr, old_size) = fast_parts(&mut self.0);
        let mut size = old_size;
        // SAFETY: `ptr` is NULL with `size == 0`, or an `av_malloc`- or
        // `av_realloc`-family block of `size` bytes we own. `av_fast_realloc`
        // returns it unchanged when `min_size <= size`; otherwise it
        // reallocates it, storing the new size, or returns NULL with `size`
        // zeroed, the block still allocated.
        let new = unsafe { ffi::av_fast_realloc(ptr.cast(), &mut size, min_size) };
        let ok = !new.is_null() || min_size <= old_size as usize;
        let (ptr, size) = if ok {
            (new.cast(), size)
        } else {
            (ptr, old_size)
        };
        // SAFETY: `ptr` is NULL with a size of 0, or a block of `size` bytes we
        // own, from `av_realloc` or as it was; `av_free` releases it and every
        // bit pattern is a valid `MaybeUninit<u8>`.
        self.0 = unsafe { HeapVec::from_raw_parts_or_empty(ptr, size as usize) }
            .expect("an av_fast buffer is NULL only when empty");
        ok
    }

    /// Wraps: av_fast_malloc
    ///
    /// Makes the buffer at least `min_size` bytes, without keeping its
    /// contents: a buffer already large enough is left alone, contents
    /// included, a smaller one is freed and replaced by a fresh, uninitialised
    /// block. `false` when that fails, which includes a `min_size` above
    /// `UINT_MAX` or the `av_max_alloc` limit; the buffer is then empty.
    #[must_use = "a failed regrowth empties the buffer"]
    pub fn reserve_discarding(&mut self, min_size: usize) -> bool {
        let (ptr, mut size) = fast_parts(&mut self.0);
        let mut slot = ptr.cast::<c_void>();
        // SAFETY: the slot holds NULL with `size == 0`, or an `av_malloc`- or
        // `av_realloc`-family block of `size` bytes we own, so `av_fast_malloc`'s
        // assertion that a kept block is non-NULL unless `min_size` is 0
        // holds. It keeps the block when `min_size <= size`; otherwise it
        // frees it and stores a fresh `av_malloc` block and its size, or NULL
        // and 0. It keeps no pointer to either slot.
        unsafe { ffi::av_fast_malloc((&raw mut slot).cast(), &mut size, min_size) };
        // SAFETY: the slot holds NULL with a size of 0, or a block of `size`
        // bytes we own; `av_free` releases it and every bit pattern is a valid
        // `MaybeUninit<u8>`.
        self.0 = unsafe { HeapVec::from_raw_parts_or_empty(slot.cast(), size as usize) }
            .expect("an av_fast buffer is NULL only when empty");
        !slot.is_null() || min_size == 0
    }
}

/// A reusable byte buffer whose fresh blocks are zeroed, regrown by
/// `av_fast_mallocz` only when too small, and released by `av_free`.
///
/// As [`FastBuffer`], but every byte is initialised: a regrowth allocates a
/// zeroed block, and a buffer large enough keeps the bytes already written.
#[derive(Debug, Default)]
pub struct ZeroedFastBuffer(HeapVec<u8>);

impl ZeroedFastBuffer {
    /// An empty buffer, which owns no block.
    #[must_use]
    pub fn new() -> Self {
        Self(HeapVec::empty())
    }

    /// The buffer's size in bytes: what C records as its `size`.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.0.len()
    }

    /// The whole buffer.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// The whole buffer, writable.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }

    /// The block as a [`HeapVec`] of its whole size.
    #[must_use]
    pub fn into_vec(self) -> HeapVec<u8> {
        self.0
    }

    /// Wraps: av_fast_mallocz
    ///
    /// Makes the buffer at least `min_size` bytes, without keeping its
    /// contents: a buffer already large enough is left alone, contents
    /// included (not cleared), a smaller one is freed and replaced by a fresh,
    /// zeroed block. `false` when that fails, which includes a `min_size`
    /// above `UINT_MAX` or the `av_max_alloc` limit; the buffer is then empty.
    #[must_use = "a failed regrowth empties the buffer"]
    pub fn reserve_discarding(&mut self, min_size: usize) -> bool {
        let (ptr, mut size) = fast_parts(&mut self.0);
        let mut slot = ptr.cast::<c_void>();
        // SAFETY: as `FastBuffer::reserve_discarding`, through
        // `av_fast_mallocz`, whose fresh block is `av_mallocz`'s.
        unsafe { ffi::av_fast_mallocz((&raw mut slot).cast(), &mut size, min_size) };
        // SAFETY: the slot holds NULL with a size of 0, a fresh zeroed block of
        // `size` bytes, or the block we gave, every byte of which was zeroed
        // or written as a `u8`; `av_free` releases it.
        self.0 = unsafe { HeapVec::from_raw_parts_or_empty(slot.cast(), size as usize) }
            .expect("an av_fast buffer is NULL only when empty");
        !slot.is_null() || min_size == 0
    }
}

/// Wraps: av_size_mult
///
/// `a * b`, or `None` when the product overflows `usize`.
#[must_use]
pub fn checked_size_mul(a: usize, b: usize) -> Option<usize> {
    let mut r = 0;
    // SAFETY: `r` is a live, writable local, which C writes only on success
    // and does not keep.
    let ret = unsafe { ffi::av_size_mult(a, b, &mut r) };
    (ret == 0).then_some(r)
}

/// Adopts a block `av_realloc` returned, of `len` elements of `T`.
///
/// # Safety
///
/// `ptr` must be a fresh, uniquely-owned `av_realloc`-family block of at
/// least `len * size_of::<T>()` bytes, aligned for `T`.
unsafe fn adopt_realloc<T>(ptr: NonNull<core::ffi::c_void>, len: usize) -> HeapVec<MaybeUninit<T>> {
    // SAFETY: per the caller, `ptr` is a uniquely-owned block of `len`
    // elements, aligned for `T`, which `av_free` releases; every bit pattern is
    // a valid `MaybeUninit<T>`.
    match unsafe { HeapVec::from_raw_parts(ptr.as_ptr().cast(), len) } {
        Some(array) => array,
        None => unreachable!("a non-NULL block adopts"),
    }
}

/// Wraps: av_realloc
///
/// The byte block `block` resized to `size` bytes, in place or moved: its
/// first `min(block.len(), size)` bytes carry over and the rest are
/// uninitialised. `Err` hands `block` back untouched when C cannot resize it:
/// `size` exceeds the `av_max_alloc` limit, or the allocation fails.
///
/// An empty `block` (`HeapVec::empty()`) allocates afresh, and a zero `size`
/// still owns a 1-byte block, as C's `realloc(ptr, size + !size)` does. The
/// result has `realloc`'s alignment, not `av_malloc`'s.
pub fn realloc_bytes(
    block: HeapVec<MaybeUninit<u8>>,
    size: usize,
) -> Result<HeapVec<MaybeUninit<u8>>, HeapVec<MaybeUninit<u8>>> {
    // SAFETY: `block`'s pointer is NULL or a live `av_malloc`-family block it
    // owns; `av_realloc` either releases it and returns a block of `size`
    // bytes, or returns NULL and leaves it untouched and owned by `block`.
    let ptr = unsafe { ffi::av_realloc(block.as_ptr().cast(), size) };
    let Some(ptr) = NonNull::new(ptr) else {
        return Err(block);
    };
    // C released the old block: disarm its owner without freeing.
    let _ = block.into_raw_parts();
    // SAFETY: `ptr` is a fresh, uniquely-owned `realloc` block of `size`
    // bytes (one for zero), which `u8` needs no alignment for.
    Ok(unsafe { adopt_realloc(ptr, size) })
}

/// Wraps: av_realloc_array
///
/// `array` resized to `len` elements, in place or moved: its first
/// `min(array.len(), len)` elements carry over and the rest are
/// uninitialised. `Err` hands `array` back untouched when C cannot resize it:
/// `len * size_of::<T>()` overflows or exceeds the `av_max_alloc` limit, or
/// the allocation fails.
///
/// An empty `array` (`HeapVec::empty()`) allocates afresh. The result has
/// `realloc`'s alignment; types aligned beyond it are rejected
/// at compile time:
///
/// ```compile_fail,E0080
/// use ffmpeg::libavutil::mem::{HeapVec, realloc_array};
/// #[repr(align(32))]
/// struct Wide([u8; 32]);
/// let _ = realloc_array::<Wide>(HeapVec::empty(), 1);
/// ```
pub fn realloc_array<T>(
    array: HeapVec<MaybeUninit<T>>,
    len: usize,
) -> Result<HeapVec<MaybeUninit<T>>, HeapVec<MaybeUninit<T>>> {
    const {
        assert!(
            align_of::<T>() <= REALLOC_ALIGN,
            "av_realloc cannot align T"
        )
    };
    // SAFETY: `array`'s pointer is NULL or a live `av_malloc`-family block it
    // owns; `av_realloc_array` either releases it and returns a block of
    // `len * size_of::<T>()` bytes, or returns NULL and leaves it untouched
    // and owned by `array`.
    let ptr = unsafe { ffi::av_realloc_array(array.as_ptr().cast(), len, size_of::<T>()) };
    let Some(ptr) = NonNull::new(ptr) else {
        return Err(array);
    };
    // C released the old block: disarm its owner without freeing.
    let _ = array.into_raw_parts();
    // SAFETY: `ptr` is a fresh, uniquely-owned `realloc` block of `len`
    // elements (the product did not overflow, or C would have returned
    // NULL), aligned to `REALLOC_ALIGN`, which bounds `align_of::<T>()`.
    Ok(unsafe { adopt_realloc(ptr, len) })
}

/// Wraps: av_realloc_array
///
/// The initialised `array` resized to `len` elements: its first
/// `min(array.len(), len)` elements carry over, and each added element `i` is
/// `f(i)`. `Err` hands `array` back untouched, as [`realloc_array`].
///
/// Elements are never dropped, as for every [`HeapVec`]: shrinking discards
/// the tail, and a panic in `f` frees the storage and leaks the elements.
pub fn resize_array_with<T>(
    array: HeapVec<T>,
    len: usize,
    mut f: impl FnMut(usize) -> T,
) -> Result<HeapVec<T>, HeapVec<T>> {
    const {
        assert!(
            align_of::<T>() <= REALLOC_ALIGN,
            "av_realloc cannot align T"
        )
    };
    let kept = array.len().min(len);
    // SAFETY: as in `realloc_array`.
    let ptr = unsafe { ffi::av_realloc_array(array.as_ptr().cast(), len, size_of::<T>()) };
    let Some(ptr) = NonNull::new(ptr) else {
        return Err(array);
    };
    // C released the old block: disarm its owner without freeing.
    let _ = array.into_raw_parts();
    // SAFETY: as in `realloc_array`.
    let mut resized = unsafe { adopt_realloc::<T>(ptr, len) };
    for (i, slot) in resized.as_mut_slice().iter_mut().enumerate().skip(kept) {
        slot.write(f(i));
    }
    // SAFETY: `realloc` carried the first `kept` elements over from the
    // initialised `array`, and the loop wrote the rest.
    Ok(unsafe { resized.assume_init() })
}

/// Wraps: av_realloc_f
///
/// `array` resized to `len` elements, as [`realloc_array`], except that a
/// failure frees `array` and returns `None`.
pub fn realloc_array_or_free<T>(
    array: HeapVec<MaybeUninit<T>>,
    len: usize,
) -> Option<HeapVec<MaybeUninit<T>>> {
    const {
        assert!(
            align_of::<T>() <= REALLOC_ALIGN,
            "av_realloc cannot align T"
        )
    };
    let (ptr, _) = array.into_raw_parts();
    // SAFETY: `ptr` is NULL or a live `av_malloc`-family block whose ownership
    // passes to C: `av_realloc_f` releases it and returns a block of
    // `len * size_of::<T>()` bytes, or frees it and returns NULL.
    let ptr = unsafe { ffi::av_realloc_f(ptr.cast(), len, size_of::<T>()) };
    // SAFETY: as in `realloc_array`.
    NonNull::new(ptr).map(|ptr| unsafe { adopt_realloc(ptr, len) })
}

/// Wraps: av_reallocp
///
/// Resizes the byte block in `slot` to `size` bytes in place, as
/// [`realloc_bytes`], except that:
///
/// - a zero `size` frees the block and leaves `slot` empty;
/// - a failure frees the block, leaves `slot` empty, and returns C's
///   `AVERROR(ENOMEM)`.
pub fn realloc_bytes_in_place(slot: &mut HeapVec<MaybeUninit<u8>>, size: usize) -> Result<(), i32> {
    let (mut ptr, _) = core::mem::take(slot).into_raw_parts();
    // SAFETY: `ptr` is a live local slot holding NULL or a live
    // `av_malloc`-family block whose ownership passes to C: `av_reallocp`
    // stores the resized block of `size` bytes there, or frees the old one
    // and stores NULL.
    let ret = unsafe { ffi::av_reallocp((&raw mut ptr).cast(), size) };
    // SAFETY: `ptr` is NULL, or a fresh, uniquely-owned `realloc` block of
    // `size` bytes, which `u8` needs no alignment for.
    *slot = unsafe { HeapVec::from_raw_parts_or_empty(ptr, if ptr.is_null() { 0 } else { size }) }
        .unwrap_or_default();
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: av_reallocp_array
///
/// Resizes the array in `slot` to `len` elements in place, as
/// [`realloc_array`], except that a failure frees the array, leaves `slot`
/// empty, and returns C's `AVERROR(ENOMEM)`. A zero `len` still owns a 1-byte
/// block, as for [`realloc_array`].
pub fn realloc_array_in_place<T>(
    slot: &mut HeapVec<MaybeUninit<T>>,
    len: usize,
) -> Result<(), i32> {
    const {
        assert!(
            align_of::<T>() <= REALLOC_ALIGN,
            "av_realloc cannot align T"
        )
    };
    let (mut ptr, _) = core::mem::take(slot).into_raw_parts();
    // SAFETY: `ptr` is a live local slot holding NULL or a live
    // `av_malloc`-family block whose ownership passes to C:
    // `av_reallocp_array` stores the resized block of `len * size_of::<T>()`
    // bytes there, or frees the old one and stores NULL.
    let ret = unsafe { ffi::av_reallocp_array((&raw mut ptr).cast(), len, size_of::<T>()) };
    // A NULL slot after a zero-byte resize (whose 1-byte block C failed to
    // allocate, yet reports success for) holds no elements.
    let len = if ptr.is_null() { 0 } else { len };
    // SAFETY: `ptr` is NULL, or a fresh, uniquely-owned `realloc` block of
    // `len` elements aligned to `REALLOC_ALIGN`, which bounds
    // `align_of::<T>()`.
    *slot = unsafe { HeapVec::from_raw_parts_or_empty(ptr, len) }.unwrap_or_default();
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: av_max_alloc
///
/// Sets the largest block, in bytes, any `av_malloc`-family allocation may
/// return from now on, process-wide; larger requests fail as out of memory.
/// The default is `INT_MAX`. C stores the limit in a relaxed atomic, so a
/// call races with no allocation, but blocks allocated earlier keep their
/// size, and other threads see the new limit eventually, not at once.
pub fn set_max_alloc(max: usize) {
    // SAFETY: `av_max_alloc` takes its argument by value and stores it
    // atomically.
    unsafe { ffi::av_max_alloc(max) }
}

/// Why [`copy_back_reference`] refused a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackReferenceError {
    /// `back` or `len` exceeds `c_int::MAX`, which C cannot take.
    TooLong,
    /// The destination run `dst..dst + len` ends past the buffer.
    PastEnd,
    /// `back` exceeds `dst`: the source would start before the buffer.
    BeforeStart,
}

impl fmt::Display for BackReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "back-reference distance or length exceeds INT_MAX",
            Self::PastEnd => "back-reference destination ends past the buffer",
            Self::BeforeStart => "back-reference source starts before the buffer",
        })
    }
}

impl std::error::Error for BackReferenceError {}

/// Wraps: av_memcpy_backptr
///
/// Fills `buf[dst..dst + len]` with the bytes `back` positions earlier, one
/// byte after another, as an LZ77 back-reference expands: a `back` shorter
/// than `len` repeats the last `back` bytes before `dst`. A `back` of 0
/// leaves the buffer unchanged.
///
/// Fails without touching `buf` when the source would start before it, the
/// destination would end past it, or `back` or `len` exceeds `c_int::MAX`.
///
/// The longest runs at a distance above 4 are filled by several C calls of
/// shorter runs: `av_memcpy_backptr` copies such a run in blocks doubling
/// from `back` bytes in an `int`, which overflows once a block of 2^30 bytes
/// or more leaves part of the run to copy (a run longer than the distance,
/// for distances of 2^30 and more), and then copies with the wrapped length.
pub fn copy_back_reference(
    buf: &mut [u8],
    dst: usize,
    back: usize,
    len: usize,
) -> Result<(), BackReferenceError> {
    // A distance beyond `c_int` is rejected below; its bound is never used.
    let max_run = max_back_reference_run(back.min(c_int::MAX as usize));
    copy_back_reference_in_runs(buf, dst, back, len, max_run)
}

/// The block length from which `av_memcpy_backptr`'s doubling block copy
/// (`back > 4`, a run of 16 bytes or more) can no longer double within an
/// `int`.
const BACK_REFERENCE_BLOCK_LIMIT: usize = 1 << 30;

/// The longest run `av_memcpy_backptr` fills at distance `back`, a distance
/// within `c_int`, without overflowing its block length.
///
/// For `back > 4` and a run of at least 16 bytes, C doubles a block from
/// `back` bytes as long as the rest of the run exceeds the block, then copies
/// the rest; the block reaching `2^30` first, `B`, is doubled only when the
/// run exceeds `2 * B - back`, and that doubling overflows `int`. The other
/// distances copy without a block length.
fn max_back_reference_run(back: usize) -> usize {
    if back <= 4 {
        return c_int::MAX as usize;
    }
    let mut block = back;
    while block < BACK_REFERENCE_BLOCK_LIMIT {
        block <<= 1;
    }
    2 * block - back
}

/// [`copy_back_reference`] through C calls of at most `max_run` bytes each,
/// which fill the same bytes: every run reads only bytes before it.
fn copy_back_reference_in_runs(
    buf: &mut [u8],
    dst: usize,
    back: usize,
    len: usize,
    max_run: usize,
) -> Result<(), BackReferenceError> {
    let (Ok(c_back), Ok(_)) = (c_int::try_from(back), c_int::try_from(len)) else {
        return Err(BackReferenceError::TooLong);
    };
    if dst.checked_add(len).is_none_or(|end| end > buf.len()) {
        return Err(BackReferenceError::PastEnd);
    }
    if back > dst {
        return Err(BackReferenceError::BeforeStart);
    }
    debug_assert!(max_run >= 1, "a run must make progress");
    let mut done = 0;
    loop {
        let run = (len - done).min(max_run);
        // `run <= len`, which fits a `c_int`.
        let c_run = run as c_int;
        // SAFETY: `dst + done <= dst + len <= buf.len()`, so the offset stays
        // within (or one past) `buf`, and the pointer derives from the whole
        // mutable slice.
        let at = unsafe { buf.as_mut_ptr().add(dst + done) };
        // SAFETY: C reads `buf[dst + done - back..dst + done + run - back]`
        // and writes `buf[dst + done..dst + done + run]`, both inside `buf`
        // since `back <= dst` and `dst + done + run <= dst + len <=
        // buf.len()`; even the empty copy's reads of up to `min(back, 4)`
        // bytes before `dst` stay at or after `buf[0]`. `run` is within
        // `max_back_reference_run(back)`, so C's block length never overflows.
        // Nothing else accesses `buf` while the exclusive borrow lasts.
        unsafe { ffi::av_memcpy_backptr(at, c_back, c_run) };
        done += run;
        if done == len {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_void;
    use ffibox::{CLenClone, CLenDrop, define_ctype};

    define_ctype!(Cell, CellRef, CellMut, ffi::AVRational);
    // SAFETY: 0/0 is a valid `AVRational` for these tests, which only read it
    // back.
    unsafe impl CZeroable for Cell {}

    // `AVRational` owns nothing, so its bytes copy as a value.
    define_ctype!(
        #[derive(Clone, Copy)]
        Plain,
        PlainRef,
        PlainMut,
        ffi::AVRational
    );
    // SAFETY: as for `Cell`.
    unsafe impl CZeroable for Plain {}

    impl PlainRef<'_> {
        fn get(&self) -> (i32, i32) {
            // SAFETY: a read of the whole object through the shared handle.
            let r = unsafe { self.as_ptr().read() };
            (r.num, r.den)
        }
    }
    impl PlainMut<'_> {
        fn set(&mut self, num: i32, den: i32) {
            // SAFETY: a write of the whole object through the exclusive handle.
            unsafe { self.as_mut_ptr().write(ffi::AVRational { num, den }) }
        }
    }

    impl CellRef<'_> {
        fn num(&self) -> i32 {
            // SAFETY: a read of a projected field through the shared handle.
            unsafe { core::ptr::addr_of!((*self.as_ptr()).num).read() }
        }
    }
    impl CellMut<'_> {
        fn set_num(&mut self, v: i32) {
            // SAFETY: a write of a projected field through the exclusive
            // handle.
            unsafe { core::ptr::addr_of_mut!((*self.as_mut_ptr()).num).write(v) }
        }
    }

    /// The byte-by-byte expansion `av_memcpy_backptr` documents.
    fn expand(buf: &mut [u8], dst: usize, back: usize, len: usize) {
        if back == 0 {
            return;
        }
        for i in dst..dst + len {
            buf[i] = buf[i - back];
        }
    }

    #[test]
    fn back_reference_expands_byte_by_byte() {
        // Every fill path: memset, the 2-, 3- and 4-byte periods, and the
        // short and doubling block copies.
        for back in 0..=20 {
            for len in [
                0, 1, 2, 3, 4, 5, 7, 8, 11, 12, 15, 16, 17, 31, 32, 33, 64, 100,
            ] {
                let mut want: Vec<u8> = (0..200).map(|i| i as u8 ^ 0x5a).collect();
                let mut got = want.clone();
                let dst = 24;
                expand(&mut want, dst, back, len);
                copy_back_reference(&mut got, dst, back, len).unwrap();
                assert_eq!(got, want, "back {back} len {len}");
            }
        }
    }

    #[test]
    fn back_reference_rejects_out_of_bounds_before_ffi() {
        let mut buf = [7u8; 8];
        use BackReferenceError::*;
        assert_eq!(copy_back_reference(&mut buf, 4, 5, 1), Err(BeforeStart));
        assert_eq!(copy_back_reference(&mut buf, 0, 1, 0), Err(BeforeStart));
        assert_eq!(copy_back_reference(&mut buf, 4, 1, 5), Err(PastEnd));
        assert_eq!(copy_back_reference(&mut buf, 9, 0, 0), Err(PastEnd));
        assert_eq!(
            copy_back_reference(&mut buf, usize::MAX, 1, 1),
            Err(PastEnd)
        );
        let big = c_int::MAX as usize + 1;
        assert_eq!(copy_back_reference(&mut buf, 4, big, 1), Err(TooLong));
        assert_eq!(copy_back_reference(&mut buf, 4, 1, big), Err(TooLong));
        assert_eq!(buf, [7; 8]);
    }

    #[test]
    fn back_reference_error_is_a_std_error() {
        use BackReferenceError::*;
        let all = [TooLong, PastEnd, BeforeStart];
        let texts: Vec<String> = all.iter().map(ToString::to_string).collect();
        assert!(texts.iter().all(|t| !t.is_empty()));
        assert_ne!(texts[0], texts[1]);
        assert_ne!(texts[1], texts[2]);
        let e: Box<dyn std::error::Error> = Box::new(PastEnd);
        assert!(e.source().is_none());
        assert_eq!(format!("{BeforeStart:?}"), "BeforeStart");
    }

    #[test]
    fn back_reference_fills_at_both_buffer_edges() {
        let mut buf = [1, 2, 3, 4, 0, 0, 0, 0];
        copy_back_reference(&mut buf, 4, 4, 4).unwrap();
        assert_eq!(buf, [1, 2, 3, 4, 1, 2, 3, 4]);
        copy_back_reference(&mut buf, 8, 8, 0).unwrap();
        let mut one = [9u8];
        copy_back_reference(&mut one, 0, 0, 1).unwrap();
        copy_back_reference(&mut one, 1, 1, 0).unwrap();
        assert_eq!(one, [9]);
    }

    /// `av_memcpy_backptr`'s doubling block copy, as C runs it in `int`
    /// arithmetic: whether the block length overflows for a run of `len` at
    /// distance `back`.
    fn c_block_overflows(back: usize, len: usize) -> bool {
        if back <= 4 || len < 16 {
            return false;
        }
        let (mut block, mut rest) = (back as i64, len as i64);
        while rest > block {
            rest -= block;
            block <<= 1;
            if block > i64::from(c_int::MAX) {
                return true;
            }
        }
        false
    }

    #[test]
    fn max_back_reference_run_is_the_last_run_c_copies_without_overflow() {
        let max = c_int::MAX as usize;
        for back in [0, 1, 4] {
            assert_eq!(max_back_reference_run(back), max, "{back}");
            assert!(!c_block_overflows(back, max), "{back}");
        }
        // Distances whose first block of 2^30 or more leaves room for every
        // run up to `INT_MAX`.
        for back in [5, 6, 7, (1 << 30) - 1] {
            assert!(max_back_reference_run(back) >= max, "{back}");
            assert!(!c_block_overflows(back, max), "{back}");
        }
        // Distances whose first block of 2^30 or more is their double.
        for (back, block) in [(1 << 29, 1 << 30), ((1 << 29) + 1, (1 << 30) + 2)] {
            assert_eq!(max_back_reference_run(back), 2 * block - back, "{back}");
        }
        // Distances of 2^30 and more: the run may be as long as the distance.
        for back in [1 << 30, (1 << 30) + 1, (3 << 29) + 7, max - 1, max] {
            let run = max_back_reference_run(back);
            assert_eq!(run, back, "{back}");
            assert!(!c_block_overflows(back, run), "{back}");
            if run < max {
                assert!(c_block_overflows(back, run + 1), "{back}");
            }
        }
        // Around the smallest distance whose bound is below `INT_MAX`.
        for back in (5..64).chain(((1 << 29) - 3)..((1 << 29) + 3)) {
            let run = max_back_reference_run(back);
            assert!(!c_block_overflows(back, run.min(max)), "{back}");
            if run < max {
                assert!(c_block_overflows(back, run + 1), "{back}");
            }
        }
    }

    #[test]
    fn back_references_in_short_runs_expand_byte_by_byte() {
        // Every run bound down to one byte, over every fill path, against
        // the one-call expansion.
        for back in [0, 1, 2, 3, 4, 5, 6, 9, 16, 17, 20] {
            for len in [0, 1, 7, 15, 16, 17, 31, 33, 64, 100] {
                for max_run in [1, 2, 3, 5, 7, 16, 17, 64, 99, 100, usize::MAX] {
                    let mut want: Vec<u8> = (0..200).map(|i| i as u8 ^ 0x5a).collect();
                    let mut got = want.clone();
                    let dst = 24;
                    expand(&mut want, dst, back, len);
                    copy_back_reference_in_runs(&mut got, dst, back, len, max_run).unwrap();
                    assert_eq!(got, want, "back {back} len {len} runs of {max_run}");
                }
            }
        }
    }

    #[test]
    fn back_references_in_runs_reject_before_the_first_call() {
        use BackReferenceError::*;
        let mut buf = [7u8; 8];
        assert_eq!(
            copy_back_reference_in_runs(&mut buf, 4, 5, 1, 1),
            Err(BeforeStart)
        );
        assert_eq!(
            copy_back_reference_in_runs(&mut buf, 4, 1, 5, 1),
            Err(PastEnd)
        );
        let big = c_int::MAX as usize + 1;
        assert_eq!(
            copy_back_reference_in_runs(&mut buf, 4, big, 1, 1),
            Err(TooLong)
        );
        assert_eq!(
            copy_back_reference_in_runs(&mut buf, 4, 1, big, 1),
            Err(TooLong)
        );
        assert_eq!(buf, [7; 8]);
    }

    #[test]
    fn heap_is_a_zero_sized_send_sync_policy() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Heap>();
        send_sync::<HeapVec<u8>>();
        assert_eq!(size_of::<Heap>(), 0);
        assert_eq!(size_of::<HeapBox<Cell>>(), size_of::<*mut c_void>());
        assert_eq!(size_of::<Option<HeapBox<Cell>>>(), size_of::<*mut c_void>());
        assert_eq!(format!("{Heap:?}"), "Heap");
        assert_eq!(<Heap as CLenClone>::ALIGN, 16);
    }

    #[test]
    fn new_zeroed_box_reads_zero_and_takes_writes() {
        let mut b = HeapBox::<Cell>::new_zeroed().unwrap();
        assert_eq!(b.as_ptr() as usize % ALIGN, 0);
        assert_eq!(b.as_ref().num(), 0);
        b.as_mut().set_num(7);
        assert_eq!(b.as_ref().num(), 7);
    }

    #[test]
    fn new_uninit_box_frees_unfilled_or_promoted() {
        drop(HeapBox::<Cell>::new_uninit().unwrap());

        let mut slot = HeapBox::<Cell>::new_uninit().unwrap();
        // SAFETY: `as_c_ptr` addresses `size_of::<AVRational>()` writable,
        // aligned bytes owned by `slot`.
        unsafe { slot.as_c_ptr().write(ffi::AVRational { num: 3, den: 4 }) };
        // SAFETY: the write above initialised the whole `AVRational`.
        let b = unsafe { slot.assume_init() };
        assert_eq!(b.as_ref().num(), 3);
    }

    #[test]
    fn box_clone_is_an_independent_aligned_av_memdup_copy() {
        let mut a = HeapBox::<Plain>::new_zeroed().unwrap();
        a.as_mut().set(3, 4);
        let b = a.clone();
        assert_ne!(a.as_ptr(), b.as_ptr());
        assert_eq!(b.as_ptr() as usize % ALIGN, 0);
        assert_eq!(b.as_ref().get(), (3, 4));

        // Writes to either side stay on it, and the source may go first.
        a.as_mut().set(-1, 2);
        let mut c = b.try_clone().unwrap();
        c.as_mut().set(5, 6);
        drop(a);
        assert_eq!((b.as_ref().get(), c.as_ref().get()), ((3, 4), (5, 6)));
        drop(b);
        assert_eq!(c.clone().as_ref().get(), (5, 6));
    }

    #[test]
    fn policy_plumbing_duplicates_one_object() {
        let src = Plain::zeroed();
        // SAFETY: `src` is a live `Plain`, only read.
        let copy = unsafe { Heap.c_dup(NonNull::from(&src)) }.unwrap();
        assert_eq!(copy.as_ptr() as usize % ALIGN, 0);
        // SAFETY: `copy` is a fresh `av_memdup` block holding a `Plain`.
        let read = unsafe { copy.as_ptr().cast::<ffi::AVRational>().read() };
        assert_eq!((read.num, read.den), (0, 0));
        // SAFETY: `copy` is a live block we own.
        unsafe { <Heap as CDrop<Plain>>::c_drop(&Heap, copy) };
    }

    #[test]
    fn adopted_av_malloc_block_frees_through_policy() {
        // SAFETY: `av_malloc` has no preconditions.
        let p = unsafe { ffi::av_malloc(size_of::<ffi::AVRational>()) }.cast::<ffi::AVRational>();
        assert!(!p.is_null());
        // SAFETY: `p` is a fresh, aligned `AVRational`-sized block.
        unsafe { p.write(ffi::AVRational { num: 1, den: 2 }) };
        // SAFETY: `p` is an initialised `av_malloc` block whose ownership
        // moves into the box.
        let b = unsafe { HeapBox::<Cell>::from_c(p) }.unwrap();
        assert_eq!(b.as_ref().num(), 1);
        // The pointer round-trips back out, and in again.
        let raw = b.into_c();
        // SAFETY: `raw` came out of the box above, still owned and live.
        let b = unsafe { HeapBox::<Cell>::from_c(raw) }.unwrap();
        drop(b);
    }

    #[test]
    fn adopted_null_is_none() {
        // SAFETY: NULL is rejected before anything is owned.
        assert!(unsafe { HeapBox::<Cell>::from_c(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { HeapVec::<u8>::from_raw_parts(core::ptr::null_mut(), 3) }.is_none());
        // SAFETY: NULL with no elements is an empty array that frees nothing.
        let empty = unsafe { HeapVec::<u8>::from_raw_parts_or_empty(core::ptr::null_mut(), 0) };
        assert!(empty.unwrap().is_empty());
    }

    #[test]
    fn policy_plumbing_copies_and_frees() {
        let src = [1u8, 2, 3, 4, 5];
        // SAFETY: `src` is 5 readable bytes, only read.
        let copy = unsafe { Heap.c_clone_len(src.as_ptr().cast_mut(), src.len()) }.unwrap();
        assert_eq!(copy.as_ptr() as usize % ALIGN, 0);
        // SAFETY: `copy` holds 5 bytes copied from `src`.
        let copied = unsafe { core::slice::from_raw_parts(copy.as_ptr(), 5) };
        assert_eq!(copied, &src);
        // SAFETY: `copy` is a live `av_memdup` block we own.
        unsafe { Heap.c_drop_len(copy.as_ptr(), 5) };

        // SAFETY: `av_malloc` has no preconditions.
        let p = NonNull::new(unsafe { ffi::av_malloc(8) }.cast::<u64>()).unwrap();
        // SAFETY: `p` is a live, unread `av_malloc` block we own.
        unsafe { <Heap as CDrop<MaybeUninit<u64>>>::c_drop(&Heap, p.cast()) };
    }

    #[test]
    fn alloc_array_rejects_overflow_and_limit() {
        assert!(alloc_array::<u64>(usize::MAX).is_none());
        assert!(alloc_array::<u8>(i32::MAX as usize + 1).is_none());
        assert!(alloc_array_zeroed::<u32>(usize::MAX / 2).is_none());
        assert!(alloc_cell_array_zeroed::<Cell>(usize::MAX / 4).is_none());
    }

    #[test]
    fn empty_and_zero_sized_arrays_still_allocate() {
        // `av_malloc(0)` hands back a 1-byte block, so these own a pointer.
        let a = alloc_array::<u32>(0).unwrap();
        assert!(a.is_empty() && !a.as_ptr().is_null());
        let z = alloc_array::<()>(1000).unwrap();
        assert_eq!((z.len(), z.byte_len()), (1000, 0));
        assert!(!z.as_ptr().is_null());
    }

    #[test]
    fn alloc_array_from_fn_frees_on_panic() {
        let r = std::panic::catch_unwind(|| {
            alloc_array_from_fn(8, |i| if i == 5 { panic!("stop") } else { i as u32 })
        });
        assert!(r.is_err());
        let a = alloc_array_from_fn(8, |i| i as u32 * 3).unwrap();
        assert_eq!(a.as_slice(), &[0, 3, 6, 9, 12, 15, 18, 21]);
    }

    #[test]
    fn cell_array_handles_read_zero() {
        let mut a = alloc_cell_array_zeroed::<Cell>(3).unwrap();
        assert!(a.as_handles().iter().all(|c| c.num() == 0));
        a.as_handles_mut().get_mut(1).unwrap().set_num(9);
        let nums: Vec<i32> = a.as_handles().iter().map(|c| c.num()).collect();
        assert_eq!(nums, [0, 9, 0]);
    }

    #[test]
    fn string_heap_is_a_zero_sized_send_sync_policy() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<StringHeap>();
        send_sync::<HeapString>();
        assert_eq!(size_of::<StringHeap>(), 0);
        assert_eq!(size_of::<HeapString>(), size_of::<*mut c_char>());
        assert_eq!(size_of::<Option<HeapString>>(), size_of::<*mut c_char>());
        assert_eq!(format!("{StringHeap:?}"), "StringHeap");
    }

    #[test]
    fn string_policy_plumbing_copies_and_frees() {
        let src = c"policy";
        // SAFETY: `src` is a live NUL-terminated string, only read.
        let copy = unsafe { StringHeap.c_dup(NonNull::from(src).cast()) }.unwrap();
        assert_ne!(copy.as_ptr().cast_const(), src.as_ptr());
        // SAFETY: `copy` is a fresh `av_strdup` string.
        assert_eq!(unsafe { CStr::from_ptr(copy.as_ptr()) }, src);
        // SAFETY: `copy` is a live `av_strdup` string we own.
        unsafe { StringHeap.c_drop(copy) };
    }

    #[test]
    fn adopted_c_strings_free_through_policy() {
        // SAFETY: a literal is a live NUL-terminated string.
        let p = unsafe { ffi::av_strdup(c"adopted".as_ptr()) };
        // SAFETY: `p` is a fresh `av_strdup` string whose ownership moves into
        // the owner.
        let s = unsafe { HeapString::from_raw(p) }.unwrap();
        assert_eq!(s.to_str(), Ok("adopted"));
        // The pointer round-trips back out, and in again.
        let raw = s.into_raw();
        assert_eq!(raw, p);
        // SAFETY: `raw` came out of the owner above, still owned and live.
        let s = unsafe { HeapString::from_raw(raw) }.unwrap();
        drop(s);

        // An `av_malloc` block filled with a string is one too.
        // SAFETY: `av_malloc` has no preconditions.
        let p = unsafe { ffi::av_malloc(3) }.cast::<u8>();
        assert!(!p.is_null());
        // SAFETY: `p` is 3 writable bytes.
        unsafe { p.copy_from_nonoverlapping(c"ok".as_ptr().cast(), 3) };
        // SAFETY: `p` now holds a NUL-terminated string we own.
        let s = unsafe { HeapString::from_raw(p.cast()) }.unwrap();
        assert_eq!(s.as_bytes(), b"ok");

        // SAFETY: NULL is rejected before anything is owned.
        assert!(unsafe { HeapString::from_raw(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn owned_string_moves_into_c_and_back() {
        let s = string_from_c_str(c"handover").unwrap();
        let raw = s.into_raw();
        // C frees what Rust handed over.
        // SAFETY: `raw` is an owned `av_strdup` string, released once here.
        unsafe { ffi::av_free(raw.cast()) };
    }

    #[test]
    fn bytes_constructor_stops_at_the_first_nul() {
        let s = string_from_bytes_until_nul(b"ab\0cd").unwrap();
        assert_eq!(s.as_bytes(), b"ab");
        let s = string_from_bytes_until_nul(b"\0").unwrap();
        assert!(s.is_empty());
        let s = string_from_bytes_until_nul(b"").unwrap();
        assert!(s.is_empty() && !s.as_ptr().is_null());
        let s = string_from_bytes_until_nul(&b"prefix"[..3]).unwrap();
        assert_eq!(s.to_str(), Ok("pre"));
    }

    #[test]
    fn dynarray_is_two_words_and_crosses_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<DynArray<u32>>();
        send_sync::<DynArray<crate::libavutil::intfloat::IntFloat64>>();
        assert!(size_of::<DynArray<u8>>() <= 2 * size_of::<usize>());
        assert!(REALLOC_ALIGN >= align_of::<*const u8>() && REALLOC_ALIGN.is_power_of_two());
    }

    #[test]
    fn dynarray_frees_its_block_through_av_freep() {
        let mut a = DynArray::<u16>::default();
        assert!(a.tab.is_null() && a.is_empty());
        assert_eq!(format!("{a:?}"), "DynArray { len: 0, .. }");
        for i in 0..9 {
            assert!(a.push_or_clear(i));
        }
        assert!(!a.tab.is_null());
        assert_eq!(format!("{a:?}"), "DynArray { len: 9, .. }");
        // `Drop` hands the slot to `av_freep`, which nulls it.
        let mut a = core::mem::ManuallyDrop::new(a);
        // SAFETY: `a` is never used or dropped again after its drop here.
        unsafe { core::mem::ManuallyDrop::drop(&mut a) };
        assert!(a.tab.is_null());
    }

    #[test]
    fn dynarray_hands_its_block_to_a_heap_vec() {
        let mut a = DynArray::<u32>::new();
        for i in 0..5 {
            assert!(a.push_or_clear(i));
        }
        let tab = a.tab;
        let v = a.into_vec();
        assert_eq!((v.as_ptr(), v.as_slice()), (tab, &[0, 1, 2, 3, 4][..]));
        assert!(DynArray::<u32>::new().into_vec().as_ptr().is_null());
    }

    #[test]
    fn dynptr_keeps_the_address() {
        let x = 5u8;
        let p: *const u8 = &x;
        assert_eq!(sealed::Sealed::into_void(p), p.cast_mut().cast());
        assert_eq!(sealed::Sealed::into_void(p.cast_mut()), p.cast_mut().cast());
        let mut a = DynArray::<*mut u8>::new();
        assert!(a.push_ptr_or_clear(core::ptr::null_mut()));
        assert_eq!(a.try_push_ptr(p.cast_mut()), Ok(()));
        assert_eq!(a.as_slice(), &[core::ptr::null_mut(), p.cast_mut()]);
    }

    #[test]
    fn fast_parts_takes_the_block_and_leaves_the_vec_empty() {
        let mut v = alloc_array_zeroed::<u8>(40).unwrap();
        let ptr = v.as_ptr();
        let (p, size) = fast_parts(&mut v);
        assert_eq!((p, size), (ptr, 40));
        assert!(v.is_empty() && v.as_ptr().is_null());
        // SAFETY: `p` is the `av_calloc` block `fast_parts` gave up.
        unsafe { ffi::av_free(p.cast()) };
    }

    #[test]
    fn failed_reserve_keeps_the_block_c_leaves_to_its_caller() {
        let mut b = FastBuffer::new();
        assert!(b.reserve(10));
        for (i, x) in b.as_mut_slice().iter_mut().enumerate() {
            x.write(i as u8);
        }
        let (ptr, cap) = (b.as_slice().as_ptr(), b.capacity());
        // C reports a size of 0 here and returns NULL; the buffer keeps its
        // old block, contents and size instead of leaking or freeing them.
        assert!(!b.reserve(c_uint::MAX as usize + 1));
        assert_eq!((b.as_slice().as_ptr(), b.capacity()), (ptr, cap));
        let v = b.into_vec();
        // SAFETY: every byte was written above.
        let v = unsafe { v.assume_init() };
        assert!(v.as_slice().iter().enumerate().all(|(i, &x)| x == i as u8));
    }

    #[test]
    fn failed_discarding_reserve_empties_the_buffer() {
        let mut b = ZeroedFastBuffer::new();
        assert!(b.reserve_discarding(1));
        assert_eq!(b.capacity(), 1 + 32);
        assert!(!b.reserve_discarding(usize::MAX));
        assert!(b.capacity() == 0 && b.into_vec().as_ptr().is_null());
        let mut b = FastBuffer::default();
        assert!(b.reserve_discarding(0) && b.capacity() == 0);
        assert!(!b.reserve_discarding(c_uint::MAX as usize + 1));
    }

    #[test]
    fn adopt_realloc_owns_the_block() {
        // SAFETY: `av_realloc(NULL, n)` allocates and has no preconditions.
        let p = unsafe { ffi::av_realloc(core::ptr::null_mut(), 12) };
        let p = NonNull::new(p).unwrap();
        // SAFETY: `p` is a fresh, uniquely-owned 12-byte `realloc` block,
        // aligned for `u32`.
        let mut a = unsafe { adopt_realloc::<u32>(p, 3) };
        assert_eq!(
            (a.len(), a.as_ptr().cast(), a.byte_len()),
            (3, p.as_ptr(), 12)
        );
        a.as_mut_slice().fill(MaybeUninit::new(7));
        // Dropping `a` frees the block through `av_free`.
    }

    #[test]
    fn resized_arrays_keep_their_prefix() {
        let a = alloc_array_from_fn(4, |i| i as u16).unwrap();
        let a = resize_array_with(a, 6, |i| 100 + i as u16).unwrap();
        assert_eq!(a.as_slice(), &[0, 1, 2, 3, 104, 105]);
        let a = resize_array_with(a, 2, |_| unreachable!()).unwrap();
        assert_eq!(a.as_slice(), &[0, 1]);
        let mut b = alloc_array::<u16>(1).unwrap();
        b.as_mut_slice()[0].write(9);
        let b = realloc_array(b, 3).unwrap();
        // SAFETY: element 0 carried over from the write above.
        assert_eq!(unsafe { b.as_slice()[0].assume_init() }, 9);
    }

    #[test]
    fn failed_resizes_hand_the_array_back_or_free_it() {
        let a = alloc_array_from_fn(2, |i| i as u64).unwrap();
        let p = a.as_ptr();
        let a = resize_array_with(a, usize::MAX, |_| 0).unwrap_err();
        assert_eq!((a.as_ptr(), a.as_slice()), (p, &[0u64, 1][..]));
        let b = realloc_array(alloc_array::<u64>(2).unwrap(), usize::MAX / 4).unwrap_err();
        assert_eq!(b.len(), 2);
        let c = realloc_bytes(alloc_array::<u8>(2).unwrap(), i32::MAX as usize + 1).unwrap_err();
        assert_eq!(c.len(), 2);
        assert!(realloc_array_or_free(b, usize::MAX).is_none());
    }

    #[test]
    fn in_place_resizes_empty_the_slot_on_zero_or_failure() {
        let mut slot = HeapVec::<MaybeUninit<u8>>::empty();
        realloc_bytes_in_place(&mut slot, 5).unwrap();
        assert_eq!(slot.len(), 5);
        realloc_bytes_in_place(&mut slot, 0).unwrap();
        assert!(slot.is_empty() && slot.as_ptr().is_null());
        realloc_bytes_in_place(&mut slot, 3).unwrap();
        assert_eq!(realloc_bytes_in_place(&mut slot, usize::MAX), Err(-12));
        assert!(slot.is_empty() && slot.as_ptr().is_null());

        let mut slot = HeapVec::<MaybeUninit<u32>>::empty();
        realloc_array_in_place(&mut slot, 0).unwrap();
        assert!(slot.is_empty() && !slot.as_ptr().is_null());
        realloc_array_in_place(&mut slot, 9).unwrap();
        assert_eq!(slot.len(), 9);
        assert_eq!(realloc_array_in_place(&mut slot, usize::MAX / 2), Err(-12));
        assert!(slot.is_empty() && slot.as_ptr().is_null());
    }

    #[test]
    fn size_mult_is_checked_mul() {
        for (a, b) in [
            (0, usize::MAX),
            (3, 7),
            (usize::MAX, 1),
            (usize::MAX, 2),
            (1 << 32, 1 << 32),
        ] {
            assert_eq!(checked_size_mul(a, b), a.checked_mul(b), "{a} * {b}");
        }
    }
}
