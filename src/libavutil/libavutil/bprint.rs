//! Safe wrappers over `libavutil/bprint.h`.
//!
//! [`PaddedStruct`] describes the structures `FF_PAD_STRUCTURE` defines, of
//! which `AVBPrint` is the one instance.
//!
//! [`BPrintFields`] is the generator's `struct ff_pad_helper_AVBPrint`:
//! `AVBPrint`'s fields without the padding, which C only measures with
//! `sizeof`. It is a plain value Rust owns; no C routine allocates, reads,
//! writes or frees one.
//!
//! [`BPrint`] is `AVBPrint` itself, a print buffer. C initialises one
//! in place and its string may live inside the structure, so Rust keeps it
//! behind an `av_malloc` block, never by value, under one of two owners:
//!
//! - [`BPrintBox`], from [`BPrint::new`] (`av_bprint_init`): the string lives in the structure or
//!   in an `av_realloc` block it owns; dropping it runs `av_bprint_finalize(buf, NULL)`, and
//!   [`BPrint::into_string`] / [`BPrint::into_bytes`] hand the string out instead;
//! - [`BPrintOnBuffer`], from [`BPrint::for_buffer`] (`av_bprint_init_for_buffer`): the string
//!   lives in a caller buffer the owner borrows, which C never reallocates or frees, so it is never
//!   finalized.
//!
//! Both hand out the same [`BPrintRef`] and [`BPrintMut`] handles. The
//! appenders are [`BPrintMut`]'s methods, and [`fmt::Write`] on it; they
//! never fail, and [`BPrintRef::is_complete`] reports whether everything
//! appended was stored.
//!
//! [`BPrintMut::append_strftime`] formats a [`Tm`], libc's `struct tm` as
//! `crustify-rs/glibc-bindings` wraps it.
//!
//! [`Tm`]: glibc_bindings::libc::time::bits::types::struct_tm_h::Tm

use core::ffi::{CStr, c_char, c_uint};
use core::fmt;
use core::marker::PhantomData;
use core::mem::{align_of, offset_of, size_of};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{CBox, CCell, CDrop, CMutSeam, CRefSeam, CZeroable, define_ctype, impl_cdrop};
use glibc_bindings::libc::time::bits::types::struct_tm_h::TmRef;

use super::avstring::{EscapeFlags, EscapeMode, text_until_nul};
use super::ffi;
use super::mem::{Heap, HeapBox, HeapString, HeapVec, string_from_c_str};

mod sealed {
    pub trait Sealed {}
}

/// Wraps: `FF_PAD_STRUCTURE`
///
/// A structure `FF_PAD_STRUCTURE(name, size, fields...)` defines: `fields`,
/// then `char reserved_padding[]` up to exactly `size` bytes, so that later
/// FFmpeg versions add fields into the padding without changing the
/// structure's size. The generator also defines
/// `struct ff_pad_helper_<name> { fields... }`, which only sizes the padding.
///
/// The layout is the instance's own binding: a generic `{ fields, padding }`
/// would start the padding after the fields' trailing alignment, not where C
/// puts it, and C may use those bytes (`AVBPrint`'s internal buffer runs from
/// `reserved_internal_buffer` through the padding to the end).
///
/// "The end" is byte `SIZE - 1`, not the last element of `reserved_padding`:
/// `offset_of!(reserved_padding) + PADDING_LEN` is only rounded up to `SIZE`
/// by the structure's trailing alignment, and C writes into those bytes too
/// (`AVBPrint`'s automatic buffer puts its NUL at byte 1023, past
/// `reserved_padding`'s end at 1020). Rust treats them as padding, which a
/// typed copy (a move, `ptr::read`) need not preserve, so an instance C has
/// initialised must stay in place and be reached through its address; the
/// pointer `AVBPrint.str` keeps into the structure forbids moving it anyway.
///
/// Sealed: implemented for each instance the generator defines, with the
/// constants checked against the instance's binding at compile time. Outside
/// the crate, a type cannot claim them:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::bprint::{BPrintFields, PaddedStruct};
/// struct Unpadded([u8; 8]);
/// impl PaddedStruct for Unpadded {
///     const SIZE: usize = 4;
///     type Fields = BPrintFields;
/// }
/// ```
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::bprint::PaddedStruct;
/// use ffmpeg_bindings::libavutil::libavutil::ffi;
/// assert_eq!(<ffi::AVBPrint as PaddedStruct>::SIZE, 1024);
/// ```
pub trait PaddedStruct: sealed::Sealed + Sized {
    /// `size`: the structure's byte size, which the padding fixes.
    const SIZE: usize;

    /// `struct ff_pad_helper_<name>`: the fields alone, as a value.
    type Fields: CCell;

    /// `sizeof(struct ff_pad_helper_<name>)`: the size of the fields alone,
    /// trailing alignment included.
    const FIELDS_SIZE: usize = size_of::<Self::Fields>();

    /// The length of `reserved_padding`.
    const PADDING_LEN: usize = Self::SIZE - Self::FIELDS_SIZE;
}

impl sealed::Sealed for ffi::AVBPrint {}

/// `FF_PAD_STRUCTURE(AVBPrint, 1024, ...)`.
impl PaddedStruct for ffi::AVBPrint {
    type Fields = BPrintFields;

    const SIZE: usize = 1024;
}

// The generator's layout, which `PaddedStruct`'s constants describe: exactly
// `SIZE` bytes, aligned as the fields, ending in `PADDING_LEN` reserved bytes
// that only the structure's trailing alignment follows.
const _: () = {
    type T = ffi::AVBPrint;
    assert!(size_of::<T>() == <T as PaddedStruct>::SIZE);
    assert!(align_of::<T>() == align_of::<ffi::ff_pad_helper_AVBPrint>());
    assert!(<T as PaddedStruct>::FIELDS_SIZE <= <T as PaddedStruct>::SIZE);
    let end = offset_of!(T, reserved_padding) + <T as PaddedStruct>::PADDING_LEN;
    assert!(end.next_multiple_of(align_of::<T>()) == <T as PaddedStruct>::SIZE);
};

define_ctype!(
    /// Wraps: ff_pad_helper_AVBPrint
    ///
    /// `AVBPrint`'s fields alone, without the padding: the structure
    /// `FF_PAD_STRUCTURE` defines only so that `sizeof` measures them, and
    /// [`PaddedStruct::Fields`] for `AVBPrint`. No C routine takes, returns,
    /// stores or frees one, so it is a plain value Rust owns, built empty by
    /// [`BPrintFields::new`] (or `Default`, or `zeroed()`) and filled through
    /// [`BPrintFieldsMut`]'s setters; its getters are on [`BPrintFieldsRef`].
    ///
    /// The fields are independent: nothing reads them as a print buffer, so
    /// no relation between `str`, `len`, `size` and `size_max` is kept or
    /// checked. `str` is the one field with an invariant: NULL, or a
    /// NUL-terminated string the value borrows (see
    /// [`BPrintFieldsRef::str`]), which it never frees.
    #[derive(Clone, Copy)]
    BPrintFields,
    BPrintFieldsRef,
    BPrintFieldsMut,
    ffi::ff_pad_helper_AVBPrint
);

// SAFETY: all-zero is a NULL `str`, zero sizes and a NUL internal byte: every
// getter accepts it (`str` reads NULL as `None`), and the type has no teardown
// or safe wrapper that could reject it.
unsafe impl CZeroable for BPrintFields {}

// SAFETY: the fields are three `unsigned`s, one `char` and `str`, which is
// NULL or addresses a string no path writes while the value can read it: the
// safe setter stores a `&'static CStr` (`Sync`), and `set_str_borrowed`'s
// contract keeps its string valid and unwritten for as long as any copy can
// read it. Nothing is freed or tied to the thread that stored it, so a value,
// or an exclusive handle to one, moves between threads as a
// `(Option<&CStr>, [u32; 3], u8)` does.
unsafe impl Send for BPrintFields {}
// SAFETY: as for `BPrintFields`: the handle is the only access to the value.
unsafe impl Send for BPrintFieldsMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `BPrintFieldsRef`
// has getters only, the struct has no interior mutability, and the string
// `str` addresses is unwritten while readable (as for `Send`), so concurrent
// shared access reads unchanging bytes.
unsafe impl Sync for BPrintFields {}
// SAFETY: as for `Sync for BPrintFields`: copies of a shared handle only read.
unsafe impl Send for BPrintFieldsRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for BPrintFieldsRef<'_> {}
// SAFETY: `&BPrintFieldsMut` reaches the value only through `as_ref`, a
// shared handle; the setters need `&mut`.
unsafe impl Sync for BPrintFieldsMut<'_> {}

impl BPrintFields {
    /// An empty value: no string, every size 0 and a NUL internal byte, as
    /// `zeroed()` builds it.
    #[must_use]
    pub fn new() -> Self {
        Self::zeroed()
    }
}

impl Default for BPrintFields {
    /// [`BPrintFields::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BPrintFields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for BPrintFieldsRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BPrintFields")
            .field("str", &self.str())
            .field("len", &self.len())
            .field("size", &self.size())
            .field("size_max", &self.size_max())
            .field("internal_buffer", &self.internal_buffer())
            .finish()
    }
}

impl<'a> BPrintFieldsRef<'a> {
    /// Field: `ff_pad_helper_AVBPrint.str`
    ///
    /// The string the value borrows, or `None` when `str` is NULL (as in a
    /// zeroed value).
    #[must_use]
    pub fn str(&self) -> Option<&'a CStr> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `str_` field is in bounds; nothing is accessed here.
        let str_ptr = unsafe { addr_of!((*self.as_ptr()).str_) };
        // SAFETY: a read of the `str` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`.
        let str = unsafe { str_ptr.read() };
        // SAFETY: every writer of `str` leaves it NULL (`zeroed`) or the
        // address of a NUL-terminated string that stays valid and unwritten
        // while this value or a copy can read it: a `&'static CStr`
        // (`set_str`), or one `set_str_borrowed`'s caller keeps alive. The
        // handle borrows the value for `'a`, so the string outlives `'a`.
        NonNull::new(str).map(|p| unsafe { CStr::from_ptr(p.as_ptr()) })
    }

    /// Field: `ff_pad_helper_AVBPrint.len`
    ///
    /// The `len` field: in an `AVBPrint`, the length of everything printed,
    /// which may exceed `size`.
    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "a stored count, not a container"
    )]
    pub fn len(&self) -> u32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `len` field is in bounds; nothing is accessed here.
        let len_ptr = unsafe { addr_of!((*self.as_ptr()).len) };
        // SAFETY: a read of the `len` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`;
        // every bit pattern is a valid `unsigned`.
        unsafe { len_ptr.read() }
    }

    /// Field: `ff_pad_helper_AVBPrint.size`
    ///
    /// The `size` field: in an `AVBPrint`, the bytes of storage `str` has.
    #[must_use]
    pub fn size(&self) -> u32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { addr_of!((*self.as_ptr()).size) };
        // SAFETY: a read of the `size` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`;
        // every bit pattern is a valid `unsigned`.
        unsafe { size_ptr.read() }
    }

    /// Field: `ff_pad_helper_AVBPrint.size_max`
    ///
    /// The `size_max` field: in an `AVBPrint`, the most storage `str` may
    /// grow to.
    #[must_use]
    pub fn size_max(&self) -> u32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `size_max` field is in bounds; nothing is accessed here.
        let size_max_ptr = unsafe { addr_of!((*self.as_ptr()).size_max) };
        // SAFETY: a read of the `size_max` field projected through the
        // shared handle's pointer to a live, initialised
        // `ff_pad_helper_AVBPrint`; every bit pattern is a valid `unsigned`.
        unsafe { size_max_ptr.read() }
    }

    /// Field: `ff_pad_helper_AVBPrint.reserved_internal_buffer`
    ///
    /// The one byte of `reserved_internal_buffer`: in an `AVBPrint`, the
    /// start of the storage that runs on through the padding.
    #[must_use]
    pub fn internal_buffer(&self) -> [u8; 1] {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `reserved_internal_buffer` field is in bounds; nothing is accessed
        // here.
        let reserved_internal_buffer_ptr =
            unsafe { addr_of!((*self.as_ptr()).reserved_internal_buffer) };
        // SAFETY: a read of the `reserved_internal_buffer` field projected
        // through the shared handle's pointer to a live, initialised
        // `ff_pad_helper_AVBPrint`; every bit pattern is a valid `char`.
        let [b] = unsafe { reserved_internal_buffer_ptr.read() };
        [b.cast_unsigned()]
    }
}

impl BPrintFieldsMut<'_> {
    /// Field: `ff_pad_helper_AVBPrint.str`
    ///
    /// Points `str` at `s`, or NULL for `None`. The value only borrows the
    /// string, which outlives it; the old one is not freed.
    pub fn set_str(&mut self, s: Option<&'static CStr>) {
        // SAFETY: `s` lives for the whole program and `CStr` is never
        // written, which is all `set_str_borrowed` asks.
        unsafe { self.set_str_borrowed(s) }
    }

    /// Field: `ff_pad_helper_AVBPrint.str`
    ///
    /// Points `str` at `s`, or NULL for `None`, with a lifetime the value
    /// cannot carry. The value only borrows the string; the old one is not
    /// freed.
    ///
    /// # Safety
    ///
    /// `s` must stay valid and unwritten until `str` can no longer return
    /// it: until this field is overwritten, or the value dropped, in this
    /// value and in every copy made of it after this call, on every thread.
    pub unsafe fn set_str_borrowed(&mut self, s: Option<&CStr>) {
        // `str` is `char *` in C, but nothing writes through it: `str()` only
        // reads.
        let str = s.map_or(ptr::null_mut(), |s| s.as_ptr().cast_mut());
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `str_` field is in bounds; nothing is accessed here.
        let str_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).str_) };
        // SAFETY: a write of the `str` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // is a borrow, so nothing leaks, and the caller keeps the new string
        // alive for as long as `str()` can return it.
        unsafe { str_ptr.write(str) }
    }

    /// Field: `ff_pad_helper_AVBPrint.len`
    ///
    /// Sets `len`, leaving the other fields as they are.
    pub fn set_len(&mut self, len: u32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `len` field is in bounds; nothing is accessed here.
        let len_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).len) };
        // SAFETY: a write of the `len` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // owns nothing to leak.
        unsafe { len_ptr.write(len) }
    }

    /// Field: `ff_pad_helper_AVBPrint.size`
    ///
    /// Sets `size`, leaving the other fields as they are.
    pub fn set_size(&mut self, size: u32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).size) };
        // SAFETY: a write of the `size` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // owns nothing to leak.
        unsafe { size_ptr.write(size) }
    }

    /// Field: `ff_pad_helper_AVBPrint.size_max`
    ///
    /// Sets `size_max`, leaving the other fields as they are.
    pub fn set_size_max(&mut self, size_max: u32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `size_max` field is in bounds; nothing is accessed here.
        let size_max_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).size_max) };
        // SAFETY: a write of the `size_max` field projected through the
        // exclusive handle's pointer to a live `ff_pad_helper_AVBPrint`; the
        // old value owns nothing to leak.
        unsafe { size_max_ptr.write(size_max) }
    }

    /// Field: `ff_pad_helper_AVBPrint.reserved_internal_buffer`
    ///
    /// Sets the one byte of `reserved_internal_buffer`.
    pub fn set_internal_buffer(&mut self, buffer: [u8; 1]) {
        let [b] = buffer;
        // SAFETY: the handle's pointer is to a live object, as the block below relies on, so the
        // place of its `reserved_internal_buffer` field is in bounds; nothing is accessed here.
        let reserved_internal_buffer_ptr =
            unsafe { &raw mut (*self.as_mut_ptr()).reserved_internal_buffer };
        // SAFETY: a write of the `reserved_internal_buffer` field projected
        // through the exclusive handle's pointer to a live
        // `ff_pad_helper_AVBPrint`; the old byte owns nothing to leak.
        unsafe {
            reserved_internal_buffer_ptr.write([b.cast_signed()]);
        }
    }
}

define_ctype!(
    /// Wraps: AVBPrint
    ///
    /// A print buffer: a string that grows as data is appended, always
    /// NUL-terminated, which may hold binary data. It counts the total length
    /// of everything appended ([`BPrintRef::len`]) even past what it could
    /// store; the stored part is [`BPrintRef::as_bytes`].
    ///
    /// Owned as a [`BPrintBox`] or a [`BPrintOnBuffer`], never by value: C
    /// points the string into the structure itself (see [`PaddedStruct`]), so
    /// an initialised buffer must not move.
    ///
    /// Every handle addresses a buffer one of the two initialisers set up, of
    /// which the safe API keeps these invariants: the string is valid for
    /// `size` bytes and NUL-terminated at `min(len, size - 1)` when `size` is
    /// non-zero, its first `min(len, size - 1)` bytes written; and a string
    /// that is neither the structure's own storage nor an `av_realloc` block
    /// (a [`BPrintOnBuffer`]'s) has `size == size_max`, so C never grows it.
    /// Only C's appenders write `str` and `size`, moving a string the buffer
    /// owns to a larger allocation; nothing writes `size_max`.
    BPrint,
    BPrintRef,
    BPrintMut,
    ffi::AVBPrint
);

// SAFETY: a buffer owns its string (the structure's own storage or an
// `av_realloc` block, released by `av_free` from any thread) or, behind a
// `BPrintOnBuffer`, borrows a caller's `&mut [u8]`, whose lifetime the owner
// carries; nothing in it is tied to a thread, so the sole owner moves as a
// `Box<Vec<u8>>` does.
unsafe impl Send for BPrint {}
// SAFETY: nothing reachable through a shared handle writes: `BPrintRef` has
// getters only and the structure has no interior mutability, so concurrent
// shared access reads unchanging bytes.
unsafe impl Sync for BPrint {}
// SAFETY: as `&BPrint`, which is `Send` because `BPrint` is `Sync`.
unsafe impl Send for BPrintRef<'_> {}
// SAFETY: as above: copies of a shared handle only read.
unsafe impl Sync for BPrintRef<'_> {}
// SAFETY: as `&mut BPrint`, which is `Send` because `BPrint` is `Send`.
unsafe impl Send for BPrintMut<'_> {}
// SAFETY: `&BPrintMut` reaches the buffer only through `as_ref`, a shared
// handle.
unsafe impl Sync for BPrintMut<'_> {}

/// Wraps: `av_bprint_finalize`
///
/// # Safety
///
/// `buf` must be an initialised buffer the caller owns, in an `av_malloc`
/// block, whose string is the structure's own storage or an `av_realloc`
/// block it owns (one `av_bprint_init` set up).
unsafe fn finalize_and_free(buf: *mut ffi::AVBPrint) {
    // SAFETY: per the caller, `buf` is an initialised buffer whose string, if
    // not inline, is its own `av_realloc` block, which `av_bprint_finalize`
    // frees with a NULL `ret_str`, leaving the structure in place.
    unsafe { ffi::av_bprint_finalize(buf, ptr::null_mut()) };
    // SAFETY: the structure's storage is a live `av_malloc` block the caller
    // owns, released once; nothing points into it any more.
    unsafe { ffi::av_free(buf.cast()) };
}

/// `av_bprint_finalize(buf, NULL)` and `av_free`, the policy of a
/// [`BPrintBox`]: the string's allocation, if any, then the structure's.
///
/// Thread-agnostic: both release through `av_free`, which any thread may
/// call, so a [`BPrintBox`] is `Send` and `Sync`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BPrintFinalize;

impl_cdrop!(BPrintFinalize, BPrint, finalize_and_free);

/// An owned print buffer from [`BPrint::new`], finalized on drop.
///
/// Not `Clone`: C has no routine that copies a buffer.
///
/// ```compile_fail,E0599
/// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrintBox;
/// fn copy(b: &BPrintBox) -> BPrintBox {
///     (*b).clone()
/// }
/// ```
pub type BPrintBox = CBox<BPrint, BPrintFinalize>;

/// The policy of a [`BPrintOnBuffer`]: `av_free` of the structure's storage
/// only, borrowing the caller buffer the string lives in for `'b`.
///
/// It never finalizes: `av_bprint_finalize` takes a string that is not the
/// structure's own storage for an `av_realloc` block, and would free the
/// caller's buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BufferStorage<'b>(PhantomData<&'b mut [u8]>);

/// Wraps: `av_free`
///
/// # Safety
///
/// `buf` must be the structure of a buffer `av_bprint_init_for_buffer` set
/// up, in an `av_malloc` block the caller owns. Its string is the caller's
/// buffer (or, for an empty one, the structure's own storage), which needs no
/// release.
unsafe fn free_storage(buf: *mut ffi::AVBPrint) {
    // SAFETY: per the caller, `buf` is a live `av_malloc` block it owns;
    // `av_free` releases it once, without reading it.
    unsafe { ffi::av_free(buf.cast()) }
}

impl_cdrop!(BufferStorage<'_>, BPrint, free_storage);

/// A print buffer writing into a caller buffer, from [`BPrint::for_buffer`].
///
/// It borrows the buffer exclusively until it drops, after which the buffer
/// holds what was printed, NUL-terminated:
///
/// ```compile_fail,E0503
/// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
/// let mut out = [0u8; 16];
/// let buf = BPrint::for_buffer(&mut out).unwrap();
/// let first = out[0];
/// drop(buf);
/// # let _ = first;
/// ```
pub type BPrintOnBuffer<'b> = CBox<BPrint, BufferStorage<'b>>;

impl BPrint {
    /// The bytes of string, NUL included, the structure holds itself: from
    /// `reserved_internal_buffer` to its end, as `av_bprint_init` sizes its
    /// automatic storage.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the structure holds a few hundred bytes of inline storage"
    )]
    pub const INLINE_CAPACITY: u32 =
        (size_of::<ffi::AVBPrint>() - offset_of!(ffi::AVBPrint, reserved_internal_buffer)) as u32;
    /// `AV_BPRINT_SIZE_AUTOMATIC` as a `size_max`: replaced by
    /// [`INLINE_CAPACITY`](Self::INLINE_CAPACITY), so the string stays in the
    /// structure and nothing is allocated for it.
    pub const SIZE_AUTOMATIC: u32 = ffi::AV_BPRINT_SIZE_AUTOMATIC;
    /// `AV_BPRINT_SIZE_COUNT_ONLY` as a `size_max`: store nothing, only count
    /// the length.
    pub const SIZE_COUNT_ONLY: u32 = ffi::AV_BPRINT_SIZE_COUNT_ONLY;
    /// `AV_BPRINT_SIZE_UNLIMITED` as a `size_max`: grow without limit (up to
    /// `u32::MAX` bytes). The macro is `((unsigned)-1)`, a cast bindgen does
    /// not evaluate.
    pub const SIZE_UNLIMITED: u32 = u32::MAX;

    /// Storage for a buffer, which `fill` initialises in place, then adopted
    /// under `policy`; `None` when the storage cannot be allocated.
    ///
    /// # Safety
    ///
    /// `fill` must initialise the buffer at the pointer it is given so that
    /// it meets [`BPrint`]'s invariants and `policy` releases it.
    unsafe fn init_with<P: CDrop<BPrint>>(
        policy: P,
        fill: impl FnOnce(*mut ffi::AVBPrint),
    ) -> Option<CBox<BPrint, P>> {
        let mut storage = HeapBox::<BPrint>::new_uninit()?;
        fill(storage.as_c_ptr());
        // SAFETY: per the caller, `fill` initialised the buffer in place.
        let storage = unsafe { storage.assume_init() };
        // SAFETY: per the caller, `policy` releases the buffer `fill` set up,
        // storage included.
        let (buf, Heap) = unsafe { storage.with_policy(policy) };
        Some(buf)
    }

    /// Wraps: `av_bprint_init`
    ///
    /// An empty buffer that may grow to `size_max` bytes, NUL included, with
    /// room for `size_init` bytes up front. The string starts inside the
    /// structure ([`INLINE_CAPACITY`](Self::INLINE_CAPACITY) bytes) and moves
    /// to an allocation when it outgrows it. `size_max` takes
    /// [`SIZE_UNLIMITED`](Self::SIZE_UNLIMITED),
    /// [`SIZE_AUTOMATIC`](Self::SIZE_AUTOMATIC),
    /// [`SIZE_COUNT_ONLY`](Self::SIZE_COUNT_ONLY) or any limit; a `size_init`
    /// that cannot be allocated leaves the buffer inline, as C does.
    ///
    /// `None` only when the structure itself cannot be allocated.
    #[must_use]
    pub fn new(size_init: u32, size_max: u32) -> Option<BPrintBox> {
        let fill = |buf| {
            // SAFETY: `buf` is writable storage for one `AVBPrint`.
            unsafe { ffi::av_bprint_init(buf, size_init, size_max) }
        };
        // SAFETY: `av_bprint_init` initialises the `AVBPrint` storage in
        // place without reading it, with an inline or `av_realloc` string,
        // which `BPrintFinalize` releases with the storage.
        unsafe { Self::init_with(BPrintFinalize, fill) }
    }

    /// Wraps: `av_bprint_init_for_buffer`
    ///
    /// An empty buffer printing into `buffer`, never beyond it: appends past
    /// `buffer.len() - 1` bytes are counted but truncated. C writes the NUL
    /// terminator at once, so `buffer[0]` is 0 afterwards. A `buffer` longer
    /// than `u32::MAX` bytes is used up to that many; an empty one counts
    /// only, as [`SIZE_COUNT_ONLY`](Self::SIZE_COUNT_ONLY).
    ///
    /// `None` only when the structure cannot be allocated.
    pub fn for_buffer(buffer: &mut [u8]) -> Option<BPrintOnBuffer<'_>> {
        let size = c_uint::try_from(buffer.len()).unwrap_or(c_uint::MAX);
        let data = buffer.as_mut_ptr().cast::<c_char>();
        let fill = |buf| {
            // SAFETY: `buf` is writable storage for one `AVBPrint`; `data` is
            // valid for `size` bytes of writes.
            unsafe { ffi::av_bprint_init_for_buffer(buf, data, size) }
        };
        // SAFETY: `av_bprint_init_for_buffer` initialises the storage in
        // place with `size == size_max`, its string `buffer`'s first `size`
        // bytes, writable and borrowed for `'b` by the owner's policy (or the
        // inline storage when `size` is 0, which C then reads nothing of);
        // `BufferStorage` frees only the storage.
        unsafe { Self::init_with(BufferStorage(PhantomData), fill) }
    }

    /// `av_bprint_finalize(buf, &str)`: the string, `(str, stored)` with
    /// `stored` bytes before its NUL, after which `buf`'s storage is freed.
    /// `None` for the string of a count-only buffer (`size` 0), which C
    /// hands back as a block holding no string, freed here.
    fn finalize(buf: BPrintBox) -> Result<Option<(*mut c_char, usize)>, i32> {
        let p = buf.into_c();
        let mut str = ptr::null_mut();
        // SAFETY: `p` is an initialised buffer `av_bprint_init` set up, which
        // we own; `str` is a writable slot. C hands the string out: its
        // `av_realloc` block resized to `min(len + 1, size)`, or an
        // `av_memdup` of the inline one, NULL on failure; and sets `size` to
        // that length.
        let ret = unsafe { ffi::av_bprint_finalize(p, &raw mut str) };
        // SAFETY: `p` is to a live object, as the access below relies on,
        // so the place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { addr_of!((*p).size) };
        // SAFETY: a read of a projected field of the live structure.
        let real_size = unsafe { size_ptr.read() };
        // SAFETY: the structure's storage is ours, released once; the string
        // no longer points into it.
        unsafe { ffi::av_free(p.cast()) };
        if ret < 0 {
            return Err(ret);
        }
        if real_size == 0 {
            // SAFETY: `str` is NULL or C's `av_malloc` block, ours to free.
            unsafe { ffi::av_free(str.cast()) };
            return Ok(None);
        }
        // `real_size` is at least 1: the bytes before the NUL.
        Ok(Some((str, real_size as usize - 1)))
    }

    /// Wraps: `av_bprint_finalize`
    ///
    /// The stored string of `buf` as an owned C string, consuming the buffer
    /// without copying an allocated string. It ends at its first NUL, as C
    /// reads it, which binary content may place early; see
    /// [`into_bytes`](Self::into_bytes). Fails with `AVERROR(ENOMEM)` when
    /// an inline string cannot be copied out.
    ///
    /// A count-only buffer yields an empty string, where C hands back a
    /// block that holds no string.
    ///
    /// # Errors
    ///
    /// Fails with `AVERROR(ENOMEM)` when an inline string cannot be copied out.
    pub fn into_string(buf: BPrintBox) -> Result<HeapString, i32> {
        let enomem = -(ffi::ENOMEM.cast_signed());
        match Self::finalize(buf)? {
            // SAFETY: on success `str` is a fresh `av_realloc` or `av_memdup`
            // block the caller owns, NUL-terminated at `stored` (C's invariant
            // `str[min(len, size - 1)] == 0`, kept by the resize to
            // `min(len + 1, size)` bytes), which `av_free` releases.
            Some((str, _)) => unsafe { HeapString::from_raw(str) }.ok_or(enomem),
            None => string_from_c_str(c"").ok_or(enomem),
        }
    }

    /// Wraps: `av_bprint_finalize`
    ///
    /// The stored bytes of `buf`, [`BPrintRef::as_bytes`] as an owned array,
    /// consuming the buffer without copying an allocated string; the NUL
    /// terminator follows them in the allocation. Fails with
    /// `AVERROR(ENOMEM)` when an inline string cannot be copied out.
    ///
    /// # Errors
    ///
    /// Fails with `AVERROR(ENOMEM)` when an inline string cannot be copied out.
    pub fn into_bytes(buf: BPrintBox) -> Result<HeapVec<u8>, i32> {
        let enomem = -(ffi::ENOMEM.cast_signed());
        match Self::finalize(buf)? {
            Some((str, stored)) => {
                // SAFETY: `str` is a fresh `av_malloc`-family block the
                // caller owns, released by `av_free`, whose first `stored`
                // bytes C wrote (the buffer's stored string).
                unsafe { HeapVec::from_raw_parts(str.cast(), stored) }.ok_or(enomem)
            }
            None => Ok(HeapVec::empty()),
        }
    }
}

impl<'a> BPrintRef<'a> {
    /// `(str, len, size)`, read by value.
    fn state(&self) -> (*mut c_char, u32, u32) {
        let p = self.as_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `len` field is in bounds; nothing is accessed here.
        let len_ptr = unsafe { &raw const (*p).len };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { &raw const (*p).size };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `str_` field is in bounds; nothing is accessed here.
        let str_ptr = unsafe { &raw const (*p).str_ };
        // SAFETY: reads of projected fields through the shared handle.
        (
            unsafe { str_ptr.read() },
            unsafe { len_ptr.read() },
            unsafe { size_ptr.read() },
        )
    }

    /// Field: AVBPrint.str
    ///
    /// The stored string: the first `min(len(), size() - 1)` bytes appended,
    /// NULs included, without the terminator. Empty for a count-only buffer.
    #[must_use]
    pub fn as_bytes(&self) -> &'a [u8] {
        let (str, len, size) = self.state();
        let stored = len.min(size.saturating_sub(1)) as usize;
        if stored == 0 {
            return &[];
        }
        // SAFETY: by `BPrint`'s invariants, `str` is valid for `size` bytes,
        // of which C wrote the first `stored`; they live in the structure, an
        // allocation it owns or the borrowed caller buffer, all alive and
        // unwritten while this shared handle lives (`'a`).
        unsafe { core::slice::from_raw_parts(str.cast::<u8>(), stored) }
    }

    /// Field: AVBPrint.str
    ///
    /// The stored string as C reads it: up to its first NUL, which binary
    /// content may place before [`as_bytes`](Self::as_bytes) ends. Empty for
    /// a count-only buffer.
    #[must_use]
    pub fn as_c_str(&self) -> &'a CStr {
        let (str, len, size) = self.state();
        if size == 0 {
            return c"";
        }
        let terminated = len.min(size - 1) as usize + 1;
        // SAFETY: as for `as_bytes`, the string's first `terminated` bytes
        // are written, the last one being the NUL terminator.
        let bytes = unsafe { core::slice::from_raw_parts(str.cast::<u8>(), terminated) };
        CStr::from_bytes_until_nul(bytes).unwrap_or_default()
    }

    /// Field: AVBPrint.len
    ///
    /// The total length of everything appended, which exceeds what is
    /// stored once the buffer is truncated (at least [`size`](Self::size)).
    #[must_use]
    pub fn len(&self) -> u32 {
        self.state().1
    }

    /// Whether nothing has been appended since the buffer was initialised or
    /// last truncated to empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Field: AVBPrint.size
    ///
    /// The bytes the string has room for, NUL included: 0 for a count-only
    /// buffer.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.state().2
    }

    /// Field: `AVBPrint.size_max`
    ///
    /// The most [`size`](Self::size) may grow to: [`BPrint::SIZE_AUTOMATIC`]
    /// replaced by [`BPrint::INLINE_CAPACITY`], and `size()` itself for a
    /// [`BPrintOnBuffer`].
    #[must_use]
    pub fn size_max(&self) -> u32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `size_max` field is in bounds; nothing is accessed here.
        let size_max_ptr = unsafe { addr_of!((*self.as_ptr()).size_max) };
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { size_max_ptr.read() }
    }

    /// Field: `AVBPrint.reserved_internal_buffer`
    /// Field: `AVBPrint.reserved_padding`
    ///
    /// Whether the string is kept in the structure itself: its inline
    /// storage starts at `reserved_internal_buffer` and runs through
    /// `reserved_padding` to the end, [`BPrint::INLINE_CAPACITY`] bytes.
    /// `false` once it has moved to an allocation, and for a
    /// [`BPrintOnBuffer`] printing into a non-empty buffer.
    #[must_use]
    pub fn is_inline(&self) -> bool {
        let p = self.as_ptr();
        // SAFETY: only the field's address is taken, through the shared
        // handle's pointer.
        let inline = unsafe { addr_of!((*p).reserved_internal_buffer) };
        ptr::eq(self.state().0.cast_const(), inline.cast())
    }
}

impl BPrintMut<'_> {
    /// Field: AVBPrint.len
    ///
    /// Shortens the buffer to `len` bytes, as C callers do by lowering `len`
    /// and re-terminating the string; a `len` at or above
    /// [`len()`](BPrintRef::len) leaves it unchanged. Truncating a truncated
    /// buffer below its [`size`](BPrintRef::size) makes it complete again.
    pub fn truncate(&mut self, len: u32) {
        let (str, cur, size) = self.as_ref().state();
        if len >= cur {
            return;
        }
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the access below relies on,
        // so the place of its `len` field is in bounds; nothing is accessed here.
        let len_ptr = unsafe { addr_of_mut!((*p).len) };
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { len_ptr.write(len) };
        if size > 0 {
            // SAFETY: as the comment below states, for this part of the operation.
            let str_3 = unsafe { str.add(len.min(size - 1) as usize) };
            // SAFETY: by `BPrint`'s invariants `str` is valid for `size`
            // bytes of writes, reached through the exclusive handle's buffer;
            // the NUL lands at `min(len, size - 1)`, within them, where C's
            // own appends keep it.
            unsafe { str_3.write(0) };
        }
    }
}

impl BPrintRef<'_> {
    /// Wraps: `av_bprint_is_complete`
    ///
    /// Whether everything appended is stored: `false` once an append was
    /// truncated, by the `size_max` limit or a failed allocation. Compare
    /// [`size`](Self::size) with [`size_max`](Self::size_max) to tell them
    /// apart. C tests `len < size`, so a count-only buffer (`size` 0) is
    /// never complete, not even before anything is appended, and
    /// [`clear`](BPrintMut::clear) or [`truncate`](BPrintMut::truncate)
    /// cannot make it so.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
    /// let mut buf = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
    /// assert!(!buf.as_ref().is_complete());
    /// buf.as_mut().append_bytes(b"counted");
    /// buf.as_mut().clear();
    /// assert!(!buf.as_ref().is_complete());
    /// let on_empty = BPrint::for_buffer(&mut []).unwrap();
    /// assert!(!on_empty.as_ref().is_complete());
    /// let stored = BPrint::new(0, BPrint::SIZE_AUTOMATIC).unwrap();
    /// assert!(stored.as_ref().is_complete());
    /// ```
    #[must_use]
    pub fn is_complete(&self) -> bool {
        // SAFETY: `av_bprint_is_complete` only reads `len` and `size` of the
        // initialised buffer the shared handle addresses.
        unsafe { ffi::av_bprint_is_complete(self.as_ptr()) != 0 }
    }
}

/// The most `len` may reach: `av_bprint_grow` keeps it at `UINT_MAX - 5`,
/// which [`BPrintMut::append_with`] respects too.
const LEN_MAX: u32 = u32::MAX - 5;

/// Re-terminates the buffer it borrows when dropped, at
/// `min(len, size - 1)` as `BPrint`'s invariants place the NUL: the
/// terminator [`BPrintMut::append_with`] restores, even when its closure
/// panics.
struct Terminate<'h, 'b>(&'h mut BPrintMut<'b>);

impl Drop for Terminate<'_, '_> {
    fn drop(&mut self) {
        let (str, len, size) = self.0.as_ref().state();
        if size > 0 {
            // SAFETY: as the comment below states, for this part of the operation.
            let str_2 = unsafe { str.add(len.min(size - 1) as usize) };
            // SAFETY: by `BPrint`'s invariants `str` is valid for `size`
            // bytes of writes, reached through the exclusive handle the
            // guard borrows; the NUL lands within them.
            unsafe { str_2.write(0) };
        }
    }
}

/// The appenders. Each stores what fits and counts the rest: an append that
/// does not fit grows a string the buffer owns, up to `size_max`, and is
/// truncated when it cannot, which [`BPrintRef::is_complete`] reports. They
/// never fail otherwise and never grow a [`BPrintOnBuffer`]'s string.
impl BPrintMut<'_> {
    /// Wraps: `av_bprint_append_data`
    ///
    /// Appends `data`, NULs included. Data longer than `u32::MAX` bytes goes
    /// in `u32::MAX`-byte pieces, as many calls; empty data changes nothing.
    pub fn append_bytes(&mut self, data: &[u8]) {
        let p = self.as_mut_ptr();
        for piece in data.chunks(c_uint::MAX as usize) {
            // SAFETY: `p` is the initialised buffer the exclusive handle
            // addresses, which `av_bprint_append_data` appends to within its
            // `size` (growing only a string it owns); `piece` is valid for
            // reads of its length, which fits the `unsigned` size, and is
            // not part of the buffer, which `&mut self` excludes.
            unsafe {
                ffi::av_bprint_append_data(
                    p,
                    piece.as_ptr().cast(),
                    c_uint::try_from(piece.len()).unwrap_or(c_uint::MAX),
                );
            }
        }
    }

    /// Wraps: `av_bprint_chars`
    ///
    /// Appends `n` copies of `byte`.
    pub fn append_repeated(&mut self, byte: u8, n: u32) {
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // `av_bprint_chars` appends to within its `size`.
        unsafe { ffi::av_bprint_chars(self.as_mut_ptr(), byte.cast_signed(), n) }
    }

    /// Wraps: `av_bprint_clear`
    ///
    /// Empties the buffer, keeping its storage: [`len`](BPrintRef::len)
    /// becomes 0, which also makes a truncated buffer with storage
    /// [complete](BPrintRef::is_complete) again.
    pub fn clear(&mut self) {
        // SAFETY: the exclusive handle addresses an initialised buffer;
        // `av_bprint_clear` writes `len` and, when it was non-zero, the
        // string's first byte, which is in its storage (or, for a count-only
        // buffer, in the structure's inline storage, where `str` points).
        unsafe { ffi::av_bprint_clear(self.as_mut_ptr()) }
    }

    /// Wraps: `av_bprint_escape`
    ///
    /// Appends `src` escaped by `mode`, as
    /// [`avstring::escape`](super::avstring::escape) returns it: `special`
    /// lists characters backslash mode also escapes.
    pub fn append_escaped(
        &mut self,
        src: &CStr,
        special: Option<&CStr>,
        mode: EscapeMode,
        flags: EscapeFlags,
    ) {
        let special = special.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // C appends to with `av_bprint_chars` and `av_bprintf`; `src` is a
        // NUL-terminated string and `special` NULL or one, which C only reads
        // during the call and which `&mut self` keeps apart from the buffer;
        // C accepts any mode and flags.
        unsafe {
            ffi::av_bprint_escape(
                self.as_mut_ptr(),
                src.as_ptr(),
                special,
                mode.into(),
                flags.bits(),
            );
        }
    }

    /// Wraps: `av_bprintf`
    ///
    /// Appends the text `args` formats; build `args` with [`format_args!`].
    /// Rust formats the text, since C's `printf` arguments have no safe Rust
    /// form, and C appends it through a `"%s"` format, so it ends at its
    /// first NUL. [`write!`] through [`fmt::Write`] keeps NULs instead.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
    /// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    /// buf.as_mut().append_fmt(format_args!("{}x{}", 1920, 1080));
    /// assert_eq!(buf.as_ref().as_bytes(), b"1920x1080");
    /// ```
    pub fn append_fmt(&mut self, args: fmt::Arguments<'_>) {
        let text = text_until_nul(args);
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // `av_bprintf` appends to within its `size`; `"%s"` consumes exactly
        // the one argument passed, a live NUL-terminated string apart from
        // the buffer.
        unsafe { ffi::av_bprintf(self.as_mut_ptr(), c"%s".as_ptr(), text.as_ptr()) }
    }

    /// Wraps: `av_vbprintf`
    ///
    /// Appends the string `s`, up to its NUL, through `av_vbprintf` with a
    /// `"%s"` format (its `va_list` built by the `crustify_av_vbprintf`
    /// shim).
    pub fn append_c_str(&mut self, s: &CStr) {
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // `av_vbprintf` appends to within its `size`; `"%s"` consumes exactly
        // the one argument passed, a live NUL-terminated string apart from
        // the buffer.
        unsafe { ffi::crustify_av_vbprintf(self.as_mut_ptr(), c"%s".as_ptr(), s.as_ptr()) }
    }

    /// Wraps: `av_bprint_strftime`
    ///
    /// Appends `tm` formatted by C's `strftime` under `fmt`, in the current
    /// `LC_TIME` locale (`%Z` and `%s` also read the time zone). An empty
    /// format appends nothing. Fields outside their usual ranges format as
    /// the C library chooses: the standard leaves those characters
    /// unspecified.
    ///
    /// `strftime` cannot tell output that does not fit from empty output, so
    /// C grows the string until it fits, and gives up on an empty expansion
    /// once the room exceeds 256 times the format's length. When it cannot
    /// grow, it formats into 1 KiB of its own and appends that, or else fills
    /// the rest of the string with `"[truncated strftime output]"` and
    /// `'!'`s and marks it truncated.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
    /// use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;
    /// let mut tm = Tm::new();
    /// let mut t = tm.as_mut();
    /// t.set_year(2000 - 1900);
    /// t.set_mon(11);
    /// t.set_mday(20);
    /// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    /// buf.as_mut().append_strftime(c"%Y-%m-%d", tm.as_ref());
    /// assert_eq!(buf.as_ref().as_bytes(), b"2000-12-20");
    /// ```
    pub fn append_strftime(&mut self, fmt: &CStr, tm: TmRef<'_>) {
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // `av_bprint_strftime` appends to within its `size`; `fmt` is a
        // NUL-terminated string and `tm` a live `struct tm` whose `tm_zone`
        // is NULL or a `'static` string (`Tm`'s invariant), which C and
        // `strftime` only read during the call.
        unsafe { ffi::av_bprint_strftime(self.as_mut_ptr(), fmt.as_ptr(), tm.as_ptr().cast()) }
    }

    /// Wraps: `av_bprint_get_buffer`
    ///
    /// Appends what `f` writes straight into the string's storage. C first
    /// makes room for `size` bytes past the stored text, growing a string the
    /// buffer owns; `f` then gets the free storage, at most `size` bytes and
    /// zeroed (the byte after it is kept for the terminator), and returns how
    /// many leading bytes it wrote, which are appended. Returns that count.
    ///
    /// The storage is shorter than `size`, or empty, when the string cannot
    /// grow enough (the `size_max` limit, a failed allocation, a
    /// [`BPrintOnBuffer`]) or the buffer is already truncated; `f` still
    /// runs. Unlike the other appenders, nothing beyond the storage `f` gets
    /// is counted.
    ///
    /// The zeroing is the one effect C's own callers do not have: its bytes
    /// past what `f` wrote stay zero, which a [`BPrintOnBuffer`]'s caller
    /// buffer shows afterwards, past the string.
    ///
    /// # Panics
    ///
    /// When `f` returns more than the length of the storage it got; the
    /// buffer is left as before the call.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
    /// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    /// let n = buf.as_mut().append_with(4, |spare| {
    ///     spare[..3].copy_from_slice(b"abc");
    ///     3
    /// });
    /// assert_eq!((n, buf.as_ref().as_bytes()), (3, &b"abc"[..]));
    /// ```
    #[expect(
        clippy::cast_possible_truncation,
        reason = "`written <= max < room`, which is below the buffer's `unsigned` size"
    )]
    pub fn append_with(&mut self, size: u32, f: impl FnOnce(&mut [u8]) -> usize) -> usize {
        let mut mem = ptr::null_mut();
        let mut room = 0;
        // SAFETY: the exclusive handle addresses an initialised buffer, which
        // C grows (only when it owns its string) to make room for `size`
        // bytes; `mem` and `room` are writable out-slots, which C sets to
        // `str + len` and the `size - len` bytes from there (NULL and 0 when
        // the buffer is truncated or has no storage).
        unsafe { ffi::av_bprint_get_buffer(self.as_mut_ptr(), size, &raw mut mem, &raw mut room) };
        let len = self.as_ref().len();
        // The last byte of the room stays for the NUL, and `len` stays within
        // the cap `av_bprint_grow` keeps it under.
        let max = room
            .saturating_sub(1)
            .min(size)
            .min(LEN_MAX.saturating_sub(len)) as usize;
        let Some(spare) = NonNull::new(mem.cast::<u8>()).filter(|_| max > 0) else {
            let written = f(&mut []);
            assert!(
                written == 0,
                "append_with: {written} bytes written into no storage"
            );
            return 0;
        };
        let spare = spare.as_ptr();
        // SAFETY: `spare` is `str + len`, followed by `room > max` writable
        // bytes of the string's storage, which the exclusive handle borrows
        // for the whole call; zeroing initialises the first `max`.
        unsafe { spare.write_bytes(0, max) };
        // `spare[0]` holds the NUL until `f` writes over it: the guard puts
        // it back if `f` or the check below panics, and otherwise terminates
        // the committed text.
        let terminate = Terminate(self);
        // SAFETY: the `max` bytes at `spare` are initialised and writable,
        // within the string's storage and apart from the structure's fields
        // (which precede the inline storage); nothing else reaches them while
        // `f` holds the slice, which it cannot keep past the call.
        let written = f(unsafe { core::slice::from_raw_parts_mut(spare, max) });
        assert!(
            written <= max,
            "append_with: {written} bytes written into {max} bytes of storage"
        );
        // SAFETY: the exclusive handle's pointer is to a live `AVBPrint`, so the place of its
        // `len` field is in bounds; nothing is accessed here.
        let len_ptr = unsafe { addr_of_mut!((*terminate.0.as_mut_ptr()).len) };
        // SAFETY: a write of the projected field; `len + written` is below `size`
        // (`written <= max < room`), so the guard's NUL lands right after the text, and at
        // most `LEN_MAX`.
        unsafe { len_ptr.write(len + written as u32) };
        drop(terminate);
        written
    }
}

/// Appends through [`BPrintMut::append_bytes`]: the text exactly, NULs
/// included, unlike [`BPrintMut::append_fmt`]. Never fails: what does not fit
/// is counted, as for every append.
///
/// ```
/// use core::fmt::Write;
///
/// use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;
/// let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
/// write!(buf.as_mut(), "{}-{}", 1, '\0').unwrap();
/// assert_eq!(buf.as_ref().as_bytes(), b"1-\0");
/// ```
impl fmt::Write for BPrintMut<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.append_bytes(s.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#[expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]
mod unit_tests {
    use super::*;

    #[test]
    fn avbprint_is_padded_to_1024_bytes() {
        type T = ffi::AVBPrint;
        // `str`, `len`, `size`, `size_max` and `reserved_internal_buffer[1]`,
        // aligned to the pointer.
        assert_eq!(<T as PaddedStruct>::FIELDS_SIZE, 24);
        assert_eq!(<T as PaddedStruct>::PADDING_LEN, 1000);
        assert_eq!(size_of::<T>(), <T as PaddedStruct>::SIZE);
        // C's padding follows the last field directly, inside the helper's
        // trailing alignment.
        assert_eq!(
            offset_of!(T, reserved_padding),
            offset_of!(T, reserved_internal_buffer) + 1
        );
        assert!(offset_of!(T, reserved_padding) < <T as PaddedStruct>::FIELDS_SIZE);
        // The padding ends 3 bytes before the structure does: its trailing
        // alignment, which C's automatic buffer covers as well.
        let padding_end = offset_of!(T, reserved_padding) + <T as PaddedStruct>::PADDING_LEN;
        assert_eq!(padding_end, 1021);
        assert_eq!(<T as PaddedStruct>::SIZE - padding_end, 3);
    }

    #[test]
    fn fields_are_the_helper_layout() {
        type F = ffi::ff_pad_helper_AVBPrint;
        assert_eq!(size_of::<BPrintFields>(), size_of::<F>());
        assert_eq!(align_of::<BPrintFields>(), align_of::<F>());
        // The helper's fields sit where `AVBPrint`'s do: the padding only
        // follows them.
        type T = ffi::AVBPrint;
        assert_eq!(offset_of!(F, str_), offset_of!(T, str_));
        assert_eq!(offset_of!(F, len), offset_of!(T, len));
        assert_eq!(offset_of!(F, size), offset_of!(T, size));
        assert_eq!(offset_of!(F, size_max), offset_of!(T, size_max));
        assert_eq!(
            offset_of!(F, reserved_internal_buffer),
            offset_of!(T, reserved_internal_buffer)
        );
    }

    #[test]
    fn new_default_and_zeroed_fields_are_empty() {
        for f in [
            BPrintFields::new(),
            BPrintFields::default(),
            BPrintFields::zeroed(),
        ] {
            let r = f.as_ref();
            assert_eq!(r.str(), None);
            assert_eq!((r.len(), r.size(), r.size_max()), (0, 0, 0));
            assert_eq!(r.internal_buffer(), [0]);
        }
    }

    #[test]
    fn setters_write_their_own_field_only() {
        let mut f = BPrintFields::zeroed();
        f.as_mut().set_str(Some(c"so far"));
        f.as_mut().set_len(u32::MAX);
        f.as_mut().set_size(1004);
        f.as_mut().set_size_max(1);
        f.as_mut().set_internal_buffer([0xFF]);
        let r = f.as_ref();
        assert_eq!(r.str(), Some(c"so far"));
        assert_eq!((r.len(), r.size(), r.size_max()), (u32::MAX, 1004, 1));
        assert_eq!(r.internal_buffer(), [0xFF]);
        // The raw fields hold exactly what was set.
        let raw = f.0;
        assert_eq!(raw.str_.cast_const(), c"so far".as_ptr());
        assert_eq!((raw.len, raw.size, raw.size_max), (u32::MAX, 1004, 1));
        assert_eq!(raw.reserved_internal_buffer, [-1]);

        f.as_mut().set_str(None);
        assert!(f.0.str_.is_null());
        assert_eq!(f.as_ref().str(), None);
        assert_eq!(f.as_ref().len(), u32::MAX);
    }

    #[test]
    fn debug_lists_every_field() {
        let mut f = BPrintFields::zeroed();
        f.as_mut().set_str(Some(c"x"));
        f.as_mut().set_len(1);
        assert_eq!(
            format!("{f:?}"),
            r#"BPrintFields { str: Some("x"), len: 1, size: 0, size_max: 0, internal_buffer: [0] }"#
        );
        assert_eq!(format!("{:?}", f.as_ref()), format!("{f:?}"));
    }

    #[test]
    fn padded_struct_fields_is_the_helper() {
        type T = ffi::AVBPrint;
        assert_eq!(
            <T as PaddedStruct>::FIELDS_SIZE,
            size_of::<<T as PaddedStruct>::Fields>()
        );
        assert_eq!(
            core::any::type_name::<<T as PaddedStruct>::Fields>(),
            core::any::type_name::<BPrintFields>()
        );
    }

    #[test]
    fn inline_capacity_is_the_storage_from_the_internal_buffer_to_the_end() {
        // `str`, `len`, `size` and `size_max` precede the inline storage,
        // which runs through `reserved_padding` and the trailing alignment.
        assert_eq!(BPrint::INLINE_CAPACITY, 1024 - 20);
        assert_eq!(
            BPrint::INLINE_CAPACITY as usize,
            1 + <ffi::AVBPrint as PaddedStruct>::PADDING_LEN + 3
        );
    }

    #[test]
    fn size_max_constants_are_cs() {
        // `AV_BPRINT_SIZE_UNLIMITED` is `((unsigned)-1)`.
        assert_eq!(BPrint::SIZE_UNLIMITED, c_uint::MAX);
        assert_eq!(BPrint::SIZE_AUTOMATIC, 1);
        assert_eq!(BPrint::SIZE_COUNT_ONLY, 0);
    }

    /// Appends `n` copies of `c`.
    fn chars(buf: &mut BPrintMut<'_>, c: u8, n: u32) {
        buf.append_repeated(c, n);
    }

    /// Corrected behaviour: C finalizes a count-only buffer into an
    /// `av_malloc(0)` block that holds no NUL, which read as a string is
    /// out of bounds; `into_string` hands back an empty string instead.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_count_only_buffer_finalizes_into_an_empty_string() {
        let mut buf = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        chars(&mut buf.as_mut(), b'x', 10);
        assert_eq!((buf.as_ref().len(), buf.as_ref().size()), (10, 0));
        let s = BPrint::into_string(buf).unwrap();
        assert_eq!(s.as_c_str(), c"");

        let mut buf = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        chars(&mut buf.as_mut(), b'x', 10);
        assert!(BPrint::into_bytes(buf).unwrap().as_slice().is_empty());
    }

    /// The finalizing policy releases a grown string with the structure;
    /// `LeakSanitizer` reports either one left behind.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dropping_a_grown_buffer_frees_its_string() {
        let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
        chars(&mut buf.as_mut(), b'y', 5000);
        assert!(!buf.as_ref().is_inline());
        assert_eq!(buf.as_ref().as_bytes(), [b'y'; 5000]);
        drop(buf);

        // Grown up front by `size_init`, never written.
        let buf = BPrint::new(4096, BPrint::SIZE_UNLIMITED).unwrap();
        assert!(!buf.as_ref().is_inline());
        assert!(buf.as_ref().size() >= 4096);
    }

    /// The buffer policy frees only the structure: `AddressSanitizer` reports
    /// a free of the caller's (Rust) buffer, which then stays readable.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dropping_a_buffer_on_a_caller_buffer_leaves_it_alone() {
        let mut out = vec![0xAA_u8; 8];
        let mut buf = BPrint::for_buffer(&mut out).unwrap();
        chars(&mut buf.as_mut(), b'z', 20);
        assert_eq!(buf.as_ref().as_bytes(), b"zzzzzzz");
        assert_eq!((buf.as_ref().len(), buf.as_ref().size_max()), (20, 8));
        drop(buf);
        assert_eq!(out, b"zzzzzzz\0");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn truncate_moves_the_terminator_within_the_string() {
        let mut out = [0xAA_u8; 8];
        let mut buf = BPrint::for_buffer(&mut out).unwrap();
        chars(&mut buf.as_mut(), b'a', 12);
        // Still truncated: the terminator stays at `size - 1`.
        buf.as_mut().truncate(9);
        assert_eq!(buf.as_ref().len(), 9);
        assert_eq!(buf.as_ref().as_bytes(), b"aaaaaaa");
        // Below `size`: complete again, terminated at `len`.
        buf.as_mut().truncate(3);
        assert_eq!(buf.as_ref().as_c_str(), c"aaa");
        // Never lengthens.
        buf.as_mut().truncate(5);
        assert_eq!(buf.as_ref().len(), 3);
        drop(buf);
        assert_eq!(&out[..5], b"aaa\0a");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_count_only_buffer_stores_nothing_and_truncates_without_writing() {
        let mut buf = BPrint::for_buffer(&mut []).unwrap();
        chars(&mut buf.as_mut(), b'q', 7);
        assert_eq!((buf.as_ref().len(), buf.as_ref().size()), (7, 0));
        assert_eq!(buf.as_ref().as_bytes(), b"");
        assert_eq!(buf.as_ref().as_c_str(), c"");
        // An empty caller buffer falls back to the inline storage.
        assert!(buf.as_ref().is_inline());
        buf.as_mut().truncate(2);
        assert_eq!(buf.as_ref().len(), 2);
    }

    /// The guard puts the NUL back at `min(len, size - 1)`, however its
    /// owner leaves, and writes nothing into a count-only buffer.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn terminate_reterminates_the_buffer_on_drop() {
        let mut out = [0xA5_u8; 4];
        let mut buf = BPrint::for_buffer(&mut out).unwrap();
        buf.as_mut().append_bytes(b"ab");
        let mut m = buf.as_mut();
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add_2 = unsafe { m.as_ref().state().0.add(2) };
        // SAFETY: byte 2 of the 4-byte caller buffer, written through the
        // exclusive handle's string.
        unsafe { v_0_add_2.write(b'x' as c_char) };
        drop(Terminate(&mut m));
        assert_eq!(buf.as_ref().as_c_str(), c"ab");

        let r = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let mut m = buf.as_mut();
            let _guard = Terminate(&mut m);
            panic!("unwinding");
        }));
        assert!(r.is_err());
        buf.as_mut().append_repeated(b'c', 9);
        // SAFETY: as the comment below states, for this part of the operation.
        let v_0_add = unsafe { buf.as_mut().as_ref().state().0.add(3) };
        // SAFETY: as above, the last byte, where the NUL of a truncated
        // buffer sits.
        unsafe { v_0_add.write(b'x' as c_char) };
        drop(Terminate(&mut buf.as_mut()));
        drop(buf);
        assert_eq!(out, *b"abc\0");

        let mut count = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        count.as_mut().append_repeated(b'q', 3);
        drop(Terminate(&mut count.as_mut()));
        assert_eq!(count.as_ref().len(), 3);
    }

    /// A closure claiming more than it got panics before anything is
    /// committed, and the terminator it wrote over comes back.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn append_with_rejects_an_overlong_count_and_restores_the_buffer() {
        let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
        buf.as_mut().append_bytes(b"ab");
        let r = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            buf.as_mut().append_with(4, |spare| {
                spare.fill(b'z');
                spare.len() + 1
            })
        }));
        assert!(r.is_err());
        assert_eq!(buf.as_ref().len(), 2);
        assert_eq!(buf.as_ref().as_c_str(), c"ab");
        assert!(buf.as_ref().is_complete());
        assert_eq!(BPrint::into_string(buf).unwrap().as_bytes(), b"ab");

        // With no storage at all, any count but 0 panics as well.
        let mut buf = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        let r = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            buf.as_mut().append_with(4, |spare| spare.len() + 1)
        }));
        assert!(r.is_err());
        assert_eq!(buf.as_ref().len(), 0);
    }

    /// A closure that panics after writing leaves the old text, terminated.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn append_with_restores_the_terminator_when_its_closure_panics() {
        let mut out = [0xA5_u8; 8];
        let mut buf = BPrint::for_buffer(&mut out).unwrap();
        buf.as_mut().append_bytes(b"ok");
        let r = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            buf.as_mut().append_with(8, |spare| {
                spare.fill(b'!');
                panic!("closure");
            })
        }));
        assert!(r.is_err());
        assert_eq!(buf.as_ref().as_c_str(), c"ok");
        drop(buf);
        // The storage after "ok" is re-terminated; the rest keeps what the
        // closure wrote, and the last byte, never handed out, is untouched.
        assert_eq!(out, *b"ok\0!!!!\xa5");
    }

    /// The storage is at most `size` bytes, zeroed, and stops short of the
    /// last byte; the count written is what is committed.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn append_with_hands_out_zeroed_storage_short_of_the_last_byte() {
        let mut out = [0xA5_u8; 8];
        let mut buf = BPrint::for_buffer(&mut out).unwrap();
        let mut seen = Vec::new();
        let n = buf.as_mut().append_with(100, |spare| {
            seen.extend_from_slice(spare);
            spare[..2].copy_from_slice(b"hi");
            2
        });
        assert_eq!((n, seen), (2, vec![0; 7]));
        let n = buf.as_mut().append_with(3, |spare| {
            assert_eq!(spare, [0; 3]);
            0
        });
        assert_eq!(n, 0);
        let n = buf.as_mut().append_with(0, |spare| spare.len());
        assert_eq!(n, 0);
        assert_eq!(buf.as_ref().as_bytes(), b"hi");
        drop(buf);
        // The first storage, 7 bytes, was zeroed; the last byte never.
        assert_eq!(out, *b"hi\0\0\0\0\0\xa5");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn write_keeps_nuls_where_append_fmt_stops() {
        use core::fmt::Write;
        let mut buf = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
        let nul = '\0';
        write!(buf.as_mut(), "a{nul}b").unwrap();
        buf.as_mut().append_fmt(format_args!("c{}d", '\0'));
        assert_eq!(buf.as_ref().as_bytes(), b"a\0bc");
        assert_eq!(buf.as_ref().as_c_str(), c"a");
    }
}
