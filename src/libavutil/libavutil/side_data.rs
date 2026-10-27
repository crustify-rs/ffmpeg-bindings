//! Safe wrappers over `libavutil/side_data.c`: the side data arrays of `frame.h`
//! ([`SideDataArray`] and its handles), the operations on them
//! (`av_frame_side_data_new`, `_add`, `_clone`, `_get`, `_remove` and
//! `_remove_by_props`, and `_free` as the array's drop), and the side data
//! descriptors and names ([`SideDataType::descriptor`], [`SideDataType::name`]).
//! The entry type, [`SideData`], and its frame are `frame.c`'s.

use core::ffi::{CStr, c_int, c_uint};
use core::hash::Hash;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ops::{BitOr, BitOrAssign, Range};
use core::ptr::{self, NonNull, addr_of_mut};
use core::{fmt, slice};

use ffibox::{CMutSeam, CRefSeam};

use super::buffer::{BufferReference, BufferReferenceBox, ENOMEM, may_add_reference};
use super::dict::{Dict, DictionaryBox};
use super::ffi;
use super::frame::{
    SideData, SideDataDescriptorRef, SideDataMut, SideDataProps, SideDataRef, SideDataType,
};

impl SideDataType {
    /// Wraps: `av_frame_side_data_desc`
    ///
    /// This side data type's descriptor in C's table, borrowed for the whole
    /// program: the table is static and never written, and its names are
    /// string literals, the descriptor's invariant. `None` for a value the
    /// table has no named entry for, such as one past the last type these
    /// bindings know.
    ///
    /// The handle is shared, as C's table is never written; a descriptor to
    /// edit is a copy, [`get`](SideDataDescriptorRef::get):
    ///
    /// ```compile_fail,E0599
    /// use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    /// let mut desc = SideDataType::STEREO_3D.descriptor().unwrap();
    /// desc.set_name(None);
    /// ```
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataProps, SideDataType};
    /// let desc = SideDataType::DISPLAY_MATRIX.descriptor().unwrap();
    /// assert_eq!(desc.name(), Some(c"3x3 displaymatrix"));
    /// assert!(desc.props().contains(SideDataProps::GLOBAL));
    /// ```
    #[must_use]
    pub fn descriptor(self) -> Option<SideDataDescriptorRef<'static>> {
        // For the unsafe operations below: `av_frame_side_data_desc` takes the enum by value,
        // checks it against the bounds of its static `const` table, and returns NULL
        // or the address of a named entry, which lives, unwritten, for the
        // whole program; its `name` is a string literal, the descriptor's
        // invariant, and a shared `'static` handle only reads it.
        {
            // SAFETY: as the comment above this block states.
            let p = unsafe { ffi::av_frame_side_data_desc(self.into_raw()) };
            // SAFETY: as the comment above this block states.
            unsafe { SideDataDescriptorRef::from_ptr(p.cast_mut()) }
        }
    }

    /// Wraps: `av_frame_side_data_name`
    ///
    /// This side data type's human-readable name, the
    /// [`name`](SideDataDescriptorRef::name) of its
    /// [`descriptor`](Self::descriptor), such as `Stereo 3D`; `None` when the
    /// table has no named entry for it.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    /// assert_eq!(SideDataType::STEREO_3D.name(), Some(c"Stereo 3D"));
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: `av_frame_side_data_name` takes the enum by value and
        // returns NULL or the `name` of an entry of the same static table,
        // a string literal.
        let name = unsafe { ffi::av_frame_side_data_name(self.into_raw()) };
        // SAFETY: a non-NULL `name` is a NUL-terminated string literal,
        // immutable for the life of the process.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }
}

/// Wraps: `av_frame_side_data_free`
/// Handwritten
///
/// An owned array of side data: C's `AVFrameSideData **sd` and `int nb_sd`
/// pair, empty as NULL and 0, as `AVCodecContext` keeps its
/// `decoded_side_data`. Dropping it frees every entry with its buffer
/// reference and metadata, and the array (`av_frame_side_data_free`).
///
/// Its entries are reached through a [`SideDataArrayRef`] (`as_ref()`) and a
/// [`SideDataArrayMut`] (`as_mut()`), the same handles a frame's own list
/// hands out. The former looks them up by kind
/// ([`find`](SideDataArrayRef::find)); the latter adds them
/// ([`add`](SideDataArrayMut::add), [`alloc_zeroed`](SideDataArrayMut::alloc_zeroed),
/// [`add_clone`](SideDataArrayMut::add_clone)) and removes them
/// ([`remove`](SideDataArrayMut::remove)). An array C built is taken over
/// with [`from_raw_parts`](Self::from_raw_parts).
///
/// Hand-written rather than a `CVec`: the C routines that grow, shrink and
/// free an array take the two variables by address and update both, the
/// count as an `int`, which a `CVec`'s `(pointer, usize)` cannot lend.
///
/// `Send` and `Sync` with [`SideData`].
pub struct SideDataArray {
    entries: *mut *mut ffi::AVFrameSideData,
    len: c_int,
}

// SAFETY: the array owns its `av_malloc` block of pointers and every entry
// behind them, all `Send` per `Send for SideData`; `av_frame_side_data_free`
// may release them on any thread.
unsafe impl Send for SideDataArray {}
// SAFETY: `&SideDataArray` reaches the entries only through
// `SideDataArrayRef`, which reads the pointer block and lends shared entry
// handles, `Sync` per `Sync for SideData`.
unsafe impl Sync for SideDataArray {}

impl SideDataArray {
    /// The empty array, C's NULL and 0.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: ptr::null_mut(),
            len: 0,
        }
    }

    /// Takes over the array C built in `entries` and `len`, as
    /// `av_frame_side_data_free` would free it.
    ///
    /// # Safety
    ///
    /// `entries` and `len` must be an array the `av_frame_side_data_*`
    /// routines built (or NULL and 0) that nothing else uses or frees from
    /// now on. That is, `len` is not negative, and `entries` is an
    /// `av_malloc`-family block of `len` pointers to distinct entries those
    /// routines allocated, every one satisfying [`SideData`]'s invariant: in
    /// particular, a payload `av_frame_side_data_new` returned must have
    /// been written in full.
    #[must_use]
    pub unsafe fn from_raw_parts(entries: *mut *mut ffi::AVFrameSideData, len: c_int) -> Self {
        Self { entries, len }
    }

    /// Gives the array to C as its `AVFrameSideData **` and count, which
    /// `av_frame_side_data_free` releases; NULL and 0 when it is empty.
    #[must_use]
    pub fn into_raw_parts(self) -> (*mut *mut ffi::AVFrameSideData, c_int) {
        let this = ManuallyDrop::new(self);
        (this.entries, this.len)
    }

    /// Shared handle to the array.
    #[inline]
    #[must_use]
    pub fn as_ref(&self) -> SideDataArrayRef<'_> {
        // SAFETY: by the type's invariant, the fields are an array of `len`
        // live entries this owner holds; the shared borrow of `self` keeps
        // it unchanged for the handle's lifetime.
        unsafe { SideDataArrayRef::from_raw_parts(self.entries, self.len) }
    }

    /// Exclusive handle to the array.
    #[inline]
    #[must_use]
    pub fn as_mut(&mut self) -> SideDataArrayMut<'_> {
        // The two variables of an array of live entries this owner holds,
        // exclusively borrowed for the handle's lifetime.
        SideDataArrayMut {
            entries: NonNull::from(&mut self.entries),
            len: NonNull::from(&mut self.len),
            _borrow: PhantomData,
        }
    }

    /// The number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Whether the array has no entry.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SideDataArray {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SideDataArray {
    fn drop(&mut self) {
        // SAFETY: the owner's array, which C frees with every entry, their
        // buffer references and metadata, NULLing and zeroing the variables.
        unsafe { ffi::av_frame_side_data_free(&raw mut self.entries, &raw mut self.len) }
    }
}

impl fmt::Debug for SideDataArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

#[expect(
    clippy::into_iter_without_iter,
    reason = "the borrowed array is iterated through `IntoIterator`; `get` reaches single entries"
)]
impl<'a> IntoIterator for &'a SideDataArray {
    type IntoIter = SideDataIter<'a>;
    type Item = SideDataRef<'a>;

    fn into_iter(self) -> SideDataIter<'a> {
        self.as_ref().into_iter()
    }
}

#[expect(
    clippy::into_iter_without_iter,
    reason = "the borrowed array is iterated through `IntoIterator`; `get` reaches single entries"
)]
impl<'a> IntoIterator for &'a mut SideDataArray {
    type IntoIter = SideDataIterMut<'a>;
    type Item = SideDataMut<'a>;

    fn into_iter(self) -> SideDataIterMut<'a> {
        self.as_mut().into_iter()
    }
}

/// Handwritten
///
/// Shared borrow of a side data array: a [`SideDataArray`], or a frame's
/// list. `Copy`, like `&[T]`; it lends shared handles to the entries.
#[derive(Clone, Copy)]
pub struct SideDataArrayRef<'a> {
    entries: *const *mut ffi::AVFrameSideData,
    len: usize,
    _borrow: PhantomData<&'a SideData>,
}

// SAFETY: the handle reads the pointer block, which nothing writes while it
// is borrowed, and lends shared entry handles, `Send` and `Sync` per
// `Sync for SideData`.
unsafe impl Send for SideDataArrayRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for SideDataArrayRef<'_> {}

impl<'a> SideDataArrayRef<'a> {
    /// The handle of the array of `len` entries at `entries`; NULL and 0
    /// (or a negative count, which C reads as none) is the empty array.
    ///
    /// # Safety
    ///
    /// `entries` and `len` must be an array of distinct live entries, each
    /// satisfying [`SideData`]'s invariant, which nothing changes or frees
    /// for `'a`.
    pub(crate) unsafe fn from_raw_parts(
        entries: *const *mut ffi::AVFrameSideData,
        len: c_int,
    ) -> Self {
        Self {
            entries,
            len: usize::try_from(len).unwrap_or(0),
            _borrow: PhantomData,
        }
    }

    /// The pointer to entry `i`, which is in bounds.
    fn entry(&self, i: usize) -> *mut ffi::AVFrameSideData {
        debug_assert!(i < self.len);
        // SAFETY: `i` is within the block `self.entries` points to, as the operation below
        // relies on, so the offset stays in bounds; nothing is accessed here.
        let entries_at = unsafe { self.entries.add(i) };
        // SAFETY: `i < len`, so the read is inside the live pointer block,
        // which nothing writes for `'a`.
        unsafe { entries_at.read() }
    }

    /// The number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the array has no entry.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Entry `i`; `None` past the end.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that an array entry is not NULL.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<SideDataRef<'a>> {
        (i < self.len).then(|| {
            // SAFETY: the array's entry `i`, live and unchanged for `'a`.
            unsafe { SideDataRef::from_ptr(self.entry(i)) }.expect("an array entry is not NULL")
        })
    }

    /// The entries, in array order.
    #[must_use]
    #[expect(
        clippy::iter_without_into_iter,
        reason = "the handle is `Copy` and iterated through `iter`"
    )]
    pub fn iter(&self) -> SideDataIter<'a> {
        SideDataIter {
            array: *self,
            range: 0..self.len,
        }
    }
}

impl fmt::Debug for SideDataArrayRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for SideDataArrayRef<'a> {
    type IntoIter = SideDataIter<'a>;
    type Item = SideDataRef<'a>;

    fn into_iter(self) -> SideDataIter<'a> {
        self.iter()
    }
}

/// Handwritten
///
/// Exclusive borrow of a side data array: a [`SideDataArray`], or a frame's
/// list. It lends exclusive handles to the entries, and reaches the shared
/// ones with [`as_ref`](Self::as_ref).
///
/// Entries it lends exclude one another, as `&mut` elements of a slice do:
///
/// ```compile_fail,E0499
/// use ffmpeg_bindings::libavutil::libavutil::side_data::SideDataArray;
/// let mut array = SideDataArray::new();
/// let mut entries = array.as_mut();
/// let first = entries.get_mut(0).unwrap();
/// let second = entries.get_mut(1).unwrap();
/// drop((first, second));
/// ```
///
/// and none outlives the array's exclusive borrow:
///
/// ```compile_fail,E0505
/// use ffmpeg_bindings::libavutil::libavutil::side_data::SideDataArray;
/// let mut array = SideDataArray::new();
/// let entry = array.as_mut().into_iter().next().unwrap();
/// drop(array);
/// drop(entry);
/// ```
pub struct SideDataArrayMut<'a> {
    // The variables holding the array's pointer and count, which hold an
    // array as `SideDataArrayRef::from_raw_parts` requires, both borrowed
    // exclusively for `'a`.
    pub(super) entries: NonNull<*mut *mut ffi::AVFrameSideData>,
    pub(super) len: NonNull<c_int>,
    pub(super) _borrow: PhantomData<&'a mut SideData>,
}

// SAFETY: the handle is the only access to the array and its entries, which
// are `Send` per `Send for SideData`.
unsafe impl Send for SideDataArrayMut<'_> {}
// SAFETY: `&SideDataArrayMut` reaches the array only through `as_ref`, a
// shared handle, `Sync` per `Sync for SideDataArrayRef`.
unsafe impl Sync for SideDataArrayMut<'_> {}

impl SideDataArrayMut<'_> {
    /// A shared handle to the array, borrowing this one.
    #[must_use]
    pub fn as_ref(&self) -> SideDataArrayRef<'_> {
        // SAFETY: as the comment below states, for this part of the operation.
        let entries_read = unsafe { self.entries.read() };
        // SAFETY: as the comment below states, for this part of the operation.
        let len_read = unsafe { self.len.read() };
        // SAFETY: reads of the two variables, live and exclusively borrowed
        // for `'a`; the array they hold stays unchanged while `self` is
        // borrowed shared.
        unsafe { SideDataArrayRef::from_raw_parts(entries_read, len_read) }
    }

    /// An exclusive handle to the array, borrowing this one.
    #[must_use]
    pub fn as_mut(&mut self) -> SideDataArrayMut<'_> {
        SideDataArrayMut {
            entries: self.entries,
            len: self.len,
            _borrow: PhantomData,
        }
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Whether the array has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entry `i`, writable; `None` past the end.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that an array entry is not NULL.
    #[must_use]
    pub fn get_mut(&mut self, i: usize) -> Option<SideDataMut<'_>> {
        let array = self.as_ref();
        (i < array.len()).then(|| {
            // SAFETY: the array's entry `i`, live, and exclusively borrowed
            // with the array through `&mut self`.
            unsafe { SideDataMut::from_ptr(array.entry(i)) }.expect("an array entry is not NULL")
        })
    }

    /// The entries, writable, in array order.
    #[must_use]
    #[expect(
        clippy::iter_without_into_iter,
        reason = "the handle is `Copy` and iterated through `iter`"
    )]
    pub fn iter_mut(&mut self) -> SideDataIterMut<'_> {
        self.as_mut().into_iter()
    }
}

/// The `AV_FRAME_SIDE_DATA_FLAG_*` flags of an addition to a side data array
/// ([`SideDataArrayMut::add`]).
///
/// C's `AV_FRAME_SIDE_DATA_FLAG_NEW_REF`, which makes C add its own
/// reference to the caller's buffer, is not a flag here: an addition takes an
/// owned reference, so a caller keeping one passes a clone
/// ([`try_clone`](super::buffer::BufferReferenceRef::try_clone)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SideDataFlags(c_uint);

impl SideDataFlags {
    /// No flag: append, unless the kind is not `MULTI` and already present.
    pub const NONE: Self = Self(0);
    /// For a kind that is not `MULTI`, replace the buffer of the entry of the
    /// kind already present, if any, freeing its metadata, rather than
    /// refuse.
    pub const REPLACE: Self = Self(ffi::AV_FRAME_SIDE_DATA_FLAG_REPLACE);
    /// Remove every entry of the kind first.
    pub const UNIQUE: Self = Self(ffi::AV_FRAME_SIDE_DATA_FLAG_UNIQUE);

    /// The flags as C's `unsigned` mask.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> c_uint {
        self.0
    }

    /// Whether every flag of `other` is set in `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for SideDataFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for SideDataFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl SideDataArrayMut<'_> {
    /// Wraps: `av_frame_side_data_add`
    ///
    /// Adds side data of `kind` whose payload is `buf`'s bytes, and returns
    /// the entry holding it; the entry takes `buf` over. `buf` comes back
    /// when C adds nothing: `kind` is not `MULTI`, an entry of it is present
    /// and [`REPLACE`](SideDataFlags::REPLACE) is not set, or C runs out of
    /// memory.
    ///
    /// With [`UNIQUE`](SideDataFlags::UNIQUE), C first removes every entry
    /// of `kind`, even when it then fails to add. With `REPLACE`, an entry of
    /// a kind that is not `MULTI` already present takes `buf` in place of its
    /// buffer, and loses its metadata; that is the entry returned. Otherwise
    /// the new entry is the array's last, with no metadata.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    /// use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    /// use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};
    /// let mut array = SideDataArray::new();
    /// let mut entries = array.as_mut();
    /// let afd = BufferReference::alloc(1).unwrap().fill(8);
    /// let entry = entries
    ///     .add(SideDataType::AFD, afd, SideDataFlags::NONE)
    ///     .unwrap();
    /// assert_eq!(entry.as_ref().data(), [8]);
    /// let again = BufferReference::alloc(1).unwrap().fill(9);
    /// let again = entries
    ///     .add(SideDataType::AFD, again, SideDataFlags::NONE)
    ///     .unwrap_err();
    /// let entry = entries
    ///     .add(SideDataType::AFD, again, SideDataFlags::REPLACE)
    ///     .unwrap();
    /// assert_eq!(entry.as_ref().data(), [9]);
    /// assert_eq!(array.len(), 1);
    /// ```
    ///
    /// # Errors
    ///
    /// With [`UNIQUE`](SideDataFlags::UNIQUE), C first removes every entry of `kind`, even when it
    /// then fails to add.
    pub fn add(
        &mut self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'_>, BufferReferenceBox> {
        self.as_mut().into_added(kind, buf, flags)
    }
}

impl<'a> SideDataArrayMut<'a> {
    /// [`add`](Self::add), lending the entry for the array's whole borrow.
    fn into_added(
        self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'a>, BufferReferenceBox> {
        let mut slot = buf.into_c();
        // Without NEW_REF, C stores the slot's reference in an entry and
        // NULLs the slot on success, and leaves both alone on failure. A new
        // entry is av_mallocz'd with that reference and its window, a
        // replaced one gets them and a NULL metadata.
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive; `slot` holds a
        // live reference whose window is initialised bytes, so the array
        // keeps `SideData`'s invariant.
        let entry = unsafe {
            ffi::av_frame_side_data_add(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
                &raw mut slot,
                flags.bits(),
            )
        };
        if entry.is_null() {
            // SAFETY: C failed, so the slot still holds the caller's
            // reference, which the box takes back.
            let buf = unsafe { BufferReferenceBox::from_c(slot) };
            return Err(buf.expect("C keeps the reference it did not add"));
        }
        debug_assert!(slot.is_null());
        // SAFETY: an entry of the array, live and exclusively borrowed with
        // it for `'a`, which `self`, consumed, no longer reaches.
        Ok(unsafe { SideDataMut::from_ptr(entry) }.expect("C returned an entry"))
    }
}

impl SideDataArray {
    /// Adds side data of `kind` whose payload is `buf`'s bytes, and returns
    /// the entry holding it, as [`SideDataArrayMut::add`] does, borrowing
    /// the array.
    ///
    /// # Errors
    ///
    /// As [`SideDataArrayMut::add`].
    pub fn add(
        &mut self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'_>, BufferReferenceBox> {
        self.as_mut().into_added(kind, buf, flags)
    }
}

impl fmt::Debug for SideDataArrayMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> IntoIterator for SideDataArrayMut<'a> {
    type IntoIter = SideDataIterMut<'a>;
    type Item = SideDataMut<'a>;

    fn into_iter(self) -> SideDataIterMut<'a> {
        let (entries, len) = (self.entries, self.len);
        // SAFETY: as the comment below states, for this part of the operation.
        let entries_read_2 = unsafe { entries.read() };
        // SAFETY: as the comment below states, for this part of the operation.
        let len_read_2 = unsafe { len.read() };
        // SAFETY: reads of the two variables, live and exclusively borrowed
        // for `'a`, which `self`, consumed, no longer reaches; the array
        // stays unchanged for `'a`.
        let array = unsafe { SideDataArrayRef::from_raw_parts(entries_read_2, len_read_2) };
        SideDataIterMut {
            range: 0..array.len(),
            array,
            _borrow: PhantomData,
        }
    }
}

/// The entries of a side data array, as shared handles, in array order.
#[derive(Clone, Debug)]
pub struct SideDataIter<'a> {
    array: SideDataArrayRef<'a>,
    range: Range<usize>,
}

impl<'a> Iterator for SideDataIter<'a> {
    type Item = SideDataRef<'a>;

    fn next(&mut self) -> Option<SideDataRef<'a>> {
        self.array.get(self.range.next()?)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for SideDataIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.array.get(self.range.next_back()?)
    }
}

impl ExactSizeIterator for SideDataIter<'_> {}

impl FusedIterator for SideDataIter<'_> {}

/// The entries of a side data array, as exclusive handles, in array order.
pub struct SideDataIterMut<'a> {
    array: SideDataArrayRef<'a>,
    range: Range<usize>,
    _borrow: PhantomData<&'a mut SideData>,
}

impl<'a> SideDataIterMut<'a> {
    /// The handle to entry `i`, which the range has just yielded.
    fn handle(&self, i: usize) -> SideDataMut<'a> {
        // SAFETY: the array's entry `i`, live and exclusively borrowed for
        // `'a`; the entries are distinct and the range yields each index
        // once, so no two handles reach one entry.
        unsafe { SideDataMut::from_ptr(self.array.entry(i)) }.expect("an array entry is not NULL")
    }
}

impl fmt::Debug for SideDataIterMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SideDataIterMut")
            .field("remaining", &self.range.len())
            .finish()
    }
}

impl<'a> Iterator for SideDataIterMut<'a> {
    type Item = SideDataMut<'a>;

    fn next(&mut self) -> Option<SideDataMut<'a>> {
        let i = self.range.next()?;
        Some(self.handle(i))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for SideDataIterMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let i = self.range.next_back()?;
        Some(self.handle(i))
    }
}

impl ExactSizeIterator for SideDataIterMut<'_> {}

impl FusedIterator for SideDataIterMut<'_> {}

impl<'a> SideDataArrayRef<'a> {
    /// Wraps: `av_frame_side_data_get_c`
    ///
    /// The first entry of `kind`, in array order; `None` when there is none.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that an array counts entries in an int.
    #[must_use]
    pub fn find(&self, kind: SideDataType) -> Option<SideDataRef<'a>> {
        // The count came from a `c_int`, so it converts back.
        let len = c_int::try_from(self.len).expect("an array counts entries in an int");
        let sd = self.entries.cast();
        // SAFETY: an array of `len` live entries, unchanged for `'a`, which C
        // only reads; it returns one of them, or NULL.
        let entry = unsafe { ffi::av_frame_side_data_get_c(sd, len, kind.into_raw()) };
        // SAFETY: NULL, or an entry of the array (const only for C's
        // const-correctness), live and unchanged for `'a`.
        unsafe { SideDataRef::from_ptr(entry.cast_mut()) }
    }

    /// Whether C's additions of `kind` with `flags` (`_new`, `_add`,
    /// `_clone`) change an entry in place rather than append or refuse:
    /// `REPLACE` without `UNIQUE`, for a kind that is not `MULTI` and has an
    /// entry here.
    fn replaces_in_place(&self, kind: SideDataType, flags: SideDataFlags) -> bool {
        let multi = kind
            .descriptor()
            .is_some_and(|d| d.props().contains(SideDataProps::MULTI));
        flags.contains(SideDataFlags::REPLACE)
            && !flags.contains(SideDataFlags::UNIQUE)
            && !multi
            && self.find(kind).is_some()
    }
}

impl SideDataMut<'_> {
    /// Makes this entry a copy of `src`, as `av_frame_side_data_clone`'s
    /// in-place replacement does and in its order: a copy of `src`'s
    /// metadata (C's flags 0, so keys equal up to ASCII case merge), then a
    /// reference to `src`'s buffer, then `src`'s window and the copy in
    /// place of the entry's own metadata. On failure the entry is as it
    /// was, and the partial copy, which C's own code leaks, is freed.
    fn assign_clone(&mut self, src: SideDataRef<'_>) -> Result<(), c_int> {
        let s = src.as_ptr();
        // SAFETY: `s` is to a live object, as the block below relies on, so the
        // place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { &raw const (*s).data };
        // SAFETY: `s` is to a live object, as the block below relies on, so the
        // place of its `metadata` field is in bounds; nothing is accessed here.
        let metadata_ptr = unsafe { &raw const (*s).metadata };
        // SAFETY: `s` is to a live object, as the block below relies on, so the
        // place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { &raw const (*s).size };
        // SAFETY: reads of three fields projected through the shared
        // handle's pointer to a live `AVFrameSideData`.
        let (metadata, data, size) = (
            unsafe { metadata_ptr.read() },
            unsafe { data_ptr.read() },
            unsafe { size_ptr.read() },
        );
        let mut copy = ptr::null_mut();
        // SAFETY: `metadata` is NULL or `src`'s live dictionary, which C only
        // reads, into a local slot.
        let ret = unsafe { ffi::av_dict_copy(&raw mut copy, metadata, 0) };
        // SAFETY: NULL, or the non-empty copy C built (in part, on failure),
        // ours alone: an `av_dict_set` that leaves no entry frees it.
        let copy = Dict::from(unsafe { DictionaryBox::from_c(copy) });
        if ret < 0 {
            return Err(ret);
        }
        let (p, from) = (self.as_mut_ptr(), src.buffer().as_ptr());
        // SAFETY: `p` is to a live object, as the operation below relies on,
        // so the place of its `buf` field is in bounds; nothing is accessed here.
        let buf_ptr = unsafe { addr_of_mut!((*p).buf) };
        // SAFETY: the entry's own reference slot, exclusively borrowed, and
        // `src`'s live reference, a distinct struct as `src` is another
        // entry: C keeps the slot, or replaces its reference with a new one
        // to `src`'s buffer, and leaves it alone on failure.
        let ret = unsafe { ffi::av_buffer_replace(buf_ptr, from) };
        if ret < 0 {
            return Err(ret);
        }
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { &raw mut (*p).data };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { &raw mut (*p).size };
        // For the unsafe operations below: writes of two fields projected through the exclusive
        // handle's pointer: `src`'s window, initialised bytes of the buffer
        // the entry now references too, by `SideData`'s invariant.
        {
            // SAFETY: as the comment above this block states.
            unsafe { data_ptr.write(data) };
            // SAFETY: as the comment above this block states.
            unsafe { size_ptr.write(size) };
        }
        self.set_metadata(copy);
        Ok(())
    }
}

impl SideDataArrayMut<'_> {
    /// Wraps: `av_frame_side_data_get`
    ///
    /// The first entry of `kind`, in array order, writable; `None` when
    /// there is none.
    #[must_use]
    pub fn find_mut(&mut self, kind: SideDataType) -> Option<SideDataMut<'_>> {
        self.as_mut().into_found(kind)
    }

    /// Wraps: `av_frame_side_data_remove`
    ///
    /// Removes every entry of `kind`, freeing each with its buffer reference
    /// and metadata. C moves the array's last entry into each freed slot, so
    /// the others may change order.
    pub fn remove(&mut self, kind: SideDataType) {
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C frees or moves is alive; C keeps the count
        // equal to the entries left, and their invariant.
        unsafe {
            ffi::av_frame_side_data_remove(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
            );
        }
    }

    /// Wraps: `av_frame_side_data_remove_by_props`
    ///
    /// Removes every entry whose kind has properties in common with `props`
    /// ([`SideDataProps::intersects`] its
    /// [`descriptor`](SideDataType::descriptor)'s), freeing each with its
    /// buffer reference and metadata, as [`remove`](Self::remove) does. Kinds
    /// C has no descriptor for stay.
    pub fn remove_by_props(&mut self, props: SideDataProps) {
        // C's `int` filter, the bits as they are (C ANDs it with an unsigned
        // `props`, which converts it back).
        let props = props.bits().cast_signed();
        // SAFETY: as in `remove`.
        unsafe {
            ffi::av_frame_side_data_remove_by_props(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                props,
            );
        }
    }

    /// Wraps: `av_frame_side_data_clone`
    ///
    /// Adds a copy of `src`, an entry of another array: one that shares its
    /// buffer, through a new reference (so neither payload is writable while
    /// both live), and holds a copy of its metadata, made as C makes it,
    /// where keys equal up to ASCII case collapse into one entry holding
    /// the last one's value (unlike [`Dict`]'s `Clone`). Flags act as for
    /// [`add`](Self::add): [`UNIQUE`](SideDataFlags::UNIQUE) first removes
    /// every entry of `src`'s kind, even when the copy then fails; for a kind
    /// that is not `MULTI` and already present,
    /// [`REPLACE`](SideDataFlags::REPLACE) makes that entry the copy in
    /// place, and without it C refuses. Otherwise the copy is the array's
    /// last entry.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::buffer::BufferReference;
    /// use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    /// use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};
    /// let mut src = SideDataArray::new();
    /// let afd = BufferReference::alloc(1).unwrap().fill(8);
    /// src.add(SideDataType::AFD, afd, SideDataFlags::NONE)
    ///     .unwrap();
    /// let mut dst = SideDataArray::new();
    /// let entry = src.as_ref().get(0).unwrap();
    /// dst.as_mut().add_clone(entry, SideDataFlags::NONE).unwrap();
    /// assert_eq!(dst.as_ref().get(0).unwrap().data(), [8]);
    /// assert!(dst.as_mut().add_clone(entry, SideDataFlags::NONE).is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EEXIST)` for a kind present without `REPLACE`, and
    /// `AVERROR(ENOMEM)` when C cannot allocate, or `src`'s buffer counts so
    /// many references that another could near wrapping its count. The array
    /// is then as `UNIQUE` left it.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that an entry of the kind is present.
    pub fn add_clone(&mut self, src: SideDataRef<'_>, flags: SideDataFlags) -> Result<(), c_int> {
        if !may_add_reference(src.buffer().ref_count()) {
            return Err(ENOMEM);
        }
        if self.as_ref().replaces_in_place(src.kind(), flags) {
            // C's in-place path leaks a metadata copy that fails part way
            // (`side_data.c:275-277`): run it here, freeing the copy.
            let entry = self.as_mut().into_found(src.kind());
            return entry
                .expect("an entry of the kind is present")
                .assign_clone(src);
        }
        // Every entry C builds or changes holds a new reference to `src`'s
        // buffer, `src`'s window into it, and a copy of its metadata.
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive; `src` a live
        // entry of another array (borrowed shared, so not this one), which C
        // only reads. The entries C writes keep `SideData`'s invariant.
        let ret = unsafe {
            ffi::av_frame_side_data_clone(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                src.as_ptr(),
                flags.bits(),
            )
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: `av_frame_side_data_new`
    ///
    /// Adds side data of `kind` with a new payload of `size` zeroed bytes,
    /// and returns the entry holding it, as [`add`](Self::add) does with a
    /// buffer C allocates; its payload is writable
    /// ([`data_mut`](SideDataMut::data_mut)) until its buffer is shared.
    /// `None` when C adds nothing: `kind` is not `MULTI`, an entry of it is
    /// present and [`REPLACE`](SideDataFlags::REPLACE) is not set, or the
    /// allocation fails. [`UNIQUE`](SideDataFlags::UNIQUE) removes every
    /// entry of `kind` first, even then.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;
    /// use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};
    /// let mut array = SideDataArray::new();
    /// let mut entry = array
    ///     .alloc_zeroed(SideDataType::AFD, 1, SideDataFlags::NONE)
    ///     .unwrap();
    /// entry.data_mut().unwrap()[0] = 10;
    /// assert_eq!(array.as_ref().find(SideDataType::AFD).unwrap().data(), [10]);
    /// ```
    pub fn alloc_zeroed(
        &mut self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut().into_new_entry(kind, size, flags, zero_into)
    }

    /// Wraps: `av_frame_side_data_new`
    ///
    /// Adds side data of `kind` with a new payload copied from `bytes`, and
    /// returns the entry holding it, as [`alloc_zeroed`](Self::alloc_zeroed)
    /// does with zeroes.
    pub fn alloc_from_slice(
        &mut self,
        kind: SideDataType,
        bytes: &[u8],
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut()
            .into_new_entry(kind, bytes.len(), flags, |run| copy_into(run, bytes))
    }
}

impl<'a> SideDataArrayMut<'a> {
    /// [`find_mut`](Self::find_mut), lending the entry for the array's
    /// whole borrow.
    fn into_found(self, kind: SideDataType) -> Option<SideDataMut<'a>> {
        // SAFETY: as the comment below states, for this part of the operation.
        let entries_read = unsafe { self.entries.read() };
        // SAFETY: as the comment below states, for this part of the operation.
        let len_read = unsafe { self.len.read() };
        // SAFETY: reads of the two variables, which hold an array of live
        // entries; C only reads it, and returns one of its entry pointers
        // (const only for C's const-correctness), or NULL.
        let entry = unsafe { ffi::av_frame_side_data_get(entries_read, len_read, kind.into_raw()) };
        // SAFETY: NULL, or an entry of the array, live and exclusively
        // borrowed with it for `'a`, which `self`, consumed, no longer
        // reaches.
        unsafe { SideDataMut::from_ptr(entry.cast_mut()) }
    }

    /// [`alloc_zeroed`](Self::alloc_zeroed) and
    /// [`alloc_from_slice`](Self::alloc_from_slice), with `init` writing
    /// every byte of the new payload before anything can read it, lending
    /// the entry for the array's whole borrow.
    fn into_new_entry(
        self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
        init: impl FnOnce(&mut [MaybeUninit<u8>]),
    ) -> Option<SideDataMut<'a>> {
        if self.as_ref().replaces_in_place(kind, flags) {
            // C's replacement path stores the buffer it allocated without
            // checking it, so a failed allocation is a NULL dereference
            // (`replace_side_data_from_buf`). Allocate first, and replace
            // through `av_frame_side_data_add`, which writes the entry's
            // fields as C's path does.
            let mut buf = BufferReference::alloc(size)?;
            init(buf.as_mut_slice());
            // SAFETY: `init` wrote every byte.
            let buf = unsafe { buf.assume_init() };
            return self.into_added(kind, buf, flags).ok();
        }
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive. With no entry
        // to replace, C appends an entry holding a fresh, sole reference to
        // `size` bytes, its window, or fails with the array as `UNIQUE` left
        // it; the payload is uninitialised until written below.
        let entry = unsafe {
            ffi::av_frame_side_data_new(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
                size,
                flags.bits(),
            )
        };
        if entry.is_null() {
            return None;
        }
        // SAFETY: `entry` is to a live object, as the block below relies on, so the
        // place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { &raw const (*entry).data };
        // SAFETY: `entry` is to a live object, as the block below relies on, so the
        // place of its `size` field is in bounds; nothing is accessed here.
        let size_ptr = unsafe { &raw const (*entry).size };
        // SAFETY: reads of the new entry's window, `size` bytes of a buffer
        // the entry alone references, which nothing has read or lent yet.
        let run = (unsafe { data_ptr.read() }, unsafe { size_ptr.read() });
        let run = match NonNull::new(run.0) {
            Some(data) if run.1 > 0 => (data, run.1),
            _ => (NonNull::dangling(), 0),
        };
        let data = run.0.cast::<MaybeUninit<u8>>().as_ptr();
        // SAFETY: the window's bytes (or nothing), writable, viewed
        // uninitialised and exclusively for `init`, which writes them all
        // before the entry is handed out, restoring `SideData`'s invariant.
        let bytes = unsafe { slice::from_raw_parts_mut(data, run.1) };
        init(bytes);
        // SAFETY: the new entry, its payload written in full, live and
        // exclusively borrowed with the array for `'a`, which `self`,
        // consumed, no longer reaches.
        unsafe { SideDataMut::from_ptr(entry) }
    }
}

/// Zeroes `dst`, in one operation.
pub(super) fn zero_into(dst: &mut [MaybeUninit<u8>]) {
    // SAFETY: writes the whole exclusively borrowed run.
    unsafe { ptr::write_bytes(dst.as_mut_ptr(), 0, dst.len()) };
}

/// Copies `src` into `dst`, of the same length, in one operation.
pub(super) fn copy_into(dst: &mut [MaybeUninit<u8>], src: &[u8]) {
    assert_eq!(
        dst.len(),
        src.len(),
        "source length differs from the payload's"
    );
    // SAFETY: two runs of `src.len()` bytes that cannot overlap, `src`
    // borrowed shared and `dst` exclusively.
    unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast::<u8>(), src.len()) };
}

impl SideDataArray {
    /// The first entry of `kind`, in array order, writable; `None` when
    /// there is none, as [`SideDataArrayMut::find_mut`] finds it, borrowing
    /// the array.
    #[must_use]
    pub fn find_mut(&mut self, kind: SideDataType) -> Option<SideDataMut<'_>> {
        self.as_mut().into_found(kind)
    }

    /// Adds side data of `kind` with `size` zeroed bytes, and returns the
    /// entry holding it, as [`SideDataArrayMut::alloc_zeroed`] does,
    /// borrowing the array.
    pub fn alloc_zeroed(
        &mut self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut().into_new_entry(kind, size, flags, zero_into)
    }

    /// Adds side data of `kind` with a payload copied from `bytes`, and
    /// returns the entry holding it, as [`SideDataArrayMut::alloc_from_slice`]
    /// does, borrowing the array.
    pub fn alloc_from_slice(
        &mut self,
        kind: SideDataType,
        bytes: &[u8],
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut()
            .into_new_entry(kind, bytes.len(), flags, |run| copy_into(run, bytes))
    }
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
mod unit_tests {
    use core::ptr;

    use super::*;
    use crate::libavutil::libavutil::buffer::{BufferReference, BufferReferenceBox};
    use crate::libavutil::libavutil::frame::{SideDataMut, SideDataRef, SideDataType};

    /// A side data array of one entry per payload, kinds `MULTI`.
    fn side_data_array(payloads: &[&[u8]]) -> SideDataArray {
        let mut a = SideDataArray::new();
        for p in payloads {
            let buf = BufferReference::alloc(p.len()).unwrap().copy_from_slice(p);
            let added = a.add(SideDataType::SEI_UNREGISTERED, buf, SideDataFlags::NONE);
            assert!(added.is_ok());
        }
        a
    }

    /// The count of references to the buffer `b` belongs to.
    fn count(b: &BufferReferenceBox) -> u32 {
        b.as_ref().ref_count()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_parts_read_a_null_payload_as_empty() {
        let a = side_data_array(&[b"abc"]);
        let buf = a.as_ref().get(0).unwrap().buffer().as_ptr().cast_mut();
        // An entry in Rust storage pointing at the array's buffer, with a
        // NULL payload of 0 and of 5 bytes; nothing releases it.
        for size in [0, 5] {
            let mut raw = ffi::AVFrameSideData {
                type_: SideDataType::AFD.into_raw(),
                data: ptr::null_mut(),
                size,
                metadata: ptr::null_mut(),
                buf,
            };
            // SAFETY: a live entry in this frame, borrowed for the block.
            let e = unsafe { SideDataRef::from_ptr(&raw mut raw) }.unwrap();
            assert_eq!(e.parts().1, 0);
            assert!(e.data().is_empty() && e.is_empty());
            assert_eq!(e.kind(), SideDataType::AFD);
        }
    }

    #[test]
    fn side_data_array_refs_read_negative_counts_as_empty() {
        for len in [c_int::MIN, -1, 0] {
            // SAFETY: an empty array, by C's reading of the count.
            let r = unsafe { SideDataArrayRef::from_raw_parts(ptr::null(), len) };
            assert!(r.is_empty() && r.get(0).is_none() && r.iter().next().is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_array_round_trips_through_raw_parts() {
        let a = side_data_array(&[b"x", b"", b"yz"]);
        let (sd, nb) = a.into_raw_parts();
        assert_eq!(nb, 3);
        // SAFETY: the array given up just above, unchanged.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        let lens: Vec<usize> = a.as_ref().iter().map(|e| e.len()).collect();
        assert_eq!(lens, [1, 0, 2]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_takes_the_window_of_its_new_reference() {
        let mut a = side_data_array(&[b"old"]);
        let mut buf = BufferReference::alloc(8)
            .unwrap()
            .copy_from_slice(b"01234567");
        buf.as_mut().advance(2);
        buf.as_mut().truncate(3);
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(0).unwrap();
        let old = e.replace_buffer(buf);
        assert_eq!(old.as_ref().data(), b"old");
        assert_eq!(e.as_ref().data(), b"234");
        assert_eq!(e.as_ref().buffer().data(), b"234");
        e.data_mut().unwrap()[0] = b'x';
        assert_eq!(e.as_ref().data(), b"x34");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_array_drop_releases_every_reference_and_dictionary() {
        let mut a = side_data_array(&[b"a", b"bb"]);
        let copies: Vec<BufferReferenceBox> = a
            .as_ref()
            .iter()
            .map(|e| e.buffer().try_clone().unwrap())
            .collect();
        assert!(copies.iter().all(|c| count(c) == 2));
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(1).unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        assert!(e.data_mut().is_none());
        drop(a);
        assert!(copies.iter().all(|c| count(c) == 1));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_debug_names_the_kind_and_sizes() {
        let mut a = side_data_array(&[b"abcd"]);
        a.as_mut()
            .get_mut(0)
            .unwrap()
            .set_metadata(Dict::from_entries([(c"k", c"v"), (c"l", c"w")]).unwrap());
        assert_eq!(
            format!("{a:?}"),
            "[SideData { kind: SEI_UNREGISTERED, len: 4, metadata: 2, .. }]"
        );
        let mut entries = a.as_mut();
        assert_eq!(
            format!("{entries:?}"),
            format!("{a_dbg:?}", a_dbg = entries.as_ref())
        );
        let e = entries.get_mut(0).unwrap();
        assert!(format!("{e:?}").starts_with("SideData {"));
        let it = SideDataArray::new();
        assert_eq!(format!("{it:?}"), "[]");
    }

    /// The array handles and iterators over entries in Rust storage, whose
    /// kinds alone are read and written (no C call, so Miri runs it): every
    /// exclusive handle the iterators lend reaches its own entry, and the
    /// array's variables stay readable through the shared handle after.
    #[test]
    fn side_data_array_handles_lend_each_entry_once() {
        let blank = |t: u32| ffi::AVFrameSideData {
            type_: ffi::AVFrameSideDataType(t),
            data: ptr::null_mut(),
            size: 0,
            metadata: ptr::null_mut(),
            buf: ptr::null_mut(),
        };
        let mut entries = [blank(1), blank(2), blank(3)];
        let mut pointers: [*mut ffi::AVFrameSideData; 3] =
            core::array::from_fn(|i| (&raw mut entries[i]));
        let mut sd = pointers.as_mut_ptr();
        let mut nb: c_int = 3;
        let mut array = SideDataArrayMut {
            entries: NonNull::from(&mut sd),
            len: NonNull::from(&mut nb),
            _borrow: PhantomData,
        };
        let mut lent: Vec<SideDataMut<'_>> = array.iter_mut().collect();
        for (i, e) in lent.iter_mut().enumerate().rev() {
            e.set_kind(SideDataType::from_raw(ffi::AVFrameSideDataType(
                10 + i as u32,
            )));
        }
        drop(lent);
        let mut back = array.as_mut().into_iter();
        back.next_back().unwrap().set_kind(SideDataType::AFD);
        array.get_mut(0).unwrap().set_kind(SideDataType::A53_CC);
        assert!(array.get_mut(3).is_none());
        let kinds: Vec<u32> = array
            .as_ref()
            .iter()
            .map(|e| e.kind().into_raw().0)
            .collect();
        let afd = SideDataType::AFD.into_raw().0;
        let a53 = SideDataType::A53_CC.into_raw().0;
        assert_eq!(kinds, [a53, 11, afd]);
        assert_eq!(array.len(), 3);
        assert!(array.as_ref().iter().rev().all(|e| e.data().is_empty()));
    }

    #[test]
    fn side_data_flags_are_cs_bits() {
        assert_eq!(SideDataFlags::NONE.bits(), 0);
        assert_eq!(
            SideDataFlags::UNIQUE.bits(),
            ffi::AV_FRAME_SIDE_DATA_FLAG_UNIQUE
        );
        assert_eq!(
            SideDataFlags::REPLACE.bits(),
            ffi::AV_FRAME_SIDE_DATA_FLAG_REPLACE
        );
        assert_eq!(SideDataFlags::default(), SideDataFlags::NONE);
    }

    /// `alloc_zeroed` replacing an entry, when its allocation fails, returns
    /// `None` and leaves the entry as it was: C's `av_frame_side_data_new`
    /// would free the entry's buffer and metadata and then dereference the
    /// NULL buffer (`replace_side_data_from_buf`), so the wrapper allocates
    /// first (a deliberate correction of C).
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_alloc_replacing_with_a_failed_allocation_keeps_the_entry() {
        let mut a = SideDataArray::new();
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, &[7], SideDataFlags::NONE)
            .unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        // More than `av_max_alloc`'s default `INT_MAX` limit, which fails
        // without touching the process-wide limit other tests share.
        let huge = 1 << 40;
        assert!(
            a.alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        let mut entries = a.as_mut();
        assert!(
            entries
                .alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        let e = a.as_ref().find(SideDataType::AFD).unwrap();
        assert_eq!(e.data(), [7]);
        assert_eq!(e.metadata().map(|m| m.len()), Some(1));
        assert_eq!(a.len(), 1);
        // Without an entry to replace, C's own path fails cleanly.
        let multi = SideDataType::SEI_UNREGISTERED;
        assert!(
            a.alloc_zeroed(multi, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        assert!(
            a.alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::UNIQUE)
                .is_none()
        );
        assert!(a.is_empty(), "UNIQUE removed the entry before failing");
    }

    /// `alloc_*` replacing an entry goes through `av_frame_side_data_add`
    /// with a buffer it filled: the entry keeps its place and kind, takes the
    /// new payload, loses its metadata and releases its old reference.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_alloc_replacing_takes_a_filled_buffer() {
        let mut a = side_data_array(&[b"m"]);
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, b"old", SideDataFlags::NONE)
            .unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        let old = e.as_ref().buffer().try_clone().unwrap();
        assert_eq!(count(&old), 2);
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, b"new!", SideDataFlags::REPLACE)
            .unwrap();
        assert_eq!(e.data_mut().unwrap(), b"new!");
        assert_eq!(count(&old), 1);
        let mut entries = a.as_mut();
        let e = entries
            .alloc_zeroed(SideDataType::AFD, 3, SideDataFlags::REPLACE)
            .unwrap();
        assert_eq!(e.as_ref().data(), [0; 3]);
        assert!(e.as_ref().metadata().is_none());
        let kinds: Vec<_> = a.as_ref().iter().map(|e| e.kind()).collect();
        assert_eq!(kinds, [SideDataType::SEI_UNREGISTERED, SideDataType::AFD]);
        assert_eq!(old.as_ref().data(), b"old");
    }

    /// `add_clone` refuses at `MAX_REFERENCES` without calling C, leaving
    /// the count and the array, and clones one below it.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_clones_stop_at_the_reference_limit() {
        use crate::libavutil::libavutil::buffer::MAX_REFERENCES;
        use crate::libavutil::libavutil::buffer::unit_tests::{counts, set_count};
        let src = side_data_array(&[b"abc"]);
        let held = src.as_ref().get(0).unwrap().buffer().try_clone().unwrap();
        let mut dst = SideDataArray::new();
        let entry = src.as_ref().get(0).unwrap();
        set_count(&held, MAX_REFERENCES);
        assert_eq!(
            dst.as_mut().add_clone(entry, SideDataFlags::NONE),
            Err(ENOMEM)
        );
        assert_eq!(counts(&held).1, MAX_REFERENCES);
        assert!(dst.is_empty());
        set_count(&held, MAX_REFERENCES - 1);
        assert_eq!(dst.as_mut().add_clone(entry, SideDataFlags::NONE), Ok(()));
        assert_eq!(counts(&held).1, MAX_REFERENCES);
        set_count(&held, 3);
        assert_eq!(counts(&held), (3, 3));
        drop(dst);
        assert_eq!(count(&held), 2);
    }

    /// `find` on a handle whose count C stored negative reads no pointer,
    /// as C's loop does not.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn side_data_find_reads_negative_counts_as_empty() {
        // SAFETY: a dangling, never-read block, and a negative count that
        // the handle reads as empty.
        let r = unsafe { SideDataArrayRef::from_raw_parts(NonNull::dangling().as_ptr(), -3) };
        assert!(r.find(SideDataType::AFD).is_none());
    }

    #[test]
    fn side_data_payload_initialisers_write_every_byte() {
        let mut run = [MaybeUninit::new(0xA5u8); 9];
        zero_into(&mut run);
        // SAFETY: every byte is initialised, before and after.
        assert_eq!(unsafe { run.map(|b| b.assume_init()) }, [0; 9]);
        copy_into(&mut run, b"012345678");
        // SAFETY: as above.
        assert_eq!(unsafe { run.map(|b| b.assume_init()) }, *b"012345678");
        zero_into(&mut []);
        copy_into(&mut [], &[]);
    }

    #[test]
    #[should_panic(expected = "source length differs")]
    fn side_data_payload_copies_need_equal_lengths() {
        copy_into(&mut [MaybeUninit::uninit(); 2], b"abc");
    }
}
