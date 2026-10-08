//! Safe wrappers over `libavutil/rational.h`.
//!
//! [`Rational`] is `AVRational`: a numerator and a denominator, two `int`s
//! that C passes and returns by value, embeds by value in its public structs
//! (time bases, frame rates, aspect ratios) and lists in arrays. It owns
//! nothing, so it has no destructor and copies as a value. It lives inline,
//! built by [`Rational::new`] (`av_make_q`), [`Rational::from_raw`] or
//! `Rational::zeroed()`; one in an `av_malloc`-family block is a
//! [`HeapBox<Rational>`](super::mem::HeapBox) (`new_zeroed`, and `Clone`
//! through `av_memdup`), and an array of them a
//! [`HeapVec<Rational>`](super::mem::HeapVec) from
//! [`alloc_cell_array_zeroed`](super::mem::alloc_cell_array_zeroed). A field
//! of a wrapped C struct is reached as a [`RationalRef`] or [`RationalMut`]
//! over the projected place.
//!
//! The value is not reduced and either part may be zero or negative: C stores
//! `0/0` for "unset" (a zeroed context's time base), `1/0` and `-1/0` for the
//! infinities `av_d2q` returns, and unreduced fractions wherever its users
//! write them, so every accessor takes any pair of `int`s.
//!
//! [`reduce`] brings a 64-bit fraction to lowest terms within a bound, as
//! [`Reduced`]: the closest [`Rational`] whose terms fit, and whether it is
//! exact.

use core::fmt;
use core::ptr::{addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: AVRational
    ///
    /// A rational number as a pair of `int`s, neither reduced nor checked.
    /// Its getters are on [`RationalRef`] (`as_ref()`), its setters on
    /// [`RationalMut`] (`as_mut()`).
    #[derive(Clone, Copy)]
    Rational,
    RationalRef,
    RationalMut,
    ffi::AVRational
);

// SAFETY: all-zero is `0/0`, a value C itself stores for an unset rational
// (every zero-initialised context's time base) and every accessor accepts as
// the two integers it is; the type has no teardown to reject it.
unsafe impl CZeroable for Rational {}

// SAFETY: an `AVRational` is 8 bytes of plain data, two `int`s with no
// pointer, sub-allocation or tie to the thread that wrote them, so a value, or
// an exclusive handle to one, moves between threads as an `[i32; 2]` does.
unsafe impl Send for Rational {}
// SAFETY: as for `Rational`: the handle is the only access to the pair.
unsafe impl Send for RationalMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `RationalRef` has
// getters only, and the struct has no interior mutability, so concurrent
// shared access reads unchanging bytes, as through `&[i32; 2]`.
unsafe impl Sync for Rational {}
// SAFETY: as for `Sync for Rational`: copies of a shared handle only read.
unsafe impl Send for RationalRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for RationalRef<'_> {}
// SAFETY: `&RationalMut` reaches the pair only through `as_ref`, a shared
// handle; the setters need `&mut`.
unsafe impl Sync for RationalMut<'_> {}

impl Rational {
    /// Wraps: av_make_q
    ///
    /// The rational `num / den`, as given: not reduced, and a zero or
    /// negative denominator is kept.
    #[must_use]
    pub fn new(num: i32, den: i32) -> Self {
        // SAFETY: `av_make_q` takes both parts by value, has no
        // preconditions, and returns the struct by value.
        Self::from_raw(unsafe { ffi::av_make_q(num, den) })
    }

    /// The rational C encodes as `raw`, whatever its parts.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVRational) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this rational, for passing it by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVRational {
        self.0
    }
}

impl From<ffi::AVRational> for Rational {
    #[inline]
    fn from(raw: ffi::AVRational) -> Self {
        Self::from_raw(raw)
    }
}

impl From<Rational> for ffi::AVRational {
    #[inline]
    fn from(q: Rational) -> Self {
        q.into_raw()
    }
}

impl fmt::Debug for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for RationalRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rational")
            .field("num", &self.num())
            .field("den", &self.den())
            .finish()
    }
}

impl RationalRef<'_> {
    /// Field: AVRational.num
    ///
    /// The numerator.
    #[must_use]
    pub fn num(&self) -> i32 {
        // SAFETY: a read of the `num` field projected through the shared
        // handle's pointer to a live, initialised `AVRational`; every bit
        // pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).num).read() }
    }

    /// Field: AVRational.den
    ///
    /// The denominator, possibly zero or negative.
    #[must_use]
    pub fn den(&self) -> i32 {
        // SAFETY: a read of the `den` field projected through the shared
        // handle's pointer to a live, initialised `AVRational`; every bit
        // pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).den).read() }
    }

    /// A copy of the whole rational, independent of the place it was read
    /// from.
    #[must_use]
    pub fn get(&self) -> Rational {
        // SAFETY: a read of the whole live, initialised `AVRational` through
        // the shared handle's pointer; it is two `int`s, so the byte copy is
        // a complete, independent value.
        Rational::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl RationalMut<'_> {
    /// Field: AVRational.num
    ///
    /// Sets the numerator, leaving the denominator as it is.
    pub fn set_num(&mut self, num: i32) {
        // SAFETY: a write of the `num` field projected through the exclusive
        // handle's pointer to a live `AVRational`; the old value owns nothing
        // to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).num).write(num) }
    }

    /// Field: AVRational.den
    ///
    /// Sets the denominator, leaving the numerator as it is; zero and
    /// negative values are stored as given.
    pub fn set_den(&mut self, den: i32) {
        // SAFETY: a write of the `den` field projected through the exclusive
        // handle's pointer to a live `AVRational`; the old value owns nothing
        // to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).den).write(den) }
    }

    /// Overwrites both parts with `q`'s.
    pub fn set(&mut self, q: Rational) {
        // SAFETY: a write of the whole `AVRational` through the exclusive
        // handle's pointer to a live one; the old value owns nothing to leak.
        unsafe { self.as_mut_ptr().write(q.into_raw()) }
    }
}

/// A fraction [`reduce`] brought to lowest terms.
#[derive(Clone, Copy, Debug)]
pub struct Reduced {
    /// The fraction: its numerator carries the sign, and its denominator is
    /// never negative, 0 for a source with a zero denominator.
    pub value: Rational,
    /// Whether `value` is the source fraction itself rather than the closest
    /// approximation whose terms stay within the bound.
    pub exact: bool,
}

/// Wraps: av_reduce
///
/// `num / den` in lowest terms, both terms at most `max` in magnitude: the
/// exact fraction when it fits, and otherwise its closest approximation within
/// the bound (by continued fractions), with [`Reduced::exact`] cleared. A zero
/// `max` still yields terms of magnitude up to 1, as in C.
///
/// `None` for arguments C cannot reduce without undefined behaviour, which
/// the call never reaches:
///
/// - a negative `max`: C's result terms are `int`, and a negative bound gives
///   meaningless terms or overflows when negating them;
/// - `num` or `den` equal to `i64::MIN`, whose absolute value C takes with a
///   signed negation that overflows.
///
/// The bound is an `i32` because each term of the result is one: C narrows
/// the terms of a wider bound to `int` and overflows negating the result.
#[must_use]
pub fn reduce(num: i64, den: i64, max: i32) -> Option<Reduced> {
    if max < 0 || num == i64::MIN || den == i64::MIN {
        return None;
    }
    let (mut dst_num, mut dst_den) = (0, 0);
    // SAFETY: both out-parameters are live, writable locals C only stores to.
    // `num` and `den` are not `INT64_MIN` and `max` lies in `0..=INT_MAX`, so
    // every term stays within `int` and none of C's arithmetic overflows.
    let exact = unsafe { ffi::av_reduce(&mut dst_num, &mut dst_den, num, den, max.into()) };
    Some(Reduced {
        value: Rational::from_raw(ffi::AVRational {
            num: dst_num,
            den: dst_den,
        }),
        exact: exact != 0,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    use crate::libavutil::mem::HeapBox;

    #[test]
    fn layout_matches_the_c_struct() {
        assert_eq!(size_of::<Rational>(), size_of::<ffi::AVRational>());
        assert_eq!(align_of::<Rational>(), align_of::<ffi::AVRational>());
        assert_eq!((size_of::<Rational>(), align_of::<Rational>()), (8, 4));
        assert_eq!(offset_of!(ffi::AVRational, num), 0);
        assert_eq!(offset_of!(ffi::AVRational, den), 4);
        assert_eq!(size_of::<RationalRef<'_>>(), size_of::<*const ()>());
        assert_eq!(size_of::<RationalMut<'_>>(), size_of::<*const ()>());
    }

    #[test]
    fn value_is_send_sync_and_copy() {
        fn send_sync<T: Send + Sync>() {}
        fn copy<T: Copy>() {}
        send_sync::<Rational>();
        send_sync::<RationalRef<'_>>();
        send_sync::<RationalMut<'_>>();
        send_sync::<HeapBox<Rational>>();
        copy::<Rational>();
        copy::<RationalRef<'_>>();

        let a = Rational::from_raw(ffi::AVRational { num: 1, den: 2 });
        let mut b = a;
        b.as_mut().set_num(3);
        assert_eq!((a.as_ref().num(), b.as_ref().num()), (1, 3));
    }

    #[test]
    fn zeroed_is_zero_over_zero() {
        let z = Rational::zeroed();
        assert_eq!((z.as_ref().num(), z.as_ref().den()), (0, 0));
    }

    #[test]
    fn raw_conversions_round_trip_every_part() {
        for (num, den) in [
            (0, 0),
            (1, 0),
            (-1, 0),
            (i32::MIN, -1),
            (i32::MAX, i32::MIN),
            (6, 4),
        ] {
            let raw = ffi::AVRational { num, den };
            let q = Rational::from(raw);
            assert_eq!((q.as_ref().num(), q.as_ref().den()), (num, den));
            let back = ffi::AVRational::from(q);
            assert_eq!((back.num, back.den), (num, den));
            let back = Rational::from_raw(raw).into_raw();
            assert_eq!((back.num, back.den), (num, den));
        }
    }

    #[test]
    fn const_from_raw_builds_constants() {
        const MS: Rational = Rational::from_raw(ffi::AVRational { num: 1, den: 1000 });
        assert_eq!((MS.as_ref().num(), MS.as_ref().den()), (1, 1000));
        const RAW: ffi::AVRational = MS.into_raw();
        assert_eq!((RAW.num, RAW.den), (1, 1000));
    }

    #[test]
    fn setters_write_one_part_through_reborrows() {
        let mut q = Rational::zeroed();
        let mut m = q.as_mut();
        m.as_mut().set_num(-7);
        assert_eq!((m.as_ref().num(), m.as_ref().den()), (-7, 0));
        m.set_den(i32::MIN);
        assert_eq!((m.as_ref().num(), m.as_ref().den()), (-7, i32::MIN));
        m.set(Rational::from_raw(ffi::AVRational { num: 2, den: 3 }));
        assert_eq!((q.as_ref().num(), q.as_ref().den()), (2, 3));
    }

    #[test]
    fn get_copies_out_of_the_place() {
        let mut q = Rational::from_raw(ffi::AVRational { num: 5, den: 9 });
        let copy = q.as_ref().get();
        q.as_mut().set_num(0);
        assert_eq!((copy.as_ref().num(), copy.as_ref().den()), (5, 9));
        assert_eq!(q.as_ref().get().as_ref().num(), 0);
    }

    #[test]
    fn debug_shows_both_parts() {
        let q = Rational::from_raw(ffi::AVRational { num: -1, den: 0 });
        assert_eq!(format!("{q:?}"), "Rational { num: -1, den: 0 }");
        assert_eq!(format!("{:?}", q.as_ref()), format!("{q:?}"));
    }

    #[test]
    fn handles_borrow_a_c_owned_struct() {
        let mut raw = ffi::AVRational {
            num: 30000,
            den: 1001,
        };
        // SAFETY: `raw` is a live, initialised struct outliving the handle,
        // and no other access to it happens while the handle is used.
        let mut m = unsafe { RationalMut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!((m.as_ref().num(), m.as_ref().den()), (30000, 1001));
        m.set_den(1000);
        assert_eq!((raw.num, raw.den), (30000, 1000));
        // SAFETY: `raw` is live and only read while the shared handle lives.
        let r = unsafe { RationalRef::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(r.get().as_ref().den(), 1000);
        // SAFETY: NULL is rejected before anything is borrowed.
        assert!(unsafe { RationalRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { RationalMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn c_fills_an_inline_value_through_the_exclusive_handle() {
        // The path a wrapper over an `AVRational *` out-parameter takes
        // (`av_opt_get_q`, `av_parse_ratio`): C stores into Rust's inline
        // storage through the exclusive handle's pointer, and the handles
        // read it back afterwards.
        let mut q = Rational::zeroed();
        let mut m = q.as_mut();
        // SAFETY: both fields are projected from the exclusive handle's
        // pointer to a live `AVRational` nothing else accesses during the
        // call; `av_reduce` stores one `int` through each, and the terms
        // are within `int` with a positive bound, so none of its arithmetic
        // overflows.
        let exact = unsafe {
            ffi::av_reduce(
                addr_of_mut!((*m.as_mut_ptr()).num),
                addr_of_mut!((*m.as_mut_ptr()).den),
                60000,
                2002,
                i32::MAX.into(),
            )
        };
        assert_eq!(exact, 1);
        assert_eq!((m.as_ref().num(), m.as_ref().den()), (30000, 1001));
        let num = m.as_ref().num();
        m.set_num(-num);
        assert_eq!((q.as_ref().num(), q.as_ref().den()), (-30000, 1001));
        assert_eq!(q.as_ref().get().into_raw().den, 1001);
    }

    #[test]
    fn heap_boxed_rational_reads_writes_and_clones() {
        let mut a = HeapBox::<Rational>::new_zeroed().unwrap();
        assert_eq!((a.as_ref().num(), a.as_ref().den()), (0, 0));
        a.as_mut()
            .set(Rational::from_raw(ffi::AVRational { num: 1, den: 25 }));
        let b = a.clone();
        a.as_mut().set_den(50);
        assert_eq!((a.as_ref().den(), b.as_ref().den()), (50, 25));

        let mut slot = HeapBox::<Rational>::new_uninit().unwrap();
        // SAFETY: `as_c_ptr` addresses 8 writable, aligned bytes `slot` owns.
        unsafe { slot.as_c_ptr().write(ffi::AVRational { num: 4, den: 3 }) };
        // SAFETY: the write above initialised the whole struct.
        let c = unsafe { slot.assume_init() };
        assert_eq!((c.as_ref().num(), c.as_ref().den()), (4, 3));
    }

    /// A reduction's numerator, denominator and exactness.
    fn parts(r: Option<Reduced>) -> Option<(i32, i32, bool)> {
        r.map(|r| (r.value.as_ref().num(), r.value.as_ref().den(), r.exact))
    }

    #[test]
    fn arguments_c_would_overflow_on_are_rejected_before_the_call() {
        assert!(reduce(1, 2, -1).is_none());
        assert!(reduce(1, 2, i32::MIN).is_none());
        assert!(reduce(i64::MIN, 3, i32::MAX).is_none());
        assert!(reduce(3, i64::MIN, i32::MAX).is_none());
        assert!(reduce(i64::MIN, i64::MIN, 0).is_none());
    }

    #[test]
    fn reduces_to_lowest_terms() {
        assert_eq!(parts(reduce(-6, 4, i32::MAX)), Some((-3, 2, true)));
        assert_eq!(parts(reduce(6, -4, i32::MAX)), Some((-3, 2, true)));
        assert_eq!(parts(reduce(-6, -4, i32::MAX)), Some((3, 2, true)));
    }

    #[test]
    fn approximates_beyond_the_bound() {
        // 355/113 is the best approximation of pi with terms <= 1000.
        let pi = reduce(3_141_592_653_589_793, 1_000_000_000_000_000, 1000);
        assert_eq!(parts(pi), Some((355, 113, false)));
        let tiny = parts(reduce(1, i64::MAX, 100)).unwrap();
        assert_eq!((tiny.0, tiny.2), (0, false));
    }

    #[test]
    fn zero_terms_reduce_exactly() {
        assert_eq!(parts(reduce(0, 5, 10)), Some((0, 1, true)));
        assert_eq!(parts(reduce(7, 0, 10)), Some((1, 0, true)));
        assert_eq!(parts(reduce(0, 0, 10)), Some((0, 0, true)));
    }
}
