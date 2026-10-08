//! Safe wrappers over `libavutil/mathematics.h`.
//!
//! [`Rounding`] is the rounding argument of the rescaling functions: one of
//! five methods, optionally with the pass-through of `i64::MIN` / `i64::MAX`.

use core::fmt;

use super::ffi;

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
    fn invalid_rounding_reports_the_value() {
        let e = Rounding::try_from(ffi::AVRounding(8196)).unwrap_err();
        assert_eq!(e.to_string(), "invalid AVRounding value 0x2004");
        let _: &dyn std::error::Error = &e;
    }
}
