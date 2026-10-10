//! Safe wrappers over `libavutil/samplefmt.h`.

use core::fmt;

use ffibox::CPlainElem;

use super::ffi;

/// Wraps: AVSampleFormat
/// Handwritten
///
/// The format of an audio sample: its native-endian C type (`u8`, `i16`,
/// `i32`, `i64`, `f32` or `f64`) and whether the channels are interleaved in
/// one plane (packed) or each held in a plane of its own (planar, the `P`
/// constants).
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVCodecContext.sample_fmt`,
/// `AVFrame.format`) and in `NONE`-terminated arrays, so it may hold any
/// `int`, and a Rust view of that memory must accept every one. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::samplefmt::SampleFormat;
/// fn is_float(f: SampleFormat) -> bool {
///     matches!(
///         f,
///         SampleFormat::FLT | SampleFormat::FLTP | SampleFormat::DBL | SampleFormat::DBLP
///     )
/// }
/// assert!(is_float(SampleFormat::FLTP));
/// assert!(!is_float(SampleFormat::NONE));
/// ```
///
/// The default is [`NONE`](Self::NONE), the value C gives an unset format:
/// `av_frame_alloc` writes it (as `-1`) into `AVFrame.format`, and it is the
/// default of `AVCodecContext.sample_fmt`.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleFormat(ffi::AVSampleFormat);

impl SampleFormat {
    /// No sample format: unknown or unset.
    pub const NONE: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE);
    /// Unsigned 8 bits.
    pub const U8: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_U8);
    /// Signed 16 bits.
    pub const S16: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S16);
    /// Signed 32 bits.
    pub const S32: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S32);
    /// `float`.
    pub const FLT: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT);
    /// `double`.
    pub const DBL: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_DBL);
    /// Unsigned 8 bits, planar.
    pub const U8P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_U8P);
    /// Signed 16 bits, planar.
    pub const S16P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S16P);
    /// Signed 32 bits, planar.
    pub const S32P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S32P);
    /// `float`, planar.
    pub const FLTP: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP);
    /// `double`, planar.
    pub const DBLP: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_DBLP);
    /// Signed 64 bits.
    pub const S64: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S64);
    /// Signed 64 bits, planar.
    pub const S64P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S64P);

    /// The number of sample formats from [`U8`](Self::U8) to
    /// [`S64P`](Self::S64P) these bindings were generated with
    /// (`AV_SAMPLE_FMT_NB`): the length of an array indexed by
    /// [`index`](Self::index).
    ///
    /// A newer libavutil of the same major version may append formats, which
    /// [`index`](Self::index) reports as `None`; this is why C warns against
    /// `AV_SAMPLE_FMT_NB` when linking dynamically.
    pub const COUNT: usize = ffi::AVSampleFormat::AV_SAMPLE_FMT_NB.0 as usize;

    /// The sample format C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVSampleFormat) -> Self {
        Self(raw)
    }

    /// The C encoding of this sample format.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVSampleFormat {
        self.0
    }

    /// This sample format's position in `0..COUNT`, or `None` for
    /// [`NONE`](Self::NONE) and any value C does not name.
    #[inline]
    pub const fn index(self) -> Option<usize> {
        let raw = self.0.0;
        if raw >= 0 && (raw as usize) < Self::COUNT {
            Some(raw as usize)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::U8 => "U8",
            Self::S16 => "S16",
            Self::S32 => "S32",
            Self::FLT => "FLT",
            Self::DBL => "DBL",
            Self::U8P => "U8P",
            Self::S16P => "S16P",
            Self::S32P => "S32P",
            Self::FLTP => "FLTP",
            Self::DBLP => "DBLP",
            Self::S64 => "S64",
            Self::S64P => "S64P",
            _ => return None,
        })
    }
}

impl Default for SampleFormat {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for SampleFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("SampleFormat").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVSampleFormat> for SampleFormat {
    #[inline]
    fn from(raw: ffi::AVSampleFormat) -> Self {
        Self::from_raw(raw)
    }
}

impl From<SampleFormat> for ffi::AVSampleFormat {
    #[inline]
    fn from(f: SampleFormat) -> Self {
        f.into_raw()
    }
}

// SAFETY: `SampleFormat` is `#[repr(transparent)]` over `ffi::AVSampleFormat`,
// itself `#[repr(transparent)]` over a `c_int`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for SampleFormat {}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_int;
    use core::mem::{align_of, size_of};

    const NAMED: [(SampleFormat, &str); 13] = [
        (SampleFormat::NONE, "NONE"),
        (SampleFormat::U8, "U8"),
        (SampleFormat::S16, "S16"),
        (SampleFormat::S32, "S32"),
        (SampleFormat::FLT, "FLT"),
        (SampleFormat::DBL, "DBL"),
        (SampleFormat::U8P, "U8P"),
        (SampleFormat::S16P, "S16P"),
        (SampleFormat::S32P, "S32P"),
        (SampleFormat::FLTP, "FLTP"),
        (SampleFormat::DBLP, "DBLP"),
        (SampleFormat::S64, "S64"),
        (SampleFormat::S64P, "S64P"),
    ];

    #[test]
    fn layout_matches_the_c_enum() {
        assert_eq!(size_of::<SampleFormat>(), size_of::<ffi::AVSampleFormat>());
        assert_eq!(
            align_of::<SampleFormat>(),
            align_of::<ffi::AVSampleFormat>()
        );
        assert_eq!(size_of::<SampleFormat>(), size_of::<c_int>());
    }

    #[test]
    fn debug_names_the_constants() {
        for (f, name) in NAMED {
            assert_eq!(format!("{f:?}"), name);
        }
    }

    #[test]
    fn debug_shows_unnamed_values_raw() {
        let nb = SampleFormat::from_raw(ffi::AVSampleFormat::AV_SAMPLE_FMT_NB);
        assert_eq!(format!("{nb:?}"), "SampleFormat(12)");
        for raw in [c_int::MIN, -2, 13, c_int::MAX] {
            let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
            assert_eq!(format!("{f:?}"), format!("SampleFormat({raw})"));
        }
    }

    #[test]
    fn index_covers_exactly_the_counted_formats() {
        let counted = &NAMED[1..];
        assert_eq!(counted.len(), SampleFormat::COUNT);
        for (i, &(f, _)) in counted.iter().enumerate() {
            assert_eq!(f.index(), Some(i));
        }
        assert_eq!(SampleFormat::NONE.index(), None);
        for raw in [c_int::MIN, -2, 12, 13, c_int::MAX] {
            assert_eq!(
                SampleFormat::from_raw(ffi::AVSampleFormat(raw)).index(),
                None
            );
        }
    }

    #[test]
    fn raw_conversions_round_trip_every_value() {
        for raw in [c_int::MIN, -2, -1, 0, 11, 12, 13, c_int::MAX] {
            let c = ffi::AVSampleFormat(raw);
            assert_eq!(SampleFormat::from(c).into_raw(), c);
            assert_eq!(ffi::AVSampleFormat::from(SampleFormat::from_raw(c)), c);
        }
    }

    #[test]
    fn constants_are_distinct() {
        for (i, (a, _)) in NAMED.iter().enumerate() {
            for (b, _) in &NAMED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn default_is_none() {
        assert_eq!(SampleFormat::default(), SampleFormat::NONE);
        assert_eq!(SampleFormat::default().into_raw().0, -1);
        assert_eq!(SampleFormat::default().index(), None);
        assert_eq!(format!("{:?}", SampleFormat::default()), "NONE");
    }

    #[test]
    fn const_fns_evaluate_at_compile_time() {
        const RAW: ffi::AVSampleFormat = SampleFormat::S16P.into_raw();
        const BACK: SampleFormat = SampleFormat::from_raw(RAW);
        const INDEX: Option<usize> = BACK.index();
        assert_eq!(RAW, ffi::AVSampleFormat::AV_SAMPLE_FMT_S16P);
        assert_eq!(INDEX, Some(6));
    }
}
