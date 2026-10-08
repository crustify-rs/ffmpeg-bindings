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
//!
//! The `av_alias*` unions view one aligned machine word as an integer, as
//! narrower integer lanes, as bytes, or as a float: the type punning the
//! aligned `AV_RN*A` / `AV_WN*A` macros perform through a cast pointer. They
//! own nothing, have no C constructor or destructor, and every bit pattern is
//! a valid value of every view, so each is a plain `Copy` value:
//!
//! - [`Alias16`] (`av_alias16`): a `u16`, or two bytes;
//! - [`Alias32`] (`av_alias32`): a `u32`, two `u16` lanes, four bytes, or an
//!   `f32`.
//!
//! Build one with `zeroed()` / `default()` or a `from_*` constructor, read the
//! views on its shared handle and write them on its exclusive one. Lanes and
//! bytes are in native order, as `to_ne_bytes` lays them out, because the
//! views share one storage. A C-allocated one is a
//! [`HeapBox`](super::mem::HeapBox), and an array a
//! [`HeapVec`](super::mem::HeapVec), reached through the same handles.

use core::fmt;
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

define_ctype!(
    /// Wraps: av_alias16
    ///
    /// Two bytes viewed as a native-endian `u16` or as bytes. Every view
    /// covers both bytes, so a write through one is wholly visible through
    /// the other.
    #[derive(Clone, Copy)]
    Alias16,
    Alias16Ref,
    Alias16Mut,
    ffi::av_alias16
);

// SAFETY: every view of `av_alias16` is a plain integer array or integer, so
// all-zero is a valid value of each, and no teardown or C routine is paired
// with it.
unsafe impl CZeroable for Alias16 {}

// SAFETY: `av_alias16` is plain data: no pointer, no interior mutability, no
// thread-affine C state, so moving it, or an exclusive handle to it, to
// another thread moves nothing a thread owns.
unsafe impl Send for Alias16 {}
// SAFETY: as above.
unsafe impl Send for Alias16Mut<'_> {}
// SAFETY: the only operations on a shared `Alias16`, `Alias16Ref` or
// `&Alias16Mut` are reads; every write needs `Alias16Mut` through `&mut`, so
// shared access from several threads never races.
unsafe impl Sync for Alias16 {}
// SAFETY: as above.
unsafe impl Send for Alias16Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias16Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias16Mut<'_> {}

impl Alias16 {
    /// A value whose `u16` view is `v`.
    #[must_use]
    pub fn from_u16(v: u16) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u16(v);
        a
    }

    /// A value whose byte view is `v`.
    #[must_use]
    pub fn from_u8x2(v: [u8; 2]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u8x2(v);
        a
    }
}

impl Default for Alias16 {
    /// All-zero.
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Bitwise equality.
impl PartialEq for Alias16 {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().u16() == other.as_ref().u16()
    }
}

impl Eq for Alias16 {}

impl fmt::Debug for Alias16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for Alias16Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Alias16")
            .field("u8x2", &self.u8x2())
            .finish()
    }
}

impl Alias16Ref<'_> {
    /// Field: av_alias16.u16
    ///
    /// Both bytes as a native-endian `u16`.
    #[must_use]
    pub fn u16(&self) -> u16 {
        // SAFETY: the handle addresses a live, initialised `av_alias16`, and
        // `u16` covers all of its bytes, any pattern of which is a `u16`.
        unsafe { addr_of!((*self.as_ptr()).u16_).read() }
    }

    /// Field: av_alias16.u8
    ///
    /// Both bytes, in memory order.
    #[must_use]
    pub fn u8x2(&self) -> [u8; 2] {
        // SAFETY: as `u16`, through the byte view.
        unsafe { addr_of!((*self.as_ptr()).u8_).read() }
    }
}

impl Alias16Mut<'_> {
    /// Field: av_alias16.u16
    ///
    /// Stores `v` over both bytes, in native byte order.
    pub fn set_u16(&mut self, v: u16) {
        // SAFETY: a write of the whole object through the exclusive handle's
        // pointer to a live `av_alias16`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u16_).write(v) }
    }

    /// Field: av_alias16.u8
    ///
    /// Stores both bytes, in memory order.
    pub fn set_u8x2(&mut self, v: [u8; 2]) {
        // SAFETY: as `set_u16`, through the byte view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u8_).write(v) }
    }
}

define_ctype!(
    /// Wraps: av_alias32
    ///
    /// Four bytes viewed as a native-endian `u32`, two `u16` lanes, bytes, or
    /// an `f32`. Every view covers all four bytes, so a write through one is
    /// wholly visible through the others.
    #[derive(Clone, Copy)]
    Alias32,
    Alias32Ref,
    Alias32Mut,
    ffi::av_alias32
);

// SAFETY: every view of `av_alias32` is a plain integer, integer array or
// float, so all-zero is a valid value of each, and no teardown or C routine is
// paired with it.
unsafe impl CZeroable for Alias32 {}

// SAFETY: `av_alias32` is plain data: no pointer, no interior mutability, no
// thread-affine C state, so moving it, or an exclusive handle to it, to
// another thread moves nothing a thread owns.
unsafe impl Send for Alias32 {}
// SAFETY: as above.
unsafe impl Send for Alias32Mut<'_> {}
// SAFETY: the only operations on a shared `Alias32`, `Alias32Ref` or
// `&Alias32Mut` are reads; every write needs `Alias32Mut` through `&mut`, so
// shared access from several threads never races.
unsafe impl Sync for Alias32 {}
// SAFETY: as above.
unsafe impl Send for Alias32Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias32Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias32Mut<'_> {}

impl Alias32 {
    /// A value whose `u32` view is `v`.
    #[must_use]
    pub fn from_u32(v: u32) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u32(v);
        a
    }

    /// A value whose `u16` lanes are `v`.
    #[must_use]
    pub fn from_u16x2(v: [u16; 2]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u16x2(v);
        a
    }

    /// A value whose byte view is `v`.
    #[must_use]
    pub fn from_u8x4(v: [u8; 4]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u8x4(v);
        a
    }

    /// A value whose `f32` view is `v`, bit for bit.
    #[must_use]
    pub fn from_f32(v: f32) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_f32(v);
        a
    }
}

impl Default for Alias32 {
    /// All-zero.
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Bitwise equality: a NaN equals itself, and `0.0` differs from `-0.0`.
impl PartialEq for Alias32 {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().u32() == other.as_ref().u32()
    }
}

impl Eq for Alias32 {}

impl fmt::Debug for Alias32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for Alias32Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Alias32")
            .field("u8x4", &self.u8x4())
            .finish()
    }
}

impl Alias32Ref<'_> {
    /// Field: av_alias32.u32
    ///
    /// All four bytes as a native-endian `u32`.
    #[must_use]
    pub fn u32(&self) -> u32 {
        // SAFETY: the handle addresses a live, initialised `av_alias32`, and
        // `u32` covers all of its bytes, any pattern of which is a `u32`.
        unsafe { addr_of!((*self.as_ptr()).u32_).read() }
    }

    /// Field: av_alias32.u16
    ///
    /// The two native-endian `u16` lanes, in memory order.
    #[must_use]
    pub fn u16x2(&self) -> [u16; 2] {
        // SAFETY: as `u32`, through the lane view.
        unsafe { addr_of!((*self.as_ptr()).u16_).read() }
    }

    /// Field: av_alias32.u8
    ///
    /// All four bytes, in memory order.
    #[must_use]
    pub fn u8x4(&self) -> [u8; 4] {
        // SAFETY: as `u32`, through the byte view.
        unsafe { addr_of!((*self.as_ptr()).u8_).read() }
    }

    /// Field: av_alias32.f32
    ///
    /// All four bytes as an `f32`, bit for bit (`f32::from_bits(self.u32())`).
    #[must_use]
    pub fn f32(&self) -> f32 {
        // SAFETY: as `u32`, through the float view; every bit pattern is an
        // `f32`.
        unsafe { addr_of!((*self.as_ptr()).f32_).read() }
    }
}

impl Alias32Mut<'_> {
    /// Field: av_alias32.u32
    ///
    /// Stores `v` over all four bytes, in native byte order.
    pub fn set_u32(&mut self, v: u32) {
        // SAFETY: a write of the whole object through the exclusive handle's
        // pointer to a live `av_alias32`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u32_).write(v) }
    }

    /// Field: av_alias32.u16
    ///
    /// Stores both `u16` lanes, in memory order.
    pub fn set_u16x2(&mut self, v: [u16; 2]) {
        // SAFETY: as `set_u32`, through the lane view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u16_).write(v) }
    }

    /// Field: av_alias32.u8
    ///
    /// Stores all four bytes, in memory order.
    pub fn set_u8x4(&mut self, v: [u8; 4]) {
        // SAFETY: as `set_u32`, through the byte view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u8_).write(v) }
    }

    /// Field: av_alias32.f32
    ///
    /// Stores `v`'s bits over all four bytes; a NaN keeps its payload.
    pub fn set_f32(&mut self, v: f32) {
        // SAFETY: as `set_u32`, through the float view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).f32_).write(v) }
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

    #[test]
    fn alias16_constructors_set_the_named_view() {
        assert_eq!(Alias16::from_u16(0x1234).as_ref().u16(), 0x1234);
        assert_eq!(
            Alias16::from_u8x2([0xab, 0xcd]).as_ref().u8x2(),
            [0xab, 0xcd]
        );
        assert_eq!(
            Alias16::from_u16(0x1234).as_ref().u8x2(),
            0x1234u16.to_ne_bytes()
        );
        assert_eq!(Alias16::default().as_ref().u16(), 0);
    }

    #[test]
    fn alias32_constructors_set_the_named_view() {
        assert_eq!(Alias32::from_u32(0xdead_beef).as_ref().u32(), 0xdead_beef);
        assert_eq!(Alias32::from_u16x2([1, 2]).as_ref().u16x2(), [1, 2]);
        assert_eq!(
            Alias32::from_u8x4([1, 2, 3, 4]).as_ref().u8x4(),
            [1, 2, 3, 4]
        );
        assert_eq!(Alias32::from_f32(1.5).as_ref().f32(), 1.5);
        assert_eq!(Alias32::from_f32(1.5).as_ref().u32(), 1.5f32.to_bits());
        assert_eq!(Alias32::default().as_ref().u32(), 0);
    }

    #[test]
    fn equality_is_bitwise() {
        assert_eq!(Alias16::from_u16(7), Alias16::from_u8x2(7u16.to_ne_bytes()));
        assert_ne!(Alias16::from_u16(7), Alias16::from_u16(8));

        let nan = Alias32::from_f32(f32::NAN);
        assert_eq!(nan, nan);
        assert_ne!(Alias32::from_f32(0.0), Alias32::from_f32(-0.0));
        assert_eq!(
            Alias32::from_u32(0x0102_0304),
            Alias32::from_u8x4(0x0102_0304u32.to_ne_bytes())
        );
    }

    #[test]
    fn nan_payload_survives_the_float_view() {
        let bits = 0x7fc0_1234;
        let a = Alias32::from_f32(f32::from_bits(bits));
        assert_eq!(a.as_ref().u32(), bits);
        assert_eq!(a.as_ref().f32().to_bits(), bits);
    }

    #[test]
    fn debug_prints_the_bytes() {
        let a = Alias16::from_u8x2([1, 2]);
        assert_eq!(format!("{a:?}"), "Alias16 { u8x2: [1, 2] }");
        assert_eq!(format!("{:?}", a.as_ref()), "Alias16 { u8x2: [1, 2] }");
        let b = Alias32::from_u8x4([1, 2, 3, 4]);
        assert_eq!(format!("{b:?}"), "Alias32 { u8x4: [1, 2, 3, 4] }");
        assert_eq!(
            format!("{:?}", b.as_ref()),
            "Alias32 { u8x4: [1, 2, 3, 4] }"
        );
    }

    #[test]
    fn copies_are_independent() {
        let a = Alias32::from_u32(1);
        let mut b = a;
        b.as_mut().set_u32(2);
        assert_eq!(a.as_ref().u32(), 1);
        assert_eq!(b.as_ref().u32(), 2);
    }

    #[test]
    fn handles_over_a_c_pointer_read_and_write_it() {
        let mut raw = ffi::av_alias32 { u32_: 0x0a0b_0c0d };
        // SAFETY: `raw` is a live, initialised `av_alias32` that outlives the
        // handle, and nothing else touches it while the handle lives.
        let mut m = unsafe { Alias32Mut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(m.as_ref().u32(), 0x0a0b_0c0d);
        m.set_u16x2([5, 6]);
        // SAFETY: the handle is gone; every field of `raw` covers its bytes.
        let lanes = unsafe { raw.u16_ };
        assert_eq!(lanes, [5, 6]);
        // SAFETY: a null pointer is accepted and yields no handle.
        assert!(unsafe { Alias16Ref::from_ptr(core::ptr::null_mut()) }.is_none());
    }
}
