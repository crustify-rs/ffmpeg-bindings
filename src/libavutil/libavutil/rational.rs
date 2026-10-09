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
//!
//! Arithmetic and comparison go through C. `+`, `-`, `*` and `/` reduce their
//! result to lowest terms within `i32::MAX`, approximating it when it does not
//! fit; equality and order are by value (`1/2 == 2/4`), with `x/0` an
//! infinity and `0/0` unordered, like NaN. [`Rational::to_f64`] and
//! [`Rational::from_f64`] convert to and from floating point, and
//! [`Rational::nearest`] picks the candidate closest to a value.

use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, Div, Mul, Sub};
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

impl Rational {
    /// Wraps: av_inv_q
    ///
    /// The reciprocal `den / num`: the two parts swapped, so `0/d` becomes
    /// `d/0` and the sign stays where it was.
    #[must_use]
    pub fn recip(self) -> Self {
        // SAFETY: `av_inv_q` takes the struct by value and only swaps its
        // parts.
        Self::from_raw(unsafe { ffi::av_inv_q(self.into_raw()) })
    }

    /// Wraps: av_q2d
    ///
    /// The value as an `f64`: `num as f64 / den as f64`, so `x/0` is an
    /// infinity of `x`'s sign and `0/0` is NaN.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        // SAFETY: `av_q2d` takes the struct by value and divides in floating
        // point, which is defined for a zero denominator.
        unsafe { ffi::av_q2d(self.into_raw()) }
    }

    /// Wraps: av_d2q
    ///
    /// The closest rational to `d` whose terms are at most `max` in
    /// magnitude: `0/0` for NaN, `1/0` or `-1/0` beyond `±(i32::MAX + 3)`
    /// (the infinities included), and otherwise the reduction of `d` as a
    /// fraction over a power of two (see [`reduce`]).
    ///
    /// `None` for a negative `max`, which C's reduction cannot bound.
    #[must_use]
    pub fn from_f64(d: f64, max: i32) -> Option<Self> {
        if max < 0 {
            return None;
        }
        // SAFETY: `av_d2q` takes its arguments by value. A NaN or a magnitude
        // beyond `INT_MAX + 3` returns before any conversion; otherwise the
        // scaled `d` lies strictly within `int64_t` (`|d| < 2^e` over the
        // denominator `2^(63 - e)`), and a non-negative `max` keeps
        // `av_reduce` within `int`.
        Some(Self::from_raw(unsafe { ffi::av_d2q(d, max) }))
    }

    /// Wraps: av_nearer_q
    ///
    /// Compares how near `a` and `b` are to `self`: [`Less`](Ordering::Less)
    /// when `a` is nearer, [`Greater`](Ordering::Greater) when `b` is, and
    /// [`Equal`](Ordering::Equal) at the same distance. (C returns 1, -1 and 0
    /// for these.)
    ///
    /// `None` where C has no meaningful answer and overflows computing one:
    /// when both denominators are `i32::MIN` (C doubles their product), and
    /// when `a` and `b` are unordered ([`partial_cmp`](PartialOrd::partial_cmp)
    /// is `None`, as with a `0/0`), where C multiplies `INT_MIN` by -1, 0 or
    /// 1: an overflow, 0 or `INT_MIN`, no verdict in any case. A negative
    /// denominator in `self`, or in exactly one of `a` and `b`, gives a
    /// meaningless but defined answer, as in C.
    #[must_use]
    pub fn cmp_distance(self, a: Self, b: Self) -> Option<Ordering> {
        let (ra, rb) = (a.into_raw(), b.into_raw());
        // C doubles the product of the denominators and multiplies its
        // verdict by `av_cmp_q(b, a)`, which is `INT_MIN` for unordered
        // values.
        if (ra.den == i32::MIN && rb.den == i32::MIN) || b.partial_cmp(&a).is_none() {
            return None;
        }
        // SAFETY: `av_nearer_q` takes the structs by value. With at most one
        // denominator `INT_MIN`, `2 * q1.den * q2.den` and the median's
        // numerator stay within `int64_t`; with `b` and `a` ordered,
        // `av_cmp_q(b, a)` is -1, 0 or 1, so the final product cannot
        // overflow.
        match unsafe { ffi::av_nearer_q(self.into_raw(), ra, rb) } {
            1 => Some(Ordering::Less),
            0 => Some(Ordering::Equal),
            _ => Some(Ordering::Greater),
        }
    }

    /// Wraps: av_find_nearest_q_idx
    ///
    /// The index of the candidate nearest `self`, the first one on a tie, as
    /// [`cmp_distance`](Self::cmp_distance) orders them.
    ///
    /// C reads its list up to an entry with a zero denominator (its `{0, 0}`
    /// terminator), so only the candidates before the first such entry are
    /// compared; `None` when there are none. `None` too when C would overflow
    /// comparing two candidates whose denominators are both `i32::MIN`
    /// (see [`cmp_distance`](Self::cmp_distance)): C compares the first
    /// candidate with itself and every later one with the nearest so far, so
    /// a first candidate with that denominator overflows, and a list holding
    /// two of them is walked as C walks it to find out whether it compares
    /// them with each other.
    #[must_use]
    pub fn nearest(self, candidates: &[Self]) -> Option<usize> {
        let end = candidates
            .iter()
            .position(|q| q.as_ref().den() == 0)
            .unwrap_or(candidates.len());
        let list = &candidates[..end];
        if list.first()?.as_ref().den() == i32::MIN {
            return None;
        }
        if list.iter().filter(|q| q.as_ref().den() == i32::MIN).count() > 1 {
            // C's loop with C's own comparisons: `cmp_distance` is `None`
            // exactly for the pair C overflows on (no denominator is zero).
            let mut nearest = 0;
            for (i, &q) in list.iter().enumerate() {
                if self.cmp_distance(q, list[nearest])? == Ordering::Less {
                    nearest = i;
                }
            }
        }
        let terminated: Vec<ffi::AVRational> = list
            .iter()
            .map(|q| q.into_raw())
            .chain([ffi::AVRational { num: 0, den: 0 }])
            .collect();
        // SAFETY: `terminated` is a live array of `AVRational`s ending in the
        // `{0, 0}` entry, the only one with a zero denominator, which C reads
        // up to and never writes. Every comparison C makes is between two
        // entries with nonzero denominators, never both `INT_MIN`: the first
        // entry's is not, and with two such entries the walk above made C's
        // comparisons and found none of them such a pair. `av_nearer_q`
        // handles those without overflow (see `cmp_distance`).
        let i = unsafe { ffi::av_find_nearest_q_idx(self.into_raw(), terminated.as_ptr()) };
        usize::try_from(i).ok()
    }

    /// Wraps: av_q2intfloat
    ///
    /// The bits of the IEEE 754 single-precision float nearest the value,
    /// ties away from zero: C computes them in integer arithmetic, so they do
    /// not depend on the platform's floating point. `0/0` is the quiet NaN
    /// `0xffc0_0000`, `0/d` is `+0.0`, and `x/0` is the infinity of `x`'s
    /// sign.
    ///
    /// `None` when either part is `i32::MIN`, whose magnitude C cannot take.
    ///
    /// Diverges from C for `x/0` with `x` negative: C drops the sign and
    /// returns `+inf`; this returns `-inf` (`0xff80_0000`).
    #[must_use]
    pub fn to_f32_bits(self) -> Option<u32> {
        let ffi::AVRational { num, den } = self.into_raw();
        if num == i32::MIN || den == i32::MIN {
            return None;
        }
        // C normalizes the sign into the numerator, then takes its magnitude
        // and shifts the sign into bit 31 of an `int`, which overflows. C
        // computes the other 31 bits from the magnitudes alone, so they come
        // from the non-negative value, and the sign bit is set here.
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        let (num, sign) = if num < 0 { (-num, 1 << 31) } else { (num, 0) };
        // SAFETY: `av_q2intfloat` takes the struct by value. Both parts are
        // non-negative, so C negates neither and its sign is 0; its shifts
        // stay below 64 bits for `int` terms.
        let bits = unsafe { ffi::av_q2intfloat(ffi::AVRational { num, den }) };
        Some(bits | sign)
    }

    /// Wraps: av_gcd_q
    ///
    /// The largest rational of which both `self` and `other` are integer
    /// multiples, the greatest common divisor of the numerators over the
    /// least common multiple of the denominators, when that multiple is below
    /// `max_den`; `None` otherwise.
    ///
    /// `None` too when both denominators are zero, which C divides by. A
    /// zero denominator in one of them gives a zero multiple, as in C.
    #[must_use]
    pub fn gcd(self, other: Self, max_den: i32) -> Option<Self> {
        let (a, b) = (self.into_raw(), other.into_raw());
        if a.den == 0 && b.den == 0 {
            return None;
        }
        // C's decision, computed as C does: `av_gcd` is zero only for two
        // zero denominators, and each `int` quotient times an `int` fits.
        let g = super::mathematics::gcd(a.den.into(), b.den.into());
        let lcm = (i64::from(a.den) / g) * i64::from(b.den);
        if lcm >= i64::from(max_den) {
            return None;
        }
        // SAFETY: `av_gcd_q` takes the structs by value; with a nonzero
        // denominator `av_gcd(a.den, b.den)` is nonzero, so its one division
        // is defined, and `lcm < max_den` selects the computed result rather
        // than `def`.
        Some(Self::from_raw(unsafe { ffi::av_gcd_q(a, b, max_den, a) }))
    }
}

/// Value equality, as C compares: `1/2 == 2/4`, `1/0 == 2/0`, and `0/0`
/// equals nothing, itself included, like NaN.
impl PartialEq for Rational {
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl PartialOrd for Rational {
    /// Wraps: av_cmp_q
    ///
    /// Orders the values, with `x/0` as the infinity of `x`'s sign. `None`
    /// where C reports `INT_MIN`: when either is `0/0`, or one is `0/d` and
    /// the other `0/0`.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // SAFETY: `av_cmp_q` takes the structs by value; its cross products
        // are computed in `int64_t`, where their difference cannot overflow
        // for `int` terms.
        match unsafe { ffi::av_cmp_q(self.into_raw(), other.into_raw()) } {
            0 => Some(Ordering::Equal),
            1 => Some(Ordering::Greater),
            -1 => Some(Ordering::Less),
            _ => None,
        }
    }
}

impl Add for Rational {
    type Output = Self;

    /// Wraps: av_add_q
    ///
    /// The sum, in lowest terms when it fits, and otherwise its closest
    /// approximation with terms within `i32::MAX` (see [`reduce`]).
    ///
    /// `i32::MIN/i32::MIN + i32::MIN/i32::MIN` overflows in C; it is computed
    /// as `1/1 + 1/1`, the same value, which C reduces to the same `2/1`.
    fn add(self, rhs: Self) -> Self {
        let (mut b, mut c) = (self.into_raw(), rhs.into_raw());
        if [b.num, b.den, c.num, c.den] == [i32::MIN; 4] {
            b = ffi::AVRational { num: 1, den: 1 };
            c = b;
        }
        // SAFETY: `av_add_q` takes the structs by value. Each cross product
        // is at most 2^62 in magnitude, reached only by `INT_MIN * INT_MIN`;
        // their sum reaches 2^63 only for the operands replaced above, so it
        // fits `int64_t`, and `av_reduce` with `INT_MAX` handles every
        // fraction of such terms.
        Self::from_raw(unsafe { ffi::av_add_q(b, c) })
    }
}

impl Sub for Rational {
    type Output = Self;

    /// Wraps: av_sub_q
    ///
    /// The difference, in lowest terms when it fits, and otherwise its
    /// closest approximation with terms within `i32::MAX` (see [`reduce`]).
    fn sub(self, rhs: Self) -> Self {
        // SAFETY: `av_sub_q` takes the structs by value. The difference of the
        // cross products lies within `±(2^63 - 2^31)`, since only
        // `INT_MIN * INT_MIN` reaches 2^62 and a product reaches at most
        // `2^62 - 2^31` below zero, and `av_reduce` with `INT_MAX` handles
        // every fraction of such terms.
        Self::from_raw(unsafe { ffi::av_sub_q(self.into_raw(), rhs.into_raw()) })
    }
}

impl Mul for Rational {
    type Output = Self;

    /// Wraps: av_mul_q
    ///
    /// The product, in lowest terms when it fits, and otherwise its closest
    /// approximation with terms within `i32::MAX` (see [`reduce`]).
    fn mul(self, rhs: Self) -> Self {
        // SAFETY: `av_mul_q` takes the structs by value; each product of two
        // `int`s fits `int64_t`, and `av_reduce` with `INT_MAX` handles every
        // fraction of such terms.
        Self::from_raw(unsafe { ffi::av_mul_q(self.into_raw(), rhs.into_raw()) })
    }
}

impl Div for Rational {
    type Output = Self;

    /// Wraps: av_div_q
    ///
    /// The quotient, `self` times `rhs`'s reciprocal, in lowest terms when it
    /// fits, and otherwise its closest approximation with terms within
    /// `i32::MAX`; dividing by `0/d` gives an infinity or `0/0`.
    fn div(self, rhs: Self) -> Self {
        // SAFETY: `av_div_q` takes the structs by value and multiplies by the
        // swapped divisor through `av_mul_q`, defined for every `int` term
        // (see `mul`).
        Self::from_raw(unsafe { ffi::av_div_q(self.into_raw(), rhs.into_raw()) })
    }
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

    /// `num/den`, for brevity.
    fn q(num: i32, den: i32) -> Rational {
        Rational::new(num, den)
    }

    /// A rational's parts.
    fn nd(r: Rational) -> (i32, i32) {
        (r.as_ref().num(), r.as_ref().den())
    }

    #[test]
    fn equality_and_order_are_by_value() {
        assert_eq!(q(1, 2), q(2, 4));
        assert_eq!(q(1, 2), q(-1, -2));
        assert_eq!(q(1, 0), q(7, 0));
        assert_ne!(q(1, 0), q(-1, 0));
        assert!(q(-1, 0) < q(i32::MIN, 1));
        assert!(q(i32::MAX, 1) < q(1, 0));
        assert!(q(1, 3) < q(1, 2));
        assert!(q(1, -3) > q(1, -2));
        // `0/0` is unordered, against itself too, like NaN.
        assert_ne!(q(0, 0), q(0, 0));
        assert_eq!(q(0, 0).partial_cmp(&q(0, 0)), None);
        assert_eq!(q(0, 0).partial_cmp(&q(1, 2)), None);
        assert_eq!(q(0, 0).partial_cmp(&q(5, 0)), None);
        assert_eq!(q(0, 5).partial_cmp(&q(0, -3)), Some(Ordering::Equal));
        assert_eq!(q(0, 5).partial_cmp(&q(3, 0)), Some(Ordering::Less));
    }

    #[test]
    fn arithmetic_reduces_and_approximates() {
        assert_eq!(nd(q(1, 2) + q(1, 3)), (5, 6));
        assert_eq!(nd(q(1, 2) - q(1, 3)), (1, 6));
        assert_eq!(nd(q(2, 3) * q(3, 4)), (1, 2));
        assert_eq!(nd(q(2, 3) / q(4, 3)), (1, 2));
        assert_eq!(nd(q(1, 2) / q(0, 1)), (1, 0));
        assert_eq!(nd(q(-1, 2) / q(0, 1)), (-1, 0));
        // Beyond `i32` terms, the closest approximation that fits.
        let r = q(1, i32::MAX) * q(1, i32::MAX);
        assert_eq!(nd(r), (0, 1));
    }

    #[test]
    fn min_over_min_sum_is_computed_as_one_plus_one() {
        // C overflows `INT64_MAX` forming this sum's numerator.
        let m = q(i32::MIN, i32::MIN);
        assert_eq!(nd(m + m), (2, 1));
        assert_eq!(nd(m + q(1, 1)), (2, 1));
        assert_eq!(nd(m - m), (0, 1));
        assert_eq!(nd(m * m), (1, 1));
        assert_eq!(nd(m / m), (1, 1));
    }

    #[test]
    fn recip_and_float_conversions() {
        assert_eq!(nd(q(0, 5).recip()), (5, 0));
        assert_eq!(nd(q(-3, 7).recip()), (7, -3));
        assert_eq!(q(1, 4).to_f64(), 0.25);
        assert_eq!(q(-1, 0).to_f64(), f64::NEG_INFINITY);
        assert!(q(0, 0).to_f64().is_nan());
        assert_eq!(Rational::from_f64(0.25, 100).map(nd), Some((1, 4)));
        assert_eq!(Rational::from_f64(f64::NAN, 100).map(nd), Some((0, 0)));
        assert_eq!(Rational::from_f64(-1e300, 100).map(nd), Some((-1, 0)));
        assert_eq!(Rational::from_f64(f64::INFINITY, 0).map(nd), Some((1, 0)));
        assert!(Rational::from_f64(0.5, -1).is_none());
        assert!(Rational::from_f64(0.5, i32::MIN).is_none());
    }

    #[test]
    fn cmp_distance_rejects_what_c_overflows_on() {
        assert_eq!(q(1, 3).cmp_distance(q(1, 4), q(1, 2)), Some(Ordering::Less));
        assert_eq!(
            q(1, 3).cmp_distance(q(1, 2), q(1, 4)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            q(1, 2).cmp_distance(q(1, 4), q(3, 4)),
            Some(Ordering::Equal)
        );
        // Unordered candidates: C multiplies its verdict by `INT_MIN`.
        assert_eq!(q(1, 2).cmp_distance(q(1, 3), q(0, 0)), None);
        assert_eq!(q(1, 2).cmp_distance(q(0, 0), q(1, 3)), None);
        // Both denominators `INT_MIN`: C doubles their product.
        let m = q(1, i32::MIN);
        assert_eq!(q(1, 2).cmp_distance(m, m), None);
        assert!(q(1, 2).cmp_distance(m, q(1, 3)).is_some());
    }

    #[test]
    fn nearest_scans_to_the_first_zero_denominator() {
        let rates = [q(24000, 1001), q(24, 1), q(25, 1), q(30000, 1001)];
        assert_eq!(q(25, 1).nearest(&rates), Some(2));
        assert_eq!(q(239, 10).nearest(&rates), Some(0));
        assert_eq!(q(1000, 1).nearest(&rates), Some(3));
        // Ties go to the first candidate.
        assert_eq!(q(1, 2).nearest(&[q(1, 4), q(3, 4)]), Some(0));
        assert_eq!(q(1, 2).nearest(&[]), None);
        // A zero denominator ends C's list.
        assert_eq!(q(25, 1).nearest(&[q(0, 0), q(25, 1)]), None);
        assert_eq!(q(25, 1).nearest(&[q(24, 1), q(7, 0), q(25, 1)]), Some(0));
    }

    #[test]
    fn nearest_rejects_two_min_denominators_only_when_c_compares_them() {
        let m = q(1, i32::MIN);
        assert_eq!(q(1, 2).nearest(&[m]), None);
        // Against a nearest so far with a positive denominator, C's verdict
        // on `m` is `-av_cmp_q(nearest, m)`: `m` (just below zero) takes
        // over from `-5/1`, and the second `m` is then compared with it.
        assert_eq!(q(1, 2).nearest(&[q(-5, 1), m, m]), None);
        assert_eq!(q(1, 2).nearest(&[q(-5, 1), m, q(-7, 1), m]), None);
        // It never takes over from `1/3`, so C never compares the two.
        assert_eq!(q(1, 2).nearest(&[q(1, 3), m, m]), Some(0));
        assert_eq!(q(1, 3).nearest(&[q(2, 3), m, q(1, 3), m]), Some(2));
        // One, not first, is never compared with itself. (Its negative
        // denominator makes C's verdict meaningless, but defined.)
        assert!(q(0, 1).nearest(&[q(1, 3), m]).is_some());
        // One past the terminator is not read.
        assert_eq!(
            q(0, 1).nearest(&[q(1, 3), m, q(0, 0), m]),
            q(0, 1).nearest(&[q(1, 3), m])
        );
    }

    #[test]
    fn f32_bits_match_rust_floats() {
        for (num, den) in [
            (1, 2),
            (-1, 2),
            (1, -2),
            (-3, -4),
            (1, 3),
            (-2, 3),
            (i32::MAX, 1),
            (1, i32::MAX),
            (-7, 1),
            (30000, 1001),
        ] {
            let want = (num as f32 / den as f32).to_bits();
            assert_eq!(q(num, den).to_f32_bits(), Some(want), "{num}/{den}");
        }
        assert_eq!(q(0, 0).to_f32_bits(), Some(0xffc0_0000));
        assert_eq!(q(0, 5).to_f32_bits(), Some(0));
        assert_eq!(q(0, -5).to_f32_bits(), Some(0));
        assert_eq!(q(3, 0).to_f32_bits(), Some(f32::INFINITY.to_bits()));
    }

    #[test]
    fn f32_bits_keep_the_sign_of_negative_infinity() {
        // C returns `+inf` here: it strips the sign before testing `den == 0`.
        assert_eq!(q(-3, 0).to_f32_bits(), Some(f32::NEG_INFINITY.to_bits()));
    }

    #[test]
    fn f32_bits_reject_min_parts() {
        assert_eq!(q(i32::MIN, 1).to_f32_bits(), None);
        assert_eq!(q(1, i32::MIN).to_f32_bits(), None);
        assert_eq!(
            q(i32::MIN + 1, 1).to_f32_bits(),
            Some((-(i32::MAX as f32)).to_bits())
        );
    }

    #[test]
    fn gcd_is_the_common_step() {
        assert_eq!(q(1, 24).gcd(q(1, 30), 1000).map(nd), Some((1, 120)));
        assert_eq!(q(3, 4).gcd(q(9, 8), 1000).map(nd), Some((3, 8)));
        // The multiple must be below `max_den`.
        assert_eq!(q(1, 24).gcd(q(1, 30), 120), None);
        assert_eq!(q(1, 24).gcd(q(1, 30), 121).map(nd), Some((1, 120)));
        // C divides by zero for two zero denominators.
        assert_eq!(q(1, 0).gcd(q(2, 0), i32::MAX), None);
        // One zero denominator: the multiple is 0.
        assert_eq!(q(4, 0).gcd(q(6, 5), 1).map(nd), Some((2, 0)));
        assert_eq!(q(4, 5).gcd(q(6, 0), 1).map(nd), Some((2, 0)));
    }

    #[test]
    fn rejections_before_the_call_touch_no_c() {
        // Built without `av_make_q`, so the test runs where C cannot (Miri).
        let r = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let m = r(1, i32::MIN);
        assert_eq!(r(1, 2).nearest(&[]), None);
        assert_eq!(r(1, 2).nearest(&[r(0, 0), r(1, 2)]), None);
        assert_eq!(r(1, 2).nearest(&[r(9, 0)]), None);
        assert_eq!(r(1, 2).nearest(&[m, r(1, 3)]), None);
        assert_eq!(r(1, 2).cmp_distance(m, m), None);
        assert_eq!(r(i32::MIN, 3).to_f32_bits(), None);
        assert_eq!(r(3, i32::MIN).to_f32_bits(), None);
        assert_eq!(r(1, 0).gcd(r(1, 0), i32::MAX), None);
        assert_eq!(r(0, 0).gcd(r(5, 0), 0), None);
        assert!(Rational::from_f64(1.0, -1).is_none());
    }
}
