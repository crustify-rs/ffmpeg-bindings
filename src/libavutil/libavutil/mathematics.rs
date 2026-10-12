//! Safe wrappers over `libavutil/mathematics.h`.
//!
//! [`Rounding`] is the rounding argument of the rescaling functions: one of
//! five methods, optionally with the pass-through of `i64::MIN` / `i64::MAX`.
//!
//! [`compare_mod`], [`bessel_i0`] and [`gcd`] (the integer greatest common
//! divisor) are plain numeric functions over the C implementations, and
//! [`rescale`] computes `a * b / c` without intermediate overflow, rounding to
//! nearest. [`rescale_rounded`] takes the rounding, and
//! [`rescale_rational`] and [`rescale_rational_rounded`] multiply by a ratio
//! of two [`Rational`]s, converting a timestamp between time bases.
//!
//! [`compare_timestamps`], [`rescale_delta`] and [`add_stable`] work on
//! timestamps, each in its own time base. Each wrapper rejects, with `None`,
//! the arguments on which C would overflow, divide by zero or abort, before
//! the call.

use core::cmp::Ordering;
use core::fmt;

use super::ffi;
use super::rational::Rational;

/// Wraps: AVRounding
///
/// The rounding of a rescale: a method ([`ZERO`](Self::ZERO),
/// [`INF`](Self::INF), [`DOWN`](Self::DOWN), [`UP`](Self::UP) or
/// [`NEAR_INF`](Self::NEAR_INF)), optionally combined with
/// [`pass_min_max`](Self::pass_min_max).
///
/// C's `enum AVRounding` mixes the methods with a flag bit, so any `unsigned`
/// fits it, and `av_rescale_rnd` returns `INT64_MIN` for any value that is not
/// a method, alone or ORed with `AV_ROUND_PASS_MINMAX`. A `Rounding` holds
/// only those accepted values; [`TryFrom`] rejects the others, and the value
/// passes to C unchanged through [`From`], as a layout-compatible newtype.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rounding(ffi::AVRounding);

impl Rounding {
    /// Round toward zero (`AV_ROUND_ZERO`).
    pub const ZERO: Self = Self(ffi::AVRounding::AV_ROUND_ZERO);
    /// Round away from zero (`AV_ROUND_INF`).
    pub const INF: Self = Self(ffi::AVRounding::AV_ROUND_INF);
    /// Round toward negative infinity (`AV_ROUND_DOWN`).
    pub const DOWN: Self = Self(ffi::AVRounding::AV_ROUND_DOWN);
    /// Round toward positive infinity (`AV_ROUND_UP`).
    pub const UP: Self = Self(ffi::AVRounding::AV_ROUND_UP);
    /// Round to nearest, halfway cases away from zero (`AV_ROUND_NEAR_INF`).
    pub const NEAR_INF: Self = Self(ffi::AVRounding::AV_ROUND_NEAR_INF);

    /// The `AV_ROUND_PASS_MINMAX` bit.
    const PASS_MINMAX: u32 = ffi::AVRounding::AV_ROUND_PASS_MINMAX.0;

    /// Returns this rounding with `AV_ROUND_PASS_MINMAX` set: a rescale passes
    /// `i64::MIN` and `i64::MAX` (and so `AV_NOPTS_VALUE`) through unchanged,
    /// and rounds every other value by this method.
    #[must_use]
    pub const fn pass_min_max(self) -> Self {
        Self(ffi::AVRounding(self.0.0 | Self::PASS_MINMAX))
    }

    /// Whether `AV_ROUND_PASS_MINMAX` is set.
    pub const fn passes_min_max(self) -> bool {
        self.0.0 & Self::PASS_MINMAX != 0
    }

    /// The rounding method alone, with `AV_ROUND_PASS_MINMAX` cleared.
    #[must_use]
    pub const fn method(self) -> Self {
        Self(ffi::AVRounding(self.0.0 & !Self::PASS_MINMAX))
    }

    /// The C value, for a raw call.
    pub const fn as_raw(self) -> ffi::AVRounding {
        self.0
    }

    /// The value as `Rounding`, when `av_rescale_rnd` accepts it: a method,
    /// alone or with `AV_ROUND_PASS_MINMAX`.
    pub const fn from_raw(raw: ffi::AVRounding) -> Option<Self> {
        match raw.0 & !Self::PASS_MINMAX {
            0..=3 | 5 => Some(Self(raw)),
            _ => None,
        }
    }

    /// The method's C name.
    const fn method_name(self) -> &'static str {
        match self.method().0 {
            ffi::AVRounding::AV_ROUND_ZERO => "ZERO",
            ffi::AVRounding::AV_ROUND_INF => "INF",
            ffi::AVRounding::AV_ROUND_DOWN => "DOWN",
            ffi::AVRounding::AV_ROUND_UP => "UP",
            _ => "NEAR_INF",
        }
    }
}

impl fmt::Debug for Rounding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Rounding::")?;
        f.write_str(self.method_name())?;
        if self.passes_min_max() {
            f.write_str(".pass_min_max()")?;
        }
        Ok(())
    }
}

impl From<Rounding> for ffi::AVRounding {
    fn from(rounding: Rounding) -> Self {
        rounding.as_raw()
    }
}

impl TryFrom<ffi::AVRounding> for Rounding {
    type Error = InvalidRounding;

    fn try_from(raw: ffi::AVRounding) -> Result<Self, Self::Error> {
        Self::from_raw(raw).ok_or(InvalidRounding(raw.0))
    }
}

/// An `enum AVRounding` value that is not a rounding method, alone or with
/// `AV_ROUND_PASS_MINMAX`; `av_rescale_rnd` rejects it with `INT64_MIN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidRounding(pub u32);

impl fmt::Display for InvalidRounding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid AVRounding value {:#x}", self.0)
    }
}

impl std::error::Error for InvalidRounding {}

/// Wraps: av_compare_mod
///
/// Compares `a % m` with `b % m` for a power-of-two `m`: the wrapped
/// difference of the low `log2(m)` bits of `a` and `b`, read as a signed
/// value in `1 - m / 2..=m / 2`, so it is negative, zero or positive as `a`
/// lags, matches or leads `b` modulo `m` (as sequence numbers that wrap).
///
/// A modulus that is not a power of two gives a meaningless but defined
/// result, as in C; `m == 0` compares the whole 64 bits.
#[inline]
pub fn compare_mod(a: u64, b: u64, m: u64) -> i64 {
    // SAFETY: `av_compare_mod` takes and returns integers by value and
    // touches no memory; its arithmetic is unsigned, defined for every input.
    unsafe { ffi::av_compare_mod(a, b, m) }
}

/// Wraps: av_bessel_i0
///
/// The zeroth-order modified Bessel function of the first kind, `I0(x)`,
/// from C's minimax rational approximations. Even in `x`; NaN yields NaN and
/// a large `|x|` overflows to infinity.
#[inline]
pub fn bessel_i0(x: f64) -> f64 {
    // SAFETY: `av_bessel_i0` takes and returns a double by value and reads
    // only its own constant tables.
    unsafe { ffi::av_bessel_i0(x) }
}

/// Wraps: av_gcd
///
/// The greatest common divisor of `a` and `b`, up to sign: non-negative when
/// both are, `b` when `a` is 0 and `a` when `b` is 0 (so `gcd(0, 0) == 0`). A
/// divisor of `2^63` comes back as `i64::MIN`, its bit pattern: `gcd(i64::MIN,
/// i64::MIN)` is `i64::MIN`.
#[must_use]
pub fn gcd(a: i64, b: i64) -> i64 {
    // SAFETY: `av_gcd` takes its arguments by value and has no preconditions.
    unsafe { ffi::av_gcd(a, b) }
}

/// Wraps: av_rescale
///
/// `a * b / c`, computed without intermediate overflow and rounded to
/// nearest, halfway cases away from zero ([`Rounding::NEAR_INF`]).
///
/// `None` when C rejects the arguments or the result does not fit an `i64`:
/// `c <= 0`, `b < 0`, or a quotient beyond the `i64` range. C reports each of
/// these as `INT64_MIN`, which is never a rescaled value: the smallest result
/// it returns is `-i64::MAX`.
#[must_use]
pub fn rescale(a: i64, b: i64, c: i64) -> Option<i64> {
    // SAFETY: `av_rescale` takes integers by value, reads no memory and
    // handles every argument, rejecting with `INT64_MIN` what it cannot
    // rescale.
    let r = unsafe { ffi::av_rescale(a, b, c) };
    (r != i64::MIN).then_some(r)
}

/// C's `INT64_MIN` result of a rescale, read as the value it is: the
/// pass-through of `a == i64::MIN` under [`Rounding::pass_min_max`] when C
/// accepted `b` and `c`, and otherwise C's report that the arguments are
/// rejected or the quotient does not fit.
fn rescaled(r: i64, a: i64, b: i64, c: i64, rounding: Rounding) -> Option<i64> {
    let passed = a == i64::MIN && rounding.passes_min_max() && b >= 0 && c > 0;
    (r != i64::MIN || passed).then_some(r)
}

/// Wraps: av_rescale_rnd
///
/// `a * b / c`, computed without intermediate overflow and rounded by
/// `rounding`. Under [`Rounding::pass_min_max`], `i64::MIN` and `i64::MAX`
/// pass through unchanged.
///
/// `None` when C rejects the arguments or the result does not fit an `i64`:
/// `c <= 0`, `b < 0`, or a quotient beyond the `i64` range. C reports each of
/// these as `INT64_MIN`, which a rescale otherwise only returns as the
/// passed-through `i64::MIN`.
#[must_use]
pub fn rescale_rounded(a: i64, b: i64, c: i64, rounding: Rounding) -> Option<i64> {
    // SAFETY: `av_rescale_rnd` takes integers by value, reads no memory and
    // handles every argument, rejecting with `INT64_MIN` what it cannot
    // rescale; `rounding` is one of the values it accepts.
    let r = unsafe { ffi::av_rescale_rnd(a, b, c, rounding.into()) };
    rescaled(r, a, b, c, rounding)
}

/// Wraps: av_rescale_q_rnd
///
/// `a * bq / cq`, the value `a` in units of `bq` expressed in units of `cq`
/// (a timestamp from one time base to another), rounded by `rounding`. Under
/// [`Rounding::pass_min_max`], `i64::MIN` (`AV_NOPTS_VALUE`) and `i64::MAX`
/// pass through unchanged.
///
/// `None` as for [`rescale_rounded`] of `a`, `bq.num * cq.den` and
/// `cq.num * bq.den`: in particular when that divisor is not positive or that
/// multiplier is negative, as for a time base with a non-positive part.
#[must_use]
pub fn rescale_rational_rounded(
    a: i64,
    bq: Rational,
    cq: Rational,
    rounding: Rounding,
) -> Option<i64> {
    let (b, c) = (bq.into_raw(), cq.into_raw());
    // SAFETY: `av_rescale_q_rnd` takes its arguments by value, forms the
    // multiplier and divisor as products of two `int`s in `int64_t`, which
    // fit, and passes them to `av_rescale_rnd` (see `rescale_rounded`).
    let r = unsafe { ffi::av_rescale_q_rnd(a, b, c, rounding.into()) };
    rescaled(
        r,
        a,
        i64::from(b.num) * i64::from(c.den),
        i64::from(c.num) * i64::from(b.den),
        rounding,
    )
}

/// Wraps: av_rescale_q
///
/// [`rescale_rational_rounded`] rounding to nearest, halfway cases away from
/// zero ([`Rounding::NEAR_INF`]): `a` in units of `bq` expressed in units of
/// `cq`.
///
/// `None` as for [`rescale_rational_rounded`]; `i64::MIN` is rescaled as
/// `-i64::MAX`, never passed through.
#[must_use]
pub fn rescale_rational(a: i64, bq: Rational, cq: Rational) -> Option<i64> {
    // SAFETY: as for `rescale_rational_rounded`, with `AV_ROUND_NEAR_INF`.
    let r = unsafe { ffi::av_rescale_q(a, bq.into_raw(), cq.into_raw()) };
    (r != i64::MIN).then_some(r)
}

/// Wraps: av_compare_ts
///
/// Orders the timestamp `ts_a` in time base `tb_a` against `ts_b` in `tb_b`.
///
/// As in C, the answer is meaningful only when each timestamp, expressed in
/// the other's time base, fits an `i64`, and both time bases are positive;
/// otherwise it is an arbitrary but defined ordering.
#[must_use]
pub fn compare_timestamps(ts_a: i64, tb_a: Rational, ts_b: i64, tb_b: Rational) -> Ordering {
    // SAFETY: `av_compare_ts` takes its arguments by value. It multiplies the
    // timestamps directly only when they and the `int` cross products of the
    // time bases are all within `INT_MAX`, and otherwise compares through
    // `av_rescale_rnd`, which handles every argument.
    unsafe { ffi::av_compare_ts(ts_a, tb_a.into_raw(), ts_b, tb_b.into_raw()) }.cmp(&0)
}

/// Wraps: av_rescale_delta
///
/// Rescales the timestamp `in_ts` from `in_tb` to `out_tb`, keeping
/// successive packets' known durations: call it once per packet of a stream,
/// with `duration` the packet's length in units of `fs_tb` (samples,
/// typically), and the same `last` every time. `last` is the stream's state,
/// a timestamp in `fs_tb`: start it at `i64::MIN` (`AV_NOPTS_VALUE`).
///
/// While the position `last` predicts lies within one and a half units of
/// `in_tb` of `in_ts`, the result follows `last` (and so the summed
/// durations), clipped to within half a unit of `in_ts`, rather than the
/// rounded `in_ts`; otherwise, or when `in_tb` is no coarser than `out_tb`,
/// it is a plain [`rescale_rational`].
///
/// `None`, with `last` unchanged, for arguments C rejects or overflows on:
/// `in_ts == i64::MIN` and a negative `duration` (on which C aborts the
/// process), and a step whose intermediate timestamps leave the `i64` range
/// (`in_ts` beyond `±(2^62 - 1)` once rounding is needed, or a rescaled
/// value too close to `i64::MAX` to add `duration` to). `None` also when C
/// cannot represent the result in `out_tb`; `last` is then updated as C
/// updates it.
pub fn rescale_delta(
    in_tb: Rational,
    in_ts: i64,
    fs_tb: Rational,
    duration: i32,
    last: &mut i64,
    out_tb: Rational,
) -> Option<i64> {
    if in_ts == i64::MIN || duration < 0 {
        return None;
    }
    // C's control flow, run ahead with the same C rescales to find the
    // additions it would overflow; `None` from a rescale is C's `INT64_MIN`.
    let rnd = |a, r| rescale_rational_rounded(a, in_tb, fs_tb, r).unwrap_or(i64::MIN);
    let simple = || {
        rnd(in_ts, Rounding::NEAR_INF)
            .checked_add(duration.into())
            .map(drop)
    };
    let (i, o) = (in_tb.into_raw(), out_tb.into_raw());
    if *last == i64::MIN
        || duration == 0
        || i64::from(i.num) * i64::from(o.den) <= i64::from(o.num) * i64::from(i.den)
    {
        simple()?;
    } else {
        let twice = in_ts.checked_mul(2)?;
        let a = rnd(twice.checked_sub(1)?, Rounding::DOWN) >> 1;
        // `twice` is even, so `twice + 1` fits.
        let b = rnd(twice + 1, Rounding::UP).checked_add(1)? >> 1;
        // `2 * a` and `2 * b` fit: both are halves of an `i64`.
        if *last < (2 * a).checked_sub(b)? || *last > (2 * b).checked_sub(a)? {
            simple()?;
        }
        // Otherwise C adds `duration` to `last` clipped into `a..=b`, which
        // lies within `±2^62`, so the sum fits.
    }
    // SAFETY: `last` is a live, exclusive `int64_t` C reads and overwrites
    // during the call only. `in_ts` is not `AV_NOPTS_VALUE` and `duration` is
    // non-negative, so neither assertion aborts, and the run above took C's
    // path with C's own rescales and found none of its additions,
    // subtractions or doublings out of range.
    let r = unsafe {
        ffi::av_rescale_delta(
            in_tb.into_raw(),
            in_ts,
            fs_tb.into_raw(),
            duration,
            last,
            out_tb.into_raw(),
        )
    };
    (r != i64::MIN).then_some(r)
}

/// Wraps: av_add_stable
///
/// The timestamp `ts`, in time base `ts_tb`, advanced by `inc` units of
/// `inc_tb`, such that adding the same increment repeatedly accumulates no
/// rounding error: the result stays on the grid of `inc_tb` multiples that
/// `ts` lies on. The sum saturates at the `i64` range, and `ts` comes back
/// unchanged when an increment is shorter than one unit of `ts_tb` but not
/// expressible in it, or when C cannot rescale `ts`.
///
/// `None` where C overflows or divides by zero, which the call never reaches:
///
/// - `inc` beyond the `i32` range: C narrows it to `int` and adds the wrong
///   value;
/// - a zero numerator of `ts_tb`, or `inc_tb` whose product with `inc` has a
///   zero denominator;
/// - an increment that is a whole negative number of `ts_tb` units (negative
///   increments are not supported by C: shorter ones return `ts` unchanged).
#[must_use]
pub fn add_stable(ts_tb: Rational, ts: i64, inc_tb: Rational, inc: i64) -> Option<i64> {
    // C's divisibility test, computed as C does, through the same
    // `av_mul_q`.
    let step = if inc == 1 {
        inc_tb
    } else {
        inc_tb * Rational::new(i32::try_from(inc).ok()?, 1)
    };
    let (s, t) = (step.into_raw(), ts_tb.into_raw());
    let m = i64::from(s.num) * i64::from(t.den);
    let d = i64::from(s.den) * i64::from(t.num);
    if d == 0 || (m % d == 0 && m / d < 0) {
        return None;
    }
    // SAFETY: `av_add_stable` takes its arguments by value. `inc` fits an
    // `int`, so C's `(AVRational){inc, 1}` is the value multiplied above, and
    // `m` and `d` are C's: `d` is nonzero, so `m % d` and `m / d` are
    // defined, and a quotient that divides exactly is non-negative, so
    // `INT64_MAX - m / d` does not overflow. The rest rescales through
    // `av_rescale_q` and saturates.
    Some(unsafe { ffi::av_add_stable(ts_tb.into_raw(), ts, inc_tb.into_raw(), inc) })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    const METHODS: [Rounding; 5] = [
        Rounding::ZERO,
        Rounding::INF,
        Rounding::DOWN,
        Rounding::UP,
        Rounding::NEAR_INF,
    ];

    #[test]
    fn methods_carry_the_c_values() {
        let raw: Vec<u32> = METHODS.iter().map(|r| r.as_raw().0).collect();
        assert_eq!(raw, [0, 1, 2, 3, 5]);
        assert_eq!(Rounding::UP.pass_min_max().as_raw().0, 3 | 8192);
        assert_eq!(size_of::<Rounding>(), size_of::<ffi::AVRounding>());
    }

    #[test]
    fn flag_toggles_without_touching_the_method() {
        for r in METHODS {
            assert!(!r.passes_min_max());
            let p = r.pass_min_max();
            assert!(p.passes_min_max());
            assert_ne!(p, r);
            assert_eq!(p.pass_min_max(), p);
            assert_eq!(p.method(), r);
            assert_eq!(r.method(), r);
        }
    }

    #[test]
    fn from_raw_accepts_methods_with_or_without_the_flag() {
        for v in [0, 1, 2, 3, 5] {
            for raw in [v, v | 8192] {
                let r = Rounding::from_raw(ffi::AVRounding(raw)).unwrap();
                assert_eq!(r.as_raw().0, raw);
                assert_eq!(Rounding::try_from(ffi::AVRounding(raw)), Ok(r));
                assert_eq!(ffi::AVRounding::from(r), ffi::AVRounding(raw));
            }
        }
    }

    #[test]
    fn from_raw_rejects_everything_else() {
        for raw in [
            4,
            6,
            7,
            8,
            4096,
            8192 | 4,
            8192 | 6,
            16384,
            8192 | 16384,
            1 << 31,
            u32::MAX,
        ] {
            assert_eq!(Rounding::from_raw(ffi::AVRounding(raw)), None, "{raw:#x}");
            assert_eq!(
                Rounding::try_from(ffi::AVRounding(raw)),
                Err(InvalidRounding(raw))
            );
        }
    }

    #[test]
    fn debug_names_the_method_and_the_flag() {
        let names: Vec<String> = METHODS.iter().map(|r| format!("{r:?}")).collect();
        assert_eq!(
            names,
            [
                "Rounding::ZERO",
                "Rounding::INF",
                "Rounding::DOWN",
                "Rounding::UP",
                "Rounding::NEAR_INF"
            ]
        );
        assert_eq!(
            format!("{:?}", Rounding::NEAR_INF.pass_min_max()),
            "Rounding::NEAR_INF.pass_min_max()"
        );
    }

    #[test]
    fn debug_forwards_writer_errors() {
        /// Accepts `budget` writes, then fails.
        struct Budget(usize);
        impl fmt::Write for Budget {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                self.0 = self.0.checked_sub(1).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let r = Rounding::UP.pass_min_max();
        for budget in 0..3 {
            assert_eq!(
                fmt::write(&mut Budget(budget), format_args!("{r:?}")),
                Err(fmt::Error)
            );
        }
        assert_eq!(fmt::write(&mut Budget(3), format_args!("{r:?}")), Ok(()));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn compare_mod_orders_wrapped_residues() {
        assert!(compare_mod(0x11, 0x02, 0x10) < 0);
        assert!(compare_mod(0x11, 0x02, 0x20) > 0);
        assert_eq!(compare_mod(0x1234, 0x5634, 0x100), 0);
        // Sequence numbers across the wrap: 1 follows 0xffff by 2.
        assert_eq!(compare_mod(1, 0xffff, 0x1_0000), 2);
        assert_eq!(compare_mod(0xffff, 1, 0x1_0000), -2);
        // Half the modulus away stays positive.
        assert_eq!(compare_mod(8, 0, 16), 8);
        assert_eq!(compare_mod(9, 0, 16), -7);
        assert_eq!(compare_mod(0, u64::MAX, 0), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn bessel_i0_is_even_and_grows() {
        assert_eq!(bessel_i0(0.0), 1.0);
        assert_eq!(bessel_i0(-0.0), 1.0);
        for x in [0.5, 1.0, 14.9, 15.0, 15.1, 100.0] {
            assert_eq!(bessel_i0(x), bessel_i0(-x));
            assert!(bessel_i0(x) > 1.0);
        }
        assert!((bessel_i0(1.0) - 1.266_065_877_752_008_4).abs() < 1e-12);
        assert!(bessel_i0(f64::NAN).is_nan());
        assert_eq!(bessel_i0(1e4), f64::INFINITY);
    }

    #[test]
    fn invalid_rounding_reports_the_value() {
        let e = Rounding::try_from(ffi::AVRounding(8196)).unwrap_err();
        assert_eq!(e.to_string(), "invalid AVRounding value 0x2004");
        let _: &dyn std::error::Error = &e;
    }

    /// `num/den`, for brevity.
    fn q(num: i32, den: i32) -> Rational {
        Rational::new(num, den)
    }

    #[test]
    fn rescaled_tells_pass_through_from_rejection() {
        let pass = Rounding::DOWN.pass_min_max();
        assert_eq!(rescaled(i64::MIN, i64::MIN, 1, 1, pass), Some(i64::MIN));
        assert_eq!(rescaled(i64::MIN, i64::MIN, 0, 1, pass), Some(i64::MIN));
        // C rejects a bad `b` or `c` before it passes anything through.
        assert_eq!(rescaled(i64::MIN, i64::MIN, -1, 1, pass), None);
        assert_eq!(rescaled(i64::MIN, i64::MIN, 1, 0, pass), None);
        assert_eq!(rescaled(i64::MIN, i64::MIN, 1, 1, Rounding::DOWN), None);
        assert_eq!(rescaled(i64::MIN, 5, 1, 1, pass), None);
        assert_eq!(rescaled(-3, i64::MIN, 1, 1, Rounding::DOWN), Some(-3));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rescale_rounded_rounds_and_passes_through() {
        assert_eq!(rescale_rounded(7, 1, 2, Rounding::ZERO), Some(3));
        assert_eq!(rescale_rounded(7, 1, 2, Rounding::INF), Some(4));
        assert_eq!(rescale_rounded(-7, 1, 2, Rounding::DOWN), Some(-4));
        assert_eq!(rescale_rounded(-7, 1, 2, Rounding::UP), Some(-3));
        assert_eq!(rescale_rounded(-7, 1, 2, Rounding::NEAR_INF), Some(-4));
        let pass = Rounding::NEAR_INF.pass_min_max();
        assert_eq!(rescale_rounded(i64::MIN, 3, 7, pass), Some(i64::MIN));
        assert_eq!(rescale_rounded(i64::MAX, 3, 7, pass), Some(i64::MAX));
        assert_eq!(
            rescale_rounded(i64::MIN, 1, 1, Rounding::NEAR_INF),
            Some(-i64::MAX)
        );
        assert_eq!(rescale_rounded(1, 1, 0, Rounding::UP), None);
        assert_eq!(rescale_rounded(1, -1, 1, Rounding::UP), None);
        assert_eq!(rescale_rounded(i64::MAX, 2, 1, Rounding::UP), None);
        assert_eq!(rescale_rounded(i64::MIN, 1, 0, pass), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rescale_rational_converts_time_bases() {
        assert_eq!(
            rescale_rational(90_000, q(1, 90_000), q(1, 1000)),
            Some(1000)
        );
        assert_eq!(
            rescale_rational_rounded(1, q(1, 3), q(1, 1), Rounding::UP),
            Some(1)
        );
        assert_eq!(rescale_rational(1, q(1, 3), q(1, 1)), Some(0));
        assert_eq!(rescale_rational(1, q(1, 3), q(0, 1)), None);
        assert_eq!(rescale_rational(1, q(-1, 3), q(1, 1)), None);
        assert_eq!(
            rescale_rational(i64::MIN, q(1, 1), q(1, 1)),
            Some(-i64::MAX)
        );
        let pass = Rounding::ZERO.pass_min_max();
        assert_eq!(
            rescale_rational_rounded(i64::MIN, q(1, 1), q(1, 2), pass),
            Some(i64::MIN)
        );
        assert_eq!(
            rescale_rational_rounded(i64::MIN, q(1, 1), q(0, 2), pass),
            None
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn compare_timestamps_orders_across_time_bases() {
        let (ms, us) = (q(1, 1000), q(1, 1_000_000));
        assert_eq!(compare_timestamps(1, ms, 1000, us), Ordering::Equal);
        assert_eq!(compare_timestamps(1, ms, 999, us), Ordering::Greater);
        assert_eq!(compare_timestamps(1, ms, 1001, us), Ordering::Less);
        let big = 1 << 40;
        assert_eq!(
            compare_timestamps(big, ms, big * 1000 + 1, us),
            Ordering::Less
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rescale_delta_rejects_what_c_aborts_on() {
        let mut last = 77;
        let tb = q(1, 44_100);
        assert_eq!(rescale_delta(tb, i64::MIN, tb, 1024, &mut last, tb), None);
        assert_eq!(rescale_delta(tb, 0, tb, -1, &mut last, tb), None);
        assert_eq!(last, 77);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rescale_delta_rejects_overflowing_steps() {
        let (coarse, fine) = (q(1, 1000), q(1, 48_000));
        // Rounding is needed (`in_tb` coarser than `out_tb`, `last` set): C
        // doubles `in_ts`.
        let mut last = 0;
        assert_eq!(
            rescale_delta(coarse, 1 << 62, fine, 1, &mut last, fine),
            None
        );
        assert_eq!(
            rescale_delta(coarse, -(1 << 62), fine, 1, &mut last, fine),
            None
        );
        assert_eq!(last, 0);
        // The plain path adds `duration` to a rescale near `i64::MAX`.
        let mut last = i64::MIN;
        let one = q(1, 1);
        assert_eq!(rescale_delta(one, i64::MAX, one, 1, &mut last, one), None);
        assert_eq!(last, i64::MIN);
        assert_eq!(
            rescale_delta(one, i64::MAX - 1, one, 1, &mut last, one),
            Some(i64::MAX - 1)
        );
        assert_eq!(last, i64::MAX);
        // `2 * in_ts + 1` rescales to `i64::MAX`, to which C adds 1.
        let (half, mut last) = (q(1, 2), 0);
        assert_eq!(
            rescale_delta(one, (1 << 62) - 1, one, 1, &mut last, half),
            None
        );
        // The upper bound overflows while the lower does not: C's
        // `2 * a - b` then overflows.
        let fine = q(1, 1 << 20);
        assert_eq!(rescale_delta(one, 1 << 42, fine, 1, &mut last, fine), None);
        // Both bounds overflow, `last` falls outside them, and the plain
        // path adds `duration` to a rescale near `i64::MAX`.
        assert_eq!(
            rescale_delta(one, (1 << 62) - 1, half, 2, &mut last, half),
            None
        );
        assert_eq!(last, 0);
        assert_eq!(
            rescale_delta(one, (1 << 62) - 1, half, 1, &mut last, half),
            Some(i64::MAX - 1)
        );
        assert_eq!(last, i64::MAX);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rescale_delta_follows_known_durations() {
        // 1024-sample packets at 44.1 kHz, stamped in milliseconds.
        let (ms, sr) = (q(1, 1000), q(1, 44_100));
        let mut last = i64::MIN;
        let mut out = Vec::new();
        for i in 0..6 {
            // Stamps within half a millisecond of the true position.
            let in_ts = rescale_rational(i * 1024, sr, ms).unwrap();
            out.push(rescale_delta(ms, in_ts, sr, 1024, &mut last, sr).unwrap());
        }
        assert_eq!(out, [0, 1024, 2048, 3072, 4096, 5120]);
        assert_eq!(last, 6 * 1024);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn add_stable_accumulates_no_error() {
        let (ts_tb, inc_tb) = (q(1, 1000), q(1, 44_100));
        let mut ts = 0;
        for _ in 0..44_100 / 1024 * 10 {
            ts = add_stable(ts_tb, ts, inc_tb, 1024).unwrap();
        }
        // 430 packets of 1024 samples, 9_984.68 ms, rounded once.
        assert_eq!(ts, 9985);
        assert_eq!(rescale_rational(430 * 1024, inc_tb, ts_tb), Some(ts));
        // Exact multiples add, and saturate nothing.
        assert_eq!(add_stable(q(1, 10), 5, q(1, 2), 3), Some(20));
        assert_eq!(add_stable(q(1, 1), i64::MAX, q(1, 1), 1), Some(i64::MAX));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn add_stable_rejects_what_c_overflows_on() {
        let tb = q(1, 1000);
        assert_eq!(add_stable(tb, 0, tb, i64::from(i32::MAX) + 1), None);
        assert_eq!(add_stable(tb, 0, tb, i64::from(i32::MIN) - 1), None);
        assert!(add_stable(tb, 0, tb, i64::from(i32::MIN)).is_none());
        // A zero `ts_tb` numerator or a zero increment denominator divides by
        // zero.
        assert_eq!(add_stable(q(0, 1), 0, tb, 1), None);
        assert_eq!(add_stable(tb, 0, q(1, 0), 1), None);
        // A whole negative number of ticks overflows `INT64_MAX - m / d`.
        assert_eq!(add_stable(tb, 10, tb, -1), None);
        assert_eq!(add_stable(tb, 10, q(-1, 1000), 1), None);
        // A shorter negative increment leaves `ts` as it is.
        assert_eq!(add_stable(q(1, 10), 10, q(-1, 1000), 1), Some(10));
    }

    #[test]
    fn rejections_before_the_call_touch_no_c() {
        // Built without `av_make_q`, so the test runs where C cannot (Miri).
        let r = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let tb = r(1, 1000);
        let mut last = 5;
        assert_eq!(rescale_delta(tb, i64::MIN, tb, 1, &mut last, tb), None);
        assert_eq!(rescale_delta(tb, 1, tb, -1, &mut last, tb), None);
        assert_eq!(rescale_delta(tb, 1, tb, i32::MIN, &mut last, tb), None);
        assert_eq!(last, 5);
        assert_eq!(add_stable(tb, 0, tb, i64::from(i32::MAX) + 1), None);
        assert_eq!(add_stable(tb, 0, tb, i64::MIN), None);
        assert_eq!(add_stable(tb, 0, tb, i64::MAX), None);
    }
}
