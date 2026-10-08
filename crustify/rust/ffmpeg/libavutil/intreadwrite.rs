//! Safe wrappers over `libavutil/intreadwrite.h`.
//!
//! The header's `unaligned_N` unions are packed (align 1), single-member
//! `uintN_t` containers: `AV_RN(N, p)` / `AV_WN(N, p, v)` cast any byte
//! pointer to one and read or store its member `l` in native byte order,
//! whatever the address's alignment. C never allocates, stores or frees one,
//! and none has a lifecycle routine: each is a plain value, or a borrowed
//! view over bytes the caller owns. Both reach the same handles:
//!
//! - a value: [`Unaligned16::new`], [`Unaligned32::new`],
//!   [`Unaligned64::new`] (or `zeroed()`), then `as_ref()` / `as_mut()`;
//! - `N / 8` bytes anywhere in a buffer: `from_bytes` on the shared or the
//!   exclusive handle ([`Unaligned32Ref::from_bytes`],
//!   [`Unaligned32Mut::from_bytes`]), as `AV_RN` and `AV_WN` do.
//!
//! The shared handles read `l` ([`Unaligned32Ref::value`]) and the exclusive
//! ones store it ([`Unaligned32Mut::set_value`]), without assuming alignment.
//! An `av_malloc`-family block of one is a [`HeapBox`](super::mem::HeapBox),
//! zeroed and `Clone` like any plain value.
//!
//! The `av_alias*` unions view one aligned machine word as an integer, as
//! narrower integer lanes, as bytes, or as a float: the type punning the
//! aligned `AV_RN*A` / `AV_WN*A` macros perform through a cast pointer. They
//! own nothing, have no C constructor or destructor, and every bit pattern is
//! a valid value of every view, so each is a plain `Copy` value:
//!
//! - [`Alias16`] (`av_alias16`): a `u16`, or two bytes;
//! - [`Alias32`] (`av_alias32`): a `u32`, two `u16` lanes, four bytes, or an
//!   `f32`;
//! - [`Alias64`] (`av_alias64`): a `u64`, two `u32` lanes, four `u16` lanes,
//!   eight bytes, an `f64`, or two `f32` lanes.
//!
//! Build one with `zeroed()` / `default()` or a `from_*` constructor, read the
//! views on its shared handle and write them on its exclusive one. Lanes and
//! bytes are in native order, as `to_ne_bytes` lays them out, because the
//! views share one storage. A C-allocated one is a
//! [`HeapBox`](super::mem::HeapBox), and an array a
//! [`HeapVec`](super::mem::HeapVec), reached through the same handles.
//!
//! ```
//! use ffmpeg::libavutil::intreadwrite::{Unaligned32Mut, Unaligned32Ref};
//!
//! let mut buf = [0u8; 7];
//! let at = buf[3..].first_chunk_mut::<4>().unwrap(); // misaligned for `u32`
//! Unaligned32Mut::from_bytes(at).set_value(0x0102_0304);
//! assert_eq!(buf[3..], 0x0102_0304u32.to_ne_bytes());
//! assert_eq!(Unaligned32Ref::from_bytes(buf[3..].first_chunk().unwrap()).value(), 0x0102_0304);
//! ```

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::ptr::{self, addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;

/// The standard value traits of an `unaligned_N` layout type, all through its
/// integer: `Debug`, `Default`, `PartialEq`, `Eq`, `Hash`, and `From` both
/// ways.
macro_rules! impl_value_traits {
    ($name:ident, $int:ty) => {
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.as_ref().value())
                    .finish()
            }
        }

        impl Default for $name {
            #[inline]
            fn default() -> Self {
                Self::zeroed()
            }
        }

        impl PartialEq for $name {
            #[inline]
            fn eq(&self, other: &Self) -> bool {
                self.as_ref().value() == other.as_ref().value()
            }
        }

        impl Eq for $name {}

        impl Hash for $name {
            #[inline]
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.as_ref().value().hash(state)
            }
        }

        impl From<$int> for $name {
            #[inline]
            fn from(value: $int) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for $int {
            #[inline]
            fn from(value: $name) -> Self {
                value.as_ref().value()
            }
        }
    };
}

define_ctype!(
    /// Wraps: unaligned_16
    ///
    /// A native-endian `u16` at any alignment: 2 bytes, aligned to 1.
    #[derive(Clone, Copy)]
    Unaligned16,
    Unaligned16Ref,
    Unaligned16Mut,
    ffi::unaligned_16
);

// SAFETY: all-zero is `l == 0`, a valid `u16` every accessor accepts, and
// there is no teardown.
unsafe impl CZeroable for Unaligned16 {}

// SAFETY: an `Unaligned16` is two bytes of plain data: no pointer, no
// C-side identity or shared state, and no interior mutability. Moving it or
// its exclusive handle (a `&mut [u8; 2]` in effect) to another thread moves
// sole access to those bytes; the shared handle only reads, so concurrent
// shared access is race-free, as for `&u16`.
unsafe impl Send for Unaligned16 {}
// SAFETY: as above.
unsafe impl Sync for Unaligned16 {}
// SAFETY: as above.
unsafe impl Send for Unaligned16Mut<'_> {}
// SAFETY: as above; a shared `&Unaligned16Mut` reaches only the getters.
unsafe impl Sync for Unaligned16Mut<'_> {}
// SAFETY: as above.
unsafe impl Send for Unaligned16Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Unaligned16Ref<'_> {}

impl Unaligned16 {
    /// A union holding `value`.
    #[inline]
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(ffi::unaligned_16 { l: value }, PhantomData)
    }
}

impl_value_traits!(Unaligned16, u16);

impl<'a> Unaligned16Ref<'a> {
    /// Views two bytes at any address as the union, the cast `AV_RN(16, p)`
    /// reads through.
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a [u8; 2]) -> Self {
        // SAFETY: `bytes` is a live, initialised 2-byte place borrowed for
        // `'a`; `unaligned_16` is 2 bytes aligned to 1, for which every bit
        // pattern is valid. The shared handle never writes through the
        // pointer.
        unsafe { Self::from_ptr(ptr::from_ref(bytes).cast_mut().cast()) }
            .expect("a reference is non-null")
    }

    /// Field: unaligned_16.l
    ///
    /// The `u16` the union holds, in native byte order.
    #[inline]
    #[must_use]
    pub fn value(&self) -> u16 {
        // SAFETY: a read of the projected field through the shared handle's
        // live pointer; the union is packed, so the read is unaligned.
        unsafe { addr_of!((*self.as_ptr()).l).read_unaligned() }
    }
}

impl<'a> Unaligned16Mut<'a> {
    /// Views two bytes at any address as the union, the cast `AV_WN(16, p, v)`
    /// stores through.
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a mut [u8; 2]) -> Self {
        // SAFETY: `bytes` is a live, initialised 2-byte place exclusively
        // borrowed for `'a`, so this is the only handle to it; `unaligned_16`
        // is 2 bytes aligned to 1, for which every bit pattern is valid.
        unsafe { Self::from_ptr(ptr::from_mut(bytes).cast()) }.expect("a reference is non-null")
    }

    /// Field: unaligned_16.l
    ///
    /// Stores `value` in native byte order.
    #[inline]
    pub fn set_value(&mut self, value: u16) {
        // SAFETY: a write of the projected field through the exclusive
        // handle's live pointer; the union is packed, so the write is
        // unaligned.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).l).write_unaligned(value) }
    }
}

define_ctype!(
    /// Wraps: unaligned_32
    ///
    /// A native-endian `u32` at any alignment: 4 bytes, aligned to 1.
    #[derive(Clone, Copy)]
    Unaligned32,
    Unaligned32Ref,
    Unaligned32Mut,
    ffi::unaligned_32
);

// SAFETY: all-zero is `l == 0`, a valid `u32` every accessor accepts, and
// there is no teardown.
unsafe impl CZeroable for Unaligned32 {}

// SAFETY: an `Unaligned32` is four bytes of plain data: no pointer, no
// C-side identity or shared state, and no interior mutability. Moving it or
// its exclusive handle (a `&mut [u8; 4]` in effect) to another thread moves
// sole access to those bytes; the shared handle only reads, so concurrent
// shared access is race-free, as for `&u32`.
unsafe impl Send for Unaligned32 {}
// SAFETY: as above.
unsafe impl Sync for Unaligned32 {}
// SAFETY: as above.
unsafe impl Send for Unaligned32Mut<'_> {}
// SAFETY: as above; a shared `&Unaligned32Mut` reaches only the getters.
unsafe impl Sync for Unaligned32Mut<'_> {}
// SAFETY: as above.
unsafe impl Send for Unaligned32Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Unaligned32Ref<'_> {}

impl Unaligned32 {
    /// A union holding `value`.
    #[inline]
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(ffi::unaligned_32 { l: value }, PhantomData)
    }
}

impl_value_traits!(Unaligned32, u32);

impl<'a> Unaligned32Ref<'a> {
    /// Views four bytes at any address as the union, the cast `AV_RN(32, p)`
    /// reads through.
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a [u8; 4]) -> Self {
        // SAFETY: `bytes` is a live, initialised 4-byte place borrowed for
        // `'a`; `unaligned_32` is 4 bytes aligned to 1, for which every bit
        // pattern is valid. The shared handle never writes through the
        // pointer.
        unsafe { Self::from_ptr(ptr::from_ref(bytes).cast_mut().cast()) }
            .expect("a reference is non-null")
    }

    /// Field: unaligned_32.l
    ///
    /// The `u32` the union holds, in native byte order.
    #[inline]
    #[must_use]
    pub fn value(&self) -> u32 {
        // SAFETY: a read of the projected field through the shared handle's
        // live pointer; the union is packed, so the read is unaligned.
        unsafe { addr_of!((*self.as_ptr()).l).read_unaligned() }
    }
}

impl<'a> Unaligned32Mut<'a> {
    /// Views four bytes at any address as the union, the cast
    /// `AV_WN(32, p, v)` stores through.
    #[inline]
    #[must_use]
    pub fn from_bytes(bytes: &'a mut [u8; 4]) -> Self {
        // SAFETY: `bytes` is a live, initialised 4-byte place exclusively
        // borrowed for `'a`, so this is the only handle to it; `unaligned_32`
        // is 4 bytes aligned to 1, for which every bit pattern is valid.
        unsafe { Self::from_ptr(ptr::from_mut(bytes).cast()) }.expect("a reference is non-null")
    }

    /// Field: unaligned_32.l
    ///
    /// Stores `value` in native byte order.
    #[inline]
    pub fn set_value(&mut self, value: u32) {
        // SAFETY: a write of the projected field through the exclusive
        // handle's live pointer; the union is packed, so the write is
        // unaligned.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).l).write_unaligned(value) }
    }
}

define_ctype!(
    /// Wraps: unaligned_64
    ///
    /// A native-endian `u64` at any alignment: 8 bytes, aligned to 1. It owns
    /// nothing, so it copies as a value.
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
    /// A union holding `value`.
    #[inline]
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(ffi::unaligned_64 { l: value }, PhantomData)
    }
}

impl_value_traits!(Unaligned64, u64);

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

define_ctype!(
    /// Wraps: av_alias64
    ///
    /// Eight bytes viewed as a native-endian `u64`, two `u32` lanes, four
    /// `u16` lanes, bytes, an `f64`, or two `f32` lanes. Every view covers all
    /// eight bytes, so a write through one is wholly visible through the
    /// others.
    #[derive(Clone, Copy)]
    Alias64,
    Alias64Ref,
    Alias64Mut,
    ffi::av_alias64
);

// SAFETY: every view of `av_alias64` is a plain integer, float or array of
// them, so all-zero is a valid value of each, and no teardown or C routine is
// paired with it.
unsafe impl CZeroable for Alias64 {}

// SAFETY: `av_alias64` is plain data: no pointer, no interior mutability, no
// thread-affine C state, so moving it, or an exclusive handle to it, to
// another thread moves nothing a thread owns.
unsafe impl Send for Alias64 {}
// SAFETY: as above.
unsafe impl Send for Alias64Mut<'_> {}
// SAFETY: the only operations on a shared `Alias64`, `Alias64Ref` or
// `&Alias64Mut` are reads; every write needs `Alias64Mut` through `&mut`, so
// shared access from several threads never races.
unsafe impl Sync for Alias64 {}
// SAFETY: as above.
unsafe impl Send for Alias64Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias64Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for Alias64Mut<'_> {}

impl Alias64 {
    /// A value whose `u64` view is `v`.
    #[must_use]
    pub fn from_u64(v: u64) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u64(v);
        a
    }

    /// A value whose `u32` lanes are `v`.
    #[must_use]
    pub fn from_u32x2(v: [u32; 2]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u32x2(v);
        a
    }

    /// A value whose `u16` lanes are `v`.
    #[must_use]
    pub fn from_u16x4(v: [u16; 4]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u16x4(v);
        a
    }

    /// A value whose byte view is `v`.
    #[must_use]
    pub fn from_u8x8(v: [u8; 8]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_u8x8(v);
        a
    }

    /// A value whose `f64` view is `v`, bit for bit.
    #[must_use]
    pub fn from_f64(v: f64) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_f64(v);
        a
    }

    /// A value whose `f32` lanes are `v`, bit for bit.
    #[must_use]
    pub fn from_f32x2(v: [f32; 2]) -> Self {
        let mut a = Self::zeroed();
        a.as_mut().set_f32x2(v);
        a
    }
}

impl Default for Alias64 {
    /// All-zero.
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Bitwise equality: a NaN equals itself, and `0.0` differs from `-0.0`.
impl PartialEq for Alias64 {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().u64() == other.as_ref().u64()
    }
}

impl Eq for Alias64 {}

/// Hashes the `u64` view, consistent with the bitwise [`PartialEq`].
impl Hash for Alias64 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().u64().hash(state)
    }
}

impl fmt::Debug for Alias64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for Alias64Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Alias64")
            .field("u8x8", &self.u8x8())
            .finish()
    }
}

impl Alias64Ref<'_> {
    /// Field: av_alias64.u64
    ///
    /// All eight bytes as a native-endian `u64`.
    #[must_use]
    pub fn u64(&self) -> u64 {
        // SAFETY: the handle addresses a live, initialised `av_alias64`, and
        // `u64` covers all of its bytes, any pattern of which is a `u64`.
        unsafe { addr_of!((*self.as_ptr()).u64_).read() }
    }

    /// Field: av_alias64.u32
    ///
    /// The two native-endian `u32` lanes, in memory order.
    #[must_use]
    pub fn u32x2(&self) -> [u32; 2] {
        // SAFETY: as `u64`, through the `u32` lane view.
        unsafe { addr_of!((*self.as_ptr()).u32_).read() }
    }

    /// Field: av_alias64.u16
    ///
    /// The four native-endian `u16` lanes, in memory order.
    #[must_use]
    pub fn u16x4(&self) -> [u16; 4] {
        // SAFETY: as `u64`, through the `u16` lane view.
        unsafe { addr_of!((*self.as_ptr()).u16_).read() }
    }

    /// Field: av_alias64.u8
    ///
    /// All eight bytes, in memory order.
    #[must_use]
    pub fn u8x8(&self) -> [u8; 8] {
        // SAFETY: as `u64`, through the byte view.
        unsafe { addr_of!((*self.as_ptr()).u8_).read() }
    }

    /// Field: av_alias64.f64
    ///
    /// All eight bytes as an `f64`, bit for bit (`f64::from_bits(self.u64())`).
    #[must_use]
    pub fn f64(&self) -> f64 {
        // SAFETY: as `u64`, through the float view; every bit pattern is an
        // `f64`.
        unsafe { addr_of!((*self.as_ptr()).f64_).read() }
    }

    /// Field: av_alias64.f32
    ///
    /// The two `f32` lanes, in memory order, bit for bit.
    #[must_use]
    pub fn f32x2(&self) -> [f32; 2] {
        // SAFETY: as `u64`, through the `f32` lane view; every bit pattern is
        // a pair of `f32`.
        unsafe { addr_of!((*self.as_ptr()).f32_).read() }
    }
}

impl Alias64Mut<'_> {
    /// Field: av_alias64.u64
    ///
    /// Stores `v` over all eight bytes, in native byte order.
    pub fn set_u64(&mut self, v: u64) {
        // SAFETY: a write of the whole object through the exclusive handle's
        // pointer to a live `av_alias64`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u64_).write(v) }
    }

    /// Field: av_alias64.u32
    ///
    /// Stores both `u32` lanes, in memory order.
    pub fn set_u32x2(&mut self, v: [u32; 2]) {
        // SAFETY: as `set_u64`, through the `u32` lane view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u32_).write(v) }
    }

    /// Field: av_alias64.u16
    ///
    /// Stores all four `u16` lanes, in memory order.
    pub fn set_u16x4(&mut self, v: [u16; 4]) {
        // SAFETY: as `set_u64`, through the `u16` lane view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u16_).write(v) }
    }

    /// Field: av_alias64.u8
    ///
    /// Stores all eight bytes, in memory order.
    pub fn set_u8x8(&mut self, v: [u8; 8]) {
        // SAFETY: as `set_u64`, through the byte view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).u8_).write(v) }
    }

    /// Field: av_alias64.f64
    ///
    /// Stores `v`'s bits over all eight bytes; a NaN keeps its payload.
    pub fn set_f64(&mut self, v: f64) {
        // SAFETY: as `set_u64`, through the float view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).f64_).write(v) }
    }

    /// Field: av_alias64.f32
    ///
    /// Stores both `f32` lanes' bits, in memory order; a NaN keeps its
    /// payload.
    pub fn set_f32x2(&mut self, v: [f32; 2]) {
        // SAFETY: as `set_u64`, through the `f32` lane view.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).f32_).write(v) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::mem::{align_of, size_of};
    use std::collections::hash_map::DefaultHasher;

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
    fn unaligned64_value_traits_follow_the_integer() {
        let a = Unaligned64::new(0x0123_4567_89ab_cdef);
        assert_eq!(a, Unaligned64::from(0x0123_4567_89ab_cdef));
        assert_ne!(a, Unaligned64::new(0));
        assert_eq!(u64::from(a), 0x0123_4567_89ab_cdef);
        assert_eq!(hash_of(&a), hash_of(&0x0123_4567_89ab_cdef_u64));
        assert_eq!(Unaligned64::default(), Unaligned64::zeroed());
        assert_eq!(Unaligned64::default().as_ref().value(), 0);
        assert_eq!(format!("{:?}", Unaligned64::new(5)), "Unaligned64(5)");
        let distinct: std::collections::HashSet<Unaligned64> =
            [a, a, Unaligned64::new(1)].into_iter().collect();
        assert_eq!(distinct.len(), 2);
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

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    #[test]
    fn layout_is_the_packed_c_union() {
        assert_eq!(size_of::<Unaligned16>(), 2);
        assert_eq!(align_of::<Unaligned16>(), 1);
        assert_eq!(size_of::<Unaligned32>(), 4);
        assert_eq!(align_of::<Unaligned32>(), 1);
        // One pointer wide, as `define_ctype!` promises.
        assert_eq!(size_of::<Unaligned16Ref<'_>>(), size_of::<*const u8>());
        assert_eq!(size_of::<Unaligned32Mut<'_>>(), size_of::<*const u8>());
    }

    #[test]
    fn new_and_conversions_round_trip() {
        const C16: Unaligned16 = Unaligned16::new(0xbeef);
        const C32: Unaligned32 = Unaligned32::new(0xdead_beef);
        assert_eq!(C16.as_ref().value(), 0xbeef);
        assert_eq!(C32.as_ref().value(), 0xdead_beef);
        for v in [0, 1, 0x00ff, 0xff00, u16::MAX] {
            assert_eq!(u16::from(Unaligned16::from(v)), v);
        }
        for v in [0, 1, 0x0000_ffff, 0xffff_0000, u32::MAX] {
            assert_eq!(u32::from(Unaligned32::from(v)), v);
        }
    }

    #[test]
    fn default_is_zeroed_and_zero() {
        assert_eq!(Unaligned16::default(), Unaligned16::zeroed());
        assert_eq!(Unaligned16::default().as_ref().value(), 0);
        assert_eq!(Unaligned32::default(), Unaligned32::zeroed());
        assert_eq!(Unaligned32::default().as_ref().value(), 0);
    }

    #[test]
    fn value_traits_follow_the_integer() {
        let a = Unaligned32::new(7);
        let b = a; // `Copy`
        #[allow(clippy::clone_on_copy)]
        let c = a.clone();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_ne!(a, Unaligned32::new(8));
        assert_eq!(hash_of(&a), hash_of(&7u32));
        assert_eq!(hash_of(&Unaligned16::new(9)), hash_of(&9u16));
        assert_eq!(
            format!("{:?}", Unaligned16::new(0x1234)),
            "Unaligned16(4660)"
        );
        assert_eq!(format!("{:?}", Unaligned32::new(5)), "Unaligned32(5)");
        assert_ne!(Unaligned16::new(1), Unaligned16::new(2));
    }

    #[test]
    fn handles_on_a_value_read_and_write_it() {
        let mut v = Unaligned16::new(1);
        let mut m = v.as_mut();
        m.set_value(2);
        assert_eq!(m.as_ref().value(), 2);
        m.as_mut().set_value(3); // a reborrow keeps `m`
        m.set_value(4);
        assert_eq!(v.as_ref().value(), 4);

        let mut w = Unaligned32::new(1);
        w.as_mut().set_value(u32::MAX);
        assert_eq!(w.as_ref().value(), u32::MAX);
    }

    #[test]
    fn byte_views_are_native_endian_at_every_offset() {
        let mut buf = [0u8; 11];
        for off in 0..=buf.len() - 4 {
            let v = 0x0a0b_0c0d_u32.rotate_left(off as u32 * 8);
            Unaligned32Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v);
            assert_eq!(buf[off..off + 4], v.to_ne_bytes());
            assert_eq!(
                Unaligned32Ref::from_bytes(buf[off..].first_chunk().unwrap()).value(),
                v
            );
        }
        for off in 0..=buf.len() - 2 {
            let v = 0x0102_u16.wrapping_mul(off as u16 + 1);
            Unaligned16Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v);
            assert_eq!(buf[off..off + 2], v.to_ne_bytes());
            assert_eq!(
                Unaligned16Ref::from_bytes(buf[off..].first_chunk().unwrap()).value(),
                v
            );
        }
    }

    #[test]
    fn a_byte_view_writes_only_its_own_bytes() {
        let mut buf = [0xa5u8; 8];
        Unaligned16Mut::from_bytes(buf[3..].first_chunk_mut().unwrap()).set_value(0);
        assert_eq!(buf, [0xa5, 0xa5, 0xa5, 0, 0, 0xa5, 0xa5, 0xa5]);
        Unaligned32Mut::from_bytes(buf[4..].first_chunk_mut().unwrap()).set_value(u32::MAX);
        assert_eq!(buf, [0xa5, 0xa5, 0xa5, 0, 0xff, 0xff, 0xff, 0xff]);
    }

    const ALIAS64_BITS: u64 = 0x0123_4567_89ab_cdef;

    #[test]
    fn alias64_layout_matches_the_c_union() {
        assert_eq!(size_of::<Alias64>(), size_of::<ffi::av_alias64>());
        assert_eq!(align_of::<Alias64>(), align_of::<ffi::av_alias64>());
        assert_eq!((size_of::<Alias64>(), align_of::<Alias64>()), (8, 8));
        assert_eq!(size_of::<Alias64Ref<'_>>(), size_of::<*const ()>());
        assert_eq!(size_of::<Alias64Mut<'_>>(), size_of::<*const ()>());
    }

    #[test]
    fn alias64_plain_value_traits() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Alias64>();
        send_sync::<Alias64Ref<'_>>();
        send_sync::<Alias64Mut<'_>>();

        assert_eq!(Alias64::default().as_ref().u64(), 0);
        assert_eq!(Alias64::zeroed(), Alias64::default());
        let a = Alias64::from_u8x8([1, 2, 3, 4, 5, 6, 7, 8]);
        let shown = "Alias64 { u8x8: [1, 2, 3, 4, 5, 6, 7, 8] }";
        assert_eq!(format!("{a:?}"), shown);
        assert_eq!(format!("{:?}", a.as_ref()), shown);
    }

    #[test]
    fn alias64_equality_is_bitwise() {
        let nan = Alias64::from_f64(f64::NAN);
        assert_eq!(nan, nan);
        assert_ne!(Alias64::from_f64(0.0), Alias64::from_f64(-0.0));
        assert_ne!(Alias64::from_u64(1), Alias64::from_u64(2));
        assert_eq!(
            Alias64::from_u64(ALIAS64_BITS),
            Alias64::from_u8x8(ALIAS64_BITS.to_ne_bytes())
        );
    }

    #[test]
    fn alias64_every_constructor_sets_its_view() {
        let ne = ALIAS64_BITS.to_ne_bytes();
        let u32x2 = [
            u32::from_ne_bytes(ne[..4].try_into().unwrap()),
            u32::from_ne_bytes(ne[4..].try_into().unwrap()),
        ];
        let u16x4 = core::array::from_fn(|i| u16::from_ne_bytes([ne[2 * i], ne[2 * i + 1]]));
        let f32x2 = u32x2.map(f32::from_bits);
        for a in [
            Alias64::from_u64(ALIAS64_BITS),
            Alias64::from_u32x2(u32x2),
            Alias64::from_u16x4(u16x4),
            Alias64::from_u8x8(ne),
            Alias64::from_f64(f64::from_bits(ALIAS64_BITS)),
            Alias64::from_f32x2(f32x2),
        ] {
            let r = a.as_ref();
            assert_eq!(r.u64(), ALIAS64_BITS);
            assert_eq!(r.u32x2(), u32x2);
            assert_eq!(r.u16x4(), u16x4);
            assert_eq!(r.u8x8(), ne);
            assert_eq!(r.f64().to_bits(), ALIAS64_BITS);
            assert_eq!(r.f32x2().map(f32::to_bits), u32x2);
        }
    }

    #[test]
    fn alias64_setters_overwrite_all_eight_bytes() {
        let mut a = Alias64::from_u64(u64::MAX);
        let mut m = a.as_mut();
        m.set_u8x8([0; 8]);
        assert_eq!(m.as_ref().u64(), 0);
        m.set_u16x4([1, 2, 3, 4]);
        assert_eq!(m.as_ref().u16x4(), [1, 2, 3, 4]);
        m.set_u32x2([5, 6]);
        assert_eq!(m.as_ref().u32x2(), [5, 6]);
        m.set_f32x2([1.5, -2.0]);
        assert_eq!(m.as_ref().f32x2(), [1.5, -2.0]);
        m.set_f64(-0.0);
        assert_eq!(m.as_ref().u64(), 1 << 63);
        m.set_u64(ALIAS64_BITS);
        assert_eq!(a.as_ref().u64(), ALIAS64_BITS);
    }

    #[test]
    fn alias64_nan_payloads_survive_both_ways() {
        // A signalling NaN with a payload, in each lane and in the whole.
        let snan64 = 0x7ff0_0000_0000_0001_u64;
        let snan32 = 0x7f80_0001_u32;
        let mut a = Alias64::default();
        a.as_mut().set_f64(f64::from_bits(snan64));
        assert_eq!(a.as_ref().u64(), snan64);
        assert_eq!(a.as_ref().f64().to_bits(), snan64);
        a.as_mut()
            .set_f32x2([f32::from_bits(snan32), f32::from_bits(!snan32)]);
        assert_eq!(a.as_ref().f32x2().map(f32::to_bits), [snan32, !snan32]);
        assert_eq!(a.as_ref().u32x2(), [snan32, !snan32]);
    }

    #[test]
    fn alias64_copies_are_independent() {
        let a = Alias64::from_u64(1);
        let mut b = a;
        b.as_mut().set_u64(2);
        assert_eq!((a.as_ref().u64(), b.as_ref().u64()), (1, 2));
    }

    #[test]
    fn alias64_handles_over_a_c_pointer_read_and_write_it() {
        let mut raw = ffi::av_alias64 { u64_: ALIAS64_BITS };
        // SAFETY: `raw` is a live, initialised `av_alias64` that outlives the
        // handle, and nothing else touches it while the handle lives.
        let mut m = unsafe { Alias64Mut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(m.as_ref().u64(), ALIAS64_BITS);
        m.set_u16x4([5, 6, 7, 8]);
        // SAFETY: the handle is gone; every field of `raw` covers its bytes.
        let lanes = unsafe { raw.u16_ };
        assert_eq!(lanes, [5, 6, 7, 8]);
        // SAFETY: a null pointer is accepted and yields no handle.
        assert!(unsafe { Alias64Ref::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn alias64_hash_follows_the_bitwise_equality() {
        fn eq_hash<T: Eq + Hash>() {}
        eq_hash::<Alias64>();

        let a = Alias64::from_u64(ALIAS64_BITS);
        assert_eq!(hash_of(&a), hash_of(&ALIAS64_BITS));
        assert_eq!(hash_of(&Alias64::from_f64(-0.0)), hash_of(&(1u64 << 63)));
        assert_eq!(
            hash_of(&Alias64::from_u8x8(ALIAS64_BITS.to_ne_bytes())),
            hash_of(&a)
        );
        let mut seen = std::collections::HashSet::new();
        for v in [
            Alias64::from_u64(0),
            Alias64::from_f64(0.0),
            Alias64::from_f64(-0.0),
            Alias64::from_f64(f64::NAN),
            Alias64::from_f64(f64::NAN),
            a,
        ] {
            seen.insert(v);
        }
        assert_eq!(seen.len(), 4);
        assert!(seen.contains(&Alias64::from_u8x8(ALIAS64_BITS.to_ne_bytes())));
    }
}
