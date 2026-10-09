//! Safe wrappers over `libavutil/bprint.h`.
//!
//! [`PaddedStruct`] describes the structures `FF_PAD_STRUCTURE` defines, of
//! which `AVBPrint` is the one instance.
//!
//! [`BPrintFields`] is the generator's `struct ff_pad_helper_AVBPrint`:
//! `AVBPrint`'s fields without the padding, which C only measures with
//! `sizeof`. It is a plain value Rust owns; no C routine allocates, reads,
//! writes or frees one.

use core::ffi::{CStr, c_char};
use core::fmt;
use core::mem::{align_of, offset_of, size_of};
use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CCell, CZeroable, define_ctype};

use super::ffi;

mod sealed {
    pub trait Sealed {}
}

/// Wraps: FF_PAD_STRUCTURE
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
/// use ffmpeg::libavutil::bprint::{BPrintFields, PaddedStruct};
/// struct Unpadded([u8; 8]);
/// impl PaddedStruct for Unpadded {
///     const SIZE: usize = 4;
///     type Fields = BPrintFields;
/// }
/// ```
///
/// ```
/// use ffmpeg::libavutil::{bprint::PaddedStruct, ffi};
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
    const SIZE: usize = 1024;
    type Fields = BPrintFields;
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
    /// Field: ff_pad_helper_AVBPrint.str
    ///
    /// The string the value borrows, or `None` when `str` is NULL (as in a
    /// zeroed value).
    #[must_use]
    pub fn str(&self) -> Option<&'a CStr> {
        // SAFETY: a read of the `str` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`.
        let str = unsafe { addr_of!((*self.as_ptr()).str_).read() };
        // SAFETY: every writer of `str` leaves it NULL (`zeroed`) or the
        // address of a NUL-terminated string that stays valid and unwritten
        // while this value or a copy can read it: a `&'static CStr`
        // (`set_str`), or one `set_str_borrowed`'s caller keeps alive. The
        // handle borrows the value for `'a`, so the string outlives `'a`.
        NonNull::new(str).map(|p| unsafe { CStr::from_ptr(p.as_ptr()) })
    }

    /// Field: ff_pad_helper_AVBPrint.len
    ///
    /// The `len` field: in an `AVBPrint`, the length of everything printed,
    /// which may exceed `size`.
    #[must_use]
    #[allow(clippy::len_without_is_empty)] // a stored count, not a container
    pub fn len(&self) -> u32 {
        // SAFETY: a read of the `len` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`;
        // every bit pattern is a valid `unsigned`.
        unsafe { addr_of!((*self.as_ptr()).len).read() }
    }

    /// Field: ff_pad_helper_AVBPrint.size
    ///
    /// The `size` field: in an `AVBPrint`, the bytes of storage `str` has.
    #[must_use]
    pub fn size(&self) -> u32 {
        // SAFETY: a read of the `size` field projected through the shared
        // handle's pointer to a live, initialised `ff_pad_helper_AVBPrint`;
        // every bit pattern is a valid `unsigned`.
        unsafe { addr_of!((*self.as_ptr()).size).read() }
    }

    /// Field: ff_pad_helper_AVBPrint.size_max
    ///
    /// The `size_max` field: in an `AVBPrint`, the most storage `str` may
    /// grow to.
    #[must_use]
    pub fn size_max(&self) -> u32 {
        // SAFETY: a read of the `size_max` field projected through the
        // shared handle's pointer to a live, initialised
        // `ff_pad_helper_AVBPrint`; every bit pattern is a valid `unsigned`.
        unsafe { addr_of!((*self.as_ptr()).size_max).read() }
    }

    /// Field: ff_pad_helper_AVBPrint.reserved_internal_buffer
    ///
    /// The one byte of `reserved_internal_buffer`: in an `AVBPrint`, the
    /// start of the storage that runs on through the padding.
    #[must_use]
    pub fn internal_buffer(&self) -> [u8; 1] {
        // SAFETY: a read of the `reserved_internal_buffer` field projected
        // through the shared handle's pointer to a live, initialised
        // `ff_pad_helper_AVBPrint`; every bit pattern is a valid `char`.
        let [b] = unsafe { addr_of!((*self.as_ptr()).reserved_internal_buffer).read() };
        [b as u8]
    }
}

impl BPrintFieldsMut<'_> {
    /// Field: ff_pad_helper_AVBPrint.str
    ///
    /// Points `str` at `s`, or NULL for `None`. The value only borrows the
    /// string, which outlives it; the old one is not freed.
    pub fn set_str(&mut self, s: Option<&'static CStr>) {
        // SAFETY: `s` lives for the whole program and `CStr` is never
        // written, which is all `set_str_borrowed` asks.
        unsafe { self.set_str_borrowed(s) }
    }

    /// Field: ff_pad_helper_AVBPrint.str
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
        let str = s.map_or(core::ptr::null_mut(), |s| s.as_ptr().cast_mut());
        // SAFETY: a write of the `str` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // is a borrow, so nothing leaks, and the caller keeps the new string
        // alive for as long as `str()` can return it.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).str_).write(str) }
    }

    /// Field: ff_pad_helper_AVBPrint.len
    ///
    /// Sets `len`, leaving the other fields as they are.
    pub fn set_len(&mut self, len: u32) {
        // SAFETY: a write of the `len` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).len).write(len) }
    }

    /// Field: ff_pad_helper_AVBPrint.size
    ///
    /// Sets `size`, leaving the other fields as they are.
    pub fn set_size(&mut self, size: u32) {
        // SAFETY: a write of the `size` field projected through the exclusive
        // handle's pointer to a live `ff_pad_helper_AVBPrint`; the old value
        // owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).size).write(size) }
    }

    /// Field: ff_pad_helper_AVBPrint.size_max
    ///
    /// Sets `size_max`, leaving the other fields as they are.
    pub fn set_size_max(&mut self, size_max: u32) {
        // SAFETY: a write of the `size_max` field projected through the
        // exclusive handle's pointer to a live `ff_pad_helper_AVBPrint`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).size_max).write(size_max) }
    }

    /// Field: ff_pad_helper_AVBPrint.reserved_internal_buffer
    ///
    /// Sets the one byte of `reserved_internal_buffer`.
    pub fn set_internal_buffer(&mut self, buffer: [u8; 1]) {
        let [b] = buffer;
        // SAFETY: a write of the `reserved_internal_buffer` field projected
        // through the exclusive handle's pointer to a live
        // `ff_pad_helper_AVBPrint`; the old byte owns nothing to leak.
        unsafe {
            addr_of_mut!((*self.as_mut_ptr()).reserved_internal_buffer).write([b as c_char])
        }
    }
}

#[cfg(test)]
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
        for f in [BPrintFields::new(), BPrintFields::default(), BPrintFields::zeroed()] {
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
        f.as_mut().set_internal_buffer([0xff]);
        let r = f.as_ref();
        assert_eq!(r.str(), Some(c"so far"));
        assert_eq!((r.len(), r.size(), r.size_max()), (u32::MAX, 1004, 1));
        assert_eq!(r.internal_buffer(), [0xff]);
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
}
