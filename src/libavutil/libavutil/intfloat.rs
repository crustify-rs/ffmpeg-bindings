//! Safe wrappers over `libavutil/intfloat.h`.
//!
//! [`IntFloat64`] is `union av_intfloat64`: one 64-bit word, read and written
//! either as its integer bits ([`IntFloat64Ref::bits`]) or as a `double`
//! ([`IntFloat64Ref::float`]). Both members span the whole word and every bit
//! pattern is a valid value of each, so there is no active member to track:
//! either view reads what the other last wrote, the reinterpretation
//! `av_int2double` and `av_double2int` are built on.
//!
//! The union owns nothing, so it has no destructor and copies as a value. It
//! lives inline, built by [`IntFloat64::from_bits`], [`IntFloat64::from_float`]
//! or `IntFloat64::zeroed()`; one in an `av_malloc`-family block is a
//! [`HeapBox<IntFloat64>`](super::mem::HeapBox) (`new_zeroed`, and `Clone`
//! through `av_memdup`), and an array of them a
//! [`HeapVec<IntFloat64>`](super::mem::HeapVec) from
//! [`alloc_cell_array_zeroed`](super::mem::alloc_cell_array_zeroed).
//!
//! [`IntFloat32`] is the header's 4-byte union of a `u32` and an `f32`, which
//! C uses to reinterpret one as the other. It owns nothing and C has no
//! routine that allocates, copies or frees one, so it is a plain `Copy` value
//! that compares and hashes bitwise. Both members span all 4 bytes and every
//! bit pattern is a valid value of each, so either member reads back whatever
//! was last written through the other, bit for bit, as in C.
//!
//! The header's conversion functions are the same punning as free functions:
//! [`float_from_bits`] / [`float_to_bits`] (`av_int2float` / `av_float2int`)
//! and [`double_from_bits`] / [`double_to_bits`] (`av_int2double` /
//! `av_double2int`).

use core::fmt;
use core::hash::{Hash, Hasher};
use core::ptr::{addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: av_intfloat64
    ///
    /// A 64-bit word with an integer and a `double` view. Its getters are on
    /// [`IntFloat64Ref`] (`as_ref()`), its setters on [`IntFloat64Mut`]
    /// (`as_mut()`).
    #[derive(Clone, Copy)]
    IntFloat64,
    IntFloat64Ref,
    IntFloat64Mut,
    ffi::av_intfloat64
);

// SAFETY: all-zero is `i == 0` and `f == +0.0`, valid for both getters, and
// the union has no teardown and no safe wrapper that could reject it.
unsafe impl CZeroable for IntFloat64 {}

// SAFETY: an `av_intfloat64` is 8 bytes of plain data, with no pointer, no
// sub-allocation and nothing tied to the thread that wrote it, so a value, or
// an exclusive handle to one, moves between threads as a `u64` does.
unsafe impl Send for IntFloat64 {}
// SAFETY: as for `IntFloat64`: the handle is the only access to the word.
unsafe impl Send for IntFloat64Mut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `IntFloat64Ref`
// has getters only, and the union has no interior mutability, so concurrent
// shared access reads unchanging bytes, as through `&u64`.
unsafe impl Sync for IntFloat64 {}
// SAFETY: as for `Sync for IntFloat64`: copies of a shared handle only read.
unsafe impl Send for IntFloat64Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for IntFloat64Ref<'_> {}
// SAFETY: `&IntFloat64Mut` reaches the word only through `as_ref`, a shared
// handle; the setters need `&mut`.
unsafe impl Sync for IntFloat64Mut<'_> {}

impl IntFloat64 {
    /// The word with its integer member set, as C's `{ .i = bits }`.
    #[must_use]
    pub fn from_bits(bits: u64) -> Self {
        let mut v = Self::zeroed();
        v.as_mut().set_bits(bits);
        v
    }

    /// The word with its `double` member set, as C's `{ .f = value }`.
    #[must_use]
    pub fn from_float(value: f64) -> Self {
        let mut v = Self::zeroed();
        v.as_mut().set_float(value);
        v
    }
}

impl Default for IntFloat64 {
    /// The all-zero word: `bits() == 0`, `float() == 0.0`.
    fn default() -> Self {
        Self::zeroed()
    }
}

impl fmt::Debug for IntFloat64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for IntFloat64Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntFloat64")
            .field("bits", &format_args!("{:#018x}", self.bits()))
            .field("float", &self.float())
            .finish()
    }
}

impl IntFloat64Ref<'_> {
    /// Field: `av_intfloat64.i`
    ///
    /// The word as an unsigned integer: the bit pattern of
    /// [`float`](Self::float), as `f64::to_bits` gives it.
    #[must_use]
    pub fn bits(&self) -> u64 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `i` field is in bounds; nothing is accessed here.
        let i_ptr = unsafe { addr_of!((*self.as_ptr()).i) };
        // SAFETY: a read of the `i` member projected through the shared
        // handle's pointer to a live `av_intfloat64`. Both members span all 8
        // initialised bytes, and any bit pattern is a valid `u64`, whichever
        // member was last written.
        unsafe { i_ptr.read() }
    }

    /// Field: `av_intfloat64.f`
    ///
    /// The word as a `double`: [`bits`](Self::bits) reinterpreted, as
    /// `f64::from_bits` does, NaN payloads included.
    #[must_use]
    pub fn float(&self) -> f64 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `f` field is in bounds; nothing is accessed here.
        let f_ptr = unsafe { addr_of!((*self.as_ptr()).f) };
        // SAFETY: a read of the `f` member projected through the shared
        // handle's pointer to a live `av_intfloat64`. Both members span all 8
        // initialised bytes, and any bit pattern is a valid `f64`.
        unsafe { f_ptr.read() }
    }
}

impl IntFloat64Mut<'_> {
    /// Field: `av_intfloat64.i`
    ///
    /// Overwrites the whole word with `bits`, which [`float`](IntFloat64Ref::float)
    /// then reads reinterpreted.
    pub fn set_bits(&mut self, bits: u64) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `i` field is in bounds; nothing is accessed here.
        let i_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).i) };
        // SAFETY: a write of the `i` member projected through the exclusive
        // handle's pointer to a live `av_intfloat64`; it covers the whole
        // union, which owns nothing to leak.
        unsafe { i_ptr.write(bits) }
    }

    /// Field: `av_intfloat64.f`
    ///
    /// Overwrites the whole word with `value`'s bit pattern, which
    /// [`bits`](IntFloat64Ref::bits) then reads.
    pub fn set_float(&mut self, value: f64) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `f` field is in bounds; nothing is accessed here.
        let f_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).f) };
        // SAFETY: a write of the `f` member projected through the exclusive
        // handle's pointer to a live `av_intfloat64`; it covers the whole
        // union, which owns nothing to leak.
        unsafe { f_ptr.write(value) }
    }
}

define_ctype!(
    /// Wraps: av_intfloat32
    ///
    /// A 32-bit value viewed as a `u32` or an `f32`. Build one with
    /// [`IntFloat32::default`] (all zero), [`IntFloat32::from_bits`] or
    /// [`IntFloat32::from_float`], read it through
    /// [`as_ref`](IntFloat32::as_ref) and write it through
    /// [`as_mut`](IntFloat32::as_mut). Equality and hashing are bitwise.
    #[derive(Clone, Copy)]
    IntFloat32,
    IntFloat32Ref,
    IntFloat32Mut,
    ffi::av_intfloat32
);

// SAFETY: both members span all 4 bytes and every bit pattern is a valid
// `u32` and `f32`, so all-zero is a valid union that every accessor accepts;
// there is no teardown to accept it.
unsafe impl CZeroable for IntFloat32 {}

// SAFETY: an `IntFloat32` is 4 bytes of plain data, with no pointer, thread
// affinity or interior mutability: moving one, or exclusive access to one, to
// another thread is moving those bytes.
unsafe impl Send for IntFloat32 {}
// SAFETY: as above.
unsafe impl Send for IntFloat32Mut<'_> {}
// SAFETY: every operation on `IntFloat32Ref` only reads, so concurrent shared
// access is concurrent reads of plain data; writes need `IntFloat32Mut`,
// which the borrow excludes while any shared handle lives.
unsafe impl Sync for IntFloat32 {}
// SAFETY: as for `Sync for IntFloat32`: a shared handle only reads.
unsafe impl Send for IntFloat32Ref<'_> {}
// SAFETY: as above.
unsafe impl Sync for IntFloat32Ref<'_> {}
// SAFETY: a shared `&IntFloat32Mut` reaches only `as_ref`, which reads.
unsafe impl Sync for IntFloat32Mut<'_> {}

impl IntFloat32 {
    /// The value whose integer member is `bits`.
    #[must_use]
    pub fn from_bits(bits: u32) -> Self {
        Self(ffi::av_intfloat32 { i: bits }, core::marker::PhantomData)
    }

    /// The value whose float member is `v`, bit for bit.
    #[must_use]
    pub fn from_float(v: f32) -> Self {
        Self(ffi::av_intfloat32 { f: v }, core::marker::PhantomData)
    }
}

impl Default for IntFloat32 {
    /// All zero: `0` and `+0.0`.
    fn default() -> Self {
        Self::zeroed()
    }
}

/// Bitwise equality, as C's `memcmp` of the two words: a NaN equals itself,
/// and `0.0` differs from `-0.0`.
impl PartialEq for IntFloat32 {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().bits() == other.as_ref().bits()
    }
}

impl Eq for IntFloat32 {}

/// Hashes the bits, consistent with the bitwise [`PartialEq`].
impl Hash for IntFloat32 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().bits().hash(state);
    }
}

impl fmt::Debug for IntFloat32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for IntFloat32Ref<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntFloat32")
            .field("bits", &format_args!("{:#010x}", self.bits()))
            .field("float", &self.float())
            .finish()
    }
}

impl IntFloat32Ref<'_> {
    /// Field: `av_intfloat32.i`
    #[must_use]
    pub fn bits(&self) -> u32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `i` field is in bounds; nothing is accessed here.
        let i_ptr = unsafe { addr_of!((*self.as_ptr()).i) };
        // SAFETY: a read of a projected member through the shared handle; it
        // spans the whole initialised union, and every bit pattern is a `u32`.
        unsafe { i_ptr.read() }
    }

    /// Field: `av_intfloat32.f`
    ///
    /// The bits read as an `f32`, NaN payloads included.
    #[must_use]
    pub fn float(&self) -> f32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `f` field is in bounds; nothing is accessed here.
        let f_ptr = unsafe { addr_of!((*self.as_ptr()).f) };
        // SAFETY: as `bits`; every bit pattern is an `f32`.
        unsafe { f_ptr.read() }
    }
}

impl IntFloat32Mut<'_> {
    /// Field: `av_intfloat32.i`
    pub fn set_bits(&mut self, bits: u32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `i` field is in bounds; nothing is accessed here.
        let i_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).i) };
        // SAFETY: a write of a projected member through the exclusive handle;
        // it spans the whole union, which the other member then reads.
        unsafe { i_ptr.write(bits) }
    }

    /// Field: `av_intfloat32.f`
    pub fn set_float(&mut self, v: f32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `f` field is in bounds; nothing is accessed here.
        let f_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).f) };
        // SAFETY: as `set_bits`, for an `f32`.
        unsafe { f_ptr.write(v) }
    }
}

/// Wraps: `av_int2float`
///
/// The `f32` whose bit pattern is `bits`, as [`f32::from_bits`] gives it, NaN
/// payloads included.
#[must_use]
pub fn float_from_bits(bits: u32) -> f32 {
    // SAFETY: `av_int2float` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_int2float(bits) }
}

/// Wraps: `av_float2int`
///
/// The bit pattern of `value`, as [`f32::to_bits`] gives it.
#[must_use]
pub fn float_to_bits(value: f32) -> u32 {
    // SAFETY: `av_float2int` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_float2int(value) }
}

/// Wraps: `av_int2double`
///
/// The `f64` whose bit pattern is `bits`, as [`f64::from_bits`] gives it, NaN
/// payloads included.
#[must_use]
pub fn double_from_bits(bits: u64) -> f64 {
    // SAFETY: `av_int2double` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_int2double(bits) }
}

/// Wraps: `av_double2int`
///
/// The bit pattern of `value`, as [`f64::to_bits`] gives it.
#[must_use]
pub fn double_to_bits(value: f64) -> u64 {
    // SAFETY: `av_double2int` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_double2int(value) }
}

#[cfg(test)]
mod unit_tests {
    use core::mem::{align_of, size_of};

    use super::*;
    use crate::libavutil::libavutil::mem::HeapBox;

    #[test]
    fn layout_matches_the_c_union() {
        assert_eq!(size_of::<IntFloat64>(), size_of::<ffi::av_intfloat64>());
        assert_eq!(align_of::<IntFloat64>(), align_of::<ffi::av_intfloat64>());
        assert_eq!(size_of::<IntFloat64Ref<'_>>(), size_of::<*const u8>());
        assert_eq!(size_of::<IntFloat64Mut<'_>>(), size_of::<*const u8>());
    }

    #[test]
    fn value_is_send_sync_and_copy() {
        fn send_sync<T: Send + Sync>() {}
        fn copy<T: Copy>() {}
        send_sync::<IntFloat64>();
        send_sync::<IntFloat64Ref<'_>>();
        send_sync::<IntFloat64Mut<'_>>();
        send_sync::<HeapBox<IntFloat64>>();
        copy::<IntFloat64>();

        let a = IntFloat64::from_bits(5);
        let mut b = a;
        b.as_mut().set_bits(6);
        assert_eq!((a.as_ref().bits(), b.as_ref().bits()), (5, 6));
    }

    #[test]
    fn zeroed_and_default_are_both_views_zero() {
        for v in [IntFloat64::zeroed(), IntFloat64::default()] {
            assert_eq!(v.as_ref().bits(), 0);
            assert_eq!(v.as_ref().float().to_bits(), 0.0f64.to_bits());
        }
    }

    #[test]
    fn constructors_set_one_member_and_the_other_reinterprets() {
        let v = IntFloat64::from_bits(0x3FF0_0000_0000_0000);
        assert_eq!(v.as_ref().float(), 1.0);
        let v = IntFloat64::from_float(-2.5);
        assert_eq!(v.as_ref().bits(), (-2.5f64).to_bits());
        let v = IntFloat64::from_float(-0.0);
        assert_eq!(v.as_ref().bits(), 1 << 63);
    }

    #[test]
    fn nan_payloads_survive_both_views() {
        // A signalling NaN with a payload, and a negative quiet one.
        for bits in [0x7FF0_0000_0000_0001u64, 0xFFF8_DEAD_BEEF_0001] {
            let mut v = IntFloat64::from_bits(bits);
            assert!(v.as_ref().float().is_nan());
            assert_eq!(v.as_ref().float().to_bits(), bits);
            let f = v.as_ref().float();
            v.as_mut().set_float(f);
            assert_eq!(v.as_ref().bits(), bits);
        }
    }

    #[test]
    fn setters_overwrite_the_whole_word_through_reborrows() {
        let mut v = IntFloat64::zeroed();
        let mut m = v.as_mut();
        m.as_mut().set_float(f64::INFINITY);
        assert_eq!(m.as_ref().bits(), 0x7FF0_0000_0000_0000);
        m.set_bits(u64::MAX);
        assert!(m.as_ref().float().is_nan());
        assert_eq!(v.as_ref().bits(), u64::MAX);
    }

    #[test]
    fn debug_shows_both_views() {
        let v = IntFloat64::from_float(1.0);
        assert_eq!(
            format!("{v:?}"),
            "IntFloat64 { bits: 0x3ff0000000000000, float: 1.0 }"
        );
        assert_eq!(format!("{:?}", v.as_ref()), format!("{v:?}"));
    }

    #[test]
    fn handles_borrow_a_c_owned_union() {
        let mut raw = ffi::av_intfloat64 {
            i: 0x4000_0000_0000_0000,
        };
        // SAFETY: `raw` is a live, initialised union outliving the handle,
        // and no other access to it happens while the handle is used.
        let mut m = unsafe { IntFloat64Mut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(m.as_ref().float(), 2.0);
        m.set_float(0.5);
        // SAFETY: the handle above is no longer used; `f` was written last.
        assert_eq!(unsafe { raw.f }, 0.5);
        // SAFETY: NULL is rejected before anything is borrowed.
        assert!(unsafe { IntFloat64Ref::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn heap_boxed_union_reads_writes_and_clones() {
        let mut a = HeapBox::<IntFloat64>::new_zeroed().unwrap();
        assert_eq!(a.as_ref().bits(), 0);
        a.as_mut().set_float(3.0);
        let b = a.clone();
        a.as_mut().set_bits(1);
        assert_eq!((a.as_ref().bits(), b.as_ref().float()), (1, 3.0));

        let mut slot = HeapBox::<IntFloat64>::new_uninit().unwrap();
        // SAFETY: `as_c_ptr` addresses 8 writable, aligned bytes `slot` owns.
        unsafe { slot.as_c_ptr().write(ffi::av_intfloat64 { f: -1.0 }) };
        // SAFETY: the write above initialised the whole union.
        let c = unsafe { slot.assume_init() };
        assert_eq!(c.as_ref().bits(), (-1.0f64).to_bits());
    }

    #[test]
    fn intfloat32_layout_matches_the_c_union() {
        assert_eq!(size_of::<IntFloat32>(), size_of::<ffi::av_intfloat32>());
        assert_eq!(align_of::<IntFloat32>(), align_of::<ffi::av_intfloat32>());
        assert_eq!((size_of::<IntFloat32>(), align_of::<IntFloat32>()), (4, 4));
        assert_eq!(size_of::<IntFloat32Ref<'_>>(), size_of::<*const ()>());
        assert_eq!(size_of::<IntFloat32Mut<'_>>(), size_of::<*const ()>());
    }

    #[test]
    fn intfloat32_plain_value_traits() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<IntFloat32>();
        send_sync::<IntFloat32Ref<'_>>();
        send_sync::<IntFloat32Mut<'_>>();

        let d = IntFloat32::default();
        assert_eq!((d.as_ref().bits(), d.as_ref().float()), (0, 0.0));
        assert!(d.as_ref().float().is_sign_positive());
        assert_eq!(IntFloat32::zeroed().as_ref().bits(), 0);
        assert_eq!(
            format!("{d:?}"),
            "IntFloat32 { bits: 0x00000000, float: 0.0 }"
        );
        let one = IntFloat32::from_float(1.0);
        assert_eq!(
            format!("{one:?}"),
            "IntFloat32 { bits: 0x3f800000, float: 1.0 }"
        );
        assert_eq!(format!("{:?}", one.as_ref()), format!("{one:?}"));
    }

    #[test]
    fn intfloat32_members_reinterpret_each_other() {
        for bits in [
            0,
            1,
            0x3F80_0000,
            0x8000_0000,
            0x7F80_0000,
            0x7FC0_0000,
            0x7F80_0001,
            u32::MAX,
        ] {
            let a = IntFloat32::from_bits(bits);
            assert_eq!(a.as_ref().float().to_bits(), bits);
            let b = IntFloat32::from_float(f32::from_bits(bits));
            assert_eq!(b.as_ref().bits(), bits);
        }
    }

    #[test]
    fn intfloat32_setters_and_copies() {
        let mut a = IntFloat32::default();
        let mut m = a.as_mut();
        m.set_float(-2.5);
        assert_eq!(m.as_ref().bits(), (-2.5f32).to_bits());
        m.set_bits(0x7F80_0001);
        assert_eq!(m.as_ref().float().to_bits(), 0x7F80_0001);
        let c = a;
        a.as_mut().set_float(1.0);
        assert_eq!((c.as_ref().bits(), a.as_ref().float()), (0x7F80_0001, 1.0));
    }

    fn hash_of<T: Hash>(v: &T) -> u64 {
        use std::hash::{BuildHasher, RandomState};
        // One fixed state for the whole test, as in `HashMap` lookups.
        thread_local!(static STATE: RandomState = RandomState::new());
        STATE.with(|s| s.hash_one(v))
    }

    #[test]
    fn intfloat32_equality_and_hash_are_bitwise() {
        fn eq_hash<T: Eq + Hash>() {}
        eq_hash::<IntFloat32>();

        let nan = IntFloat32::from_bits(0x7FC0_1234);
        assert_eq!(nan, nan);
        assert_eq!(nan, IntFloat32::from_float(f32::from_bits(0x7FC0_1234)));
        assert_ne!(nan, IntFloat32::from_bits(0x7FC0_1235));
        assert_ne!(IntFloat32::from_float(0.0), IntFloat32::from_float(-0.0));
        assert_eq!(
            IntFloat32::from_float(1.5),
            IntFloat32::from_bits(0x3FC0_0000)
        );
        assert_eq!(IntFloat32::default(), IntFloat32::zeroed());

        assert_eq!(hash_of(&nan), hash_of(&0x7FC0_1234u32));
        assert_eq!(
            hash_of(&IntFloat32::from_float(-0.0)),
            hash_of(&(1u32 << 31))
        );
        let mut seen = std::collections::HashSet::new();
        for bits in [0, 1, 1 << 31, 0x7FC0_1234, u32::MAX, 0, 1] {
            seen.insert(IntFloat32::from_bits(bits));
        }
        assert_eq!(seen.len(), 5);
        assert!(seen.contains(&IntFloat32::from_float(f32::from_bits(u32::MAX))));
    }
}
