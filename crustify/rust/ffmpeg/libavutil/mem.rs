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

use core::mem::{MaybeUninit, align_of, size_of};
use core::ptr::NonNull;

use ffibox::{
    CAlloc, CAllocZeroed, CBox, CCell, CDrop, CDupClone, CPlainElem, CVec, CZeroable,
    impl_clenclone, impl_clendrop,
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
}
