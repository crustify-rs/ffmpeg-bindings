//! Safe wrappers over `libavutil/bprint.h`.
//!
//! [`PaddedStruct`] describes the structures `FF_PAD_STRUCTURE` defines, of
//! which `AVBPrint` is the one instance.

use core::mem::{align_of, offset_of, size_of};

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
/// Sealed: implemented for each instance the generator defines, with the
/// constants checked against the instance's binding at compile time. Outside
/// the crate, a type cannot claim them:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::bprint::PaddedStruct;
/// struct Unpadded([u8; 8]);
/// impl PaddedStruct for Unpadded {
///     const SIZE: usize = 4;
///     const FIELDS_SIZE: usize = 8;
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

    /// `sizeof(struct ff_pad_helper_<name>)`: the size of the fields alone,
    /// trailing alignment included.
    const FIELDS_SIZE: usize;

    /// The length of `reserved_padding`.
    const PADDING_LEN: usize = Self::SIZE - Self::FIELDS_SIZE;
}

impl sealed::Sealed for ffi::AVBPrint {}

/// `FF_PAD_STRUCTURE(AVBPrint, 1024, ...)`.
impl PaddedStruct for ffi::AVBPrint {
    const SIZE: usize = 1024;
    const FIELDS_SIZE: usize = size_of::<ffi::ff_pad_helper_AVBPrint>();
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
    }
}
