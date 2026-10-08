//! Safe wrappers over `libavutil/intreadwrite.h`.
//!
//! [`Unaligned64`] is `union unaligned_64`, the packed (align 1) view that
//! `AV_RN64` / `AV_WN64` cast a byte pointer to for one native-endian 64-bit
//! access at any address. C never allocates, stores or frees one: it is a
//! plain value, or a borrowed view over 8 bytes the caller owns. Both reach
//! the same handles:
//!
//! - a value: [`Unaligned64::new`] (or `Unaligned64::zeroed()`), then
//!   `as_ref()` / `as_mut()`;
//! - 8 bytes anywhere in a buffer: [`Unaligned64Ref::from_bytes`] and
//!   [`Unaligned64Mut::from_bytes`], as `AV_RN64(p)` and `AV_WN64(p, v)` do.
//!
//! [`Unaligned64Ref::value`] reads and [`Unaligned64Mut::set_value`] writes the
//! `l` field without assuming alignment.

use core::ptr::{self, addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: unaligned_64
    ///
    /// A native-endian `u64` stored with alignment 1. It owns nothing, so it
    /// copies as a value.
    #[derive(Clone, Copy)]
    Unaligned64,
    Unaligned64Ref,
    Unaligned64Mut,
    ffi::unaligned_64
);

// SAFETY: the union's one field is a `u64`, for which all-zero (like every
// bit pattern) is a valid value; there is no teardown.
unsafe impl CZeroable for Unaligned64 {}

// SAFETY: an `Unaligned64` is 8 bytes of plain data with no C-side identity,
// no pointer and no teardown, so moving it, or the exclusive handle borrowing
// it, to another thread is moving a `u64` or a `&mut [u8; 8]`.
unsafe impl Send for Unaligned64 {}
// SAFETY: as above.
unsafe impl Send for Unaligned64Mut<'_> {}
// SAFETY: no interior mutability: the only write path is
// `Unaligned64Mut::set_value`, which needs the exclusive handle, so shared
// handles on several threads only read, like `&u64`.
unsafe impl Sync for Unaligned64 {}
// SAFETY: as above.
unsafe impl Send for Unaligned64Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Unaligned64Ref<'_> {}
// SAFETY: a shared `&Unaligned64Mut` reaches only the getters, through
// `as_ref`.
unsafe impl Sync for Unaligned64Mut<'_> {}

impl Unaligned64 {
    /// A value holding `value`.
    #[inline]
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(ffi::unaligned_64 { l: value }, core::marker::PhantomData)
    }
}

impl From<u64> for Unaligned64 {
    #[inline]
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl<'a> Unaligned64Ref<'a> {
    /// A view of `bytes` as an `Unaligned64`, at whatever alignment they have:
    /// the cast `AV_RN64(p)` reads through. Take 8 bytes out of a slice with
    /// `<&[u8; 8]>::try_from(&buf[i..i + 8])`; fewer do not make a view:
    ///
    /// ```compile_fail,E0308
    /// use ffmpeg::libavutil::intreadwrite::Unaligned64Ref;
    /// let _ = Unaligned64Ref::from_bytes(&[0u8; 4]);
    /// ```
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a [u8; 8]) -> Self {
        let ptr = ptr::from_ref(bytes).cast::<ffi::unaligned_64>().cast_mut();
        // SAFETY: `ptr` addresses 8 initialised bytes, borrowed shared for
        // `'a`; `unaligned_64` has size 8 and alignment 1, and every bit
        // pattern is a valid one. The shared handle never writes, so the
        // `cast_mut` grants no write.
        let handle = unsafe { Self::from_ptr(ptr) };
        handle.expect("a reference is never null")
    }
}

impl Unaligned64Ref<'_> {
    /// Field: unaligned_64.l
    ///
    /// The stored value, read without assuming alignment (`AV_RN64`). The
    /// shared handle has no setter:
    ///
    /// ```compile_fail,E0599
    /// use ffmpeg::libavutil::intreadwrite::Unaligned64Ref;
    /// Unaligned64Ref::from_bytes(&[0; 8]).set_value(1);
    /// ```
    #[inline]
    #[must_use]
    pub fn value(&self) -> u64 {
        // SAFETY: the handle addresses a live `unaligned_64`; the projected
        // field of the packed union may be unaligned, hence `read_unaligned`.
        unsafe { addr_of!((*self.as_ptr()).l).read_unaligned() }
    }
}

impl<'a> Unaligned64Mut<'a> {
    /// An exclusive view of `bytes` as an `Unaligned64`, at whatever alignment
    /// they have: the cast `AV_WN64(p, v)` writes through.
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a mut [u8; 8]) -> Self {
        let ptr = ptr::from_mut(bytes).cast::<ffi::unaligned_64>();
        // SAFETY: `ptr` addresses 8 initialised bytes, borrowed exclusively
        // for `'a`, so no other handle reaches them while this one lives;
        // `unaligned_64` has size 8 and alignment 1, and every bit pattern is
        // a valid one.
        let handle = unsafe { Self::from_ptr(ptr) };
        handle.expect("a reference is never null")
    }

    /// Field: unaligned_64.l
    ///
    /// Stores `value`, without assuming alignment (`AV_WN64`).
    #[inline]
    pub fn set_value(&mut self, value: u64) {
        // SAFETY: the exclusive handle addresses a live, writable
        // `unaligned_64`; the projected field of the packed union may be
        // unaligned, hence `write_unaligned`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).l).write_unaligned(value) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn layout_matches_the_c_union() {
        assert_eq!(size_of::<Unaligned64>(), 8);
        assert_eq!(align_of::<Unaligned64>(), 1);
        assert_eq!(size_of::<Unaligned64Ref<'_>>(), size_of::<*const u8>());
        assert_eq!(size_of::<Unaligned64Mut<'_>>(), size_of::<*const u8>());
        assert_eq!(Unaligned64::zeroed().as_ref().value(), 0);
    }

    #[test]
    fn value_round_trips_through_the_union() {
        let mut v = Unaligned64::from(0x0102_0304_0506_0708);
        assert_eq!(v.as_ref().value(), 0x0102_0304_0506_0708);
        v.as_mut().set_value(u64::MAX);
        // SAFETY: a read of the union's only field of a live local value.
        assert_eq!(unsafe { v.0.l }, u64::MAX);
        const C: Unaligned64 = Unaligned64::new(7);
        let copy = C;
        assert_eq!((C.as_ref().value(), copy.as_ref().value()), (7, 7));
    }

    #[test]
    fn handles_address_the_borrowed_bytes() {
        let mut buf = [0u8; 8];
        let p = buf.as_ptr();
        assert_eq!(Unaligned64Ref::from_bytes(&buf).as_ptr().cast(), p);
        let mut m = Unaligned64Mut::from_bytes(&mut buf);
        assert_eq!(m.as_mut_ptr().cast_const().cast(), p);
        m.set_value(u64::from_ne_bytes(*b"abcdefgh"));
        assert_eq!(&buf, b"abcdefgh");
    }

    #[test]
    fn every_offset_reads_and_writes_native_endian() {
        let mut buf = [0u8; 23];
        for off in 0..16 {
            let v = 0x8877_6655_4433_2211u64.rotate_left(off as u32 * 8);
            let window: &mut [u8; 8] = (&mut buf[off..off + 8]).try_into().unwrap();
            Unaligned64Mut::from_bytes(window).set_value(v);
            assert_eq!(buf[off..off + 8], v.to_ne_bytes());
            let window: &[u8; 8] = buf[off..off + 8].try_into().unwrap();
            assert_eq!(Unaligned64Ref::from_bytes(window).value(), v);
        }
    }
}
