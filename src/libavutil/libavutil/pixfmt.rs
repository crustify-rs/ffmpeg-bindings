//! Safe wrappers over `libavutil/pixfmt.h`.

use core::fmt;

use ffibox::CPlainElem;

use super::ffi;

/// Wraps: AVColorSpace
/// Handwritten
///
/// The YUV colour space (matrix coefficients) of a picture, as numbered by
/// ISO/IEC 23091-2:2019 subclause 8.3.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVFrame.colorspace`,
/// `AVCodecContext.colorspace`), so it may hold any value, and a Rust view of
/// that memory must accept every one. Match on the associated constants, with
/// a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::ColorSpace;
/// fn is_bt2020(c: ColorSpace) -> bool {
///     matches!(c, ColorSpace::BT2020_NCL | ColorSpace::BT2020_CL)
/// }
/// assert!(is_bt2020(ColorSpace::BT2020_CL));
/// assert!(!is_bt2020(ColorSpace::default()));
/// ```
///
/// `AVCOL_SPC_NB` is not part of FFmpeg's ABI and has no constant. The
/// default is [`UNSPECIFIED`](Self::UNSPECIFIED), the value C gives a fresh
/// frame or codec context; a zero-filled field holds [`RGB`](Self::RGB).
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorSpace(ffi::AVColorSpace);

impl ColorSpace {
    /// GBR order of the RGB coefficients; also IEC 61966-2-1 (sRGB), YZX and
    /// ST 428-1.
    pub const RGB: Self = Self(ffi::AVColorSpace::AVCOL_SPC_RGB);
    /// ITU-R BT.709; also BT.1361, IEC 61966-2-4 xvYCC709 and SMPTE RP 177
    /// Annex B.
    pub const BT709: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT709);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorSpace::AVCOL_SPC_UNSPECIFIED);
    /// Reserved for future use by ITU-T and ISO/IEC.
    pub const RESERVED: Self = Self(ffi::AVColorSpace::AVCOL_SPC_RESERVED);
    /// FCC Title 47 Code of Federal Regulations 73.682 (a)(20).
    pub const FCC: Self = Self(ffi::AVColorSpace::AVCOL_SPC_FCC);
    /// ITU-R BT.470 BG; also BT.601-6 625, BT.1358 625, BT.1700 625 PAL and
    /// SECAM, and IEC 61966-2-4 xvYCC601.
    pub const BT470BG: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT470BG);
    /// SMPTE 170 M; also ITU-R BT.601-6 525, BT.1358 525 and BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE170M);
    /// SMPTE 240 M.
    pub const SMPTE240M: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE240M);
    /// YCgCo, used by Dirac / VC-2 and H.264 FRext.
    pub const YCGCO: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO);
    /// Another name for [`YCGCO`](Self::YCGCO).
    pub const YCOCG: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCOCG);
    /// ITU-R BT.2020 non-constant luminance.
    pub const BT2020_NCL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL);
    /// ITU-R BT.2020 constant luminance.
    pub const BT2020_CL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT2020_CL);
    /// SMPTE 2085, Y'D'zD'x.
    pub const SMPTE2085: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE2085);
    /// Chromaticity-derived non-constant luminance.
    pub const CHROMA_DERIVED_NCL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_CHROMA_DERIVED_NCL);
    /// Chromaticity-derived constant luminance.
    pub const CHROMA_DERIVED_CL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_CHROMA_DERIVED_CL);
    /// ITU-R BT.2100-0 ICtCp.
    pub const ICTCP: Self = Self(ffi::AVColorSpace::AVCOL_SPC_ICTCP);
    /// SMPTE ST 2128 IPT-C2.
    pub const IPT_C2: Self = Self(ffi::AVColorSpace::AVCOL_SPC_IPT_C2);
    /// YCgCo-R, even addition of bits.
    pub const YCGCO_RE: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO_RE);
    /// YCgCo-R, odd addition of bits.
    pub const YCGCO_RO: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO_RO);

    /// The colour space C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVColorSpace) -> Self {
        Self(raw)
    }

    /// The C encoding of this colour space.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVColorSpace {
        self.0
    }

    /// The name of the constant this value equals, if any; an alias reports
    /// the constant it aliases.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::RGB => "RGB",
            Self::BT709 => "BT709",
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::RESERVED => "RESERVED",
            Self::FCC => "FCC",
            Self::BT470BG => "BT470BG",
            Self::SMPTE170M => "SMPTE170M",
            Self::SMPTE240M => "SMPTE240M",
            Self::YCGCO => "YCGCO",
            Self::BT2020_NCL => "BT2020_NCL",
            Self::BT2020_CL => "BT2020_CL",
            Self::SMPTE2085 => "SMPTE2085",
            Self::CHROMA_DERIVED_NCL => "CHROMA_DERIVED_NCL",
            Self::CHROMA_DERIVED_CL => "CHROMA_DERIVED_CL",
            Self::ICTCP => "ICTCP",
            Self::IPT_C2 => "IPT_C2",
            Self::YCGCO_RE => "YCGCO_RE",
            Self::YCGCO_RO => "YCGCO_RO",
            _ => return None,
        })
    }
}

impl Default for ColorSpace {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ColorSpace").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVColorSpace> for ColorSpace {
    #[inline]
    fn from(raw: ffi::AVColorSpace) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ColorSpace> for ffi::AVColorSpace {
    #[inline]
    fn from(c: ColorSpace) -> Self {
        c.into_raw()
    }
}

// SAFETY: `ColorSpace` is `#[repr(transparent)]` over `ffi::AVColorSpace`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for ColorSpace {}

/// Wraps: AVColorTransferCharacteristic
/// Handwritten
///
/// The colour transfer characteristic (opto-electronic transfer function) of
/// a picture, as numbered by ISO/IEC 23091-2:2019 subclause 8.2, with
/// FFmpeg's extensions from [`EXT_BASE`](Self::EXT_BASE) up. Named after
/// `av_color_transfer_name`, its C consumer.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVFrame.color_trc`,
/// `AVCodecContext.color_trc`), so it may hold any value, and a Rust view of
/// that memory must accept every one. Match on the associated constants, with
/// a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::ColorTransfer;
/// fn is_hdr(t: ColorTransfer) -> bool {
///     matches!(t, ColorTransfer::SMPTE2084 | ColorTransfer::ARIB_STD_B67)
/// }
/// assert!(is_hdr(ColorTransfer::SMPTEST2084));
/// assert!(!is_hdr(ColorTransfer::default()));
/// ```
///
/// `AVCOL_TRC_NB` and `AVCOL_TRC_EXT_NB` are not part of FFmpeg's ABI and
/// have no constant. The default is [`UNSPECIFIED`](Self::UNSPECIFIED), the
/// value C gives a fresh frame or codec context; a zero-filled field holds
/// [`RESERVED0`](Self::RESERVED0).
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorTransfer(ffi::AVColorTransferCharacteristic);

impl ColorTransfer {
    /// Reserved.
    pub const RESERVED0: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_RESERVED0);
    /// ITU-R BT.709; also BT.1361.
    pub const BT709: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_UNSPECIFIED);
    /// Reserved.
    pub const RESERVED: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_RESERVED);
    /// Gamma 2.2: ITU-R BT.470 M; also BT.1700 625 PAL and SECAM.
    pub const GAMMA22: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_GAMMA22);
    /// Gamma 2.8: ITU-R BT.470 BG.
    pub const GAMMA28: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_GAMMA28);
    /// SMPTE 170 M; also ITU-R BT.601-6 525 or 625, BT.1358 525 or 625 and
    /// BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE170M);
    /// SMPTE 240 M.
    pub const SMPTE240M: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE240M);
    /// Linear transfer characteristics.
    pub const LINEAR: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LINEAR);
    /// Logarithmic transfer characteristic (100:1 range).
    pub const LOG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LOG);
    /// Logarithmic transfer characteristic (100 * sqrt(10) : 1 range).
    pub const LOG_SQRT: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LOG_SQRT);
    /// IEC 61966-2-4.
    pub const IEC61966_2_4: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_IEC61966_2_4);
    /// ITU-R BT.1361 extended colour gamut.
    pub const BT1361_ECG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT1361_ECG);
    /// IEC 61966-2-1 (sRGB or sYCC).
    pub const IEC61966_2_1: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_IEC61966_2_1);
    /// ITU-R BT.2020 for 10-bit systems.
    pub const BT2020_10: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT2020_10);
    /// ITU-R BT.2020 for 12-bit systems.
    pub const BT2020_12: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT2020_12);
    /// SMPTE ST 2084 (PQ) for 10-, 12-, 14- and 16-bit systems.
    pub const SMPTE2084: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084);
    /// Another name for [`SMPTE2084`](Self::SMPTE2084).
    pub const SMPTEST2084: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTEST2084);
    /// SMPTE ST 428-1.
    pub const SMPTE428: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE428);
    /// Another name for [`SMPTE428`](Self::SMPTE428).
    pub const SMPTEST428_1: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTEST428_1);
    /// ARIB STD-B67, known as hybrid log-gamma (HLG).
    pub const ARIB_STD_B67: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_ARIB_STD_B67);
    /// The first of FFmpeg's extensions beyond ISO/IEC 23091-2; the value of
    /// [`V_LOG`](Self::V_LOG).
    pub const EXT_BASE: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_BASE);
    /// Panasonic V-Log, an FFmpeg extension.
    pub const V_LOG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_V_LOG);

    /// The transfer characteristic C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVColorTransferCharacteristic) -> Self {
        Self(raw)
    }

    /// The C encoding of this transfer characteristic.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVColorTransferCharacteristic {
        self.0
    }

    /// The name of the constant this value equals, if any; an alias reports
    /// the constant it aliases, and [`EXT_BASE`](Self::EXT_BASE) the
    /// extension it numbers.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::RESERVED0 => "RESERVED0",
            Self::BT709 => "BT709",
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::RESERVED => "RESERVED",
            Self::GAMMA22 => "GAMMA22",
            Self::GAMMA28 => "GAMMA28",
            Self::SMPTE170M => "SMPTE170M",
            Self::SMPTE240M => "SMPTE240M",
            Self::LINEAR => "LINEAR",
            Self::LOG => "LOG",
            Self::LOG_SQRT => "LOG_SQRT",
            Self::IEC61966_2_4 => "IEC61966_2_4",
            Self::BT1361_ECG => "BT1361_ECG",
            Self::IEC61966_2_1 => "IEC61966_2_1",
            Self::BT2020_10 => "BT2020_10",
            Self::BT2020_12 => "BT2020_12",
            Self::SMPTE2084 => "SMPTE2084",
            Self::SMPTE428 => "SMPTE428",
            Self::ARIB_STD_B67 => "ARIB_STD_B67",
            Self::V_LOG => "V_LOG",
            _ => return None,
        })
    }
}

impl Default for ColorTransfer {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for ColorTransfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ColorTransfer").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVColorTransferCharacteristic> for ColorTransfer {
    #[inline]
    fn from(raw: ffi::AVColorTransferCharacteristic) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ColorTransfer> for ffi::AVColorTransferCharacteristic {
    #[inline]
    fn from(t: ColorTransfer) -> Self {
        t.into_raw()
    }
}

// SAFETY: `ColorTransfer` is `#[repr(transparent)]` over
// `ffi::AVColorTransferCharacteristic`, itself `#[repr(transparent)]` over a
// `c_uint`, so every bit pattern is a valid value, and it is a value with no
// C-side identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for ColorTransfer {}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_uint;
    use core::mem::{align_of, size_of};

    const COLOR_SPACE_NAMED: [(ColorSpace, &str); 18] = [
        (ColorSpace::RGB, "RGB"),
        (ColorSpace::BT709, "BT709"),
        (ColorSpace::UNSPECIFIED, "UNSPECIFIED"),
        (ColorSpace::RESERVED, "RESERVED"),
        (ColorSpace::FCC, "FCC"),
        (ColorSpace::BT470BG, "BT470BG"),
        (ColorSpace::SMPTE170M, "SMPTE170M"),
        (ColorSpace::SMPTE240M, "SMPTE240M"),
        (ColorSpace::YCGCO, "YCGCO"),
        (ColorSpace::BT2020_NCL, "BT2020_NCL"),
        (ColorSpace::BT2020_CL, "BT2020_CL"),
        (ColorSpace::SMPTE2085, "SMPTE2085"),
        (ColorSpace::CHROMA_DERIVED_NCL, "CHROMA_DERIVED_NCL"),
        (ColorSpace::CHROMA_DERIVED_CL, "CHROMA_DERIVED_CL"),
        (ColorSpace::ICTCP, "ICTCP"),
        (ColorSpace::IPT_C2, "IPT_C2"),
        (ColorSpace::YCGCO_RE, "YCGCO_RE"),
        (ColorSpace::YCGCO_RO, "YCGCO_RO"),
    ];

    #[test]
    fn color_space_layout_matches_the_c_enum() {
        assert_eq!(size_of::<ColorSpace>(), size_of::<ffi::AVColorSpace>());
        assert_eq!(align_of::<ColorSpace>(), align_of::<ffi::AVColorSpace>());
        assert_eq!(size_of::<ColorSpace>(), size_of::<c_uint>());
    }

    #[test]
    fn color_space_constants_are_the_declared_values_and_distinct() {
        for (i, (c, _)) in COLOR_SPACE_NAMED.iter().enumerate() {
            assert_eq!(c.into_raw().0, i as c_uint);
        }
        assert_eq!(ColorSpace::YCOCG, ColorSpace::YCGCO);
    }

    #[test]
    fn color_space_debug_names_the_constants() {
        for (c, name) in COLOR_SPACE_NAMED {
            assert_eq!(format!("{c:?}"), name);
        }
        assert_eq!(format!("{:?}", ColorSpace::YCOCG), "YCGCO");
    }

    #[test]
    fn color_space_debug_shows_unnamed_values_raw() {
        let nb = ColorSpace::from_raw(ffi::AVColorSpace::AVCOL_SPC_NB);
        assert_eq!(format!("{nb:?}"), "ColorSpace(18)");
        for raw in [19, 255, 256, 1 << 31, c_uint::MAX] {
            let c = ColorSpace::from_raw(ffi::AVColorSpace(raw));
            assert_eq!(format!("{c:?}"), format!("ColorSpace({raw})"));
        }
    }

    #[test]
    fn color_space_default_is_unspecified() {
        assert_eq!(ColorSpace::default(), ColorSpace::UNSPECIFIED);
        assert_eq!(ColorSpace::default().into_raw().0, 2);
    }

    #[test]
    fn color_space_raw_conversions_round_trip_every_value() {
        for raw in [0, 2, 17, 18, 256, 1 << 31, c_uint::MAX] {
            let c = ffi::AVColorSpace(raw);
            assert_eq!(ColorSpace::from(c).into_raw(), c);
            assert_eq!(ffi::AVColorSpace::from(ColorSpace::from_raw(c)), c);
        }
    }

    const COLOR_TRANSFER_NAMED: [(ColorTransfer, &str); 20] = [
        (ColorTransfer::RESERVED0, "RESERVED0"),
        (ColorTransfer::BT709, "BT709"),
        (ColorTransfer::UNSPECIFIED, "UNSPECIFIED"),
        (ColorTransfer::RESERVED, "RESERVED"),
        (ColorTransfer::GAMMA22, "GAMMA22"),
        (ColorTransfer::GAMMA28, "GAMMA28"),
        (ColorTransfer::SMPTE170M, "SMPTE170M"),
        (ColorTransfer::SMPTE240M, "SMPTE240M"),
        (ColorTransfer::LINEAR, "LINEAR"),
        (ColorTransfer::LOG, "LOG"),
        (ColorTransfer::LOG_SQRT, "LOG_SQRT"),
        (ColorTransfer::IEC61966_2_4, "IEC61966_2_4"),
        (ColorTransfer::BT1361_ECG, "BT1361_ECG"),
        (ColorTransfer::IEC61966_2_1, "IEC61966_2_1"),
        (ColorTransfer::BT2020_10, "BT2020_10"),
        (ColorTransfer::BT2020_12, "BT2020_12"),
        (ColorTransfer::SMPTE2084, "SMPTE2084"),
        (ColorTransfer::SMPTE428, "SMPTE428"),
        (ColorTransfer::ARIB_STD_B67, "ARIB_STD_B67"),
        (ColorTransfer::V_LOG, "V_LOG"),
    ];

    #[test]
    fn color_transfer_layout_matches_the_c_enum() {
        assert_eq!(
            size_of::<ColorTransfer>(),
            size_of::<ffi::AVColorTransferCharacteristic>()
        );
        assert_eq!(
            align_of::<ColorTransfer>(),
            align_of::<ffi::AVColorTransferCharacteristic>()
        );
        assert_eq!(size_of::<ColorTransfer>(), size_of::<c_uint>());
    }

    #[test]
    fn color_transfer_constants_are_the_declared_values_and_distinct() {
        // The standard values run 0..=18, then the extensions start at 256.
        for (i, (t, _)) in COLOR_TRANSFER_NAMED[..19].iter().enumerate() {
            assert_eq!(t.into_raw().0, i as c_uint);
        }
        assert_eq!(ColorTransfer::V_LOG.into_raw().0, 256);
        assert_eq!(ColorTransfer::EXT_BASE, ColorTransfer::V_LOG);
        assert_eq!(ColorTransfer::SMPTEST2084, ColorTransfer::SMPTE2084);
        assert_eq!(ColorTransfer::SMPTEST428_1, ColorTransfer::SMPTE428);
    }

    #[test]
    fn color_transfer_debug_names_the_constants() {
        for (t, name) in COLOR_TRANSFER_NAMED {
            assert_eq!(format!("{t:?}"), name);
        }
        assert_eq!(format!("{:?}", ColorTransfer::SMPTEST2084), "SMPTE2084");
        assert_eq!(format!("{:?}", ColorTransfer::SMPTEST428_1), "SMPTE428");
        assert_eq!(format!("{:?}", ColorTransfer::EXT_BASE), "V_LOG");
    }

    #[test]
    fn color_transfer_debug_shows_unnamed_values_raw() {
        let nb = ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic::AVCOL_TRC_NB);
        assert_eq!(format!("{nb:?}"), "ColorTransfer(19)");
        let ext_nb = ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_NB);
        assert_eq!(format!("{ext_nb:?}"), "ColorTransfer(257)");
        for raw in [20, 255, 258, 1 << 31, c_uint::MAX] {
            let t = ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(raw));
            assert_eq!(format!("{t:?}"), format!("ColorTransfer({raw})"));
        }
    }

    #[test]
    fn color_transfer_default_is_unspecified() {
        assert_eq!(ColorTransfer::default(), ColorTransfer::UNSPECIFIED);
        assert_eq!(ColorTransfer::default().into_raw().0, 2);
    }

    #[test]
    fn color_transfer_raw_conversions_round_trip_every_value() {
        for raw in [0, 2, 18, 19, 256, 257, 1 << 31, c_uint::MAX] {
            let c = ffi::AVColorTransferCharacteristic(raw);
            assert_eq!(ColorTransfer::from(c).into_raw(), c);
            assert_eq!(
                ffi::AVColorTransferCharacteristic::from(ColorTransfer::from_raw(c)),
                c
            );
        }
    }
}
