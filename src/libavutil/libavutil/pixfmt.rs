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

/// Wraps: AVAlphaMode
/// Handwritten
///
/// How a picture's alpha channel relates to its colour values.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// a public struct field its users write directly (`AVFrame.alpha_mode`), and
/// `av_alpha_mode_from_name` returns `AVERROR(EINVAL)` typed as the enum, so
/// it may hold any value, and a Rust view of that memory must accept every
/// one. Match on the associated constants, with a `_` arm for the values C
/// does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::AlphaMode;
/// fn needs_unpremultiply(mode: AlphaMode) -> bool {
///     matches!(mode, AlphaMode::PREMULTIPLIED)
/// }
/// assert!(needs_unpremultiply(AlphaMode::PREMULTIPLIED));
/// assert!(!needs_unpremultiply(AlphaMode::default()));
/// ```
///
/// `AVALPHA_MODE_NB` is not part of FFmpeg's ABI and has no constant. The
/// default is [`AlphaMode::UNSPECIFIED`], the zeroed C value and the one
/// `av_frame_unref` resets a frame to.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlphaMode(ffi::AVAlphaMode);

impl AlphaMode {
    /// Unknown alpha handling, or no alpha channel.
    pub const UNSPECIFIED: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_UNSPECIFIED);
    /// The alpha channel is multiplied into the colour values.
    pub const PREMULTIPLIED: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_PREMULTIPLIED);
    /// The alpha channel is independent of the colour values.
    pub const STRAIGHT: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_STRAIGHT);

    /// The alpha mode C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVAlphaMode) -> Self {
        Self(raw)
    }

    /// The C encoding of this alpha mode.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVAlphaMode {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::PREMULTIPLIED => "PREMULTIPLIED",
            Self::STRAIGHT => "STRAIGHT",
            _ => return None,
        })
    }
}

impl Default for AlphaMode {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for AlphaMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("AlphaMode").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVAlphaMode> for AlphaMode {
    #[inline]
    fn from(raw: ffi::AVAlphaMode) -> Self {
        Self::from_raw(raw)
    }
}

impl From<AlphaMode> for ffi::AVAlphaMode {
    #[inline]
    fn from(mode: AlphaMode) -> Self {
        mode.into_raw()
    }
}

// SAFETY: `AlphaMode` is `#[repr(transparent)]` over `ffi::AVAlphaMode`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for AlphaMode {}

/// Wraps: AVPixelFormat
/// Handwritten
///
/// The layout of a picture's pixels in memory: its planes, the order and
/// depth of their components, its chroma subsampling and byte order, or the
/// hardware surface that holds it.
///
/// A newtype over the C enum rather than a Rust `enum`, as
/// [`MediaType`](super::avutil::MediaType) is: C stores the value in public
/// struct fields and format lists its users write directly
/// (`AVCodecContext.pix_fmt`, `AVHWFramesContext.sw_format`), and a newer
/// library may return a format this crate does not name, so it may hold any
/// `int`, and a Rust view of that memory must accept every one. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// fn is_nv12_family(f: PixelFormat) -> bool {
///     matches!(f, PixelFormat::NV12 | PixelFormat::NV21)
/// }
/// assert!(is_nv12_family(PixelFormat::NV21));
/// assert!(!is_nv12_family(PixelFormat::NONE));
/// ```
///
/// Formats whose components span several bytes come in a big-endian `BE` and
/// a little-endian `LE` constant. C's native-endian names for them
/// (`AV_PIX_FMT_GRAY16`, `AV_PIX_FMT_RGB32`, ...) are associated constants
/// too, resolved for the target's byte order:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// #[cfg(target_endian = "little")]
/// assert_eq!(PixelFormat::GRAY16, PixelFormat::GRAY16LE);
/// #[cfg(target_endian = "big")]
/// assert_eq!(PixelFormat::GRAY16, PixelFormat::GRAY16BE);
/// ```
///
/// The C names `AV_PIX_FMT_0RGB`, `AV_PIX_FMT_0BGR`, `AV_PIX_FMT_0RGB32` and
/// `AV_PIX_FMT_0BGR32` are not Rust identifiers, so their constants are
/// [`XRGB`](Self::XRGB), [`XBGR`](Self::XBGR), [`XRGB32`](Self::XRGB32) and
/// [`XBGR32`](Self::XBGR32). There is no count: `AV_PIX_FMT_NB` differs
/// between library versions, and the header warns against using it with a
/// shared libavutil.
///
/// The default is [`NONE`](Self::NONE), the value C gives an unset format:
/// `av_frame_alloc` writes it (as `-1`) into `AVFrame.format`, and it is the
/// default of `AVCodecContext.pix_fmt`.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelFormat(ffi::AVPixelFormat);

impl PixelFormat {
    /// The pixel format C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVPixelFormat) -> Self {
        Self(raw)
    }

    /// The C encoding of this pixel format.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVPixelFormat {
        self.0
    }
}

/// Declares each named pixel format as an associated constant of
/// [`PixelFormat`] over its C enumerator, and the constants' names for
/// `Debug`.
macro_rules! pixel_formats {
    ($($(#[$meta:meta])* $name:ident = $c:ident;)*) => {
        impl PixelFormat {
            $(
                $(#[$meta])*
                pub const $name: Self = Self(ffi::AVPixelFormat::$c);
            )*

            /// The name of the constant this value equals, if any: the
            /// primary one, for a value that also has an alias.
            const fn name(self) -> Option<&'static str> {
                Some(match self {
                    $(Self::$name => stringify!($name),)*
                    _ => return None,
                })
            }
        }
    };
}

/// `be` on a big-endian target and `le` on a little-endian one, as C's
/// `AV_PIX_FMT_NE(be, le)` selects with `AV_HAVE_BIGENDIAN`.
const fn native_endian(be: PixelFormat, le: PixelFormat) -> PixelFormat {
    if cfg!(target_endian = "big") { be } else { le }
}

pixel_formats! {
    /// No pixel format: the value of an unset format field, the result of a failed lookup, and
    /// the terminator of C's format lists.
    NONE = AV_PIX_FMT_NONE;
    /// Planar YUV 4:2:0, 12bpp, (1 Cr & Cb sample per 2x2 Y samples).
    YUV420P = AV_PIX_FMT_YUV420P;
    /// Packed YUV 4:2:2, 16bpp, Y0 Cb Y1 Cr.
    YUYV422 = AV_PIX_FMT_YUYV422;
    /// Packed RGB 8:8:8, 24bpp, RGBRGB...
    RGB24 = AV_PIX_FMT_RGB24;
    /// Packed RGB 8:8:8, 24bpp, BGRBGR...
    BGR24 = AV_PIX_FMT_BGR24;
    /// Planar YUV 4:2:2, 16bpp, (1 Cr & Cb sample per 2x1 Y samples).
    YUV422P = AV_PIX_FMT_YUV422P;
    /// Planar YUV 4:4:4, 24bpp, (1 Cr & Cb sample per 1x1 Y samples).
    YUV444P = AV_PIX_FMT_YUV444P;
    /// Planar YUV 4:1:0, 9bpp, (1 Cr & Cb sample per 4x4 Y samples).
    YUV410P = AV_PIX_FMT_YUV410P;
    /// Planar YUV 4:1:1, 12bpp, (1 Cr & Cb sample per 4x1 Y samples).
    YUV411P = AV_PIX_FMT_YUV411P;
    /// Y, 8bpp.
    GRAY8 = AV_PIX_FMT_GRAY8;
    /// Y, 1bpp, 0 is white, 1 is black, in each byte pixels are ordered from the msb to the
    /// lsb.
    MONOWHITE = AV_PIX_FMT_MONOWHITE;
    /// Y, 1bpp, 0 is black, 1 is white, in each byte pixels are ordered from the msb to the
    /// lsb.
    MONOBLACK = AV_PIX_FMT_MONOBLACK;
    /// 8 bits with [`RGB32`](Self::RGB32) palette.
    PAL8 = AV_PIX_FMT_PAL8;
    /// Planar YUV 4:2:0, 12bpp, full scale (JPEG), deprecated in favor of
    /// [`YUV420P`](Self::YUV420P) and setting `color_range`.
    YUVJ420P = AV_PIX_FMT_YUVJ420P;
    /// Planar YUV 4:2:2, 16bpp, full scale (JPEG), deprecated in favor of
    /// [`YUV422P`](Self::YUV422P) and setting `color_range`.
    YUVJ422P = AV_PIX_FMT_YUVJ422P;
    /// Planar YUV 4:4:4, 24bpp, full scale (JPEG), deprecated in favor of
    /// [`YUV444P`](Self::YUV444P) and setting `color_range`.
    YUVJ444P = AV_PIX_FMT_YUVJ444P;
    /// Packed YUV 4:2:2, 16bpp, Cb Y0 Cr Y1.
    UYVY422 = AV_PIX_FMT_UYVY422;
    /// Packed YUV 4:1:1, 12bpp, Cb Y0 Y1 Cr Y2 Y3.
    UYYVYY411 = AV_PIX_FMT_UYYVYY411;
    /// Packed RGB 3:3:2, 8bpp, (msb)2B 3G 3R(lsb).
    BGR8 = AV_PIX_FMT_BGR8;
    /// Packed RGB 1:2:1 bitstream, 4bpp, (msb)1B 2G 1R(lsb), a byte contains two pixels, the
    /// first pixel in the byte is the one composed by the 4 msb bits.
    BGR4 = AV_PIX_FMT_BGR4;
    /// Packed RGB 1:2:1, 8bpp, (msb)1B 2G 1R(lsb).
    BGR4_BYTE = AV_PIX_FMT_BGR4_BYTE;
    /// Packed RGB 3:3:2, 8bpp, (msb)3R 3G 2B(lsb).
    RGB8 = AV_PIX_FMT_RGB8;
    /// Packed RGB 1:2:1 bitstream, 4bpp, (msb)1R 2G 1B(lsb), a byte contains two pixels, the
    /// first pixel in the byte is the one composed by the 4 msb bits.
    RGB4 = AV_PIX_FMT_RGB4;
    /// Packed RGB 1:2:1, 8bpp, (msb)1R 2G 1B(lsb).
    RGB4_BYTE = AV_PIX_FMT_RGB4_BYTE;
    /// Planar YUV 4:2:0, 12bpp, 1 plane for Y and 1 plane for the UV components, which are
    /// interleaved (first byte U and the following byte V).
    NV12 = AV_PIX_FMT_NV12;
    /// As [`NV12`](Self::NV12), but U and V bytes are swapped.
    NV21 = AV_PIX_FMT_NV21;
    /// Packed ARGB 8:8:8:8, 32bpp, ARGBARGB...
    ARGB = AV_PIX_FMT_ARGB;
    /// Packed RGBA 8:8:8:8, 32bpp, RGBARGBA...
    RGBA = AV_PIX_FMT_RGBA;
    /// Packed ABGR 8:8:8:8, 32bpp, ABGRABGR...
    ABGR = AV_PIX_FMT_ABGR;
    /// Packed BGRA 8:8:8:8, 32bpp, BGRABGRA...
    BGRA = AV_PIX_FMT_BGRA;
    /// Y, 16bpp, big-endian.
    GRAY16BE = AV_PIX_FMT_GRAY16BE;
    /// Y, 16bpp, little-endian.
    GRAY16LE = AV_PIX_FMT_GRAY16LE;
    /// Planar YUV 4:4:0 (1 Cr & Cb sample per 1x2 Y samples).
    YUV440P = AV_PIX_FMT_YUV440P;
    /// Planar YUV 4:4:0 full scale (JPEG), deprecated in favor of [`YUV440P`](Self::YUV440P)
    /// and setting `color_range`.
    YUVJ440P = AV_PIX_FMT_YUVJ440P;
    /// Planar YUV 4:2:0, 20bpp, (1 Cr & Cb sample per 2x2 Y & A samples).
    YUVA420P = AV_PIX_FMT_YUVA420P;
    /// Packed RGB 16:16:16, 48bpp, 16R, 16G, 16B, the 2-byte value for each R/G/B component is
    /// stored as big-endian.
    RGB48BE = AV_PIX_FMT_RGB48BE;
    /// Packed RGB 16:16:16, 48bpp, 16R, 16G, 16B, the 2-byte value for each R/G/B component is
    /// stored as little-endian.
    RGB48LE = AV_PIX_FMT_RGB48LE;
    /// Packed RGB 5:6:5, 16bpp, (msb) 5R 6G 5B(lsb), big-endian.
    RGB565BE = AV_PIX_FMT_RGB565BE;
    /// Packed RGB 5:6:5, 16bpp, (msb) 5R 6G 5B(lsb), little-endian.
    RGB565LE = AV_PIX_FMT_RGB565LE;
    /// Packed RGB 5:5:5, 16bpp, (msb)1X 5R 5G 5B(lsb), big-endian, X=unused/undefined.
    RGB555BE = AV_PIX_FMT_RGB555BE;
    /// Packed RGB 5:5:5, 16bpp, (msb)1X 5R 5G 5B(lsb), little-endian, X=unused/undefined.
    RGB555LE = AV_PIX_FMT_RGB555LE;
    /// Packed BGR 5:6:5, 16bpp, (msb) 5B 6G 5R(lsb), big-endian.
    BGR565BE = AV_PIX_FMT_BGR565BE;
    /// Packed BGR 5:6:5, 16bpp, (msb) 5B 6G 5R(lsb), little-endian.
    BGR565LE = AV_PIX_FMT_BGR565LE;
    /// Packed BGR 5:5:5, 16bpp, (msb)1X 5B 5G 5R(lsb), big-endian, X=unused/undefined.
    BGR555BE = AV_PIX_FMT_BGR555BE;
    /// Packed BGR 5:5:5, 16bpp, (msb)1X 5B 5G 5R(lsb), little-endian, X=unused/undefined.
    BGR555LE = AV_PIX_FMT_BGR555LE;
    /// Hardware acceleration through VA-API, `data[3]` contains a VASurfaceID.
    VAAPI = AV_PIX_FMT_VAAPI;
    /// Planar YUV 4:2:0, 24bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian.
    YUV420P16LE = AV_PIX_FMT_YUV420P16LE;
    /// Planar YUV 4:2:0, 24bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian.
    YUV420P16BE = AV_PIX_FMT_YUV420P16BE;
    /// Planar YUV 4:2:2, 32bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian.
    YUV422P16LE = AV_PIX_FMT_YUV422P16LE;
    /// Planar YUV 4:2:2, 32bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    YUV422P16BE = AV_PIX_FMT_YUV422P16BE;
    /// Planar YUV 4:4:4, 48bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian.
    YUV444P16LE = AV_PIX_FMT_YUV444P16LE;
    /// Planar YUV 4:4:4, 48bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian.
    YUV444P16BE = AV_PIX_FMT_YUV444P16BE;
    /// HW decoding through DXVA2, `Picture.data[3]` contains a LPDIRECT3DSURFACE9 pointer.
    DXVA2_VLD = AV_PIX_FMT_DXVA2_VLD;
    /// Packed RGB 4:4:4, 16bpp, (msb)4X 4R 4G 4B(lsb), little-endian, X=unused/undefined.
    RGB444LE = AV_PIX_FMT_RGB444LE;
    /// Packed RGB 4:4:4, 16bpp, (msb)4X 4R 4G 4B(lsb), big-endian, X=unused/undefined.
    RGB444BE = AV_PIX_FMT_RGB444BE;
    /// Packed BGR 4:4:4, 16bpp, (msb)4X 4B 4G 4R(lsb), little-endian, X=unused/undefined.
    BGR444LE = AV_PIX_FMT_BGR444LE;
    /// Packed BGR 4:4:4, 16bpp, (msb)4X 4B 4G 4R(lsb), big-endian, X=unused/undefined.
    BGR444BE = AV_PIX_FMT_BGR444BE;
    /// 8 bits gray, 8 bits alpha.
    YA8 = AV_PIX_FMT_YA8;
    /// Packed RGB 16:16:16, 48bpp, 16B, 16G, 16R, the 2-byte value for each R/G/B component is
    /// stored as big-endian.
    BGR48BE = AV_PIX_FMT_BGR48BE;
    /// Packed RGB 16:16:16, 48bpp, 16B, 16G, 16R, the 2-byte value for each R/G/B component is
    /// stored as little-endian.
    BGR48LE = AV_PIX_FMT_BGR48LE;
    /// Planar YUV 4:2:0, 13.5bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian.
    ///
    /// The following 12 formats have the disadvantage of needing 1 format for each bit depth.
    /// Notice that each 9/10 bits sample is stored in 16 bits with extra padding. If you want
    /// to support multiple bit depths, then using `AV_PIX_FMT_YUV420P16*` with the bpp stored
    /// separately is better.
    YUV420P9BE = AV_PIX_FMT_YUV420P9BE;
    /// Planar YUV 4:2:0, 13.5bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian.
    YUV420P9LE = AV_PIX_FMT_YUV420P9LE;
    /// Planar YUV 4:2:0, 15bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian.
    YUV420P10BE = AV_PIX_FMT_YUV420P10BE;
    /// Planar YUV 4:2:0, 15bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian.
    YUV420P10LE = AV_PIX_FMT_YUV420P10LE;
    /// Planar YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    YUV422P10BE = AV_PIX_FMT_YUV422P10BE;
    /// Planar YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian.
    YUV422P10LE = AV_PIX_FMT_YUV422P10LE;
    /// Planar YUV 4:4:4, 27bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian.
    YUV444P9BE = AV_PIX_FMT_YUV444P9BE;
    /// Planar YUV 4:4:4, 27bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian.
    YUV444P9LE = AV_PIX_FMT_YUV444P9LE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian.
    YUV444P10BE = AV_PIX_FMT_YUV444P10BE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian.
    YUV444P10LE = AV_PIX_FMT_YUV444P10LE;
    /// Planar YUV 4:2:2, 18bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    YUV422P9BE = AV_PIX_FMT_YUV422P9BE;
    /// Planar YUV 4:2:2, 18bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian.
    YUV422P9LE = AV_PIX_FMT_YUV422P9LE;
    /// Planar GBR 4:4:4 24bpp.
    GBRP = AV_PIX_FMT_GBRP;
    /// Planar GBR 4:4:4 27bpp, big-endian.
    GBRP9BE = AV_PIX_FMT_GBRP9BE;
    /// Planar GBR 4:4:4 27bpp, little-endian.
    GBRP9LE = AV_PIX_FMT_GBRP9LE;
    /// Planar GBR 4:4:4 30bpp, big-endian.
    GBRP10BE = AV_PIX_FMT_GBRP10BE;
    /// Planar GBR 4:4:4 30bpp, little-endian.
    GBRP10LE = AV_PIX_FMT_GBRP10LE;
    /// Planar GBR 4:4:4 48bpp, big-endian.
    GBRP16BE = AV_PIX_FMT_GBRP16BE;
    /// Planar GBR 4:4:4 48bpp, little-endian.
    GBRP16LE = AV_PIX_FMT_GBRP16LE;
    /// Planar YUV 4:2:2 24bpp, (1 Cr & Cb sample per 2x1 Y & A samples).
    YUVA422P = AV_PIX_FMT_YUVA422P;
    /// Planar YUV 4:4:4 32bpp, (1 Cr & Cb sample per 1x1 Y & A samples).
    YUVA444P = AV_PIX_FMT_YUVA444P;
    /// Planar YUV 4:2:0 22.5bpp, (1 Cr & Cb sample per 2x2 Y & A samples), big-endian.
    YUVA420P9BE = AV_PIX_FMT_YUVA420P9BE;
    /// Planar YUV 4:2:0 22.5bpp, (1 Cr & Cb sample per 2x2 Y & A samples), little-endian.
    YUVA420P9LE = AV_PIX_FMT_YUVA420P9LE;
    /// Planar YUV 4:2:2 27bpp, (1 Cr & Cb sample per 2x1 Y & A samples), big-endian.
    YUVA422P9BE = AV_PIX_FMT_YUVA422P9BE;
    /// Planar YUV 4:2:2 27bpp, (1 Cr & Cb sample per 2x1 Y & A samples), little-endian.
    YUVA422P9LE = AV_PIX_FMT_YUVA422P9LE;
    /// Planar YUV 4:4:4 36bpp, (1 Cr & Cb sample per 1x1 Y & A samples), big-endian.
    YUVA444P9BE = AV_PIX_FMT_YUVA444P9BE;
    /// Planar YUV 4:4:4 36bpp, (1 Cr & Cb sample per 1x1 Y & A samples), little-endian.
    YUVA444P9LE = AV_PIX_FMT_YUVA444P9LE;
    /// Planar YUV 4:2:0 25bpp, (1 Cr & Cb sample per 2x2 Y & A samples, big-endian).
    YUVA420P10BE = AV_PIX_FMT_YUVA420P10BE;
    /// Planar YUV 4:2:0 25bpp, (1 Cr & Cb sample per 2x2 Y & A samples, little-endian).
    YUVA420P10LE = AV_PIX_FMT_YUVA420P10LE;
    /// Planar YUV 4:2:2 30bpp, (1 Cr & Cb sample per 2x1 Y & A samples, big-endian).
    YUVA422P10BE = AV_PIX_FMT_YUVA422P10BE;
    /// Planar YUV 4:2:2 30bpp, (1 Cr & Cb sample per 2x1 Y & A samples, little-endian).
    YUVA422P10LE = AV_PIX_FMT_YUVA422P10LE;
    /// Planar YUV 4:4:4 40bpp, (1 Cr & Cb sample per 1x1 Y & A samples, big-endian).
    YUVA444P10BE = AV_PIX_FMT_YUVA444P10BE;
    /// Planar YUV 4:4:4 40bpp, (1 Cr & Cb sample per 1x1 Y & A samples, little-endian).
    YUVA444P10LE = AV_PIX_FMT_YUVA444P10LE;
    /// Planar YUV 4:2:0 40bpp, (1 Cr & Cb sample per 2x2 Y & A samples, big-endian).
    YUVA420P16BE = AV_PIX_FMT_YUVA420P16BE;
    /// Planar YUV 4:2:0 40bpp, (1 Cr & Cb sample per 2x2 Y & A samples, little-endian).
    YUVA420P16LE = AV_PIX_FMT_YUVA420P16LE;
    /// Planar YUV 4:2:2 48bpp, (1 Cr & Cb sample per 2x1 Y & A samples, big-endian).
    YUVA422P16BE = AV_PIX_FMT_YUVA422P16BE;
    /// Planar YUV 4:2:2 48bpp, (1 Cr & Cb sample per 2x1 Y & A samples, little-endian).
    YUVA422P16LE = AV_PIX_FMT_YUVA422P16LE;
    /// Planar YUV 4:4:4 64bpp, (1 Cr & Cb sample per 1x1 Y & A samples, big-endian).
    YUVA444P16BE = AV_PIX_FMT_YUVA444P16BE;
    /// Planar YUV 4:4:4 64bpp, (1 Cr & Cb sample per 1x1 Y & A samples, little-endian).
    YUVA444P16LE = AV_PIX_FMT_YUVA444P16LE;
    /// HW acceleration through VDPAU, `Picture.data[3]` contains a VdpVideoSurface.
    VDPAU = AV_PIX_FMT_VDPAU;
    /// Packed XYZ 4:4:4, 36 bpp, (msb) 12X, 12Y, 12Z (lsb), the 2-byte value for each X/Y/Z is
    /// stored as little-endian, the 4 lower bits are set to 0.
    XYZ12LE = AV_PIX_FMT_XYZ12LE;
    /// Packed XYZ 4:4:4, 36 bpp, (msb) 12X, 12Y, 12Z (lsb), the 2-byte value for each X/Y/Z is
    /// stored as big-endian, the 4 lower bits are set to 0.
    XYZ12BE = AV_PIX_FMT_XYZ12BE;
    /// Interleaved chroma YUV 4:2:2, 16bpp, (1 Cr & Cb sample per 2x1 Y samples).
    NV16 = AV_PIX_FMT_NV16;
    /// Interleaved chroma YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples),
    /// little-endian.
    NV20LE = AV_PIX_FMT_NV20LE;
    /// Interleaved chroma YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    NV20BE = AV_PIX_FMT_NV20BE;
    /// Packed RGBA 16:16:16:16, 64bpp, 16R, 16G, 16B, 16A, the 2-byte value for each R/G/B/A
    /// component is stored as big-endian.
    RGBA64BE = AV_PIX_FMT_RGBA64BE;
    /// Packed RGBA 16:16:16:16, 64bpp, 16R, 16G, 16B, 16A, the 2-byte value for each R/G/B/A
    /// component is stored as little-endian.
    RGBA64LE = AV_PIX_FMT_RGBA64LE;
    /// Packed RGBA 16:16:16:16, 64bpp, 16B, 16G, 16R, 16A, the 2-byte value for each R/G/B/A
    /// component is stored as big-endian.
    BGRA64BE = AV_PIX_FMT_BGRA64BE;
    /// Packed RGBA 16:16:16:16, 64bpp, 16B, 16G, 16R, 16A, the 2-byte value for each R/G/B/A
    /// component is stored as little-endian.
    BGRA64LE = AV_PIX_FMT_BGRA64LE;
    /// Packed YUV 4:2:2, 16bpp, Y0 Cr Y1 Cb.
    YVYU422 = AV_PIX_FMT_YVYU422;
    /// 16 bits gray, 16 bits alpha (big-endian).
    YA16BE = AV_PIX_FMT_YA16BE;
    /// 16 bits gray, 16 bits alpha (little-endian).
    YA16LE = AV_PIX_FMT_YA16LE;
    /// Planar GBRA 4:4:4:4 32bpp.
    GBRAP = AV_PIX_FMT_GBRAP;
    /// Planar GBRA 4:4:4:4 64bpp, big-endian.
    GBRAP16BE = AV_PIX_FMT_GBRAP16BE;
    /// Planar GBRA 4:4:4:4 64bpp, little-endian.
    GBRAP16LE = AV_PIX_FMT_GBRAP16LE;
    /// HW acceleration through QSV, `data[3]` contains a pointer to the mfxFrameSurface1
    /// structure.
    ///
    /// Before FFmpeg 5.0: `mfxFrameSurface1.Data.MemId` contains a pointer when importing the
    /// following frames as QSV frames:
    ///
    /// VAAPI: `mfxFrameSurface1.Data.MemId` contains a pointer to VASurfaceID.
    ///
    /// DXVA2: `mfxFrameSurface1.Data.MemId` contains a pointer to IDirect3DSurface9.
    ///
    /// FFmpeg 5.0 and above: `mfxFrameSurface1.Data.MemId` contains a pointer to the mfxHDLPair
    /// structure when importing the following frames as QSV frames:
    ///
    /// VAAPI: mfxHDLPair.first contains a VASurfaceID pointer. mfxHDLPair.second is always
    /// `MFX_INFINITE`.
    ///
    /// DXVA2: mfxHDLPair.first contains IDirect3DSurface9 pointer. mfxHDLPair.second is always
    /// `MFX_INFINITE`.
    ///
    /// D3D11: mfxHDLPair.first contains a ID3D11Texture2D pointer. mfxHDLPair.second contains
    /// the texture array index of the frame if the ID3D11Texture2D is an array texture, or
    /// always `MFX_INFINITE` if it is a normal texture.
    QSV = AV_PIX_FMT_QSV;
    /// HW acceleration though MMAL, `data[3]` contains a pointer to the `MMAL_BUFFER_HEADER_T`
    /// structure.
    MMAL = AV_PIX_FMT_MMAL;
    /// HW decoding through Direct3D11 via old API, `Picture.data[3]` contains a
    /// ID3D11VideoDecoderOutputView pointer.
    D3D11VA_VLD = AV_PIX_FMT_D3D11VA_VLD;
    /// HW acceleration through CUDA. `data[i]` contain CUdeviceptr pointers exactly as for
    /// system memory frames.
    CUDA = AV_PIX_FMT_CUDA;
    /// Packed RGB 8:8:8, 32bpp, XRGBXRGB... X=unused/undefined.
    XRGB = AV_PIX_FMT_0RGB;
    /// Packed RGB 8:8:8, 32bpp, RGBXRGBX... X=unused/undefined.
    RGB0 = AV_PIX_FMT_RGB0;
    /// Packed BGR 8:8:8, 32bpp, XBGRXBGR... X=unused/undefined.
    XBGR = AV_PIX_FMT_0BGR;
    /// Packed BGR 8:8:8, 32bpp, BGRXBGRX... X=unused/undefined.
    BGR0 = AV_PIX_FMT_BGR0;
    /// Planar YUV 4:2:0,18bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian.
    YUV420P12BE = AV_PIX_FMT_YUV420P12BE;
    /// Planar YUV 4:2:0,18bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian.
    YUV420P12LE = AV_PIX_FMT_YUV420P12LE;
    /// Planar YUV 4:2:0,21bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian.
    YUV420P14BE = AV_PIX_FMT_YUV420P14BE;
    /// Planar YUV 4:2:0,21bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian.
    YUV420P14LE = AV_PIX_FMT_YUV420P14LE;
    /// Planar YUV 4:2:2,24bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    YUV422P12BE = AV_PIX_FMT_YUV422P12BE;
    /// Planar YUV 4:2:2,24bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian.
    YUV422P12LE = AV_PIX_FMT_YUV422P12LE;
    /// Planar YUV 4:2:2,28bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian.
    YUV422P14BE = AV_PIX_FMT_YUV422P14BE;
    /// Planar YUV 4:2:2,28bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian.
    YUV422P14LE = AV_PIX_FMT_YUV422P14LE;
    /// Planar YUV 4:4:4,36bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian.
    YUV444P12BE = AV_PIX_FMT_YUV444P12BE;
    /// Planar YUV 4:4:4,36bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian.
    YUV444P12LE = AV_PIX_FMT_YUV444P12LE;
    /// Planar YUV 4:4:4,42bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian.
    YUV444P14BE = AV_PIX_FMT_YUV444P14BE;
    /// Planar YUV 4:4:4,42bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian.
    YUV444P14LE = AV_PIX_FMT_YUV444P14LE;
    /// Planar GBR 4:4:4 36bpp, big-endian.
    GBRP12BE = AV_PIX_FMT_GBRP12BE;
    /// Planar GBR 4:4:4 36bpp, little-endian.
    GBRP12LE = AV_PIX_FMT_GBRP12LE;
    /// Planar GBR 4:4:4 42bpp, big-endian.
    GBRP14BE = AV_PIX_FMT_GBRP14BE;
    /// Planar GBR 4:4:4 42bpp, little-endian.
    GBRP14LE = AV_PIX_FMT_GBRP14LE;
    /// Planar YUV 4:1:1, 12bpp, (1 Cr & Cb sample per 4x1 Y samples) full scale (JPEG),
    /// deprecated in favor of [`YUV411P`](Self::YUV411P) and setting `color_range`.
    YUVJ411P = AV_PIX_FMT_YUVJ411P;
    /// Bayer, BGBG..(odd line), GRGR..(even line), 8-bit samples.
    BAYER_BGGR8 = AV_PIX_FMT_BAYER_BGGR8;
    /// Bayer, RGRG..(odd line), GBGB..(even line), 8-bit samples.
    BAYER_RGGB8 = AV_PIX_FMT_BAYER_RGGB8;
    /// Bayer, GBGB..(odd line), RGRG..(even line), 8-bit samples.
    BAYER_GBRG8 = AV_PIX_FMT_BAYER_GBRG8;
    /// Bayer, GRGR..(odd line), BGBG..(even line), 8-bit samples.
    BAYER_GRBG8 = AV_PIX_FMT_BAYER_GRBG8;
    /// Bayer, BGBG..(odd line), GRGR..(even line), 16-bit samples, little-endian.
    BAYER_BGGR16LE = AV_PIX_FMT_BAYER_BGGR16LE;
    /// Bayer, BGBG..(odd line), GRGR..(even line), 16-bit samples, big-endian.
    BAYER_BGGR16BE = AV_PIX_FMT_BAYER_BGGR16BE;
    /// Bayer, RGRG..(odd line), GBGB..(even line), 16-bit samples, little-endian.
    BAYER_RGGB16LE = AV_PIX_FMT_BAYER_RGGB16LE;
    /// Bayer, RGRG..(odd line), GBGB..(even line), 16-bit samples, big-endian.
    BAYER_RGGB16BE = AV_PIX_FMT_BAYER_RGGB16BE;
    /// Bayer, GBGB..(odd line), RGRG..(even line), 16-bit samples, little-endian.
    BAYER_GBRG16LE = AV_PIX_FMT_BAYER_GBRG16LE;
    /// Bayer, GBGB..(odd line), RGRG..(even line), 16-bit samples, big-endian.
    BAYER_GBRG16BE = AV_PIX_FMT_BAYER_GBRG16BE;
    /// Bayer, GRGR..(odd line), BGBG..(even line), 16-bit samples, little-endian.
    BAYER_GRBG16LE = AV_PIX_FMT_BAYER_GRBG16LE;
    /// Bayer, GRGR..(odd line), BGBG..(even line), 16-bit samples, big-endian.
    BAYER_GRBG16BE = AV_PIX_FMT_BAYER_GRBG16BE;
    /// Planar YUV 4:4:0,20bpp, (1 Cr & Cb sample per 1x2 Y samples), little-endian.
    YUV440P10LE = AV_PIX_FMT_YUV440P10LE;
    /// Planar YUV 4:4:0,20bpp, (1 Cr & Cb sample per 1x2 Y samples), big-endian.
    YUV440P10BE = AV_PIX_FMT_YUV440P10BE;
    /// Planar YUV 4:4:0,24bpp, (1 Cr & Cb sample per 1x2 Y samples), little-endian.
    YUV440P12LE = AV_PIX_FMT_YUV440P12LE;
    /// Planar YUV 4:4:0,24bpp, (1 Cr & Cb sample per 1x2 Y samples), big-endian.
    YUV440P12BE = AV_PIX_FMT_YUV440P12BE;
    /// Packed AYUV 4:4:4,64bpp (1 Cr & Cb sample per 1x1 Y & A samples), little-endian.
    AYUV64LE = AV_PIX_FMT_AYUV64LE;
    /// Packed AYUV 4:4:4,64bpp (1 Cr & Cb sample per 1x1 Y & A samples), big-endian.
    AYUV64BE = AV_PIX_FMT_AYUV64BE;
    /// Hardware decoding through Videotoolbox.
    VIDEOTOOLBOX = AV_PIX_FMT_VIDEOTOOLBOX;
    /// Like NV12, with 10bpp per component, data in the high bits, zeros in the low bits,
    /// little-endian.
    P010LE = AV_PIX_FMT_P010LE;
    /// Like NV12, with 10bpp per component, data in the high bits, zeros in the low bits,
    /// big-endian.
    P010BE = AV_PIX_FMT_P010BE;
    /// Planar GBR 4:4:4:4 48bpp, big-endian.
    GBRAP12BE = AV_PIX_FMT_GBRAP12BE;
    /// Planar GBR 4:4:4:4 48bpp, little-endian.
    GBRAP12LE = AV_PIX_FMT_GBRAP12LE;
    /// Planar GBR 4:4:4:4 40bpp, big-endian.
    GBRAP10BE = AV_PIX_FMT_GBRAP10BE;
    /// Planar GBR 4:4:4:4 40bpp, little-endian.
    GBRAP10LE = AV_PIX_FMT_GBRAP10LE;
    /// Hardware decoding through MediaCodec.
    MEDIACODEC = AV_PIX_FMT_MEDIACODEC;
    /// Y, 12bpp, big-endian.
    GRAY12BE = AV_PIX_FMT_GRAY12BE;
    /// Y, 12bpp, little-endian.
    GRAY12LE = AV_PIX_FMT_GRAY12LE;
    /// Y, 10bpp, big-endian.
    GRAY10BE = AV_PIX_FMT_GRAY10BE;
    /// Y, 10bpp, little-endian.
    GRAY10LE = AV_PIX_FMT_GRAY10LE;
    /// Like NV12, with 16bpp per component, little-endian.
    P016LE = AV_PIX_FMT_P016LE;
    /// Like NV12, with 16bpp per component, big-endian.
    P016BE = AV_PIX_FMT_P016BE;
    /// Hardware surfaces for Direct3D11.
    ///
    /// This is preferred over the legacy [`D3D11VA_VLD`](Self::D3D11VA_VLD). The new D3D11
    /// hwaccel API and filtering support [`D3D11`](Self::D3D11) only.
    ///
    /// `data[0]` contains a ID3D11Texture2D pointer, and `data[1]` contains the texture array
    /// index of the frame as `intptr_t` if the ID3D11Texture2D is an array texture (or always 0
    /// if it's a normal texture).
    D3D11 = AV_PIX_FMT_D3D11;
    /// Y, 9bpp, big-endian.
    GRAY9BE = AV_PIX_FMT_GRAY9BE;
    /// Y, 9bpp, little-endian.
    GRAY9LE = AV_PIX_FMT_GRAY9LE;
    /// IEEE-754 single precision planar GBR 4:4:4, 96bpp, big-endian.
    GBRPF32BE = AV_PIX_FMT_GBRPF32BE;
    /// IEEE-754 single precision planar GBR 4:4:4, 96bpp, little-endian.
    GBRPF32LE = AV_PIX_FMT_GBRPF32LE;
    /// IEEE-754 single precision planar GBRA 4:4:4:4, 128bpp, big-endian.
    GBRAPF32BE = AV_PIX_FMT_GBRAPF32BE;
    /// IEEE-754 single precision planar GBRA 4:4:4:4, 128bpp, little-endian.
    GBRAPF32LE = AV_PIX_FMT_GBRAPF32LE;
    /// DRM-managed buffers exposed through PRIME buffer sharing.
    ///
    /// `data[0]` points to an AVDRMFrameDescriptor.
    DRM_PRIME = AV_PIX_FMT_DRM_PRIME;
    /// Hardware surfaces for OpenCL.
    ///
    /// `data[i]` contain 2D image objects (typed in C as `cl_mem`, used in OpenCL as
    /// `image2d_t`) for each plane of the surface.
    OPENCL = AV_PIX_FMT_OPENCL;
    /// Y, 14bpp, big-endian.
    GRAY14BE = AV_PIX_FMT_GRAY14BE;
    /// Y, 14bpp, little-endian.
    GRAY14LE = AV_PIX_FMT_GRAY14LE;
    /// IEEE-754 single precision Y, 32bpp, big-endian.
    GRAYF32BE = AV_PIX_FMT_GRAYF32BE;
    /// IEEE-754 single precision Y, 32bpp, little-endian.
    GRAYF32LE = AV_PIX_FMT_GRAYF32LE;
    /// Planar YUV 4:2:2,24bpp, (1 Cr & Cb sample per 2x1 Y samples), 12b alpha, big-endian.
    YUVA422P12BE = AV_PIX_FMT_YUVA422P12BE;
    /// Planar YUV 4:2:2,24bpp, (1 Cr & Cb sample per 2x1 Y samples), 12b alpha, little-endian.
    YUVA422P12LE = AV_PIX_FMT_YUVA422P12LE;
    /// Planar YUV 4:4:4,36bpp, (1 Cr & Cb sample per 1x1 Y samples), 12b alpha, big-endian.
    YUVA444P12BE = AV_PIX_FMT_YUVA444P12BE;
    /// Planar YUV 4:4:4,36bpp, (1 Cr & Cb sample per 1x1 Y samples), 12b alpha, little-endian.
    YUVA444P12LE = AV_PIX_FMT_YUVA444P12LE;
    /// Planar YUV 4:4:4, 24bpp, 1 plane for Y and 1 plane for the UV components, which are
    /// interleaved (first byte U and the following byte V).
    NV24 = AV_PIX_FMT_NV24;
    /// As [`NV24`](Self::NV24), but U and V bytes are swapped.
    NV42 = AV_PIX_FMT_NV42;
    /// Vulkan hardware images.
    ///
    /// `data[0]` points to an AVVkFrame.
    VULKAN = AV_PIX_FMT_VULKAN;
    /// Packed YUV 4:2:2 like YUYV422, 20bpp, data in the high bits, big-endian.
    Y210BE = AV_PIX_FMT_Y210BE;
    /// Packed YUV 4:2:2 like YUYV422, 20bpp, data in the high bits, little-endian.
    Y210LE = AV_PIX_FMT_Y210LE;
    /// Packed RGB 10:10:10, 30bpp, (msb)2X 10R 10G 10B(lsb), little-endian, X=unused/undefined.
    X2RGB10LE = AV_PIX_FMT_X2RGB10LE;
    /// Packed RGB 10:10:10, 30bpp, (msb)2X 10R 10G 10B(lsb), big-endian, X=unused/undefined.
    X2RGB10BE = AV_PIX_FMT_X2RGB10BE;
    /// Packed BGR 10:10:10, 30bpp, (msb)2X 10B 10G 10R(lsb), little-endian, X=unused/undefined.
    X2BGR10LE = AV_PIX_FMT_X2BGR10LE;
    /// Packed BGR 10:10:10, 30bpp, (msb)2X 10B 10G 10R(lsb), big-endian, X=unused/undefined.
    X2BGR10BE = AV_PIX_FMT_X2BGR10BE;
    /// Interleaved chroma YUV 4:2:2, 20bpp, data in the high bits, big-endian.
    P210BE = AV_PIX_FMT_P210BE;
    /// Interleaved chroma YUV 4:2:2, 20bpp, data in the high bits, little-endian.
    P210LE = AV_PIX_FMT_P210LE;
    /// Interleaved chroma YUV 4:4:4, 30bpp, data in the high bits, big-endian.
    P410BE = AV_PIX_FMT_P410BE;
    /// Interleaved chroma YUV 4:4:4, 30bpp, data in the high bits, little-endian.
    P410LE = AV_PIX_FMT_P410LE;
    /// Interleaved chroma YUV 4:2:2, 32bpp, big-endian.
    P216BE = AV_PIX_FMT_P216BE;
    /// Interleaved chroma YUV 4:2:2, 32bpp, little-endian.
    P216LE = AV_PIX_FMT_P216LE;
    /// Interleaved chroma YUV 4:4:4, 48bpp, big-endian.
    P416BE = AV_PIX_FMT_P416BE;
    /// Interleaved chroma YUV 4:4:4, 48bpp, little-endian.
    P416LE = AV_PIX_FMT_P416LE;
    /// Packed VUYA 4:4:4:4, 32bpp (1 Cr & Cb sample per 1x1 Y & A samples), VUYAVUYA...
    VUYA = AV_PIX_FMT_VUYA;
    /// IEEE-754 half precision packed RGBA 16:16:16:16, 64bpp, RGBARGBA..., big-endian.
    RGBAF16BE = AV_PIX_FMT_RGBAF16BE;
    /// IEEE-754 half precision packed RGBA 16:16:16:16, 64bpp, RGBARGBA..., little-endian.
    RGBAF16LE = AV_PIX_FMT_RGBAF16LE;
    /// Packed VUYX 4:4:4:4, 32bpp, Variant of VUYA where alpha channel is left undefined.
    VUYX = AV_PIX_FMT_VUYX;
    /// Like NV12, with 12bpp per component, data in the high bits, zeros in the low bits,
    /// little-endian.
    P012LE = AV_PIX_FMT_P012LE;
    /// Like NV12, with 12bpp per component, data in the high bits, zeros in the low bits,
    /// big-endian.
    P012BE = AV_PIX_FMT_P012BE;
    /// Packed YUV 4:2:2 like YUYV422, 24bpp, data in the high bits, zeros in the low bits,
    /// big-endian.
    Y212BE = AV_PIX_FMT_Y212BE;
    /// Packed YUV 4:2:2 like YUYV422, 24bpp, data in the high bits, zeros in the low bits,
    /// little-endian.
    Y212LE = AV_PIX_FMT_Y212LE;
    /// Packed XVYU 4:4:4, 32bpp, (msb)2X 10V 10Y 10U(lsb), big-endian, variant of Y410 where
    /// alpha channel is left undefined.
    XV30BE = AV_PIX_FMT_XV30BE;
    /// Packed XVYU 4:4:4, 32bpp, (msb)2X 10V 10Y 10U(lsb), little-endian, variant of Y410 where
    /// alpha channel is left undefined.
    XV30LE = AV_PIX_FMT_XV30LE;
    /// Packed XVYU 4:4:4, 48bpp, data in the high bits, zeros in the low bits, big-endian,
    /// variant of Y412 where alpha channel is left undefined.
    XV36BE = AV_PIX_FMT_XV36BE;
    /// Packed XVYU 4:4:4, 48bpp, data in the high bits, zeros in the low bits, little-endian,
    /// variant of Y412 where alpha channel is left undefined.
    XV36LE = AV_PIX_FMT_XV36LE;
    /// IEEE-754 single precision packed RGB 32:32:32, 96bpp, RGBRGB..., big-endian.
    RGBF32BE = AV_PIX_FMT_RGBF32BE;
    /// IEEE-754 single precision packed RGB 32:32:32, 96bpp, RGBRGB..., little-endian.
    RGBF32LE = AV_PIX_FMT_RGBF32LE;
    /// IEEE-754 single precision packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., big-endian.
    RGBAF32BE = AV_PIX_FMT_RGBAF32BE;
    /// IEEE-754 single precision packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., little-endian.
    RGBAF32LE = AV_PIX_FMT_RGBAF32LE;
    /// Interleaved chroma YUV 4:2:2, 24bpp, data in the high bits, big-endian.
    P212BE = AV_PIX_FMT_P212BE;
    /// Interleaved chroma YUV 4:2:2, 24bpp, data in the high bits, little-endian.
    P212LE = AV_PIX_FMT_P212LE;
    /// Interleaved chroma YUV 4:4:4, 36bpp, data in the high bits, big-endian.
    P412BE = AV_PIX_FMT_P412BE;
    /// Interleaved chroma YUV 4:4:4, 36bpp, data in the high bits, little-endian.
    P412LE = AV_PIX_FMT_P412LE;
    /// Planar GBR 4:4:4:4 56bpp, big-endian.
    GBRAP14BE = AV_PIX_FMT_GBRAP14BE;
    /// Planar GBR 4:4:4:4 56bpp, little-endian.
    GBRAP14LE = AV_PIX_FMT_GBRAP14LE;
    /// Hardware surfaces for Direct3D 12.
    ///
    /// `data[0]` points to an AVD3D12VAFrame.
    D3D12 = AV_PIX_FMT_D3D12;
    /// Packed AYUV 4:4:4:4, 32bpp (1 Cr & Cb sample per 1x1 Y & A samples), AYUVAYUV...
    AYUV = AV_PIX_FMT_AYUV;
    /// Packed UYVA 4:4:4:4, 32bpp (1 Cr & Cb sample per 1x1 Y & A samples), UYVAUYVA...
    UYVA = AV_PIX_FMT_UYVA;
    /// Packed VYU 4:4:4, 24bpp (1 Cr & Cb sample per 1x1 Y), VYUVYU...
    VYU444 = AV_PIX_FMT_VYU444;
    /// Packed VYUX 4:4:4 like XV30, 32bpp, (msb)10V 10Y 10U 2X(lsb), big-endian.
    V30XBE = AV_PIX_FMT_V30XBE;
    /// Packed VYUX 4:4:4 like XV30, 32bpp, (msb)10V 10Y 10U 2X(lsb), little-endian.
    V30XLE = AV_PIX_FMT_V30XLE;
    /// IEEE-754 half precision packed RGB 16:16:16, 48bpp, RGBRGB..., big-endian.
    RGBF16BE = AV_PIX_FMT_RGBF16BE;
    /// IEEE-754 half precision packed RGB 16:16:16, 48bpp, RGBRGB..., little-endian.
    RGBF16LE = AV_PIX_FMT_RGBF16LE;
    /// Packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., big-endian.
    RGBA128BE = AV_PIX_FMT_RGBA128BE;
    /// Packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., little-endian.
    RGBA128LE = AV_PIX_FMT_RGBA128LE;
    /// Packed RGBA 32:32:32, 96bpp, RGBRGB..., big-endian.
    RGB96BE = AV_PIX_FMT_RGB96BE;
    /// Packed RGBA 32:32:32, 96bpp, RGBRGB..., little-endian.
    RGB96LE = AV_PIX_FMT_RGB96LE;
    /// Packed YUV 4:2:2 like YUYV422, 32bpp, big-endian.
    Y216BE = AV_PIX_FMT_Y216BE;
    /// Packed YUV 4:2:2 like YUYV422, 32bpp, little-endian.
    Y216LE = AV_PIX_FMT_Y216LE;
    /// Packed XVYU 4:4:4, 64bpp, big-endian, variant of Y416 where alpha channel is left
    /// undefined.
    XV48BE = AV_PIX_FMT_XV48BE;
    /// Packed XVYU 4:4:4, 64bpp, little-endian, variant of Y416 where alpha channel is left
    /// undefined.
    XV48LE = AV_PIX_FMT_XV48LE;
    /// IEEE-754 half precision planer GBR 4:4:4, 48bpp, big-endian.
    GBRPF16BE = AV_PIX_FMT_GBRPF16BE;
    /// IEEE-754 half precision planer GBR 4:4:4, 48bpp, little-endian.
    GBRPF16LE = AV_PIX_FMT_GBRPF16LE;
    /// IEEE-754 half precision planar GBRA 4:4:4:4, 64bpp, big-endian.
    GBRAPF16BE = AV_PIX_FMT_GBRAPF16BE;
    /// IEEE-754 half precision planar GBRA 4:4:4:4, 64bpp, little-endian.
    GBRAPF16LE = AV_PIX_FMT_GBRAPF16LE;
    /// IEEE-754 half precision Y, 16bpp, big-endian.
    GRAYF16BE = AV_PIX_FMT_GRAYF16BE;
    /// IEEE-754 half precision Y, 16bpp, little-endian.
    GRAYF16LE = AV_PIX_FMT_GRAYF16LE;
    /// HW acceleration through AMF. `data[0]` contain AMFSurface pointer.
    AMF_SURFACE = AV_PIX_FMT_AMF_SURFACE;
    /// Y, 32bpp, big-endian.
    GRAY32BE = AV_PIX_FMT_GRAY32BE;
    /// Y, 32bpp, little-endian.
    GRAY32LE = AV_PIX_FMT_GRAY32LE;
    /// IEEE-754 single precision packed YA, 32 bits gray, 32 bits alpha, 64bpp, big-endian.
    YAF32BE = AV_PIX_FMT_YAF32BE;
    /// IEEE-754 single precision packed YA, 32 bits gray, 32 bits alpha, 64bpp, little-endian.
    YAF32LE = AV_PIX_FMT_YAF32LE;
    /// IEEE-754 half precision packed YA, 16 bits gray, 16 bits alpha, 32bpp, big-endian.
    YAF16BE = AV_PIX_FMT_YAF16BE;
    /// IEEE-754 half precision packed YA, 16 bits gray, 16 bits alpha, 32bpp, little-endian.
    YAF16LE = AV_PIX_FMT_YAF16LE;
    /// Planar GBRA 4:4:4:4 128bpp, big-endian.
    GBRAP32BE = AV_PIX_FMT_GBRAP32BE;
    /// Planar GBRA 4:4:4:4 128bpp, little-endian.
    GBRAP32LE = AV_PIX_FMT_GBRAP32LE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), lowest bits zero,
    /// big-endian.
    YUV444P10MSBBE = AV_PIX_FMT_YUV444P10MSBBE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), lowest bits zero,
    /// little-endian.
    YUV444P10MSBLE = AV_PIX_FMT_YUV444P10MSBLE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), lowest bits zero,
    /// big-endian.
    YUV444P12MSBBE = AV_PIX_FMT_YUV444P12MSBBE;
    /// Planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), lowest bits zero,
    /// little-endian.
    YUV444P12MSBLE = AV_PIX_FMT_YUV444P12MSBLE;
    /// Planar GBR 4:4:4 30bpp, lowest bits zero, big-endian.
    GBRP10MSBBE = AV_PIX_FMT_GBRP10MSBBE;
    /// Planar GBR 4:4:4 30bpp, lowest bits zero, little-endian.
    GBRP10MSBLE = AV_PIX_FMT_GBRP10MSBLE;
    /// Planar GBR 4:4:4 36bpp, lowest bits zero, big-endian.
    GBRP12MSBBE = AV_PIX_FMT_GBRP12MSBBE;
    /// Planar GBR 4:4:4 36bpp, lowest bits zero, little-endian.
    GBRP12MSBLE = AV_PIX_FMT_GBRP12MSBLE;
    /// Hardware decoding through OpenHarmony.
    OHCODEC = AV_PIX_FMT_OHCODEC;
    /// CUDA block-linear (opaque). `data[0]` is a CUarray in block-linear layout, e.g. from
    /// NVDEC opaque decode. Use for zero-copy to NVENC (CUDA array input).
    CUARRAY = AV_PIX_FMT_CUARRAY;
}

impl PixelFormat {
    /// Another name for [`YA8`](Self::YA8) (`AV_PIX_FMT_Y400A`).
    pub const Y400A: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_Y400A);
    /// Another name for [`YA8`](Self::YA8) (`AV_PIX_FMT_GRAY8A`).
    pub const GRAY8A: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_GRAY8A);
    /// Another name for [`GBRP`](Self::GBRP) (`AV_PIX_FMT_GBR24P`).
    pub const GBR24P: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_GBR24P);

    /// [`ARGB`](Self::ARGB) on big-endian targets and [`BGRA`](Self::BGRA) on little-endian
    /// ones: `AV_PIX_FMT_RGB32`, in native byte order.
    pub const RGB32: Self = native_endian(Self::ARGB, Self::BGRA);
    /// [`RGBA`](Self::RGBA) on big-endian targets and [`ABGR`](Self::ABGR) on little-endian
    /// ones: `AV_PIX_FMT_RGB32_1`, in native byte order.
    pub const RGB32_1: Self = native_endian(Self::RGBA, Self::ABGR);
    /// [`ABGR`](Self::ABGR) on big-endian targets and [`RGBA`](Self::RGBA) on little-endian
    /// ones: `AV_PIX_FMT_BGR32`, in native byte order.
    pub const BGR32: Self = native_endian(Self::ABGR, Self::RGBA);
    /// [`BGRA`](Self::BGRA) on big-endian targets and [`ARGB`](Self::ARGB) on little-endian
    /// ones: `AV_PIX_FMT_BGR32_1`, in native byte order.
    pub const BGR32_1: Self = native_endian(Self::BGRA, Self::ARGB);
    /// [`XRGB`](Self::XRGB) on big-endian targets and [`BGR0`](Self::BGR0) on little-endian
    /// ones: `AV_PIX_FMT_0RGB32`, in native byte order.
    pub const XRGB32: Self = native_endian(Self::XRGB, Self::BGR0);
    /// [`XBGR`](Self::XBGR) on big-endian targets and [`RGB0`](Self::RGB0) on little-endian
    /// ones: `AV_PIX_FMT_0BGR32`, in native byte order.
    pub const XBGR32: Self = native_endian(Self::XBGR, Self::RGB0);
    /// [`GRAY9BE`](Self::GRAY9BE) on big-endian targets and [`GRAY9LE`](Self::GRAY9LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY9`, in native byte order.
    pub const GRAY9: Self = native_endian(Self::GRAY9BE, Self::GRAY9LE);
    /// [`GRAY10BE`](Self::GRAY10BE) on big-endian targets and [`GRAY10LE`](Self::GRAY10LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY10`, in native byte order.
    pub const GRAY10: Self = native_endian(Self::GRAY10BE, Self::GRAY10LE);
    /// [`GRAY12BE`](Self::GRAY12BE) on big-endian targets and [`GRAY12LE`](Self::GRAY12LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY12`, in native byte order.
    pub const GRAY12: Self = native_endian(Self::GRAY12BE, Self::GRAY12LE);
    /// [`GRAY14BE`](Self::GRAY14BE) on big-endian targets and [`GRAY14LE`](Self::GRAY14LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY14`, in native byte order.
    pub const GRAY14: Self = native_endian(Self::GRAY14BE, Self::GRAY14LE);
    /// [`GRAY16BE`](Self::GRAY16BE) on big-endian targets and [`GRAY16LE`](Self::GRAY16LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY16`, in native byte order.
    pub const GRAY16: Self = native_endian(Self::GRAY16BE, Self::GRAY16LE);
    /// [`GRAY32BE`](Self::GRAY32BE) on big-endian targets and [`GRAY32LE`](Self::GRAY32LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY32`, in native byte order.
    pub const GRAY32: Self = native_endian(Self::GRAY32BE, Self::GRAY32LE);
    /// [`YA16BE`](Self::YA16BE) on big-endian targets and [`YA16LE`](Self::YA16LE) on
    /// little-endian ones: `AV_PIX_FMT_YA16`, in native byte order.
    pub const YA16: Self = native_endian(Self::YA16BE, Self::YA16LE);
    /// [`RGB48BE`](Self::RGB48BE) on big-endian targets and [`RGB48LE`](Self::RGB48LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB48`, in native byte order.
    pub const RGB48: Self = native_endian(Self::RGB48BE, Self::RGB48LE);
    /// [`RGB565BE`](Self::RGB565BE) on big-endian targets and [`RGB565LE`](Self::RGB565LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB565`, in native byte order.
    pub const RGB565: Self = native_endian(Self::RGB565BE, Self::RGB565LE);
    /// [`RGB555BE`](Self::RGB555BE) on big-endian targets and [`RGB555LE`](Self::RGB555LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB555`, in native byte order.
    pub const RGB555: Self = native_endian(Self::RGB555BE, Self::RGB555LE);
    /// [`RGB444BE`](Self::RGB444BE) on big-endian targets and [`RGB444LE`](Self::RGB444LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB444`, in native byte order.
    pub const RGB444: Self = native_endian(Self::RGB444BE, Self::RGB444LE);
    /// [`RGBA64BE`](Self::RGBA64BE) on big-endian targets and [`RGBA64LE`](Self::RGBA64LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBA64`, in native byte order.
    pub const RGBA64: Self = native_endian(Self::RGBA64BE, Self::RGBA64LE);
    /// [`BGR48BE`](Self::BGR48BE) on big-endian targets and [`BGR48LE`](Self::BGR48LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR48`, in native byte order.
    pub const BGR48: Self = native_endian(Self::BGR48BE, Self::BGR48LE);
    /// [`BGR565BE`](Self::BGR565BE) on big-endian targets and [`BGR565LE`](Self::BGR565LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR565`, in native byte order.
    pub const BGR565: Self = native_endian(Self::BGR565BE, Self::BGR565LE);
    /// [`BGR555BE`](Self::BGR555BE) on big-endian targets and [`BGR555LE`](Self::BGR555LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR555`, in native byte order.
    pub const BGR555: Self = native_endian(Self::BGR555BE, Self::BGR555LE);
    /// [`BGR444BE`](Self::BGR444BE) on big-endian targets and [`BGR444LE`](Self::BGR444LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR444`, in native byte order.
    pub const BGR444: Self = native_endian(Self::BGR444BE, Self::BGR444LE);
    /// [`BGRA64BE`](Self::BGRA64BE) on big-endian targets and [`BGRA64LE`](Self::BGRA64LE) on
    /// little-endian ones: `AV_PIX_FMT_BGRA64`, in native byte order.
    pub const BGRA64: Self = native_endian(Self::BGRA64BE, Self::BGRA64LE);
    /// [`YUV420P9BE`](Self::YUV420P9BE) on big-endian targets and
    /// [`YUV420P9LE`](Self::YUV420P9LE) on little-endian ones: `AV_PIX_FMT_YUV420P9`, in native
    /// byte order.
    pub const YUV420P9: Self = native_endian(Self::YUV420P9BE, Self::YUV420P9LE);
    /// [`YUV422P9BE`](Self::YUV422P9BE) on big-endian targets and
    /// [`YUV422P9LE`](Self::YUV422P9LE) on little-endian ones: `AV_PIX_FMT_YUV422P9`, in native
    /// byte order.
    pub const YUV422P9: Self = native_endian(Self::YUV422P9BE, Self::YUV422P9LE);
    /// [`YUV444P9BE`](Self::YUV444P9BE) on big-endian targets and
    /// [`YUV444P9LE`](Self::YUV444P9LE) on little-endian ones: `AV_PIX_FMT_YUV444P9`, in native
    /// byte order.
    pub const YUV444P9: Self = native_endian(Self::YUV444P9BE, Self::YUV444P9LE);
    /// [`YUV420P10BE`](Self::YUV420P10BE) on big-endian targets and
    /// [`YUV420P10LE`](Self::YUV420P10LE) on little-endian ones: `AV_PIX_FMT_YUV420P10`, in
    /// native byte order.
    pub const YUV420P10: Self = native_endian(Self::YUV420P10BE, Self::YUV420P10LE);
    /// [`YUV422P10BE`](Self::YUV422P10BE) on big-endian targets and
    /// [`YUV422P10LE`](Self::YUV422P10LE) on little-endian ones: `AV_PIX_FMT_YUV422P10`, in
    /// native byte order.
    pub const YUV422P10: Self = native_endian(Self::YUV422P10BE, Self::YUV422P10LE);
    /// [`YUV440P10BE`](Self::YUV440P10BE) on big-endian targets and
    /// [`YUV440P10LE`](Self::YUV440P10LE) on little-endian ones: `AV_PIX_FMT_YUV440P10`, in
    /// native byte order.
    pub const YUV440P10: Self = native_endian(Self::YUV440P10BE, Self::YUV440P10LE);
    /// [`YUV444P10BE`](Self::YUV444P10BE) on big-endian targets and
    /// [`YUV444P10LE`](Self::YUV444P10LE) on little-endian ones: `AV_PIX_FMT_YUV444P10`, in
    /// native byte order.
    pub const YUV444P10: Self = native_endian(Self::YUV444P10BE, Self::YUV444P10LE);
    /// [`YUV420P12BE`](Self::YUV420P12BE) on big-endian targets and
    /// [`YUV420P12LE`](Self::YUV420P12LE) on little-endian ones: `AV_PIX_FMT_YUV420P12`, in
    /// native byte order.
    pub const YUV420P12: Self = native_endian(Self::YUV420P12BE, Self::YUV420P12LE);
    /// [`YUV422P12BE`](Self::YUV422P12BE) on big-endian targets and
    /// [`YUV422P12LE`](Self::YUV422P12LE) on little-endian ones: `AV_PIX_FMT_YUV422P12`, in
    /// native byte order.
    pub const YUV422P12: Self = native_endian(Self::YUV422P12BE, Self::YUV422P12LE);
    /// [`YUV440P12BE`](Self::YUV440P12BE) on big-endian targets and
    /// [`YUV440P12LE`](Self::YUV440P12LE) on little-endian ones: `AV_PIX_FMT_YUV440P12`, in
    /// native byte order.
    pub const YUV440P12: Self = native_endian(Self::YUV440P12BE, Self::YUV440P12LE);
    /// [`YUV444P12BE`](Self::YUV444P12BE) on big-endian targets and
    /// [`YUV444P12LE`](Self::YUV444P12LE) on little-endian ones: `AV_PIX_FMT_YUV444P12`, in
    /// native byte order.
    pub const YUV444P12: Self = native_endian(Self::YUV444P12BE, Self::YUV444P12LE);
    /// [`YUV420P14BE`](Self::YUV420P14BE) on big-endian targets and
    /// [`YUV420P14LE`](Self::YUV420P14LE) on little-endian ones: `AV_PIX_FMT_YUV420P14`, in
    /// native byte order.
    pub const YUV420P14: Self = native_endian(Self::YUV420P14BE, Self::YUV420P14LE);
    /// [`YUV422P14BE`](Self::YUV422P14BE) on big-endian targets and
    /// [`YUV422P14LE`](Self::YUV422P14LE) on little-endian ones: `AV_PIX_FMT_YUV422P14`, in
    /// native byte order.
    pub const YUV422P14: Self = native_endian(Self::YUV422P14BE, Self::YUV422P14LE);
    /// [`YUV444P14BE`](Self::YUV444P14BE) on big-endian targets and
    /// [`YUV444P14LE`](Self::YUV444P14LE) on little-endian ones: `AV_PIX_FMT_YUV444P14`, in
    /// native byte order.
    pub const YUV444P14: Self = native_endian(Self::YUV444P14BE, Self::YUV444P14LE);
    /// [`YUV420P16BE`](Self::YUV420P16BE) on big-endian targets and
    /// [`YUV420P16LE`](Self::YUV420P16LE) on little-endian ones: `AV_PIX_FMT_YUV420P16`, in
    /// native byte order.
    pub const YUV420P16: Self = native_endian(Self::YUV420P16BE, Self::YUV420P16LE);
    /// [`YUV422P16BE`](Self::YUV422P16BE) on big-endian targets and
    /// [`YUV422P16LE`](Self::YUV422P16LE) on little-endian ones: `AV_PIX_FMT_YUV422P16`, in
    /// native byte order.
    pub const YUV422P16: Self = native_endian(Self::YUV422P16BE, Self::YUV422P16LE);
    /// [`YUV444P16BE`](Self::YUV444P16BE) on big-endian targets and
    /// [`YUV444P16LE`](Self::YUV444P16LE) on little-endian ones: `AV_PIX_FMT_YUV444P16`, in
    /// native byte order.
    pub const YUV444P16: Self = native_endian(Self::YUV444P16BE, Self::YUV444P16LE);
    /// [`YUV444P10MSBBE`](Self::YUV444P10MSBBE) on big-endian targets and
    /// [`YUV444P10MSBLE`](Self::YUV444P10MSBLE) on little-endian ones:
    /// `AV_PIX_FMT_YUV444P10MSB`, in native byte order.
    pub const YUV444P10MSB: Self = native_endian(Self::YUV444P10MSBBE, Self::YUV444P10MSBLE);
    /// [`YUV444P12MSBBE`](Self::YUV444P12MSBBE) on big-endian targets and
    /// [`YUV444P12MSBLE`](Self::YUV444P12MSBLE) on little-endian ones:
    /// `AV_PIX_FMT_YUV444P12MSB`, in native byte order.
    pub const YUV444P12MSB: Self = native_endian(Self::YUV444P12MSBBE, Self::YUV444P12MSBLE);
    /// [`GBRP9BE`](Self::GBRP9BE) on big-endian targets and [`GBRP9LE`](Self::GBRP9LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP9`, in native byte order.
    pub const GBRP9: Self = native_endian(Self::GBRP9BE, Self::GBRP9LE);
    /// [`GBRP10BE`](Self::GBRP10BE) on big-endian targets and [`GBRP10LE`](Self::GBRP10LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP10`, in native byte order.
    pub const GBRP10: Self = native_endian(Self::GBRP10BE, Self::GBRP10LE);
    /// [`GBRP12BE`](Self::GBRP12BE) on big-endian targets and [`GBRP12LE`](Self::GBRP12LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP12`, in native byte order.
    pub const GBRP12: Self = native_endian(Self::GBRP12BE, Self::GBRP12LE);
    /// [`GBRP14BE`](Self::GBRP14BE) on big-endian targets and [`GBRP14LE`](Self::GBRP14LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP14`, in native byte order.
    pub const GBRP14: Self = native_endian(Self::GBRP14BE, Self::GBRP14LE);
    /// [`GBRP16BE`](Self::GBRP16BE) on big-endian targets and [`GBRP16LE`](Self::GBRP16LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP16`, in native byte order.
    pub const GBRP16: Self = native_endian(Self::GBRP16BE, Self::GBRP16LE);
    /// [`GBRAP10BE`](Self::GBRAP10BE) on big-endian targets and [`GBRAP10LE`](Self::GBRAP10LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRAP10`, in native byte order.
    pub const GBRAP10: Self = native_endian(Self::GBRAP10BE, Self::GBRAP10LE);
    /// [`GBRAP12BE`](Self::GBRAP12BE) on big-endian targets and [`GBRAP12LE`](Self::GBRAP12LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRAP12`, in native byte order.
    pub const GBRAP12: Self = native_endian(Self::GBRAP12BE, Self::GBRAP12LE);
    /// [`GBRAP14BE`](Self::GBRAP14BE) on big-endian targets and [`GBRAP14LE`](Self::GBRAP14LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRAP14`, in native byte order.
    pub const GBRAP14: Self = native_endian(Self::GBRAP14BE, Self::GBRAP14LE);
    /// [`GBRAP16BE`](Self::GBRAP16BE) on big-endian targets and [`GBRAP16LE`](Self::GBRAP16LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRAP16`, in native byte order.
    pub const GBRAP16: Self = native_endian(Self::GBRAP16BE, Self::GBRAP16LE);
    /// [`GBRAP32BE`](Self::GBRAP32BE) on big-endian targets and [`GBRAP32LE`](Self::GBRAP32LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRAP32`, in native byte order.
    pub const GBRAP32: Self = native_endian(Self::GBRAP32BE, Self::GBRAP32LE);
    /// [`GBRP10MSBBE`](Self::GBRP10MSBBE) on big-endian targets and
    /// [`GBRP10MSBLE`](Self::GBRP10MSBLE) on little-endian ones: `AV_PIX_FMT_GBRP10MSB`, in
    /// native byte order.
    pub const GBRP10MSB: Self = native_endian(Self::GBRP10MSBBE, Self::GBRP10MSBLE);
    /// [`GBRP12MSBBE`](Self::GBRP12MSBBE) on big-endian targets and
    /// [`GBRP12MSBLE`](Self::GBRP12MSBLE) on little-endian ones: `AV_PIX_FMT_GBRP12MSB`, in
    /// native byte order.
    pub const GBRP12MSB: Self = native_endian(Self::GBRP12MSBBE, Self::GBRP12MSBLE);
    /// [`BAYER_BGGR16BE`](Self::BAYER_BGGR16BE) on big-endian targets and
    /// [`BAYER_BGGR16LE`](Self::BAYER_BGGR16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_BGGR16`, in native byte order.
    pub const BAYER_BGGR16: Self = native_endian(Self::BAYER_BGGR16BE, Self::BAYER_BGGR16LE);
    /// [`BAYER_RGGB16BE`](Self::BAYER_RGGB16BE) on big-endian targets and
    /// [`BAYER_RGGB16LE`](Self::BAYER_RGGB16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_RGGB16`, in native byte order.
    pub const BAYER_RGGB16: Self = native_endian(Self::BAYER_RGGB16BE, Self::BAYER_RGGB16LE);
    /// [`BAYER_GBRG16BE`](Self::BAYER_GBRG16BE) on big-endian targets and
    /// [`BAYER_GBRG16LE`](Self::BAYER_GBRG16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_GBRG16`, in native byte order.
    pub const BAYER_GBRG16: Self = native_endian(Self::BAYER_GBRG16BE, Self::BAYER_GBRG16LE);
    /// [`BAYER_GRBG16BE`](Self::BAYER_GRBG16BE) on big-endian targets and
    /// [`BAYER_GRBG16LE`](Self::BAYER_GRBG16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_GRBG16`, in native byte order.
    pub const BAYER_GRBG16: Self = native_endian(Self::BAYER_GRBG16BE, Self::BAYER_GRBG16LE);
    /// [`GBRPF16BE`](Self::GBRPF16BE) on big-endian targets and [`GBRPF16LE`](Self::GBRPF16LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRPF16`, in native byte order.
    pub const GBRPF16: Self = native_endian(Self::GBRPF16BE, Self::GBRPF16LE);
    /// [`GBRAPF16BE`](Self::GBRAPF16BE) on big-endian targets and
    /// [`GBRAPF16LE`](Self::GBRAPF16LE) on little-endian ones: `AV_PIX_FMT_GBRAPF16`, in native
    /// byte order.
    pub const GBRAPF16: Self = native_endian(Self::GBRAPF16BE, Self::GBRAPF16LE);
    /// [`GBRPF32BE`](Self::GBRPF32BE) on big-endian targets and [`GBRPF32LE`](Self::GBRPF32LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRPF32`, in native byte order.
    pub const GBRPF32: Self = native_endian(Self::GBRPF32BE, Self::GBRPF32LE);
    /// [`GBRAPF32BE`](Self::GBRAPF32BE) on big-endian targets and
    /// [`GBRAPF32LE`](Self::GBRAPF32LE) on little-endian ones: `AV_PIX_FMT_GBRAPF32`, in native
    /// byte order.
    pub const GBRAPF32: Self = native_endian(Self::GBRAPF32BE, Self::GBRAPF32LE);
    /// [`GRAYF16BE`](Self::GRAYF16BE) on big-endian targets and [`GRAYF16LE`](Self::GRAYF16LE)
    /// on little-endian ones: `AV_PIX_FMT_GRAYF16`, in native byte order.
    pub const GRAYF16: Self = native_endian(Self::GRAYF16BE, Self::GRAYF16LE);
    /// [`GRAYF32BE`](Self::GRAYF32BE) on big-endian targets and [`GRAYF32LE`](Self::GRAYF32LE)
    /// on little-endian ones: `AV_PIX_FMT_GRAYF32`, in native byte order.
    pub const GRAYF32: Self = native_endian(Self::GRAYF32BE, Self::GRAYF32LE);
    /// [`YAF16BE`](Self::YAF16BE) on big-endian targets and [`YAF16LE`](Self::YAF16LE) on
    /// little-endian ones: `AV_PIX_FMT_YAF16`, in native byte order.
    pub const YAF16: Self = native_endian(Self::YAF16BE, Self::YAF16LE);
    /// [`YAF32BE`](Self::YAF32BE) on big-endian targets and [`YAF32LE`](Self::YAF32LE) on
    /// little-endian ones: `AV_PIX_FMT_YAF32`, in native byte order.
    pub const YAF32: Self = native_endian(Self::YAF32BE, Self::YAF32LE);
    /// [`YUVA420P9BE`](Self::YUVA420P9BE) on big-endian targets and
    /// [`YUVA420P9LE`](Self::YUVA420P9LE) on little-endian ones: `AV_PIX_FMT_YUVA420P9`, in
    /// native byte order.
    pub const YUVA420P9: Self = native_endian(Self::YUVA420P9BE, Self::YUVA420P9LE);
    /// [`YUVA422P9BE`](Self::YUVA422P9BE) on big-endian targets and
    /// [`YUVA422P9LE`](Self::YUVA422P9LE) on little-endian ones: `AV_PIX_FMT_YUVA422P9`, in
    /// native byte order.
    pub const YUVA422P9: Self = native_endian(Self::YUVA422P9BE, Self::YUVA422P9LE);
    /// [`YUVA444P9BE`](Self::YUVA444P9BE) on big-endian targets and
    /// [`YUVA444P9LE`](Self::YUVA444P9LE) on little-endian ones: `AV_PIX_FMT_YUVA444P9`, in
    /// native byte order.
    pub const YUVA444P9: Self = native_endian(Self::YUVA444P9BE, Self::YUVA444P9LE);
    /// [`YUVA420P10BE`](Self::YUVA420P10BE) on big-endian targets and
    /// [`YUVA420P10LE`](Self::YUVA420P10LE) on little-endian ones: `AV_PIX_FMT_YUVA420P10`, in
    /// native byte order.
    pub const YUVA420P10: Self = native_endian(Self::YUVA420P10BE, Self::YUVA420P10LE);
    /// [`YUVA422P10BE`](Self::YUVA422P10BE) on big-endian targets and
    /// [`YUVA422P10LE`](Self::YUVA422P10LE) on little-endian ones: `AV_PIX_FMT_YUVA422P10`, in
    /// native byte order.
    pub const YUVA422P10: Self = native_endian(Self::YUVA422P10BE, Self::YUVA422P10LE);
    /// [`YUVA444P10BE`](Self::YUVA444P10BE) on big-endian targets and
    /// [`YUVA444P10LE`](Self::YUVA444P10LE) on little-endian ones: `AV_PIX_FMT_YUVA444P10`, in
    /// native byte order.
    pub const YUVA444P10: Self = native_endian(Self::YUVA444P10BE, Self::YUVA444P10LE);
    /// [`YUVA422P12BE`](Self::YUVA422P12BE) on big-endian targets and
    /// [`YUVA422P12LE`](Self::YUVA422P12LE) on little-endian ones: `AV_PIX_FMT_YUVA422P12`, in
    /// native byte order.
    pub const YUVA422P12: Self = native_endian(Self::YUVA422P12BE, Self::YUVA422P12LE);
    /// [`YUVA444P12BE`](Self::YUVA444P12BE) on big-endian targets and
    /// [`YUVA444P12LE`](Self::YUVA444P12LE) on little-endian ones: `AV_PIX_FMT_YUVA444P12`, in
    /// native byte order.
    pub const YUVA444P12: Self = native_endian(Self::YUVA444P12BE, Self::YUVA444P12LE);
    /// [`YUVA420P16BE`](Self::YUVA420P16BE) on big-endian targets and
    /// [`YUVA420P16LE`](Self::YUVA420P16LE) on little-endian ones: `AV_PIX_FMT_YUVA420P16`, in
    /// native byte order.
    pub const YUVA420P16: Self = native_endian(Self::YUVA420P16BE, Self::YUVA420P16LE);
    /// [`YUVA422P16BE`](Self::YUVA422P16BE) on big-endian targets and
    /// [`YUVA422P16LE`](Self::YUVA422P16LE) on little-endian ones: `AV_PIX_FMT_YUVA422P16`, in
    /// native byte order.
    pub const YUVA422P16: Self = native_endian(Self::YUVA422P16BE, Self::YUVA422P16LE);
    /// [`YUVA444P16BE`](Self::YUVA444P16BE) on big-endian targets and
    /// [`YUVA444P16LE`](Self::YUVA444P16LE) on little-endian ones: `AV_PIX_FMT_YUVA444P16`, in
    /// native byte order.
    pub const YUVA444P16: Self = native_endian(Self::YUVA444P16BE, Self::YUVA444P16LE);
    /// [`XYZ12BE`](Self::XYZ12BE) on big-endian targets and [`XYZ12LE`](Self::XYZ12LE) on
    /// little-endian ones: `AV_PIX_FMT_XYZ12`, in native byte order.
    pub const XYZ12: Self = native_endian(Self::XYZ12BE, Self::XYZ12LE);
    /// [`NV20BE`](Self::NV20BE) on big-endian targets and [`NV20LE`](Self::NV20LE) on
    /// little-endian ones: `AV_PIX_FMT_NV20`, in native byte order.
    pub const NV20: Self = native_endian(Self::NV20BE, Self::NV20LE);
    /// [`AYUV64BE`](Self::AYUV64BE) on big-endian targets and [`AYUV64LE`](Self::AYUV64LE) on
    /// little-endian ones: `AV_PIX_FMT_AYUV64`, in native byte order.
    pub const AYUV64: Self = native_endian(Self::AYUV64BE, Self::AYUV64LE);
    /// [`P010BE`](Self::P010BE) on big-endian targets and [`P010LE`](Self::P010LE) on
    /// little-endian ones: `AV_PIX_FMT_P010`, in native byte order.
    pub const P010: Self = native_endian(Self::P010BE, Self::P010LE);
    /// [`P012BE`](Self::P012BE) on big-endian targets and [`P012LE`](Self::P012LE) on
    /// little-endian ones: `AV_PIX_FMT_P012`, in native byte order.
    pub const P012: Self = native_endian(Self::P012BE, Self::P012LE);
    /// [`P016BE`](Self::P016BE) on big-endian targets and [`P016LE`](Self::P016LE) on
    /// little-endian ones: `AV_PIX_FMT_P016`, in native byte order.
    pub const P016: Self = native_endian(Self::P016BE, Self::P016LE);
    /// [`Y210BE`](Self::Y210BE) on big-endian targets and [`Y210LE`](Self::Y210LE) on
    /// little-endian ones: `AV_PIX_FMT_Y210`, in native byte order.
    pub const Y210: Self = native_endian(Self::Y210BE, Self::Y210LE);
    /// [`Y212BE`](Self::Y212BE) on big-endian targets and [`Y212LE`](Self::Y212LE) on
    /// little-endian ones: `AV_PIX_FMT_Y212`, in native byte order.
    pub const Y212: Self = native_endian(Self::Y212BE, Self::Y212LE);
    /// [`Y216BE`](Self::Y216BE) on big-endian targets and [`Y216LE`](Self::Y216LE) on
    /// little-endian ones: `AV_PIX_FMT_Y216`, in native byte order.
    pub const Y216: Self = native_endian(Self::Y216BE, Self::Y216LE);
    /// [`XV30BE`](Self::XV30BE) on big-endian targets and [`XV30LE`](Self::XV30LE) on
    /// little-endian ones: `AV_PIX_FMT_XV30`, in native byte order.
    pub const XV30: Self = native_endian(Self::XV30BE, Self::XV30LE);
    /// [`XV36BE`](Self::XV36BE) on big-endian targets and [`XV36LE`](Self::XV36LE) on
    /// little-endian ones: `AV_PIX_FMT_XV36`, in native byte order.
    pub const XV36: Self = native_endian(Self::XV36BE, Self::XV36LE);
    /// [`XV48BE`](Self::XV48BE) on big-endian targets and [`XV48LE`](Self::XV48LE) on
    /// little-endian ones: `AV_PIX_FMT_XV48`, in native byte order.
    pub const XV48: Self = native_endian(Self::XV48BE, Self::XV48LE);
    /// [`V30XBE`](Self::V30XBE) on big-endian targets and [`V30XLE`](Self::V30XLE) on
    /// little-endian ones: `AV_PIX_FMT_V30X`, in native byte order.
    pub const V30X: Self = native_endian(Self::V30XBE, Self::V30XLE);
    /// [`X2RGB10BE`](Self::X2RGB10BE) on big-endian targets and [`X2RGB10LE`](Self::X2RGB10LE)
    /// on little-endian ones: `AV_PIX_FMT_X2RGB10`, in native byte order.
    pub const X2RGB10: Self = native_endian(Self::X2RGB10BE, Self::X2RGB10LE);
    /// [`X2BGR10BE`](Self::X2BGR10BE) on big-endian targets and [`X2BGR10LE`](Self::X2BGR10LE)
    /// on little-endian ones: `AV_PIX_FMT_X2BGR10`, in native byte order.
    pub const X2BGR10: Self = native_endian(Self::X2BGR10BE, Self::X2BGR10LE);
    /// [`P210BE`](Self::P210BE) on big-endian targets and [`P210LE`](Self::P210LE) on
    /// little-endian ones: `AV_PIX_FMT_P210`, in native byte order.
    pub const P210: Self = native_endian(Self::P210BE, Self::P210LE);
    /// [`P410BE`](Self::P410BE) on big-endian targets and [`P410LE`](Self::P410LE) on
    /// little-endian ones: `AV_PIX_FMT_P410`, in native byte order.
    pub const P410: Self = native_endian(Self::P410BE, Self::P410LE);
    /// [`P212BE`](Self::P212BE) on big-endian targets and [`P212LE`](Self::P212LE) on
    /// little-endian ones: `AV_PIX_FMT_P212`, in native byte order.
    pub const P212: Self = native_endian(Self::P212BE, Self::P212LE);
    /// [`P412BE`](Self::P412BE) on big-endian targets and [`P412LE`](Self::P412LE) on
    /// little-endian ones: `AV_PIX_FMT_P412`, in native byte order.
    pub const P412: Self = native_endian(Self::P412BE, Self::P412LE);
    /// [`P216BE`](Self::P216BE) on big-endian targets and [`P216LE`](Self::P216LE) on
    /// little-endian ones: `AV_PIX_FMT_P216`, in native byte order.
    pub const P216: Self = native_endian(Self::P216BE, Self::P216LE);
    /// [`P416BE`](Self::P416BE) on big-endian targets and [`P416LE`](Self::P416LE) on
    /// little-endian ones: `AV_PIX_FMT_P416`, in native byte order.
    pub const P416: Self = native_endian(Self::P416BE, Self::P416LE);
    /// [`RGBF16BE`](Self::RGBF16BE) on big-endian targets and [`RGBF16LE`](Self::RGBF16LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBF16`, in native byte order.
    pub const RGBF16: Self = native_endian(Self::RGBF16BE, Self::RGBF16LE);
    /// [`RGBAF16BE`](Self::RGBAF16BE) on big-endian targets and [`RGBAF16LE`](Self::RGBAF16LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBAF16`, in native byte order.
    pub const RGBAF16: Self = native_endian(Self::RGBAF16BE, Self::RGBAF16LE);
    /// [`RGBF32BE`](Self::RGBF32BE) on big-endian targets and [`RGBF32LE`](Self::RGBF32LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBF32`, in native byte order.
    pub const RGBF32: Self = native_endian(Self::RGBF32BE, Self::RGBF32LE);
    /// [`RGBAF32BE`](Self::RGBAF32BE) on big-endian targets and [`RGBAF32LE`](Self::RGBAF32LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBAF32`, in native byte order.
    pub const RGBAF32: Self = native_endian(Self::RGBAF32BE, Self::RGBAF32LE);
    /// [`RGB96BE`](Self::RGB96BE) on big-endian targets and [`RGB96LE`](Self::RGB96LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB96`, in native byte order.
    pub const RGB96: Self = native_endian(Self::RGB96BE, Self::RGB96LE);
    /// [`RGBA128BE`](Self::RGBA128BE) on big-endian targets and [`RGBA128LE`](Self::RGBA128LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBA128`, in native byte order.
    pub const RGBA128: Self = native_endian(Self::RGBA128BE, Self::RGBA128LE);
}

impl Default for PixelFormat {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for PixelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("PixelFormat").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVPixelFormat> for PixelFormat {
    #[inline]
    fn from(raw: ffi::AVPixelFormat) -> Self {
        Self::from_raw(raw)
    }
}

impl From<PixelFormat> for ffi::AVPixelFormat {
    #[inline]
    fn from(f: PixelFormat) -> Self {
        f.into_raw()
    }
}

// SAFETY: `PixelFormat` is `#[repr(transparent)]` over `ffi::AVPixelFormat`,
// itself `#[repr(transparent)]` over a `c_int`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for PixelFormat {}

/// Wraps: AVColorPrimaries
/// Handwritten
///
/// The chromaticity coordinates of a picture's source primaries, as
/// ISO/IEC 23091-2 subclause 8.1 and ITU-T H.273 number them, plus FFmpeg's
/// own extensions from [`EXT_BASE`](Self::EXT_BASE) up.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVFrame.color_primaries`,
/// `AVCodecParameters.color_primaries`), so it may hold any value, and a Rust
/// view of that memory must accept every one. Match on the associated
/// constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::ColorPrimaries;
/// fn is_wide_gamut(p: ColorPrimaries) -> bool {
///     matches!(p, ColorPrimaries::BT2020 | ColorPrimaries::SMPTE432)
/// }
/// assert!(is_wide_gamut(ColorPrimaries::BT2020));
/// assert!(!is_wide_gamut(ColorPrimaries::UNSPECIFIED));
/// ```
///
/// `AVCOL_PRI_NB` and `AVCOL_PRI_EXT_NB` are not part of FFmpeg's ABI and
/// have no constant. The default is [`UNSPECIFIED`](Self::UNSPECIFIED), the
/// value C's own initialisers (`av_frame_alloc`, `avcodec_parameters_alloc`)
/// write; zero-filled storage reads [`RESERVED0`](Self::RESERVED0) instead.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorPrimaries(ffi::AVColorPrimaries);

impl ColorPrimaries {
    /// Reserved code point 0; the value of a zero-filled field.
    pub const RESERVED0: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_RESERVED0);
    /// ITU-R BT.709, also ITU-R BT.1361 / IEC 61966-2-4 / SMPTE RP 177 Annex B.
    pub const BT709: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT709);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED);
    /// Reserved code point 3.
    pub const RESERVED: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_RESERVED);
    /// ITU-R BT.470 System M, also FCC Title 47 CFR 73.682 (a)(20).
    pub const BT470M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT470M);
    /// ITU-R BT.470 System B, G, also ITU-R BT.601-6 625 / BT.1358 625 /
    /// BT.1700 625 PAL & SECAM.
    pub const BT470BG: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT470BG);
    /// SMPTE 170M, also ITU-R BT.601-6 525 / BT.1358 525 / BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE170M);
    /// SMPTE 240M, identical to [`SMPTE170M`](Self::SMPTE170M); also called
    /// "SMPTE C" even though it uses D65.
    pub const SMPTE240M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE240M);
    /// Generic film: colour filters using Illuminant C.
    pub const FILM: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_FILM);
    /// ITU-R BT.2020.
    pub const BT2020: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT2020);
    /// SMPTE ST 428-1 (CIE 1931 XYZ).
    pub const SMPTE428: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE428);
    /// The same value as [`SMPTE428`](Self::SMPTE428).
    pub const SMPTEST428_1: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTEST428_1);
    /// SMPTE ST 431-2 (2011) / DCI P3.
    pub const SMPTE431: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE431);
    /// SMPTE ST 432-1 (2010) / P3 D65 / Display P3.
    pub const SMPTE432: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE432);
    /// EBU Tech. 3213-E, one of the JEDEC P22 group phosphors.
    pub const EBU3213: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_EBU3213);
    /// The same value as [`EBU3213`](Self::EBU3213).
    pub const JEDEC_P22: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_JEDEC_P22);
    /// The first of FFmpeg's extensions, which are not part of H.273; the
    /// same value as [`V_GAMUT`](Self::V_GAMUT).
    pub const EXT_BASE: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE);
    /// Panasonic V-Gamut.
    pub const V_GAMUT: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_V_GAMUT);

    /// The primaries C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVColorPrimaries) -> Self {
        Self(raw)
    }

    /// The C encoding of these primaries.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVColorPrimaries {
        self.0
    }

    /// The name of the constant this value equals, if any; of two constants
    /// with one value, the first C declares.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::RESERVED0 => "RESERVED0",
            Self::BT709 => "BT709",
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::RESERVED => "RESERVED",
            Self::BT470M => "BT470M",
            Self::BT470BG => "BT470BG",
            Self::SMPTE170M => "SMPTE170M",
            Self::SMPTE240M => "SMPTE240M",
            Self::FILM => "FILM",
            Self::BT2020 => "BT2020",
            Self::SMPTE428 => "SMPTE428",
            Self::SMPTE431 => "SMPTE431",
            Self::SMPTE432 => "SMPTE432",
            Self::EBU3213 => "EBU3213",
            Self::V_GAMUT => "V_GAMUT",
            _ => return None,
        })
    }
}

impl Default for ColorPrimaries {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for ColorPrimaries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ColorPrimaries").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVColorPrimaries> for ColorPrimaries {
    #[inline]
    fn from(raw: ffi::AVColorPrimaries) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ColorPrimaries> for ffi::AVColorPrimaries {
    #[inline]
    fn from(p: ColorPrimaries) -> Self {
        p.into_raw()
    }
}

// SAFETY: `ColorPrimaries` is `#[repr(transparent)]` over
// `ffi::AVColorPrimaries`, itself `#[repr(transparent)]` over a `c_uint`, so
// every bit pattern is a valid value, and it is a value with no C-side
// identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for ColorPrimaries {}

/// Wraps: AVColorRange
/// Handwritten
///
/// The range of values a picture's samples take: limited ("MPEG", "TV")
/// with head- and footroom, or full ("JPEG", "PC"), as ITU-R BT.709,
/// BT.2020 and BT.2100 define them.
///
/// A newtype over the C enum rather than a Rust `enum`, as
/// [`ColorPrimaries`] is: C stores the value in public struct fields its users
/// write directly (`AVFrame.color_range`, `AVCodecContext.color_range`), so it
/// may hold any value, and a Rust view of that memory must accept every one.
/// Match on the associated constants, with a `_` arm for the values C does
/// not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::ColorRange;
/// fn luma_black_8bit(r: ColorRange) -> u8 {
///     match r {
///         ColorRange::JPEG => 0,
///         _ => 16,
///     }
/// }
/// assert_eq!(luma_black_8bit(ColorRange::JPEG), 0);
/// assert_eq!(luma_black_8bit(ColorRange::default()), 16);
/// ```
///
/// `AVCOL_RANGE_NB` is not part of FFmpeg's ABI and has no constant. The
/// default is [`UNSPECIFIED`](Self::UNSPECIFIED), both the zero-filled value
/// and the one C's own initialisers write.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColorRange(ffi::AVColorRange);

impl ColorRange {
    /// Unspecified; the value of a zero-filled field.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorRange::AVCOL_RANGE_UNSPECIFIED);
    /// Narrow or limited range: `(219 * E + 16) * 2^(n-8)` for luma, e.g.
    /// 16-235 at 8 bits, and `(224 * E + 128) * 2^(n-8)` for chroma, e.g.
    /// 16-240 at 8 bits.
    pub const MPEG: Self = Self(ffi::AVColorRange::AVCOL_RANGE_MPEG);
    /// Full range: `(2^n - 1) * E` for RGB and luma, e.g. 0-255 at 8 bits,
    /// and `(2^n - 1) * E + 2^(n - 1)` for chroma, e.g. 1-255 at 8 bits.
    pub const JPEG: Self = Self(ffi::AVColorRange::AVCOL_RANGE_JPEG);

    /// The range C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVColorRange) -> Self {
        Self(raw)
    }

    /// The C encoding of this range.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVColorRange {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::MPEG => "MPEG",
            Self::JPEG => "JPEG",
            _ => return None,
        })
    }
}

impl Default for ColorRange {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for ColorRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ColorRange").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVColorRange> for ColorRange {
    #[inline]
    fn from(raw: ffi::AVColorRange) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ColorRange> for ffi::AVColorRange {
    #[inline]
    fn from(r: ColorRange) -> Self {
        r.into_raw()
    }
}

// SAFETY: `ColorRange` is `#[repr(transparent)]` over `ffi::AVColorRange`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of
// C memory is the whole of it.
unsafe impl CPlainElem for ColorRange {}

/// Wraps: AVChromaLocation
/// Handwritten
///
/// Where the first (top left) chroma sample of a subsampled picture sits
/// relative to the luma samples:
///
/// ```text
///                1st 2nd       1st 2nd horizontal luma sample positions
///                 v   v         v   v
///                 ______        ______
/// 1st luma line > |X   X ...    |3 4 X ...     X are luma samples,
///                 |             |1 2           1-6 are possible chroma positions
/// 2nd luma line > |X   X ...    |5 6 X ...     0 is undefined/unknown position
/// ```
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVFrame.chroma_location`,
/// `AVCodecParameters.chroma_location`), so it may hold any `unsigned`, and a
/// Rust view of that memory must accept every one. Match on the associated
/// constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::pixfmt::ChromaLocation;
/// fn is_cosited_left(loc: ChromaLocation) -> bool {
///     matches!(
///         loc,
///         ChromaLocation::LEFT | ChromaLocation::TOPLEFT | ChromaLocation::BOTTOMLEFT
///     )
/// }
/// assert!(is_cosited_left(ChromaLocation::TOPLEFT));
/// assert!(!is_cosited_left(ChromaLocation::UNSPECIFIED));
/// ```
///
/// `AVCHROMA_LOC_NB` is not part of FFmpeg's ABI and has no constant. The
/// default is [`UNSPECIFIED`](Self::UNSPECIFIED), the zeroed C value.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChromaLocation(ffi::AVChromaLocation);

impl ChromaLocation {
    /// Unknown or undefined (position 0).
    pub const UNSPECIFIED: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_UNSPECIFIED);
    /// Position 1: MPEG-2/4 4:2:0, the H.264 default for 4:2:0.
    pub const LEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_LEFT);
    /// Position 2: MPEG-1 4:2:0, JPEG 4:2:0, H.263 4:2:0.
    pub const CENTER: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_CENTER);
    /// Position 3: ITU-R 601, SMPTE 274M 296M S314M (DV 4:1:1), MPEG-2 4:2:2.
    pub const TOPLEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_TOPLEFT);
    /// Position 4.
    pub const TOP: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_TOP);
    /// Position 5.
    pub const BOTTOMLEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_BOTTOMLEFT);
    /// Position 6.
    pub const BOTTOM: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_BOTTOM);

    /// The chroma location C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVChromaLocation) -> Self {
        Self(raw)
    }

    /// The C encoding of this chroma location.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVChromaLocation {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::UNSPECIFIED => "UNSPECIFIED",
            Self::LEFT => "LEFT",
            Self::CENTER => "CENTER",
            Self::TOPLEFT => "TOPLEFT",
            Self::TOP => "TOP",
            Self::BOTTOMLEFT => "BOTTOMLEFT",
            Self::BOTTOM => "BOTTOM",
            _ => return None,
        })
    }
}

impl Default for ChromaLocation {
    #[inline]
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl fmt::Debug for ChromaLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("ChromaLocation").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVChromaLocation> for ChromaLocation {
    #[inline]
    fn from(raw: ffi::AVChromaLocation) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ChromaLocation> for ffi::AVChromaLocation {
    #[inline]
    fn from(loc: ChromaLocation) -> Self {
        loc.into_raw()
    }
}

// SAFETY: `ChromaLocation` is `#[repr(transparent)]` over
// `ffi::AVChromaLocation`, itself `#[repr(transparent)]` over a `c_uint`, so
// every bit pattern is a valid value, and it is a value with no C-side
// identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for ChromaLocation {}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::{c_int, c_uint};
    use core::mem::{align_of, size_of};
    use std::collections::HashSet;

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

    const ALPHA_MODE_NAMED: [(AlphaMode, &str); 3] = [
        (AlphaMode::UNSPECIFIED, "UNSPECIFIED"),
        (AlphaMode::PREMULTIPLIED, "PREMULTIPLIED"),
        (AlphaMode::STRAIGHT, "STRAIGHT"),
    ];

    /// `AVALPHA_MODE_NB`, `AVERROR(EINVAL)` as C's `c_uint` and the extremes.
    const ALPHA_MODE_UNNAMED: [c_uint; 5] = [3, 4, (-22i32) as c_uint, 1 << 31, c_uint::MAX];

    #[test]
    fn alpha_mode_layout_matches_the_c_enum() {
        assert_eq!(size_of::<AlphaMode>(), size_of::<ffi::AVAlphaMode>());
        assert_eq!(align_of::<AlphaMode>(), align_of::<ffi::AVAlphaMode>());
        assert_eq!(size_of::<AlphaMode>(), size_of::<c_uint>());
    }

    #[test]
    fn alpha_mode_constants_are_the_c_enumerators() {
        let raw = ALPHA_MODE_NAMED.map(|(m, _)| m.into_raw().0);
        assert_eq!(raw, [0, 1, 2]);
        assert_eq!(ffi::AVAlphaMode::AVALPHA_MODE_NB.0, 3);
    }

    #[test]
    fn alpha_mode_debug_names_the_constants() {
        for (m, name) in ALPHA_MODE_NAMED {
            assert_eq!(format!("{m:?}"), name);
        }
    }

    #[test]
    fn alpha_mode_debug_shows_unnamed_values_raw() {
        for raw in ALPHA_MODE_UNNAMED {
            let m = AlphaMode::from_raw(ffi::AVAlphaMode(raw));
            assert_eq!(format!("{m:?}"), format!("AlphaMode({raw})"));
        }
    }

    #[test]
    fn alpha_mode_default_is_the_zeroed_value() {
        assert_eq!(AlphaMode::default(), AlphaMode::UNSPECIFIED);
        assert_eq!(AlphaMode::default().into_raw().0, 0);
    }

    #[test]
    fn alpha_mode_raw_conversions_round_trip_every_value() {
        let named = ALPHA_MODE_NAMED.map(|(m, _)| m.into_raw().0);
        for raw in named.into_iter().chain(ALPHA_MODE_UNNAMED) {
            let c = ffi::AVAlphaMode(raw);
            assert_eq!(AlphaMode::from(c).into_raw(), c);
            assert_eq!(ffi::AVAlphaMode::from(AlphaMode::from_raw(c)), c);
        }
    }

    #[test]
    fn alpha_mode_constants_are_distinct() {
        for (i, (a, _)) in ALPHA_MODE_NAMED.iter().enumerate() {
            for (b, _) in &ALPHA_MODE_NAMED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn pixel_format_layout_matches_the_c_enum() {
        assert_eq!(size_of::<PixelFormat>(), size_of::<ffi::AVPixelFormat>());
        assert_eq!(align_of::<PixelFormat>(), align_of::<ffi::AVPixelFormat>());
        assert_eq!(size_of::<PixelFormat>(), size_of::<c_int>());
    }

    #[test]
    fn pixel_format_every_enumerator_has_one_distinct_name() {
        // The enumerators run densely from `AV_PIX_FMT_NONE` (-1) up to
        // `AV_PIX_FMT_NB`, which names none.
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        let mut names = HashSet::new();
        for raw in -1..nb {
            let name = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).name();
            let name = name.unwrap_or_else(|| panic!("raw {raw} has no name"));
            assert!(names.insert(name), "{name} names two values");
        }
        assert_eq!(names.len(), (nb + 1) as usize);
        assert_eq!(PixelFormat::from_raw(ffi::AVPixelFormat(nb)).name(), None);
    }

    #[test]
    fn pixel_format_debug_names_the_constants() {
        for (f, name) in [
            (PixelFormat::NONE, "NONE"),
            (PixelFormat::YUV420P, "YUV420P"),
            (PixelFormat::BGR4_BYTE, "BGR4_BYTE"),
            (PixelFormat::XRGB, "XRGB"),
            (PixelFormat::XBGR, "XBGR"),
            (PixelFormat::P010LE, "P010LE"),
            (PixelFormat::CUARRAY, "CUARRAY"),
        ] {
            assert_eq!(format!("{f:?}"), name);
        }
    }

    #[test]
    fn pixel_format_debug_names_an_alias_by_its_primary_constant() {
        assert_eq!(format!("{:?}", PixelFormat::Y400A), "YA8");
        assert_eq!(format!("{:?}", PixelFormat::GRAY8A), "YA8");
        assert_eq!(format!("{:?}", PixelFormat::GBR24P), "GBRP");
    }

    #[test]
    fn pixel_format_debug_shows_unnamed_values_raw() {
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        for raw in [c_int::MIN, -2, nb, nb + 1, c_int::MAX] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(format!("{f:?}"), format!("PixelFormat({raw})"));
        }
    }

    #[test]
    fn pixel_format_native_endian_picks_the_target_byte_order() {
        let pairs = [
            (PixelFormat::RGB32, PixelFormat::ARGB, PixelFormat::BGRA),
            (PixelFormat::RGB32_1, PixelFormat::RGBA, PixelFormat::ABGR),
            (PixelFormat::BGR32, PixelFormat::ABGR, PixelFormat::RGBA),
            (PixelFormat::BGR32_1, PixelFormat::BGRA, PixelFormat::ARGB),
            (PixelFormat::XRGB32, PixelFormat::XRGB, PixelFormat::BGR0),
            (PixelFormat::XBGR32, PixelFormat::XBGR, PixelFormat::RGB0),
            (
                PixelFormat::GRAY16,
                PixelFormat::GRAY16BE,
                PixelFormat::GRAY16LE,
            ),
            (
                PixelFormat::YUV420P10,
                PixelFormat::YUV420P10BE,
                PixelFormat::YUV420P10LE,
            ),
            (
                PixelFormat::RGBA128,
                PixelFormat::RGBA128BE,
                PixelFormat::RGBA128LE,
            ),
        ];
        for (ne, be, le) in pairs {
            let expected = if cfg!(target_endian = "big") { be } else { le };
            assert_eq!(ne, expected);
            assert_eq!(native_endian(be, le), expected);
        }
    }

    #[test]
    fn pixel_format_raw_conversions_round_trip_every_value() {
        for raw in [c_int::MIN, -2, -1, 0, 1, 267, 268, c_int::MAX] {
            let c = ffi::AVPixelFormat(raw);
            assert_eq!(PixelFormat::from(c).into_raw(), c);
            assert_eq!(ffi::AVPixelFormat::from(PixelFormat::from_raw(c)), c);
        }
    }

    #[test]
    fn pixel_format_default_is_none() {
        assert_eq!(PixelFormat::default(), PixelFormat::NONE);
        assert_eq!(PixelFormat::default().into_raw().0, -1);
        assert_eq!(format!("{:?}", PixelFormat::default()), "NONE");
    }

    /// Every named primaries value with its `Debug` name, aliases folded.
    const PRIMARIES_NAMED: [(ColorPrimaries, &str); 15] = [
        (ColorPrimaries::RESERVED0, "RESERVED0"),
        (ColorPrimaries::BT709, "BT709"),
        (ColorPrimaries::UNSPECIFIED, "UNSPECIFIED"),
        (ColorPrimaries::RESERVED, "RESERVED"),
        (ColorPrimaries::BT470M, "BT470M"),
        (ColorPrimaries::BT470BG, "BT470BG"),
        (ColorPrimaries::SMPTE170M, "SMPTE170M"),
        (ColorPrimaries::SMPTE240M, "SMPTE240M"),
        (ColorPrimaries::FILM, "FILM"),
        (ColorPrimaries::BT2020, "BT2020"),
        (ColorPrimaries::SMPTE428, "SMPTE428"),
        (ColorPrimaries::SMPTE431, "SMPTE431"),
        (ColorPrimaries::SMPTE432, "SMPTE432"),
        (ColorPrimaries::EBU3213, "EBU3213"),
        (ColorPrimaries::V_GAMUT, "V_GAMUT"),
    ];

    const RANGE_NAMED: [(ColorRange, &str); 3] = [
        (ColorRange::UNSPECIFIED, "UNSPECIFIED"),
        (ColorRange::MPEG, "MPEG"),
        (ColorRange::JPEG, "JPEG"),
    ];

    #[test]
    fn color_primaries_layout_matches_the_c_enum() {
        assert_eq!(
            size_of::<ColorPrimaries>(),
            size_of::<ffi::AVColorPrimaries>()
        );
        assert_eq!(
            align_of::<ColorPrimaries>(),
            align_of::<ffi::AVColorPrimaries>()
        );
        assert_eq!(size_of::<ColorPrimaries>(), size_of::<c_uint>());
    }

    #[test]
    fn color_primaries_debug_names_the_constants() {
        for (p, name) in PRIMARIES_NAMED {
            assert_eq!(format!("{p:?}"), name);
        }
    }

    #[test]
    fn color_primaries_aliases_debug_as_their_first_name() {
        assert_eq!(ColorPrimaries::SMPTEST428_1, ColorPrimaries::SMPTE428);
        assert_eq!(ColorPrimaries::JEDEC_P22, ColorPrimaries::EBU3213);
        assert_eq!(ColorPrimaries::EXT_BASE, ColorPrimaries::V_GAMUT);
        assert_eq!(format!("{:?}", ColorPrimaries::SMPTEST428_1), "SMPTE428");
        assert_eq!(format!("{:?}", ColorPrimaries::JEDEC_P22), "EBU3213");
        assert_eq!(format!("{:?}", ColorPrimaries::EXT_BASE), "V_GAMUT");
    }

    #[test]
    fn color_primaries_debug_shows_unnamed_values_raw() {
        // The H.273 gap, both `NB` values, and the far ends of `c_uint`.
        for raw in [13, 21, 23, 255, 257, 1 << 31, c_uint::MAX] {
            let p = ColorPrimaries::from_raw(ffi::AVColorPrimaries(raw));
            assert_eq!(format!("{p:?}"), format!("ColorPrimaries({raw})"));
        }
    }

    #[test]
    fn color_primaries_constants_are_distinct() {
        for (i, (a, _)) in PRIMARIES_NAMED.iter().enumerate() {
            for (b, _) in &PRIMARIES_NAMED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn color_primaries_raw_conversions_round_trip_every_value() {
        for raw in [0, 1, 12, 13, 22, 23, 256, 257, 1 << 31, c_uint::MAX] {
            let c = ffi::AVColorPrimaries(raw);
            assert_eq!(ColorPrimaries::from(c).into_raw(), c);
            assert_eq!(ffi::AVColorPrimaries::from(ColorPrimaries::from_raw(c)), c);
        }
    }

    #[test]
    fn color_primaries_default_is_unspecified_not_zero() {
        assert_eq!(ColorPrimaries::default(), ColorPrimaries::UNSPECIFIED);
        assert_ne!(ColorPrimaries::default().into_raw().0, 0);
    }

    #[test]
    fn color_range_layout_matches_the_c_enum() {
        assert_eq!(size_of::<ColorRange>(), size_of::<ffi::AVColorRange>());
        assert_eq!(align_of::<ColorRange>(), align_of::<ffi::AVColorRange>());
        assert_eq!(size_of::<ColorRange>(), size_of::<c_uint>());
    }

    #[test]
    fn color_range_debug_names_the_constants() {
        for (r, name) in RANGE_NAMED {
            assert_eq!(format!("{r:?}"), name);
        }
    }

    #[test]
    fn color_range_debug_shows_unnamed_values_raw() {
        for raw in [3, 4, 1 << 20, 1 << 31, c_uint::MAX] {
            let r = ColorRange::from_raw(ffi::AVColorRange(raw));
            assert_eq!(format!("{r:?}"), format!("ColorRange({raw})"));
        }
    }

    #[test]
    fn color_range_constants_are_distinct_and_ordered() {
        for (i, (r, _)) in RANGE_NAMED.iter().enumerate() {
            assert_eq!(r.into_raw().0 as usize, i);
            for (s, _) in &RANGE_NAMED[i + 1..] {
                assert_ne!(r, s);
            }
        }
    }

    #[test]
    fn color_range_raw_conversions_round_trip_every_value() {
        for raw in [0, 1, 2, 3, 1 << 31, c_uint::MAX] {
            let c = ffi::AVColorRange(raw);
            assert_eq!(ColorRange::from(c).into_raw(), c);
            assert_eq!(ffi::AVColorRange::from(ColorRange::from_raw(c)), c);
        }
    }

    #[test]
    fn color_range_default_is_the_zero_value() {
        assert_eq!(ColorRange::default(), ColorRange::UNSPECIFIED);
        assert_eq!(ColorRange::default().into_raw().0, 0);
    }

    const CHROMA_LOCATION_NAMED: [(ChromaLocation, &str); 7] = [
        (ChromaLocation::UNSPECIFIED, "UNSPECIFIED"),
        (ChromaLocation::LEFT, "LEFT"),
        (ChromaLocation::CENTER, "CENTER"),
        (ChromaLocation::TOPLEFT, "TOPLEFT"),
        (ChromaLocation::TOP, "TOP"),
        (ChromaLocation::BOTTOMLEFT, "BOTTOMLEFT"),
        (ChromaLocation::BOTTOM, "BOTTOM"),
    ];

    /// Values C may store though it names none of them, `AVCHROMA_LOC_NB`
    /// included.
    const CHROMA_LOCATION_UNNAMED: [c_uint; 6] =
        [7, 8, 1 << 16, 1 << 31, c_uint::MAX - 1, c_uint::MAX];

    #[test]
    fn chroma_location_layout_matches_the_c_enum() {
        assert_eq!(
            size_of::<ChromaLocation>(),
            size_of::<ffi::AVChromaLocation>()
        );
        assert_eq!(
            align_of::<ChromaLocation>(),
            align_of::<ffi::AVChromaLocation>()
        );
        assert_eq!(size_of::<ChromaLocation>(), size_of::<c_uint>());
        assert_eq!(
            size_of::<Option<ChromaLocation>>(),
            2 * size_of::<ChromaLocation>(),
            "every bit pattern is a value, so there is no niche"
        );
    }

    #[test]
    fn chroma_location_constants_are_the_numbered_positions() {
        for (i, (loc, _)) in CHROMA_LOCATION_NAMED.iter().enumerate() {
            assert_eq!(loc.into_raw().0, i as c_uint);
        }
        let nb = ffi::AVChromaLocation::AVCHROMA_LOC_NB.0;
        assert_eq!(nb as usize, CHROMA_LOCATION_NAMED.len());
        assert!(CHROMA_LOCATION_UNNAMED.contains(&nb));
    }

    #[test]
    fn chroma_location_debug_names_the_constants() {
        for (loc, name) in CHROMA_LOCATION_NAMED {
            assert_eq!(format!("{loc:?}"), name);
            assert_eq!(loc.name(), Some(name));
        }
    }

    #[test]
    fn chroma_location_debug_shows_unnamed_values_raw() {
        for raw in CHROMA_LOCATION_UNNAMED {
            let loc = ChromaLocation::from_raw(ffi::AVChromaLocation(raw));
            assert_eq!(loc.name(), None);
            assert_eq!(format!("{loc:?}"), format!("ChromaLocation({raw})"));
            assert!(CHROMA_LOCATION_NAMED.iter().all(|&(n, _)| n != loc));
        }
    }

    #[test]
    fn chroma_location_default_is_the_zeroed_value() {
        assert_eq!(ChromaLocation::default(), ChromaLocation::UNSPECIFIED);
        assert_eq!(ChromaLocation::default().into_raw().0, 0);
    }

    #[test]
    fn chroma_location_raw_conversions_round_trip_every_value() {
        let all = CHROMA_LOCATION_NAMED
            .map(|(l, _)| l.into_raw().0)
            .into_iter()
            .chain(CHROMA_LOCATION_UNNAMED);
        for raw in all {
            let c = ffi::AVChromaLocation(raw);
            assert_eq!(ChromaLocation::from(c).into_raw(), c);
            assert_eq!(ffi::AVChromaLocation::from(ChromaLocation::from_raw(c)), c);
            assert_eq!(ChromaLocation::from_raw(c), ChromaLocation::from(c));
        }
    }

    #[test]
    #[allow(clippy::clone_on_copy)]
    fn chroma_location_clone_and_hash_follow_the_encoding() {
        use std::collections::HashSet;
        let all: Vec<ChromaLocation> = CHROMA_LOCATION_NAMED
            .map(|(v, _)| v)
            .into_iter()
            .chain(
                CHROMA_LOCATION_UNNAMED.map(|r| ChromaLocation::from_raw(ffi::AVChromaLocation(r))),
            )
            .collect();
        for v in &all {
            assert_eq!(v.clone().into_raw(), v.into_raw());
        }
        let set: HashSet<ChromaLocation> = all.iter().copied().collect();
        assert_eq!(set.len(), all.len());
        assert!(set.contains(&ChromaLocation::from_raw(ffi::AVChromaLocation(4))));
        assert!(set.contains(&ChromaLocation::TOP));
    }
}
