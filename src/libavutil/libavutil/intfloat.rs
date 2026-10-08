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

use core::fmt;
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
    /// Field: av_intfloat64.i
    ///
    /// The word as an unsigned integer: the bit pattern of
    /// [`float`](Self::float), as `f64::to_bits` gives it.
    #[must_use]
    pub fn bits(&self) -> u64 {
        // SAFETY: a read of the `i` member projected through the shared
        // handle's pointer to a live `av_intfloat64`. Both members span all 8
        // initialised bytes, and any bit pattern is a valid `u64`, whichever
        // member was last written.
        unsafe { addr_of!((*self.as_ptr()).i).read() }
    }

    /// Field: av_intfloat64.f
    ///
    /// The word as a `double`: [`bits`](Self::bits) reinterpreted, as
    /// `f64::from_bits` does, NaN payloads included.
    #[must_use]
    pub fn float(&self) -> f64 {
        // SAFETY: a read of the `f` member projected through the shared
        // handle's pointer to a live `av_intfloat64`. Both members span all 8
        // initialised bytes, and any bit pattern is a valid `f64`.
        unsafe { addr_of!((*self.as_ptr()).f).read() }
    }
}

impl IntFloat64Mut<'_> {
    /// Field: av_intfloat64.i
    ///
    /// Overwrites the whole word with `bits`, which [`float`](IntFloat64Ref::float)
    /// then reads reinterpreted.
    pub fn set_bits(&mut self, bits: u64) {
        // SAFETY: a write of the `i` member projected through the exclusive
        // handle's pointer to a live `av_intfloat64`; it covers the whole
        // union, which owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).i).write(bits) }
    }

    /// Field: av_intfloat64.f
    ///
    /// Overwrites the whole word with `value`'s bit pattern, which
    /// [`bits`](IntFloat64Ref::bits) then reads.
    pub fn set_float(&mut self, value: f64) {
        // SAFETY: a write of the `f` member projected through the exclusive
        // handle's pointer to a live `av_intfloat64`; it covers the whole
        // union, which owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).f).write(value) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::mem::{align_of, size_of};

    use crate::libavutil::mem::HeapBox;

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
        let v = IntFloat64::from_bits(0x3ff0_0000_0000_0000);
        assert_eq!(v.as_ref().float(), 1.0);
        let v = IntFloat64::from_float(-2.5);
        assert_eq!(v.as_ref().bits(), (-2.5f64).to_bits());
        let v = IntFloat64::from_float(-0.0);
        assert_eq!(v.as_ref().bits(), 1 << 63);
    }

    #[test]
    fn nan_payloads_survive_both_views() {
        // A signalling NaN with a payload, and a negative quiet one.
        for bits in [0x7ff0_0000_0000_0001u64, 0xfff8_dead_beef_0001] {
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
        assert_eq!(m.as_ref().bits(), 0x7ff0_0000_0000_0000);
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
}
