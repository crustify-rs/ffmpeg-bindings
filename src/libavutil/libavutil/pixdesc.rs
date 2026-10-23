//! Safe wrappers over `libavutil/pixdesc.c`: `pixdesc.h`, and the pixel format and color
//! enums of `pixfmt.h`, [`PixelFormat`], [`ColorSpace`], [`ColorTransfer`],
//! [`ColorPrimaries`], [`ColorRange`], [`ChromaLocation`] and [`AlphaMode`], which it
//! implements.
//!
//! [`ComponentDescriptor`] is `AVComponentDescriptor`: where one component
//! (a colour channel or alpha) of a pixel format lives in memory, as five
//! `int`s. C embeds four of them by value in every pixel format descriptor
//! and copies them by value to read them; nothing allocates, frees or clones
//! one on its own, so it is a value type with no owner. A component a format
//! does not use is all-zero, which [`ComponentDescriptor::new`] builds. A
//! field of a wrapped C struct is reached as a [`ComponentDescriptorRef`] or
//! [`ComponentDescriptorMut`] over the projected place.
//!
//! [`PixelFormatDescriptor`] is `AVPixFmtDescriptor`: how a pixel format
//! lays out its pixels, with four [`ComponentDescriptor`]s embedded by value
//! and its [`PixelFormatFlags`]. C keeps its descriptors in one static,
//! never-written table, so a descriptor has no owner either: a Rust-held one
//! is a value, and its `name` and `alias` strings are borrowed for the whole
//! program.
//!
//! The color property lookups by name (`av_color_range_from_name` and its
//! siblings) are associated functions of the enums
//! they produce, such as [`ColorRange::from_name`], and so are their
//! inverses (`av_color_range_name` and its siblings), such as
//! [`ColorRange::name`]: C returns its names from static tables, so they are
//! `&'static CStr`s that the lookups take back. The pixel format name lookups,
//! descriptor queries ([`PixelFormat::plane_count`]) and conversion loss
//! queries are methods of [`PixelFormat`], the last reporting a [`Loss`].
//!
//! [`PixelFormat::descriptor`] borrows a format's descriptor from C's table
//! for the whole program, and [`descriptors`] walks the table; a descriptor
//! counts its bits per pixel ([`PixelFormatDescriptorRef::bits_per_pixel`]).
//! [`read_image_line`] and [`write_image_line`] move one component of a row
//! of pixels between an image's [`Planes`] and a `u16` or `u32` line, as a
//! descriptor lays them out. C trusts every descriptor it is given, so these
//! check one first, refusing what only a Rust-built descriptor can hold
//! ([`InvalidDescriptor`], [`ImageLineError`]).

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitOr, BitOrAssign, Range};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{CPlainElem, CSlice, CSliceMut, CZeroable, define_ctype};

use super::ffi;
use super::imgutils::{Planes, PlanesMut};
/// Wraps: `AVColorSpace`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorSpace;
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
    /// ITU-R BT.2020 constant luminance.
    pub const BT2020_CL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT2020_CL);
    /// ITU-R BT.2020 non-constant luminance.
    pub const BT2020_NCL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL);
    /// ITU-R BT.470 BG; also BT.601-6 625, BT.1358 625, BT.1700 625 PAL and
    /// SECAM, and IEC 61966-2-4 xvYCC601.
    pub const BT470BG: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT470BG);
    /// ITU-R BT.709; also BT.1361, IEC 61966-2-4 xvYCC709 and SMPTE RP 177
    /// Annex B.
    pub const BT709: Self = Self(ffi::AVColorSpace::AVCOL_SPC_BT709);
    /// Chromaticity-derived constant luminance.
    pub const CHROMA_DERIVED_CL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_CHROMA_DERIVED_CL);
    /// Chromaticity-derived non-constant luminance.
    pub const CHROMA_DERIVED_NCL: Self = Self(ffi::AVColorSpace::AVCOL_SPC_CHROMA_DERIVED_NCL);
    /// FCC Title 47 Code of Federal Regulations 73.682 (a)(20).
    pub const FCC: Self = Self(ffi::AVColorSpace::AVCOL_SPC_FCC);
    /// ITU-R BT.2100-0 `ICtCp`.
    pub const ICTCP: Self = Self(ffi::AVColorSpace::AVCOL_SPC_ICTCP);
    /// SMPTE ST 2128 IPT-C2.
    pub const IPT_C2: Self = Self(ffi::AVColorSpace::AVCOL_SPC_IPT_C2);
    /// Reserved for future use by ITU-T and ISO/IEC.
    pub const RESERVED: Self = Self(ffi::AVColorSpace::AVCOL_SPC_RESERVED);
    /// GBR order of the RGB coefficients; also IEC 61966-2-1 (sRGB), YZX and
    /// ST 428-1.
    pub const RGB: Self = Self(ffi::AVColorSpace::AVCOL_SPC_RGB);
    /// SMPTE 170 M; also ITU-R BT.601-6 525, BT.1358 525 and BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE170M);
    /// SMPTE 2085, Y'D'zD'x.
    pub const SMPTE2085: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE2085);
    /// SMPTE 240 M.
    pub const SMPTE240M: Self = Self(ffi::AVColorSpace::AVCOL_SPC_SMPTE240M);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorSpace::AVCOL_SPC_UNSPECIFIED);
    /// `YCgCo`, used by Dirac / VC-2 and H.264 `FRext`.
    pub const YCGCO: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO);
    /// YCgCo-R, even addition of bits.
    pub const YCGCO_RE: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO_RE);
    /// YCgCo-R, odd addition of bits.
    pub const YCGCO_RO: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCGCO_RO);
    /// Another name for [`YCGCO`](Self::YCGCO).
    pub const YCOCG: Self = Self(ffi::AVColorSpace::AVCOL_SPC_YCOCG);

    /// The colour space C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVColorSpace) -> Self {
        Self(raw)
    }

    /// The C encoding of this colour space.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVColorSpace {
        self.0
    }

    /// The name of the constant this value equals, if any; an alias reports
    /// the constant it aliases.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

/// Wraps: `AVColorTransferCharacteristic`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorTransfer;
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
    /// ARIB STD-B67, known as hybrid log-gamma (HLG).
    pub const ARIB_STD_B67: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_ARIB_STD_B67);
    /// ITU-R BT.1361 extended colour gamut.
    pub const BT1361_ECG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT1361_ECG);
    /// ITU-R BT.2020 for 10-bit systems.
    pub const BT2020_10: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT2020_10);
    /// ITU-R BT.2020 for 12-bit systems.
    pub const BT2020_12: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT2020_12);
    /// ITU-R BT.709; also BT.1361.
    pub const BT709: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_BT709);
    /// The first of FFmpeg's extensions beyond ISO/IEC 23091-2; the value of
    /// [`V_LOG`](Self::V_LOG).
    pub const EXT_BASE: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_BASE);
    /// Gamma 2.2: ITU-R BT.470 M; also BT.1700 625 PAL and SECAM.
    pub const GAMMA22: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_GAMMA22);
    /// Gamma 2.8: ITU-R BT.470 BG.
    pub const GAMMA28: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_GAMMA28);
    /// IEC 61966-2-1 (sRGB or sYCC).
    pub const IEC61966_2_1: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_IEC61966_2_1);
    /// IEC 61966-2-4.
    pub const IEC61966_2_4: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_IEC61966_2_4);
    /// Linear transfer characteristics.
    pub const LINEAR: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LINEAR);
    /// Logarithmic transfer characteristic (100:1 range).
    pub const LOG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LOG);
    /// Logarithmic transfer characteristic (100 * sqrt(10) : 1 range).
    pub const LOG_SQRT: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_LOG_SQRT);
    /// Reserved.
    pub const RESERVED: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_RESERVED);
    /// Reserved.
    pub const RESERVED0: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_RESERVED0);
    /// SMPTE 170 M; also ITU-R BT.601-6 525 or 625, BT.1358 525 or 625 and
    /// BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE170M);
    /// SMPTE ST 2084 (PQ) for 10-, 12-, 14- and 16-bit systems.
    pub const SMPTE2084: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084);
    /// SMPTE 240 M.
    pub const SMPTE240M: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE240M);
    /// SMPTE ST 428-1.
    pub const SMPTE428: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTE428);
    /// Another name for [`SMPTE2084`](Self::SMPTE2084).
    pub const SMPTEST2084: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTEST2084);
    /// Another name for [`SMPTE428`](Self::SMPTE428).
    pub const SMPTEST428_1: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_SMPTEST428_1);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_UNSPECIFIED);
    /// Panasonic V-Log, an FFmpeg extension.
    pub const V_LOG: Self = Self(ffi::AVColorTransferCharacteristic::AVCOL_TRC_V_LOG);

    /// The transfer characteristic C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVColorTransferCharacteristic) -> Self {
        Self(raw)
    }

    /// The C encoding of this transfer characteristic.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVColorTransferCharacteristic {
        self.0
    }

    /// The name of the constant this value equals, if any; an alias reports
    /// the constant it aliases, and [`EXT_BASE`](Self::EXT_BASE) the
    /// extension it numbers.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

/// Wraps: `AVAlphaMode`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::AlphaMode;
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
    /// The alpha channel is multiplied into the colour values.
    pub const PREMULTIPLIED: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_PREMULTIPLIED);
    /// The alpha channel is independent of the colour values.
    pub const STRAIGHT: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_STRAIGHT);
    /// Unknown alpha handling, or no alpha channel.
    pub const UNSPECIFIED: Self = Self(ffi::AVAlphaMode::AVALPHA_MODE_UNSPECIFIED);

    /// The alpha mode C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVAlphaMode) -> Self {
        Self(raw)
    }

    /// The C encoding of this alpha mode.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVAlphaMode {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

/// Wraps: `AVPixelFormat`
/// Handwritten
///
/// The layout of a picture's pixels in memory: its planes, the order and
/// depth of their components, its chroma subsampling and byte order, or the
/// hardware surface that holds it.
///
/// A newtype over the C enum rather than a Rust `enum`, as
/// [`MediaType`](super::utils::MediaType) is: C stores the value in public
/// struct fields and format lists its users write directly
/// (`AVCodecContext.pix_fmt`, `AVHWFramesContext.sw_format`), and a newer
/// library may return a format this crate does not name, so it may hold any
/// `int`, and a Rust view of that memory must accept every one. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
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
    #[must_use]
    pub const fn from_raw(raw: ffi::AVPixelFormat) -> Self {
        Self(raw)
    }

    /// The C encoding of this pixel format.
    #[inline]
    #[must_use]
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
            const fn constant_name(self) -> Option<&'static str> {
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
    /// Hardware acceleration through VA-API, `data[3]` contains a `VASurfaceID`.
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
    /// HW acceleration through VDPAU, `Picture.data[3]` contains a `VdpVideoSurface`.
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
    /// VAAPI: `mfxFrameSurface1.Data.MemId` contains a pointer to `VASurfaceID`.
    ///
    /// DXVA2: `mfxFrameSurface1.Data.MemId` contains a pointer to `IDirect3DSurface9`.
    ///
    /// FFmpeg 5.0 and above: `mfxFrameSurface1.Data.MemId` contains a pointer to the mfxHDLPair
    /// structure when importing the following frames as QSV frames:
    ///
    /// VAAPI: mfxHDLPair.first contains a `VASurfaceID` pointer. mfxHDLPair.second is always
    /// `MFX_INFINITE`.
    ///
    /// DXVA2: mfxHDLPair.first contains `IDirect3DSurface9` pointer. mfxHDLPair.second is always
    /// `MFX_INFINITE`.
    ///
    /// D3D11: mfxHDLPair.first contains a `ID3D11Texture2D` pointer. mfxHDLPair.second contains
    /// the texture array index of the frame if the `ID3D11Texture2D` is an array texture, or
    /// always `MFX_INFINITE` if it is a normal texture.
    QSV = AV_PIX_FMT_QSV;
    /// HW acceleration though MMAL, `data[3]` contains a pointer to the `MMAL_BUFFER_HEADER_T`
    /// structure.
    MMAL = AV_PIX_FMT_MMAL;
    /// HW decoding through Direct3D11 via old API, `Picture.data[3]` contains a
    /// `ID3D11VideoDecoderOutputView` pointer.
    D3D11VA_VLD = AV_PIX_FMT_D3D11VA_VLD;
    /// HW acceleration through CUDA. `data[i]` contain `CUdeviceptr` pointers exactly as for
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
    /// `data[0]` contains a `ID3D11Texture2D` pointer, and `data[1]` contains the texture array
    /// index of the frame as `intptr_t` if the `ID3D11Texture2D` is an array texture (or always 0
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
    /// `data[0]` points to an `AVDRMFrameDescriptor`.
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
    /// `data[0]` points to an `AVVkFrame`.
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
    /// `data[0]` points to an `AVD3D12VAFrame`.
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
    /// HW acceleration through AMF. `data[0]` contain `AMFSurface` pointer.
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
    /// CUDA block-linear (opaque). `data[0]` is a `CUarray` in block-linear layout, e.g. from
    /// NVDEC opaque decode. Use for zero-copy to NVENC (CUDA array input).
    CUARRAY = AV_PIX_FMT_CUARRAY;
}

impl PixelFormat {
    /// [`AYUV64BE`](Self::AYUV64BE) on big-endian targets and [`AYUV64LE`](Self::AYUV64LE) on
    /// little-endian ones: `AV_PIX_FMT_AYUV64`, in native byte order.
    pub const AYUV64: Self = native_endian(Self::AYUV64BE, Self::AYUV64LE);
    /// [`BAYER_BGGR16BE`](Self::BAYER_BGGR16BE) on big-endian targets and
    /// [`BAYER_BGGR16LE`](Self::BAYER_BGGR16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_BGGR16`, in native byte order.
    pub const BAYER_BGGR16: Self = native_endian(Self::BAYER_BGGR16BE, Self::BAYER_BGGR16LE);
    /// [`BAYER_GBRG16BE`](Self::BAYER_GBRG16BE) on big-endian targets and
    /// [`BAYER_GBRG16LE`](Self::BAYER_GBRG16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_GBRG16`, in native byte order.
    pub const BAYER_GBRG16: Self = native_endian(Self::BAYER_GBRG16BE, Self::BAYER_GBRG16LE);
    /// [`BAYER_GRBG16BE`](Self::BAYER_GRBG16BE) on big-endian targets and
    /// [`BAYER_GRBG16LE`](Self::BAYER_GRBG16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_GRBG16`, in native byte order.
    pub const BAYER_GRBG16: Self = native_endian(Self::BAYER_GRBG16BE, Self::BAYER_GRBG16LE);
    /// [`BAYER_RGGB16BE`](Self::BAYER_RGGB16BE) on big-endian targets and
    /// [`BAYER_RGGB16LE`](Self::BAYER_RGGB16LE) on little-endian ones:
    /// `AV_PIX_FMT_BAYER_RGGB16`, in native byte order.
    pub const BAYER_RGGB16: Self = native_endian(Self::BAYER_RGGB16BE, Self::BAYER_RGGB16LE);
    /// [`ABGR`](Self::ABGR) on big-endian targets and [`RGBA`](Self::RGBA) on little-endian
    /// ones: `AV_PIX_FMT_BGR32`, in native byte order.
    pub const BGR32: Self = native_endian(Self::ABGR, Self::RGBA);
    /// [`BGRA`](Self::BGRA) on big-endian targets and [`ARGB`](Self::ARGB) on little-endian
    /// ones: `AV_PIX_FMT_BGR32_1`, in native byte order.
    pub const BGR32_1: Self = native_endian(Self::BGRA, Self::ARGB);
    /// [`BGR444BE`](Self::BGR444BE) on big-endian targets and [`BGR444LE`](Self::BGR444LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR444`, in native byte order.
    pub const BGR444: Self = native_endian(Self::BGR444BE, Self::BGR444LE);
    /// [`BGR48BE`](Self::BGR48BE) on big-endian targets and [`BGR48LE`](Self::BGR48LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR48`, in native byte order.
    pub const BGR48: Self = native_endian(Self::BGR48BE, Self::BGR48LE);
    /// [`BGR555BE`](Self::BGR555BE) on big-endian targets and [`BGR555LE`](Self::BGR555LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR555`, in native byte order.
    pub const BGR555: Self = native_endian(Self::BGR555BE, Self::BGR555LE);
    /// [`BGR565BE`](Self::BGR565BE) on big-endian targets and [`BGR565LE`](Self::BGR565LE) on
    /// little-endian ones: `AV_PIX_FMT_BGR565`, in native byte order.
    pub const BGR565: Self = native_endian(Self::BGR565BE, Self::BGR565LE);
    /// [`BGRA64BE`](Self::BGRA64BE) on big-endian targets and [`BGRA64LE`](Self::BGRA64LE) on
    /// little-endian ones: `AV_PIX_FMT_BGRA64`, in native byte order.
    pub const BGRA64: Self = native_endian(Self::BGRA64BE, Self::BGRA64LE);
    /// Another name for [`GBRP`](Self::GBRP) (`AV_PIX_FMT_GBR24P`).
    pub const GBR24P: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_GBR24P);
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
    /// [`GBRAPF16BE`](Self::GBRAPF16BE) on big-endian targets and
    /// [`GBRAPF16LE`](Self::GBRAPF16LE) on little-endian ones: `AV_PIX_FMT_GBRAPF16`, in native
    /// byte order.
    pub const GBRAPF16: Self = native_endian(Self::GBRAPF16BE, Self::GBRAPF16LE);
    /// [`GBRAPF32BE`](Self::GBRAPF32BE) on big-endian targets and
    /// [`GBRAPF32LE`](Self::GBRAPF32LE) on little-endian ones: `AV_PIX_FMT_GBRAPF32`, in native
    /// byte order.
    pub const GBRAPF32: Self = native_endian(Self::GBRAPF32BE, Self::GBRAPF32LE);
    /// [`GBRP10BE`](Self::GBRP10BE) on big-endian targets and [`GBRP10LE`](Self::GBRP10LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP10`, in native byte order.
    pub const GBRP10: Self = native_endian(Self::GBRP10BE, Self::GBRP10LE);
    /// [`GBRP10MSBBE`](Self::GBRP10MSBBE) on big-endian targets and
    /// [`GBRP10MSBLE`](Self::GBRP10MSBLE) on little-endian ones: `AV_PIX_FMT_GBRP10MSB`, in
    /// native byte order.
    pub const GBRP10MSB: Self = native_endian(Self::GBRP10MSBBE, Self::GBRP10MSBLE);
    /// [`GBRP12BE`](Self::GBRP12BE) on big-endian targets and [`GBRP12LE`](Self::GBRP12LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP12`, in native byte order.
    pub const GBRP12: Self = native_endian(Self::GBRP12BE, Self::GBRP12LE);
    /// [`GBRP12MSBBE`](Self::GBRP12MSBBE) on big-endian targets and
    /// [`GBRP12MSBLE`](Self::GBRP12MSBLE) on little-endian ones: `AV_PIX_FMT_GBRP12MSB`, in
    /// native byte order.
    pub const GBRP12MSB: Self = native_endian(Self::GBRP12MSBBE, Self::GBRP12MSBLE);
    /// [`GBRP14BE`](Self::GBRP14BE) on big-endian targets and [`GBRP14LE`](Self::GBRP14LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP14`, in native byte order.
    pub const GBRP14: Self = native_endian(Self::GBRP14BE, Self::GBRP14LE);
    /// [`GBRP16BE`](Self::GBRP16BE) on big-endian targets and [`GBRP16LE`](Self::GBRP16LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP16`, in native byte order.
    pub const GBRP16: Self = native_endian(Self::GBRP16BE, Self::GBRP16LE);
    /// [`GBRP9BE`](Self::GBRP9BE) on big-endian targets and [`GBRP9LE`](Self::GBRP9LE) on
    /// little-endian ones: `AV_PIX_FMT_GBRP9`, in native byte order.
    pub const GBRP9: Self = native_endian(Self::GBRP9BE, Self::GBRP9LE);
    /// [`GBRPF16BE`](Self::GBRPF16BE) on big-endian targets and [`GBRPF16LE`](Self::GBRPF16LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRPF16`, in native byte order.
    pub const GBRPF16: Self = native_endian(Self::GBRPF16BE, Self::GBRPF16LE);
    /// [`GBRPF32BE`](Self::GBRPF32BE) on big-endian targets and [`GBRPF32LE`](Self::GBRPF32LE)
    /// on little-endian ones: `AV_PIX_FMT_GBRPF32`, in native byte order.
    pub const GBRPF32: Self = native_endian(Self::GBRPF32BE, Self::GBRPF32LE);
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
    /// Another name for [`YA8`](Self::YA8) (`AV_PIX_FMT_GRAY8A`).
    pub const GRAY8A: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_GRAY8A);
    /// [`GRAY9BE`](Self::GRAY9BE) on big-endian targets and [`GRAY9LE`](Self::GRAY9LE) on
    /// little-endian ones: `AV_PIX_FMT_GRAY9`, in native byte order.
    pub const GRAY9: Self = native_endian(Self::GRAY9BE, Self::GRAY9LE);
    /// [`GRAYF16BE`](Self::GRAYF16BE) on big-endian targets and [`GRAYF16LE`](Self::GRAYF16LE)
    /// on little-endian ones: `AV_PIX_FMT_GRAYF16`, in native byte order.
    pub const GRAYF16: Self = native_endian(Self::GRAYF16BE, Self::GRAYF16LE);
    /// [`GRAYF32BE`](Self::GRAYF32BE) on big-endian targets and [`GRAYF32LE`](Self::GRAYF32LE)
    /// on little-endian ones: `AV_PIX_FMT_GRAYF32`, in native byte order.
    pub const GRAYF32: Self = native_endian(Self::GRAYF32BE, Self::GRAYF32LE);
    /// [`NV20BE`](Self::NV20BE) on big-endian targets and [`NV20LE`](Self::NV20LE) on
    /// little-endian ones: `AV_PIX_FMT_NV20`, in native byte order.
    pub const NV20: Self = native_endian(Self::NV20BE, Self::NV20LE);
    /// [`P010BE`](Self::P010BE) on big-endian targets and [`P010LE`](Self::P010LE) on
    /// little-endian ones: `AV_PIX_FMT_P010`, in native byte order.
    pub const P010: Self = native_endian(Self::P010BE, Self::P010LE);
    /// [`P012BE`](Self::P012BE) on big-endian targets and [`P012LE`](Self::P012LE) on
    /// little-endian ones: `AV_PIX_FMT_P012`, in native byte order.
    pub const P012: Self = native_endian(Self::P012BE, Self::P012LE);
    /// [`P016BE`](Self::P016BE) on big-endian targets and [`P016LE`](Self::P016LE) on
    /// little-endian ones: `AV_PIX_FMT_P016`, in native byte order.
    pub const P016: Self = native_endian(Self::P016BE, Self::P016LE);
    /// [`P210BE`](Self::P210BE) on big-endian targets and [`P210LE`](Self::P210LE) on
    /// little-endian ones: `AV_PIX_FMT_P210`, in native byte order.
    pub const P210: Self = native_endian(Self::P210BE, Self::P210LE);
    /// [`P212BE`](Self::P212BE) on big-endian targets and [`P212LE`](Self::P212LE) on
    /// little-endian ones: `AV_PIX_FMT_P212`, in native byte order.
    pub const P212: Self = native_endian(Self::P212BE, Self::P212LE);
    /// [`P216BE`](Self::P216BE) on big-endian targets and [`P216LE`](Self::P216LE) on
    /// little-endian ones: `AV_PIX_FMT_P216`, in native byte order.
    pub const P216: Self = native_endian(Self::P216BE, Self::P216LE);
    /// [`P410BE`](Self::P410BE) on big-endian targets and [`P410LE`](Self::P410LE) on
    /// little-endian ones: `AV_PIX_FMT_P410`, in native byte order.
    pub const P410: Self = native_endian(Self::P410BE, Self::P410LE);
    /// [`P412BE`](Self::P412BE) on big-endian targets and [`P412LE`](Self::P412LE) on
    /// little-endian ones: `AV_PIX_FMT_P412`, in native byte order.
    pub const P412: Self = native_endian(Self::P412BE, Self::P412LE);
    /// [`P416BE`](Self::P416BE) on big-endian targets and [`P416LE`](Self::P416LE) on
    /// little-endian ones: `AV_PIX_FMT_P416`, in native byte order.
    pub const P416: Self = native_endian(Self::P416BE, Self::P416LE);
    /// [`ARGB`](Self::ARGB) on big-endian targets and [`BGRA`](Self::BGRA) on little-endian
    /// ones: `AV_PIX_FMT_RGB32`, in native byte order.
    pub const RGB32: Self = native_endian(Self::ARGB, Self::BGRA);
    /// [`RGBA`](Self::RGBA) on big-endian targets and [`ABGR`](Self::ABGR) on little-endian
    /// ones: `AV_PIX_FMT_RGB32_1`, in native byte order.
    pub const RGB32_1: Self = native_endian(Self::RGBA, Self::ABGR);
    /// [`RGB444BE`](Self::RGB444BE) on big-endian targets and [`RGB444LE`](Self::RGB444LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB444`, in native byte order.
    pub const RGB444: Self = native_endian(Self::RGB444BE, Self::RGB444LE);
    /// [`RGB48BE`](Self::RGB48BE) on big-endian targets and [`RGB48LE`](Self::RGB48LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB48`, in native byte order.
    pub const RGB48: Self = native_endian(Self::RGB48BE, Self::RGB48LE);
    /// [`RGB555BE`](Self::RGB555BE) on big-endian targets and [`RGB555LE`](Self::RGB555LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB555`, in native byte order.
    pub const RGB555: Self = native_endian(Self::RGB555BE, Self::RGB555LE);
    /// [`RGB565BE`](Self::RGB565BE) on big-endian targets and [`RGB565LE`](Self::RGB565LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB565`, in native byte order.
    pub const RGB565: Self = native_endian(Self::RGB565BE, Self::RGB565LE);
    /// [`RGB96BE`](Self::RGB96BE) on big-endian targets and [`RGB96LE`](Self::RGB96LE) on
    /// little-endian ones: `AV_PIX_FMT_RGB96`, in native byte order.
    pub const RGB96: Self = native_endian(Self::RGB96BE, Self::RGB96LE);
    /// [`RGBA128BE`](Self::RGBA128BE) on big-endian targets and [`RGBA128LE`](Self::RGBA128LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBA128`, in native byte order.
    pub const RGBA128: Self = native_endian(Self::RGBA128BE, Self::RGBA128LE);
    /// [`RGBA64BE`](Self::RGBA64BE) on big-endian targets and [`RGBA64LE`](Self::RGBA64LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBA64`, in native byte order.
    pub const RGBA64: Self = native_endian(Self::RGBA64BE, Self::RGBA64LE);
    /// [`RGBAF16BE`](Self::RGBAF16BE) on big-endian targets and [`RGBAF16LE`](Self::RGBAF16LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBAF16`, in native byte order.
    pub const RGBAF16: Self = native_endian(Self::RGBAF16BE, Self::RGBAF16LE);
    /// [`RGBAF32BE`](Self::RGBAF32BE) on big-endian targets and [`RGBAF32LE`](Self::RGBAF32LE)
    /// on little-endian ones: `AV_PIX_FMT_RGBAF32`, in native byte order.
    pub const RGBAF32: Self = native_endian(Self::RGBAF32BE, Self::RGBAF32LE);
    /// [`RGBF16BE`](Self::RGBF16BE) on big-endian targets and [`RGBF16LE`](Self::RGBF16LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBF16`, in native byte order.
    pub const RGBF16: Self = native_endian(Self::RGBF16BE, Self::RGBF16LE);
    /// [`RGBF32BE`](Self::RGBF32BE) on big-endian targets and [`RGBF32LE`](Self::RGBF32LE) on
    /// little-endian ones: `AV_PIX_FMT_RGBF32`, in native byte order.
    pub const RGBF32: Self = native_endian(Self::RGBF32BE, Self::RGBF32LE);
    /// [`V30XBE`](Self::V30XBE) on big-endian targets and [`V30XLE`](Self::V30XLE) on
    /// little-endian ones: `AV_PIX_FMT_V30X`, in native byte order.
    pub const V30X: Self = native_endian(Self::V30XBE, Self::V30XLE);
    /// [`X2BGR10BE`](Self::X2BGR10BE) on big-endian targets and [`X2BGR10LE`](Self::X2BGR10LE)
    /// on little-endian ones: `AV_PIX_FMT_X2BGR10`, in native byte order.
    pub const X2BGR10: Self = native_endian(Self::X2BGR10BE, Self::X2BGR10LE);
    /// [`X2RGB10BE`](Self::X2RGB10BE) on big-endian targets and [`X2RGB10LE`](Self::X2RGB10LE)
    /// on little-endian ones: `AV_PIX_FMT_X2RGB10`, in native byte order.
    pub const X2RGB10: Self = native_endian(Self::X2RGB10BE, Self::X2RGB10LE);
    /// [`XBGR`](Self::XBGR) on big-endian targets and [`RGB0`](Self::RGB0) on little-endian
    /// ones: `AV_PIX_FMT_0BGR32`, in native byte order.
    pub const XBGR32: Self = native_endian(Self::XBGR, Self::RGB0);
    /// [`XRGB`](Self::XRGB) on big-endian targets and [`BGR0`](Self::BGR0) on little-endian
    /// ones: `AV_PIX_FMT_0RGB32`, in native byte order.
    pub const XRGB32: Self = native_endian(Self::XRGB, Self::BGR0);
    /// [`XV30BE`](Self::XV30BE) on big-endian targets and [`XV30LE`](Self::XV30LE) on
    /// little-endian ones: `AV_PIX_FMT_XV30`, in native byte order.
    pub const XV30: Self = native_endian(Self::XV30BE, Self::XV30LE);
    /// [`XV36BE`](Self::XV36BE) on big-endian targets and [`XV36LE`](Self::XV36LE) on
    /// little-endian ones: `AV_PIX_FMT_XV36`, in native byte order.
    pub const XV36: Self = native_endian(Self::XV36BE, Self::XV36LE);
    /// [`XV48BE`](Self::XV48BE) on big-endian targets and [`XV48LE`](Self::XV48LE) on
    /// little-endian ones: `AV_PIX_FMT_XV48`, in native byte order.
    pub const XV48: Self = native_endian(Self::XV48BE, Self::XV48LE);
    /// [`XYZ12BE`](Self::XYZ12BE) on big-endian targets and [`XYZ12LE`](Self::XYZ12LE) on
    /// little-endian ones: `AV_PIX_FMT_XYZ12`, in native byte order.
    pub const XYZ12: Self = native_endian(Self::XYZ12BE, Self::XYZ12LE);
    /// [`Y210BE`](Self::Y210BE) on big-endian targets and [`Y210LE`](Self::Y210LE) on
    /// little-endian ones: `AV_PIX_FMT_Y210`, in native byte order.
    pub const Y210: Self = native_endian(Self::Y210BE, Self::Y210LE);
    /// [`Y212BE`](Self::Y212BE) on big-endian targets and [`Y212LE`](Self::Y212LE) on
    /// little-endian ones: `AV_PIX_FMT_Y212`, in native byte order.
    pub const Y212: Self = native_endian(Self::Y212BE, Self::Y212LE);
    /// [`Y216BE`](Self::Y216BE) on big-endian targets and [`Y216LE`](Self::Y216LE) on
    /// little-endian ones: `AV_PIX_FMT_Y216`, in native byte order.
    pub const Y216: Self = native_endian(Self::Y216BE, Self::Y216LE);
    /// Another name for [`YA8`](Self::YA8) (`AV_PIX_FMT_Y400A`).
    pub const Y400A: Self = Self(ffi::AVPixelFormat::AV_PIX_FMT_Y400A);
    /// [`YA16BE`](Self::YA16BE) on big-endian targets and [`YA16LE`](Self::YA16LE) on
    /// little-endian ones: `AV_PIX_FMT_YA16`, in native byte order.
    pub const YA16: Self = native_endian(Self::YA16BE, Self::YA16LE);
    /// [`YAF16BE`](Self::YAF16BE) on big-endian targets and [`YAF16LE`](Self::YAF16LE) on
    /// little-endian ones: `AV_PIX_FMT_YAF16`, in native byte order.
    pub const YAF16: Self = native_endian(Self::YAF16BE, Self::YAF16LE);
    /// [`YAF32BE`](Self::YAF32BE) on big-endian targets and [`YAF32LE`](Self::YAF32LE) on
    /// little-endian ones: `AV_PIX_FMT_YAF32`, in native byte order.
    pub const YAF32: Self = native_endian(Self::YAF32BE, Self::YAF32LE);
    /// [`YUV420P10BE`](Self::YUV420P10BE) on big-endian targets and
    /// [`YUV420P10LE`](Self::YUV420P10LE) on little-endian ones: `AV_PIX_FMT_YUV420P10`, in
    /// native byte order.
    pub const YUV420P10: Self = native_endian(Self::YUV420P10BE, Self::YUV420P10LE);
    /// [`YUV420P12BE`](Self::YUV420P12BE) on big-endian targets and
    /// [`YUV420P12LE`](Self::YUV420P12LE) on little-endian ones: `AV_PIX_FMT_YUV420P12`, in
    /// native byte order.
    pub const YUV420P12: Self = native_endian(Self::YUV420P12BE, Self::YUV420P12LE);
    /// [`YUV420P14BE`](Self::YUV420P14BE) on big-endian targets and
    /// [`YUV420P14LE`](Self::YUV420P14LE) on little-endian ones: `AV_PIX_FMT_YUV420P14`, in
    /// native byte order.
    pub const YUV420P14: Self = native_endian(Self::YUV420P14BE, Self::YUV420P14LE);
    /// [`YUV420P16BE`](Self::YUV420P16BE) on big-endian targets and
    /// [`YUV420P16LE`](Self::YUV420P16LE) on little-endian ones: `AV_PIX_FMT_YUV420P16`, in
    /// native byte order.
    pub const YUV420P16: Self = native_endian(Self::YUV420P16BE, Self::YUV420P16LE);
    /// [`YUV420P9BE`](Self::YUV420P9BE) on big-endian targets and
    /// [`YUV420P9LE`](Self::YUV420P9LE) on little-endian ones: `AV_PIX_FMT_YUV420P9`, in native
    /// byte order.
    pub const YUV420P9: Self = native_endian(Self::YUV420P9BE, Self::YUV420P9LE);
    /// [`YUV422P10BE`](Self::YUV422P10BE) on big-endian targets and
    /// [`YUV422P10LE`](Self::YUV422P10LE) on little-endian ones: `AV_PIX_FMT_YUV422P10`, in
    /// native byte order.
    pub const YUV422P10: Self = native_endian(Self::YUV422P10BE, Self::YUV422P10LE);
    /// [`YUV422P12BE`](Self::YUV422P12BE) on big-endian targets and
    /// [`YUV422P12LE`](Self::YUV422P12LE) on little-endian ones: `AV_PIX_FMT_YUV422P12`, in
    /// native byte order.
    pub const YUV422P12: Self = native_endian(Self::YUV422P12BE, Self::YUV422P12LE);
    /// [`YUV422P14BE`](Self::YUV422P14BE) on big-endian targets and
    /// [`YUV422P14LE`](Self::YUV422P14LE) on little-endian ones: `AV_PIX_FMT_YUV422P14`, in
    /// native byte order.
    pub const YUV422P14: Self = native_endian(Self::YUV422P14BE, Self::YUV422P14LE);
    /// [`YUV422P16BE`](Self::YUV422P16BE) on big-endian targets and
    /// [`YUV422P16LE`](Self::YUV422P16LE) on little-endian ones: `AV_PIX_FMT_YUV422P16`, in
    /// native byte order.
    pub const YUV422P16: Self = native_endian(Self::YUV422P16BE, Self::YUV422P16LE);
    /// [`YUV422P9BE`](Self::YUV422P9BE) on big-endian targets and
    /// [`YUV422P9LE`](Self::YUV422P9LE) on little-endian ones: `AV_PIX_FMT_YUV422P9`, in native
    /// byte order.
    pub const YUV422P9: Self = native_endian(Self::YUV422P9BE, Self::YUV422P9LE);
    /// [`YUV440P10BE`](Self::YUV440P10BE) on big-endian targets and
    /// [`YUV440P10LE`](Self::YUV440P10LE) on little-endian ones: `AV_PIX_FMT_YUV440P10`, in
    /// native byte order.
    pub const YUV440P10: Self = native_endian(Self::YUV440P10BE, Self::YUV440P10LE);
    /// [`YUV440P12BE`](Self::YUV440P12BE) on big-endian targets and
    /// [`YUV440P12LE`](Self::YUV440P12LE) on little-endian ones: `AV_PIX_FMT_YUV440P12`, in
    /// native byte order.
    pub const YUV440P12: Self = native_endian(Self::YUV440P12BE, Self::YUV440P12LE);
    /// [`YUV444P10BE`](Self::YUV444P10BE) on big-endian targets and
    /// [`YUV444P10LE`](Self::YUV444P10LE) on little-endian ones: `AV_PIX_FMT_YUV444P10`, in
    /// native byte order.
    pub const YUV444P10: Self = native_endian(Self::YUV444P10BE, Self::YUV444P10LE);
    /// [`YUV444P10MSBBE`](Self::YUV444P10MSBBE) on big-endian targets and
    /// [`YUV444P10MSBLE`](Self::YUV444P10MSBLE) on little-endian ones:
    /// `AV_PIX_FMT_YUV444P10MSB`, in native byte order.
    pub const YUV444P10MSB: Self = native_endian(Self::YUV444P10MSBBE, Self::YUV444P10MSBLE);
    /// [`YUV444P12BE`](Self::YUV444P12BE) on big-endian targets and
    /// [`YUV444P12LE`](Self::YUV444P12LE) on little-endian ones: `AV_PIX_FMT_YUV444P12`, in
    /// native byte order.
    pub const YUV444P12: Self = native_endian(Self::YUV444P12BE, Self::YUV444P12LE);
    /// [`YUV444P12MSBBE`](Self::YUV444P12MSBBE) on big-endian targets and
    /// [`YUV444P12MSBLE`](Self::YUV444P12MSBLE) on little-endian ones:
    /// `AV_PIX_FMT_YUV444P12MSB`, in native byte order.
    pub const YUV444P12MSB: Self = native_endian(Self::YUV444P12MSBBE, Self::YUV444P12MSBLE);
    /// [`YUV444P14BE`](Self::YUV444P14BE) on big-endian targets and
    /// [`YUV444P14LE`](Self::YUV444P14LE) on little-endian ones: `AV_PIX_FMT_YUV444P14`, in
    /// native byte order.
    pub const YUV444P14: Self = native_endian(Self::YUV444P14BE, Self::YUV444P14LE);
    /// [`YUV444P16BE`](Self::YUV444P16BE) on big-endian targets and
    /// [`YUV444P16LE`](Self::YUV444P16LE) on little-endian ones: `AV_PIX_FMT_YUV444P16`, in
    /// native byte order.
    pub const YUV444P16: Self = native_endian(Self::YUV444P16BE, Self::YUV444P16LE);
    /// [`YUV444P9BE`](Self::YUV444P9BE) on big-endian targets and
    /// [`YUV444P9LE`](Self::YUV444P9LE) on little-endian ones: `AV_PIX_FMT_YUV444P9`, in native
    /// byte order.
    pub const YUV444P9: Self = native_endian(Self::YUV444P9BE, Self::YUV444P9LE);
    /// [`YUVA420P10BE`](Self::YUVA420P10BE) on big-endian targets and
    /// [`YUVA420P10LE`](Self::YUVA420P10LE) on little-endian ones: `AV_PIX_FMT_YUVA420P10`, in
    /// native byte order.
    pub const YUVA420P10: Self = native_endian(Self::YUVA420P10BE, Self::YUVA420P10LE);
    /// [`YUVA420P16BE`](Self::YUVA420P16BE) on big-endian targets and
    /// [`YUVA420P16LE`](Self::YUVA420P16LE) on little-endian ones: `AV_PIX_FMT_YUVA420P16`, in
    /// native byte order.
    pub const YUVA420P16: Self = native_endian(Self::YUVA420P16BE, Self::YUVA420P16LE);
    /// [`YUVA420P9BE`](Self::YUVA420P9BE) on big-endian targets and
    /// [`YUVA420P9LE`](Self::YUVA420P9LE) on little-endian ones: `AV_PIX_FMT_YUVA420P9`, in
    /// native byte order.
    pub const YUVA420P9: Self = native_endian(Self::YUVA420P9BE, Self::YUVA420P9LE);
    /// [`YUVA422P10BE`](Self::YUVA422P10BE) on big-endian targets and
    /// [`YUVA422P10LE`](Self::YUVA422P10LE) on little-endian ones: `AV_PIX_FMT_YUVA422P10`, in
    /// native byte order.
    pub const YUVA422P10: Self = native_endian(Self::YUVA422P10BE, Self::YUVA422P10LE);
    /// [`YUVA422P12BE`](Self::YUVA422P12BE) on big-endian targets and
    /// [`YUVA422P12LE`](Self::YUVA422P12LE) on little-endian ones: `AV_PIX_FMT_YUVA422P12`, in
    /// native byte order.
    pub const YUVA422P12: Self = native_endian(Self::YUVA422P12BE, Self::YUVA422P12LE);
    /// [`YUVA422P16BE`](Self::YUVA422P16BE) on big-endian targets and
    /// [`YUVA422P16LE`](Self::YUVA422P16LE) on little-endian ones: `AV_PIX_FMT_YUVA422P16`, in
    /// native byte order.
    pub const YUVA422P16: Self = native_endian(Self::YUVA422P16BE, Self::YUVA422P16LE);
    /// [`YUVA422P9BE`](Self::YUVA422P9BE) on big-endian targets and
    /// [`YUVA422P9LE`](Self::YUVA422P9LE) on little-endian ones: `AV_PIX_FMT_YUVA422P9`, in
    /// native byte order.
    pub const YUVA422P9: Self = native_endian(Self::YUVA422P9BE, Self::YUVA422P9LE);
    /// [`YUVA444P10BE`](Self::YUVA444P10BE) on big-endian targets and
    /// [`YUVA444P10LE`](Self::YUVA444P10LE) on little-endian ones: `AV_PIX_FMT_YUVA444P10`, in
    /// native byte order.
    pub const YUVA444P10: Self = native_endian(Self::YUVA444P10BE, Self::YUVA444P10LE);
    /// [`YUVA444P12BE`](Self::YUVA444P12BE) on big-endian targets and
    /// [`YUVA444P12LE`](Self::YUVA444P12LE) on little-endian ones: `AV_PIX_FMT_YUVA444P12`, in
    /// native byte order.
    pub const YUVA444P12: Self = native_endian(Self::YUVA444P12BE, Self::YUVA444P12LE);
    /// [`YUVA444P16BE`](Self::YUVA444P16BE) on big-endian targets and
    /// [`YUVA444P16LE`](Self::YUVA444P16LE) on little-endian ones: `AV_PIX_FMT_YUVA444P16`, in
    /// native byte order.
    pub const YUVA444P16: Self = native_endian(Self::YUVA444P16BE, Self::YUVA444P16LE);
    /// [`YUVA444P9BE`](Self::YUVA444P9BE) on big-endian targets and
    /// [`YUVA444P9LE`](Self::YUVA444P9LE) on little-endian ones: `AV_PIX_FMT_YUVA444P9`, in
    /// native byte order.
    pub const YUVA444P9: Self = native_endian(Self::YUVA444P9BE, Self::YUVA444P9LE);
}

impl Default for PixelFormat {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for PixelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.constant_name() {
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

/// Wraps: `AVColorPrimaries`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorPrimaries;
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
    /// ITU-R BT.2020.
    pub const BT2020: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT2020);
    /// ITU-R BT.470 System B, G, also ITU-R BT.601-6 625 / BT.1358 625 /
    /// BT.1700 625 PAL & SECAM.
    pub const BT470BG: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT470BG);
    /// ITU-R BT.470 System M, also FCC Title 47 CFR 73.682 (a)(20).
    pub const BT470M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT470M);
    /// ITU-R BT.709, also ITU-R BT.1361 / IEC 61966-2-4 / SMPTE RP 177 Annex B.
    pub const BT709: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_BT709);
    /// EBU Tech. 3213-E, one of the JEDEC P22 group phosphors.
    pub const EBU3213: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_EBU3213);
    /// The first of FFmpeg's extensions, which are not part of H.273; the
    /// same value as [`V_GAMUT`](Self::V_GAMUT).
    pub const EXT_BASE: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_EXT_BASE);
    /// Generic film: colour filters using Illuminant C.
    pub const FILM: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_FILM);
    /// The same value as [`EBU3213`](Self::EBU3213).
    pub const JEDEC_P22: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_JEDEC_P22);
    /// Reserved code point 3.
    pub const RESERVED: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_RESERVED);
    /// Reserved code point 0; the value of a zero-filled field.
    pub const RESERVED0: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_RESERVED0);
    /// SMPTE 170M, also ITU-R BT.601-6 525 / BT.1358 525 / BT.1700 NTSC.
    pub const SMPTE170M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE170M);
    /// SMPTE 240M, identical to [`SMPTE170M`](Self::SMPTE170M); also called
    /// "SMPTE C" even though it uses D65.
    pub const SMPTE240M: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE240M);
    /// SMPTE ST 428-1 (CIE 1931 XYZ).
    pub const SMPTE428: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE428);
    /// SMPTE ST 431-2 (2011) / DCI P3.
    pub const SMPTE431: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE431);
    /// SMPTE ST 432-1 (2010) / P3 D65 / Display P3.
    pub const SMPTE432: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTE432);
    /// The same value as [`SMPTE428`](Self::SMPTE428).
    pub const SMPTEST428_1: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_SMPTEST428_1);
    /// Unspecified.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_UNSPECIFIED);
    /// Panasonic V-Gamut.
    pub const V_GAMUT: Self = Self(ffi::AVColorPrimaries::AVCOL_PRI_V_GAMUT);

    /// The primaries C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVColorPrimaries) -> Self {
        Self(raw)
    }

    /// The C encoding of these primaries.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVColorPrimaries {
        self.0
    }

    /// The name of the constant this value equals, if any; of two constants
    /// with one value, the first C declares.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

/// Wraps: `AVColorRange`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorRange;
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
    /// Full range: `(2^n - 1) * E` for RGB and luma, e.g. 0-255 at 8 bits,
    /// and `(2^n - 1) * E + 2^(n - 1)` for chroma, e.g. 1-255 at 8 bits.
    pub const JPEG: Self = Self(ffi::AVColorRange::AVCOL_RANGE_JPEG);
    /// Narrow or limited range: `(219 * E + 16) * 2^(n-8)` for luma, e.g.
    /// 16-235 at 8 bits, and `(224 * E + 128) * 2^(n-8)` for chroma, e.g.
    /// 16-240 at 8 bits.
    pub const MPEG: Self = Self(ffi::AVColorRange::AVCOL_RANGE_MPEG);
    /// Unspecified; the value of a zero-filled field.
    pub const UNSPECIFIED: Self = Self(ffi::AVColorRange::AVCOL_RANGE_UNSPECIFIED);

    /// The range C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVColorRange) -> Self {
        Self(raw)
    }

    /// The C encoding of this range.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVColorRange {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

/// Wraps: `AVChromaLocation`
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
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ChromaLocation;
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
    /// Position 6.
    pub const BOTTOM: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_BOTTOM);
    /// Position 5.
    pub const BOTTOMLEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_BOTTOMLEFT);
    /// Position 2: MPEG-1 4:2:0, JPEG 4:2:0, H.263 4:2:0.
    pub const CENTER: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_CENTER);
    /// Position 1: MPEG-2/4 4:2:0, the H.264 default for 4:2:0.
    pub const LEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_LEFT);
    /// Position 4.
    pub const TOP: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_TOP);
    /// Position 3: ITU-R 601, SMPTE 274M 296M S314M (DV 4:1:1), MPEG-2 4:2:2.
    pub const TOPLEFT: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_TOPLEFT);
    /// Unknown or undefined (position 0).
    pub const UNSPECIFIED: Self = Self(ffi::AVChromaLocation::AVCHROMA_LOC_UNSPECIFIED);

    /// The chroma location C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVChromaLocation) -> Self {
        Self(raw)
    }

    /// The C encoding of this chroma location.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVChromaLocation {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
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
        match self.constant_name() {
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

define_ctype!(
    /// Wraps: AVComponentDescriptor
    ///
    /// The memory layout of one component of a pixel format: its plane, the
    /// distance between consecutive pixels, its offset in the first pixel,
    /// and its bit position and width. Its getters are on
    /// [`ComponentDescriptorRef`] (`as_ref()`), its setters on
    /// [`ComponentDescriptorMut`] (`as_mut()`).
    ///
    /// No field is range-checked, by C or here: C's own table stores an
    /// offset of `-1` for the big-endian 16-bit packed RGB formats, and its
    /// users may write any `int`.
    #[derive(Clone, Copy)]
    ComponentDescriptor,
    ComponentDescriptorRef,
    ComponentDescriptorMut,
    ffi::AVComponentDescriptor
);

// SAFETY: all-zero is the descriptor C itself stores for every component a
// pixel format does not use (and for every component of a hardware format),
// which its readers skip on `depth == 0`; every accessor takes it as the five
// integers it is, and the type has no teardown to reject it.
unsafe impl CZeroable for ComponentDescriptor {}

// SAFETY: an `AVComponentDescriptor` is 20 bytes of plain data, five `int`s
// with no pointer, sub-allocation or tie to the thread that wrote them, so a
// value, or an exclusive handle to one, moves between threads as an
// `[i32; 5]` does.
unsafe impl Send for ComponentDescriptor {}
// SAFETY: as for `ComponentDescriptor`: the handle is the only access to the
// five fields.
unsafe impl Send for ComponentDescriptorMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `ComponentDescriptorRef` has getters only, and the struct has no interior
// mutability, so concurrent shared access reads unchanging bytes, as through
// `&[i32; 5]`.
unsafe impl Sync for ComponentDescriptor {}
// SAFETY: as for `Sync for ComponentDescriptor`: copies of a shared handle
// only read.
unsafe impl Send for ComponentDescriptorRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for ComponentDescriptorRef<'_> {}
// SAFETY: `&ComponentDescriptorMut` reaches the fields only through `as_ref`,
// a shared handle; the setters need `&mut`.
unsafe impl Sync for ComponentDescriptorMut<'_> {}

impl ComponentDescriptor {
    /// The all-zero descriptor: plane 0, no step, offset or shift, and a
    /// depth of 0, which is how C marks a component a format does not have.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::zeroed()
    }

    /// The descriptor C encodes as `raw`, whatever its fields.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVComponentDescriptor) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this descriptor, for storing it in a C struct by
    /// value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVComponentDescriptor {
        self.0
    }
}

impl Default for ComponentDescriptor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl From<ffi::AVComponentDescriptor> for ComponentDescriptor {
    #[inline]
    fn from(raw: ffi::AVComponentDescriptor) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ComponentDescriptor> for ffi::AVComponentDescriptor {
    #[inline]
    fn from(d: ComponentDescriptor) -> Self {
        d.into_raw()
    }
}

/// Field by field: two descriptors are equal when all five fields are.
impl PartialEq for ComponentDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().fields() == other.as_ref().fields()
    }
}

impl Eq for ComponentDescriptor {}

impl Hash for ComponentDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().fields().hash(state);
    }
}

impl fmt::Debug for ComponentDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for ComponentDescriptorRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentDescriptor")
            .field("plane", &self.plane())
            .field("step", &self.step())
            .field("offset", &self.offset())
            .field("shift", &self.shift())
            .field("depth", &self.depth())
            .finish()
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the conventions' getters and setters take `&self` and `&mut self` on the handles"
)]
impl ComponentDescriptorRef<'_> {
    /// Field: AVComponentDescriptor.plane
    ///
    /// Which of the four planes holds the component.
    #[must_use]
    pub fn plane(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `plane` field is in bounds; nothing is accessed here.
        let plane_ptr = unsafe { addr_of!((*self.as_ptr()).plane) };
        // SAFETY: a read of the `plane` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { plane_ptr.read() }
    }

    /// Field: AVComponentDescriptor.step
    ///
    /// The number of elements between two horizontally consecutive pixels:
    /// bits for bitstream formats, bytes otherwise.
    #[must_use]
    pub fn step(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `step` field is in bounds; nothing is accessed here.
        let step_ptr = unsafe { addr_of!((*self.as_ptr()).step) };
        // SAFETY: a read of the `step` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { step_ptr.read() }
    }

    /// Field: AVComponentDescriptor.offset
    ///
    /// The number of elements before the component of the first pixel: bits
    /// for bitstream formats, bytes otherwise. It may be negative.
    #[must_use]
    pub fn offset(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `offset` field is in bounds; nothing is accessed here.
        let offset_ptr = unsafe { addr_of!((*self.as_ptr()).offset) };
        // SAFETY: a read of the `offset` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { offset_ptr.read() }
    }

    /// Field: AVComponentDescriptor.shift
    ///
    /// The number of least significant bits to shift away to get the value.
    #[must_use]
    pub fn shift(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `shift` field is in bounds; nothing is accessed here.
        let shift_ptr = unsafe { addr_of!((*self.as_ptr()).shift) };
        // SAFETY: a read of the `shift` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { shift_ptr.read() }
    }

    /// Field: AVComponentDescriptor.depth
    ///
    /// The number of bits in the component; 0 for a component the format
    /// does not have.
    #[must_use]
    pub fn depth(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `depth` field is in bounds; nothing is accessed here.
        let depth_ptr = unsafe { addr_of!((*self.as_ptr()).depth) };
        // SAFETY: a read of the `depth` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { depth_ptr.read() }
    }

    /// The five fields in declaration order, for comparison and hashing.
    fn fields(&self) -> [i32; 5] {
        [
            self.plane(),
            self.step(),
            self.offset(),
            self.shift(),
            self.depth(),
        ]
    }

    /// A copy of the whole descriptor, independent of the place it was read
    /// from.
    #[must_use]
    pub fn get(&self) -> ComponentDescriptor {
        // SAFETY: a read of the whole live, initialised
        // `AVComponentDescriptor` through the shared handle's pointer; it is
        // five `int`s, so the byte copy is a complete, independent value.
        ComponentDescriptor::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl ComponentDescriptorMut<'_> {
    /// Field: AVComponentDescriptor.plane
    ///
    /// Sets the plane holding the component, leaving the other fields as
    /// they are.
    pub fn set_plane(&mut self, plane: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `plane` field is in bounds; nothing is accessed here.
        let plane_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).plane) };
        // SAFETY: a write of the `plane` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { plane_ptr.write(plane) }
    }

    /// Field: AVComponentDescriptor.step
    ///
    /// Sets the distance between consecutive pixels, leaving the other
    /// fields as they are.
    pub fn set_step(&mut self, step: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `step` field is in bounds; nothing is accessed here.
        let step_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).step) };
        // SAFETY: a write of the `step` field projected through the exclusive
        // handle's pointer to a live `AVComponentDescriptor`; the old value
        // owns nothing to leak.
        unsafe { step_ptr.write(step) }
    }

    /// Field: AVComponentDescriptor.offset
    ///
    /// Sets the offset of the first pixel's component, leaving the other
    /// fields as they are; a negative offset is stored as given.
    pub fn set_offset(&mut self, offset: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `offset` field is in bounds; nothing is accessed here.
        let offset_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).offset) };
        // SAFETY: a write of the `offset` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { offset_ptr.write(offset) }
    }

    /// Field: AVComponentDescriptor.shift
    ///
    /// Sets the number of low bits to shift away, leaving the other fields
    /// as they are.
    pub fn set_shift(&mut self, shift: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `shift` field is in bounds; nothing is accessed here.
        let shift_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).shift) };
        // SAFETY: a write of the `shift` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { shift_ptr.write(shift) }
    }

    /// Field: AVComponentDescriptor.depth
    ///
    /// Sets the component's width in bits, leaving the other fields as they
    /// are.
    pub fn set_depth(&mut self, depth: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `depth` field is in bounds; nothing is accessed here.
        let depth_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).depth) };
        // SAFETY: a write of the `depth` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { depth_ptr.write(depth) }
    }

    /// Overwrites all five fields with `d`'s.
    pub fn set(&mut self, d: ComponentDescriptor) {
        // SAFETY: a write of the whole `AVComponentDescriptor` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(d.into_raw()) }
    }
}

/// The set of `AV_PIX_FMT_FLAG_*` bits a [`PixelFormatDescriptor`] carries:
/// how the format stores its values (big-endian, palettized, bit-packed,
/// planar, floating-point) and what they are (RGB, XYZ, Bayer, alpha, a
/// hardware surface).
///
/// C keeps them in a `uint64_t`, so any bit pattern is a valid value and the
/// set operations carry unnamed bits along.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PixelFormatFlags(u64);

impl PixelFormatFlags {
    /// The format has a straight (not premultiplied) alpha channel in some
    /// form, palettized formats included.
    pub const ALPHA: Self = Self(ffi::AV_PIX_FMT_FLAG_ALPHA as u64);
    /// The values follow a Bayer pattern.
    pub const BAYER: Self = Self(ffi::AV_PIX_FMT_FLAG_BAYER as u64);
    /// Multi-byte values are big-endian.
    pub const BIG_ENDIAN: Self = Self(ffi::AV_PIX_FMT_FLAG_BE as u64);
    /// A component's values are bit-wise packed end to end.
    pub const BITSTREAM: Self = Self(ffi::AV_PIX_FMT_FLAG_BITSTREAM as u64);
    /// IEEE-754 floating-point values, whose precision follows the pixel
    /// size.
    pub const FLOAT: Self = Self(ffi::AV_PIX_FMT_FLAG_FLOAT as u64);
    /// A hardware-accelerated format, whose planes are not in memory.
    pub const HWACCEL: Self = Self(ffi::AV_PIX_FMT_FLAG_HWACCEL as u64);
    /// Every named flag with its constant's name, in bit order.
    const KNOWN: [(Self, &'static str); 10] = [
        (Self::BIG_ENDIAN, "BIG_ENDIAN"),
        (Self::PALETTE, "PALETTE"),
        (Self::BITSTREAM, "BITSTREAM"),
        (Self::HWACCEL, "HWACCEL"),
        (Self::PLANAR, "PLANAR"),
        (Self::RGB, "RGB"),
        (Self::ALPHA, "ALPHA"),
        (Self::BAYER, "BAYER"),
        (Self::FLOAT, "FLOAT"),
        (Self::XYZ, "XYZ"),
    ];
    /// Plane 1 holds a palette, and plane 0 holds indexes into it.
    pub const PALETTE: Self = Self(ffi::AV_PIX_FMT_FLAG_PAL as u64);
    /// At least one component is not in plane 0.
    pub const PLANAR: Self = Self(ffi::AV_PIX_FMT_FLAG_PLANAR as u64);
    /// RGB-like data, rather than YUV or grayscale.
    pub const RGB: Self = Self(ffi::AV_PIX_FMT_FLAG_RGB as u64);
    /// XYZ-like data, rather than YUV, RGB or grayscale.
    pub const XYZ: Self = Self(ffi::AV_PIX_FMT_FLAG_XYZ as u64);

    /// The set with no flag.
    #[inline]
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    #[must_use]
    pub const fn from_bits_retain(bits: u64) -> Self {
        Self(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Whether no bit is set.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl fmt::Debug for PixelFormatFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PixelFormatFlags(")?;
        let mut rest = self.0;
        let mut first = true;
        for &(flag, name) in &Self::KNOWN {
            if self.contains(flag) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                rest &= !flag.0;
                first = false;
            }
        }
        if rest != 0 || first {
            if !first {
                f.write_str(" | ")?;
            }
            write!(f, "{rest:#x}")?;
        }
        f.write_str(")")
    }
}

impl BitOr for PixelFormatFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PixelFormatFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for PixelFormatFlags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

define_ctype!(
    /// Wraps: AVPixFmtDescriptor
    ///
    /// How a pixel format stores its pixels in up to four planes: its name
    /// and aliases, its component count and layouts, its chroma subsampling
    /// and its [`PixelFormatFlags`]. Its getters are on
    /// [`PixelFormatDescriptorRef`] (`as_ref()`), its setters on
    /// [`PixelFormatDescriptorMut`] (`as_mut()`).
    ///
    /// C's descriptors live in libavutil's static, never-written table; no C
    /// routine allocates, frees or copies one, so a Rust-held descriptor is a
    /// value with no owner, and copying one shares its strings.
    ///
    /// Its `name` and `alias` strings are borrowed for the whole program:
    /// each is NULL or a NUL-terminated string that is never freed or
    /// written, as C's table holds string literals, the setters take only
    /// `&'static CStr`, and [`from_raw`](Self::from_raw) requires it. That
    /// is what lets a getter hand them out as `&'static CStr`, which outlive
    /// the descriptor and can be set into another one.
    ///
    /// No other field is range-checked, by C or here. C's readers of a whole
    /// descriptor (`av_get_bits_per_pixel`, `av_read_image_line2` and their
    /// siblings) index its components by the component count and each
    /// component's plane, and shift by the chroma shifts and depths, so a
    /// wrapper passing a Rust-built descriptor to them checks those first.
    #[derive(Clone, Copy)]
    PixelFormatDescriptor,
    PixelFormatDescriptorRef,
    PixelFormatDescriptorMut,
    ffi::AVPixFmtDescriptor
);

// SAFETY: all-zero is a descriptor C itself stores for every pixel format
// its table leaves out: NULL `name` and `alias`, which the getters read as
// `None` and `av_pix_fmt_desc_next` skips; no component, flag or chroma
// shift. Every accessor takes it, and the type has no teardown to reject it.
unsafe impl CZeroable for PixelFormatDescriptor {}

// SAFETY: a descriptor is integers, four `AVComponentDescriptor` values and
// two pointers to strings that, by the type's invariant, live for the whole
// program and are never written; nothing in it is tied to the thread that
// wrote it, so a value, or an exclusive handle to one, moves between threads
// as a `(u64, &'static CStr)` does.
unsafe impl Send for PixelFormatDescriptor {}
// SAFETY: as for `PixelFormatDescriptor`: the handle is the only access to
// the descriptor's fields, and the strings it reaches are never written.
unsafe impl Send for PixelFormatDescriptorMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `PixelFormatDescriptorRef` has getters only, the struct has no interior
// mutability, and the strings are never written by anyone, so concurrent
// shared access reads unchanging bytes, as through a `&(u64, &'static CStr)`.
unsafe impl Sync for PixelFormatDescriptor {}
// SAFETY: as for `Sync for PixelFormatDescriptor`: copies of a shared handle
// only read.
unsafe impl Send for PixelFormatDescriptorRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for PixelFormatDescriptorRef<'_> {}
// SAFETY: `&PixelFormatDescriptorMut` reaches the fields only through
// `as_ref`, a shared handle; the setters need `&mut`.
unsafe impl Sync for PixelFormatDescriptorMut<'_> {}

impl PixelFormatDescriptor {
    /// The all-zero descriptor: no name, alias, component, flag or chroma
    /// subsampling, as C's table stores for a format it leaves out.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::zeroed()
    }

    /// The descriptor C encodes as `raw`.
    ///
    /// # Safety
    ///
    /// `raw.name` and `raw.alias` must each be NULL or point to a
    /// NUL-terminated string that is never freed or written for the rest of
    /// the program, such as a string literal or a field of a descriptor in
    /// C's own table.
    #[inline]
    #[must_use]
    pub const unsafe fn from_raw(raw: ffi::AVPixFmtDescriptor) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this descriptor, for storing it in a C struct by
    /// value or passing it to a C reader.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVPixFmtDescriptor {
        self.0
    }
}

impl Default for PixelFormatDescriptor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl From<PixelFormatDescriptor> for ffi::AVPixFmtDescriptor {
    #[inline]
    fn from(d: PixelFormatDescriptor) -> Self {
        d.into_raw()
    }
}

/// Field by field: two descriptors are equal when their names and aliases
/// hold the same bytes (or are both absent) and every other field is equal.
impl PartialEq for PixelFormatDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().fields() == other.as_ref().fields()
    }
}

impl Eq for PixelFormatDescriptor {}

impl Hash for PixelFormatDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().fields().hash(state);
    }
}

impl fmt::Debug for PixelFormatDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for PixelFormatDescriptorRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixelFormatDescriptor")
            .field("name", &self.name())
            .field("component_count", &self.component_count())
            .field("log2_chroma_w", &self.log2_chroma_w())
            .field("log2_chroma_h", &self.log2_chroma_h())
            .field("flags", &self.flags())
            .field("components", &self.fields().5)
            .field("alias", &self.alias())
            .finish()
    }
}

/// The fields of a [`PixelFormatDescriptor`] in declaration order, strings
/// by content, for comparison and hashing.
type DescriptorFields = (
    Option<&'static CStr>,
    u8,
    u8,
    u8,
    u64,
    [ComponentDescriptor; 4],
    Option<&'static CStr>,
);

impl<'a> PixelFormatDescriptorRef<'a> {
    /// Field: AVPixFmtDescriptor.name
    ///
    /// The format's name, such as `yuv420p`; `None` for a descriptor without
    /// one, such as the all-zero descriptor C stores for a format its table
    /// leaves out.
    #[must_use]
    pub fn name(&self) -> Option<&'static CStr> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `name` field is in bounds; nothing is accessed here.
        let name_ptr = unsafe { addr_of!((*self.as_ptr()).name) };
        // SAFETY: a read of the `name` pointer projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`.
        let p = unsafe { name_ptr.read() };
        // SAFETY: by the type's invariant, a non-null `name` addresses a
        // NUL-terminated string that is never freed or written for the rest
        // of the program.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// Field: `AVPixFmtDescriptor.nb_components`
    ///
    /// The number of components each pixel has, 1 to 4 in C's table (0 for
    /// a format it leaves out); the first ones of
    /// [`components`](Self::components) describe them.
    #[must_use]
    pub fn component_count(&self) -> u8 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `nb_components` field is in bounds; nothing is accessed here.
        let nb_components_ptr = unsafe { addr_of!((*self.as_ptr()).nb_components) };
        // SAFETY: a read of the `nb_components` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { nb_components_ptr.read() }
    }

    /// Field: `AVPixFmtDescriptor.log2_chroma_w`
    ///
    /// How far to shift the luma width right, rounding up, to get the chroma
    /// width: 1 for `yuv420p`.
    #[must_use]
    pub fn log2_chroma_w(&self) -> u8 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `log2_chroma_w` field is in bounds; nothing is accessed here.
        let log2_chroma_w_ptr = unsafe { addr_of!((*self.as_ptr()).log2_chroma_w) };
        // SAFETY: a read of the `log2_chroma_w` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { log2_chroma_w_ptr.read() }
    }

    /// Field: `AVPixFmtDescriptor.log2_chroma_h`
    ///
    /// How far to shift the luma height right, rounding up, to get the
    /// chroma height: 1 for `yuv420p`.
    #[must_use]
    pub fn log2_chroma_h(&self) -> u8 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `log2_chroma_h` field is in bounds; nothing is accessed here.
        let log2_chroma_h_ptr = unsafe { addr_of!((*self.as_ptr()).log2_chroma_h) };
        // SAFETY: a read of the `log2_chroma_h` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { log2_chroma_h_ptr.read() }
    }

    /// Field: AVPixFmtDescriptor.flags
    ///
    /// The format's [`PixelFormatFlags`], unnamed bits included.
    #[must_use]
    pub fn flags(&self) -> PixelFormatFlags {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `flags` field is in bounds; nothing is accessed here.
        let flags_ptr = unsafe { addr_of!((*self.as_ptr()).flags) };
        // SAFETY: a read of the `flags` field projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`;
        // every bit pattern is a valid `uint64_t`.
        PixelFormatFlags(unsafe { flags_ptr.read() })
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The layouts of the four component slots, borrowed from the
    /// descriptor: luma (or red), then chroma (or green and blue), with
    /// alpha always last; slots past the component count are all-zero in
    /// C's table.
    #[must_use]
    pub fn components(&self) -> CSlice<'a, ComponentDescriptor> {
        // SAFETY: the handle's pointer is to a live object, as the block below relies on, so the
        // place of its `comp` field is in bounds; nothing is accessed here.
        let comp_ptr = unsafe { &raw const (*self.as_ptr()).comp };
        // For the unsafe operations below: `comp` projected through the shared handle's pointer to
        // a live descriptor is four contiguous, initialised
        // `AVComponentDescriptor`s, which `ComponentDescriptor` is
        // `#[repr(transparent)]` over, living as long as the descriptor does,
        // i.e. for `'a`; the projection of a non-null handle pointer is
        // non-null, and a shared run only reads through it.
        {
            let p = comp_ptr.cast::<ComponentDescriptor>();
            // SAFETY: as the comment below states, for this part of the operation.
            let new_unchecked_4 = unsafe { NonNull::new_unchecked(p.cast_mut()) };
            // SAFETY: as the comment above this block states.
            unsafe { CSlice::from_raw_parts(new_unchecked_4, 4) }
        }
    }

    /// Field: AVPixFmtDescriptor.alias
    ///
    /// The format's alternative names, comma-separated, such as `gray8,y8`
    /// for `gray`; `None` for most formats.
    #[must_use]
    pub fn alias(&self) -> Option<&'static CStr> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `alias` field is in bounds; nothing is accessed here.
        let alias_ptr = unsafe { addr_of!((*self.as_ptr()).alias) };
        // SAFETY: a read of the `alias` pointer projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`.
        let p = unsafe { alias_ptr.read() };
        // SAFETY: by the type's invariant, a non-null `alias` addresses a
        // NUL-terminated string that is never freed or written for the rest
        // of the program.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// The fields in declaration order, for comparison, hashing and
    /// formatting.
    fn fields(&self) -> DescriptorFields {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `comp` field is in bounds; nothing is accessed here.
        let comp_ptr = unsafe { addr_of!((*self.as_ptr()).comp) };
        // SAFETY: a read of the `comp` array projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`; it is
        // twenty `int`s, so the copy is a complete, independent value.
        let comp = unsafe { comp_ptr.read() };
        (
            self.name(),
            self.component_count(),
            self.log2_chroma_w(),
            self.log2_chroma_h(),
            self.flags().bits(),
            comp.map(ComponentDescriptor::from_raw),
            self.alias(),
        )
    }

    /// A copy of the whole descriptor, independent of the place it was read
    /// from; its strings are shared, as they live for the whole program.
    #[must_use]
    pub fn get(&self) -> PixelFormatDescriptor {
        // SAFETY: a read of the whole live, initialised `AVPixFmtDescriptor`
        // through the shared handle's pointer; it is plain data, and its two
        // string pointers keep the type's invariant in the copy, since the
        // strings live, unwritten, for the whole program.
        PixelFormatDescriptor(unsafe { self.as_ptr().read() }, core::marker::PhantomData)
    }
}

impl<'a> PixelFormatDescriptorMut<'a> {
    /// Field: AVPixFmtDescriptor.name
    ///
    /// Sets the format's name, or clears it with `None`, leaving the other
    /// fields as they are. The string is borrowed for the whole program, as
    /// C's own names are.
    pub fn set_name(&mut self, name: Option<&'static CStr>) {
        let p = name.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `name` field is in bounds; nothing is accessed here.
        let name_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).name) };
        // SAFETY: a write of the `name` pointer projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; `p` is
        // NULL or a `'static` string, never written, which keeps the type's
        // invariant, and the old value is borrowed, so nothing leaks.
        unsafe { name_ptr.write(p) }
    }

    /// Field: `AVPixFmtDescriptor.nb_components`
    ///
    /// Sets the number of components each pixel has, leaving the other
    /// fields as they are. Any value is stored; C's table only holds 0 to 4.
    pub fn set_component_count(&mut self, count: u8) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `nb_components` field is in bounds; nothing is accessed here.
        let nb_components_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).nb_components) };
        // SAFETY: a write of the `nb_components` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { nb_components_ptr.write(count) }
    }

    /// Field: `AVPixFmtDescriptor.log2_chroma_w`
    ///
    /// Sets the chroma width shift, leaving the other fields as they are.
    pub fn set_log2_chroma_w(&mut self, shift: u8) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `log2_chroma_w` field is in bounds; nothing is accessed here.
        let log2_chroma_w_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).log2_chroma_w) };
        // SAFETY: a write of the `log2_chroma_w` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { log2_chroma_w_ptr.write(shift) }
    }

    /// Field: `AVPixFmtDescriptor.log2_chroma_h`
    ///
    /// Sets the chroma height shift, leaving the other fields as they are.
    pub fn set_log2_chroma_h(&mut self, shift: u8) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `log2_chroma_h` field is in bounds; nothing is accessed here.
        let log2_chroma_h_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).log2_chroma_h) };
        // SAFETY: a write of the `log2_chroma_h` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { log2_chroma_h_ptr.write(shift) }
    }

    /// Field: AVPixFmtDescriptor.flags
    ///
    /// Sets the format's flags, unnamed bits included, leaving the other
    /// fields as they are.
    pub fn set_flags(&mut self, flags: PixelFormatFlags) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `flags` field is in bounds; nothing is accessed here.
        let flags_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).flags) };
        // SAFETY: a write of the `flags` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { flags_ptr.write(flags.0) }
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The four component slots, exclusively borrowed from the descriptor
    /// for writing in place through their handles.
    #[must_use]
    pub fn components_mut(&mut self) -> CSliceMut<'_, ComponentDescriptor> {
        // SAFETY: the handle's pointer is to a live object, as the block below relies on, so the
        // place of its `comp` field is in bounds; nothing is accessed here.
        let comp_ptr = unsafe { &raw mut (*self.as_mut_ptr()).comp };
        // For the unsafe operations below: `comp` projected through the exclusive handle's pointer
        // to a live descriptor is four contiguous, initialised `AVComponentDescriptor`s,
        // which `ComponentDescriptor` is `#[repr(transparent)]` over; the projection of a
        // non-null handle pointer is non-null; the run is bound by `&mut self`, so this
        // handle, the only access to the descriptor, is frozen while it lives.
        {
            let p = comp_ptr.cast::<ComponentDescriptor>();
            // SAFETY: as the comment below states, for this part of the operation.
            let new_unchecked_3 = unsafe { NonNull::new_unchecked(p) };
            // SAFETY: as the comment above this block states.
            unsafe { CSliceMut::from_raw_parts(new_unchecked_3, 4) }
        }
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The four component slots, exclusively borrowed for the handle's whole
    /// lifetime, giving the handle up: the by-value counterpart of
    /// [`components_mut`](Self::components_mut), for a run that outlives a
    /// temporary handle (`d.as_mut().into_components_mut()`).
    #[must_use]
    pub fn into_components_mut(mut self) -> CSliceMut<'a, ComponentDescriptor> {
        // SAFETY: the handle's pointer is to a live object, as the block below relies on, so the
        // place of its `comp` field is in bounds; nothing is accessed here.
        let comp_ptr = unsafe { &raw mut (*self.as_mut_ptr()).comp };
        // For the unsafe operations below: as in `components_mut`; the handle is consumed, so the
        // run is the only access left to the descriptor for `'a`.
        {
            let p = comp_ptr.cast::<ComponentDescriptor>();
            // SAFETY: as the comment below states, for this part of the operation.
            let new_unchecked_2 = unsafe { NonNull::new_unchecked(p) };
            // SAFETY: as the comment above this block states.
            unsafe { CSliceMut::from_raw_parts(new_unchecked_2, 4) }
        }
    }

    /// Field: AVPixFmtDescriptor.alias
    ///
    /// Sets the format's comma-separated alternative names, or clears them
    /// with `None`, leaving the other fields as they are. The string is
    /// borrowed for the whole program, as C's own aliases are.
    pub fn set_alias(&mut self, alias: Option<&'static CStr>) {
        let p = alias.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `alias` field is in bounds; nothing is accessed here.
        let alias_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).alias) };
        // SAFETY: a write of the `alias` pointer projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; `p` is
        // NULL or a `'static` string, never written, which keeps the type's
        // invariant, and the old value is borrowed, so nothing leaks.
        unsafe { alias_ptr.write(p) }
    }

    /// Overwrites every field with `d`'s.
    pub fn set(&mut self, d: PixelFormatDescriptor) {
        // SAFETY: a write of the whole `AVPixFmtDescriptor` through the
        // exclusive handle's pointer to a live one; `d` keeps the type's
        // invariant, and the old value borrows its strings, so nothing leaks.
        unsafe { self.as_mut_ptr().write(d.into_raw()) }
    }
}

/// The value a `*_from_name` lookup returned, or `None` for the
/// `AVERROR(EINVAL)` it returns for an unknown name.
fn found(ret: c_int) -> Option<c_uint> {
    c_uint::try_from(ret).ok()
}

impl ColorRange {
    /// Wraps: `av_color_range_from_name`
    ///
    /// The color range `name` names: `unknown`, `tv` or `pc`; `None` for any
    /// other name. Names are matched exactly, case included.
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only
        // compares during the call.
        let ret = unsafe { ffi::av_color_range_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorRange(raw)))
    }
}

impl ColorPrimaries {
    /// Wraps: `av_color_primaries_from_name`
    ///
    /// The color primaries `name` names, such as `bt709` or `vgamut`; `None`
    /// for any other name. Names are matched exactly, case included; the
    /// `reserved` name shared by two values yields the first,
    /// [`RESERVED0`](Self::RESERVED0).
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_primaries_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorPrimaries(raw)))
    }
}

impl ColorTransfer {
    /// Wraps: `av_color_transfer_from_name`
    ///
    /// The transfer characteristic `name` names, such as `bt709`,
    /// `smpte2084` or `vlog`; `None` for any other name. Names are matched
    /// exactly, case included; the `reserved` name shared by two values
    /// yields the first, [`RESERVED0`](Self::RESERVED0).
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_transfer_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorTransferCharacteristic(raw)))
    }
}

impl ColorSpace {
    /// Wraps: `av_color_space_from_name`
    ///
    /// The color space `name` names, such as `gbr` or `bt709`; `None` for any
    /// other name. Names are matched exactly, case included.
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_space_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorSpace(raw)))
    }
}

impl ChromaLocation {
    /// Wraps: `av_chroma_location_from_name`
    ///
    /// The chroma location `name` names, such as `left` or `center`; `None`
    /// for any other name. Names are matched exactly, case included.
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_chroma_location_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVChromaLocation(raw)))
    }
}

impl PixelFormat {
    /// Wraps: `av_pix_fmt_swap_endianness`
    ///
    /// The same format in the other byte order: [`RGB48LE`](Self::RGB48LE)
    /// for [`RGB48BE`](Self::RGB48BE) and back. `None` for a format with no
    /// byte order (one whose C name does not end in `be` or `le`, such as
    /// [`YUV420P`](Self::YUV420P)), for one whose other-endian twin C does
    /// not list, and for [`NONE`](Self::NONE) and every value C does not
    /// name.
    #[must_use]
    pub fn swap_endianness(self) -> Option<Self> {
        // SAFETY: C bounds-checks the value against its descriptor table
        // (any `int` is accepted) and only reads the table and a local copy
        // of the name.
        let raw = unsafe { ffi::av_pix_fmt_swap_endianness(self.into_raw()) };
        let swapped = Self::from_raw(raw);
        (swapped != Self::NONE).then_some(swapped)
    }
}

/// The string a `*_name` getter returned, or `None` for its NULL.
///
/// # Safety
///
/// `name` is NULL or the address of a NUL-terminated string that stays
/// allocated and unwritten for the rest of the process, as C's static name
/// tables are.
unsafe fn static_name(name: *const c_char) -> Option<&'static CStr> {
    // SAFETY: the caller guarantees a non-NULL `name` is a NUL-terminated
    // string that is never freed or written, so the `'static` borrow cannot
    // dangle or observe a change.
    (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
}

impl ColorRange {
    /// Wraps: `av_color_range_name`
    ///
    /// The name C gives this range, `unknown`, `tv` or `pc`, which
    /// [`from_name`](Self::from_name) maps back; `None` for any value C does
    /// not name.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ColorRange;
    /// assert_eq!(ColorRange::JPEG.name(), Some(c"pc"));
    /// assert_eq!(ColorRange::from_name(c"tv"), Some(ColorRange::MPEG));
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let color_range_name = unsafe { ffi::av_color_range_name(self.into_raw()) };
        // SAFETY: `av_color_range_name` takes the enum by value, accepts any
        // value, and returns NULL or a string literal from its static table.
        unsafe { static_name(color_range_name) }
    }
}

impl ColorPrimaries {
    /// Wraps: `av_color_primaries_name`
    ///
    /// The name C gives these primaries, such as `bt709` or `vgamut`; `None`
    /// for the values C leaves unnamed (13 to 21, and any other it does not
    /// know). [`RESERVED0`](Self::RESERVED0) and [`RESERVED`](Self::RESERVED)
    /// are both `reserved`, which [`from_name`](Self::from_name) maps back to
    /// the first.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let color_primaries_name = unsafe { ffi::av_color_primaries_name(self.into_raw()) };
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(color_primaries_name) }
    }
}

impl ColorTransfer {
    /// Wraps: `av_color_transfer_name`
    ///
    /// The name C gives this transfer characteristic, such as `bt709`,
    /// `smpte2084` or `vlog`; `None` for any value C does not name.
    /// [`RESERVED0`](Self::RESERVED0) and [`RESERVED`](Self::RESERVED) are
    /// both `reserved`, which [`from_name`](Self::from_name) maps back to the
    /// first.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let color_transfer_name = unsafe { ffi::av_color_transfer_name(self.into_raw()) };
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(color_transfer_name) }
    }
}

impl ColorSpace {
    /// Wraps: `av_color_space_name`
    ///
    /// The name C gives this color space, such as `gbr` or `bt709`; `None`
    /// for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let color_space_name = unsafe { ffi::av_color_space_name(self.into_raw()) };
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(color_space_name) }
    }
}

impl ChromaLocation {
    /// Wraps: `av_chroma_location_name`
    ///
    /// The name C gives this chroma location, such as `unspecified`, `left`
    /// or `topleft`; `None` for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let chroma_location_name = unsafe { ffi::av_chroma_location_name(self.into_raw()) };
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(chroma_location_name) }
    }

    /// Wraps: `av_chroma_location_enum_to_pos`
    ///
    /// Where this location puts the first chroma sample, as the `(x, y)`
    /// position swscale takes: luma sample `(0, 0)` is the origin and luma
    /// sample `(1, 1)` is `(256, 256)`, so each coordinate is 0, 128 or 256.
    /// `None` for [`UNSPECIFIED`](Self::UNSPECIFIED) and any value C does not
    /// name.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::ChromaLocation;
    /// assert_eq!(ChromaLocation::LEFT.position(), Some((0, 128)));
    /// assert_eq!(
    ///     ChromaLocation::from_position(0, 128),
    ///     Some(ChromaLocation::LEFT)
    /// );
    /// assert_eq!(ChromaLocation::UNSPECIFIED.position(), None);
    /// ```
    #[must_use]
    pub fn position(self) -> Option<(i32, i32)> {
        let (mut x, mut y) = (0, 0);
        // SAFETY: `x` and `y` are live, writable `int`s, which C writes only
        // when it returns 0 and does not keep.
        let ret =
            unsafe { ffi::av_chroma_location_enum_to_pos(&raw mut x, &raw mut y, self.into_raw()) };
        (ret == 0).then_some((x, y))
    }

    /// Wraps: `av_chroma_location_pos_to_enum`
    ///
    /// The location whose [`position`](Self::position) is `(x, y)`, or
    /// `None` when no location puts the first chroma sample there, which C
    /// reports as [`UNSPECIFIED`](Self::UNSPECIFIED).
    #[must_use]
    pub fn from_position(x: i32, y: i32) -> Option<Self> {
        // SAFETY: `av_chroma_location_pos_to_enum` takes two integers by
        // value and touches no caller memory.
        let raw = unsafe { ffi::av_chroma_location_pos_to_enum(x, y) };
        Some(Self::from_raw(raw)).filter(|&loc| loc != Self::UNSPECIFIED)
    }
}

impl AlphaMode {
    /// Wraps: `av_alpha_mode_name`
    ///
    /// The name C gives this alpha mode, `unspecified`, `premultiplied` or
    /// `straight`; `None` for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let alpha_mode_name = unsafe { ffi::av_alpha_mode_name(self.into_raw()) };
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(alpha_mode_name) }
    }

    /// Wraps: `av_alpha_mode_from_name`
    ///
    /// The alpha mode `name` names: `unspecified`, `premultiplied` or
    /// `straight`; `None` for any other name. Names are matched exactly,
    /// case included.
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_alpha_mode_from_name(name.as_ptr()) };
        // C returns its `AVERROR(EINVAL)` through the unsigned enum: read
        // the bits back as the `int` they were.
        found(ret.0.cast_signed()).map(|raw| Self::from_raw(ffi::AVAlphaMode(raw)))
    }
}

/// The kinds of information a pixel format conversion loses (`FF_LOSS_*`),
/// as [`PixelFormat::conversion_loss`] reports them and
/// [`PixelFormat::best_of_two_with_loss`] takes the ones to ignore.
///
/// C keeps them in an `int`, so any bit pattern is a valid value and the set
/// operations carry unnamed bits along.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Loss(c_int);

impl Loss {
    /// Loss of the alpha channel.
    pub const ALPHA: Self = Self(ffi::FF_LOSS_ALPHA.cast_signed());
    /// Loss of chroma altogether, such as RGB to gray.
    pub const CHROMA: Self = Self(ffi::FF_LOSS_CHROMA.cast_signed());
    /// Quantization to a palette.
    pub const COLORQUANT: Self = Self(ffi::FF_LOSS_COLORQUANT.cast_signed());
    /// A conversion between color spaces (RGB, YUV, gray, ...).
    pub const COLORSPACE: Self = Self(ffi::FF_LOSS_COLORSPACE.cast_signed());
    /// Loss of bits per component.
    pub const DEPTH: Self = Self(ffi::FF_LOSS_DEPTH.cast_signed());
    /// Bits per component the destination has to spare.
    pub const EXCESS_DEPTH: Self = Self(ffi::FF_LOSS_EXCESS_DEPTH.cast_signed());
    /// Chroma resolution the destination has to spare: it subsamples less.
    pub const EXCESS_RESOLUTION: Self = Self(ffi::FF_LOSS_EXCESS_RESOLUTION.cast_signed());
    /// Every named kind with its constant's name, in bit order.
    const KNOWN: [(Self, &'static str); 8] = [
        (Self::RESOLUTION, "RESOLUTION"),
        (Self::DEPTH, "DEPTH"),
        (Self::COLORSPACE, "COLORSPACE"),
        (Self::ALPHA, "ALPHA"),
        (Self::COLORQUANT, "COLORQUANT"),
        (Self::CHROMA, "CHROMA"),
        (Self::EXCESS_RESOLUTION, "EXCESS_RESOLUTION"),
        (Self::EXCESS_DEPTH, "EXCESS_DEPTH"),
    ];
    /// Loss of chroma resolution: the destination subsamples more.
    pub const RESOLUTION: Self = Self(ffi::FF_LOSS_RESOLUTION.cast_signed());

    /// The set with no kind: a lossless conversion.
    #[inline]
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    #[must_use]
    pub const fn from_bits_retain(bits: c_int) -> Self {
        Self(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> c_int {
        self.0
    }

    /// Whether no bit is set.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl fmt::Debug for Loss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Loss(")?;
        let mut rest = self.0;
        let mut first = true;
        for &(kind, name) in &Self::KNOWN {
            if self.contains(kind) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                rest &= !kind.0;
                first = false;
            }
        }
        if rest != 0 || first {
            if !first {
                f.write_str(" | ")?;
            }
            write!(f, "{rest:#x}")?;
        }
        f.write_str(")")
    }
}

impl BitOr for Loss {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Loss {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Loss {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// What `av_get_pix_fmt_loss` returned: the loss set, or its negative
/// error code.
fn loss_or_error(ret: c_int) -> Result<Loss, i32> {
    if ret < 0 { Err(ret) } else { Ok(Loss(ret)) }
}

impl PixelFormat {
    /// Wraps: `av_get_pix_fmt_name`
    ///
    /// The short name C gives this pixel format, such as `yuv420p` or
    /// `rgb48le`, which [`from_name`](Self::from_name) maps back; `None` for
    /// [`NONE`](Self::NONE) and any value C does not know.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// assert_eq!(PixelFormat::YUV420P.name(), Some(c"yuv420p"));
    /// assert_eq!(PixelFormat::NONE.name(), None);
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as the comment below states, for this part of the operation.
        let get_pix_fmt_name = unsafe { ffi::av_get_pix_fmt_name(self.into_raw()) };
        // SAFETY: `av_get_pix_fmt_name` takes the enum by value, accepts any
        // value, and returns NULL or the `name` of a descriptor in C's static
        // table, a string literal.
        unsafe { static_name(get_pix_fmt_name) }
    }

    /// Wraps: `av_get_pix_fmt`
    ///
    /// The pixel format `name` names: a descriptor's name or one of its
    /// aliases, `rgb32` and `bgr32` for the native-endian packed 32-bit RGB
    /// formats, or, failing those, `name` with the target's endianness
    /// suffix (`gray16` is [`GRAY16LE`](Self::GRAY16LE) on a little-endian
    /// target). `None` for any other name; names are matched exactly, case
    /// included.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// assert_eq!(
    ///     PixelFormat::from_name(c"yuv420p"),
    ///     Some(PixelFormat::YUV420P)
    /// );
    /// assert_eq!(PixelFormat::from_name(c"gray16"), Some(PixelFormat::GRAY16));
    /// assert_eq!(PixelFormat::from_name(c"no such format"), None);
    /// ```
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only
        // compares, and copies into a bounded local with `snprintf`, during
        // the call.
        let raw = unsafe { ffi::av_get_pix_fmt(name.as_ptr()) };
        Some(Self::from_raw(raw)).filter(|&f| f != Self::NONE)
    }

    /// Wraps: `av_get_pix_fmt_loss`
    ///
    /// The kinds of information converting from this pixel format to `dst`
    /// loses; with `has_alpha` false, losing the alpha channel does not
    /// count. A conversion to the same format loses nothing.
    ///
    /// # Errors
    ///
    /// C's negative code when no loss can be computed: `-1` for the same
    /// hardware format on both sides, `-2` for a hardware format on either
    /// side otherwise, `-3` for a format with no components, `-4` for a
    /// value C does not know.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::{Loss, PixelFormat};
    /// let loss = PixelFormat::RGB24
    ///     .conversion_loss(PixelFormat::GRAY8, false)
    ///     .unwrap();
    /// assert!(loss.contains(Loss::CHROMA | Loss::COLORSPACE));
    /// assert_eq!(
    ///     PixelFormat::NONE.conversion_loss(PixelFormat::GRAY8, false),
    ///     Err(-4)
    /// );
    /// ```
    pub fn conversion_loss(self, dst: PixelFormat, has_alpha: bool) -> Result<Loss, i32> {
        // SAFETY: `av_get_pix_fmt_loss` takes three integers by value,
        // looks each format up with a bounds check, and touches no caller
        // memory.
        let ret = unsafe {
            ffi::av_get_pix_fmt_loss(dst.into_raw(), self.into_raw(), c_int::from(has_alpha))
        };
        loss_or_error(ret)
    }

    /// Wraps: `av_find_best_pix_fmt_of_2`
    ///
    /// Which of `dst1` and `dst2` this pixel format converts to with the
    /// least loss, counting the loss of alpha only when `has_alpha` is set.
    /// Ties go to the format with fewer bits per pixel, then to the one with
    /// fewer components, then to `dst1`. A candidate C does not know loses
    /// to the other without scoring; when `dst1` is unknown, `dst2` is the
    /// answer even if it is unknown too.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let best = PixelFormat::RGBA.best_of_two(PixelFormat::RGB24, PixelFormat::BGRA, true);
    /// assert_eq!(best, PixelFormat::BGRA);
    /// ```
    #[must_use]
    pub fn best_of_two(self, dst1: PixelFormat, dst2: PixelFormat, has_alpha: bool) -> PixelFormat {
        // SAFETY: `av_find_best_pix_fmt_of_2` takes the formats and flag by
        // value and skips the loss word when its pointer is NULL.
        let raw = unsafe {
            ffi::av_find_best_pix_fmt_of_2(
                dst1.into_raw(),
                dst2.into_raw(),
                self.into_raw(),
                c_int::from(has_alpha),
                ptr::null_mut(),
            )
        };
        Self::from_raw(raw)
    }

    /// Wraps: `av_find_best_pix_fmt_of_2`
    ///
    /// As [`best_of_two`](Self::best_of_two), scoring only the kinds of loss
    /// not in `ignore`, and returning the chosen format with the loss of
    /// converting to it, as [`conversion_loss`](Self::conversion_loss)
    /// reports it: every kind counts there, alpha only with `has_alpha`.
    /// `ignore` plays no part when either candidate is unknown.
    pub fn best_of_two_with_loss(
        self,
        dst1: PixelFormat,
        dst2: PixelFormat,
        has_alpha: bool,
        ignore: Loss,
    ) -> (PixelFormat, Result<Loss, i32>) {
        let mut loss = ignore.bits();
        // SAFETY: the formats and flag are passed by value; `loss` is a
        // live, writable `int` that C reads and overwrites during the call
        // and does not keep.
        let raw = unsafe {
            ffi::av_find_best_pix_fmt_of_2(
                dst1.into_raw(),
                dst2.into_raw(),
                self.into_raw(),
                c_int::from(has_alpha),
                &raw mut loss,
            )
        };
        (Self::from_raw(raw), loss_or_error(loss))
    }
}

impl PixelFormat {
    /// Wraps: `av_get_pix_fmt_string`
    ///
    /// Writes a line describing this pixel format into `buf` and returns it,
    /// borrowed from `buf`: its name left-aligned in 11 columns, then its
    /// number of components and bits per pixel right-aligned in 7 and 10
    /// (`"yuv420p           3         12"`), or, for [`NONE`](Self::NONE)
    /// and every other negative value, the header
    /// `"name nb_components nb_bits"` of a table of such lines.
    ///
    /// The text is truncated to fit `buf`, NUL included. `None` for an empty
    /// `buf`, into which C writes nothing, and for a value past the last
    /// format these bindings know (`AV_PIX_FMT_NB`), for which C would read
    /// past its descriptor table.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let mut buf = [0u8; 64];
    /// assert_eq!(
    ///     PixelFormat::YUV420P.describe(&mut buf),
    ///     Some(c"yuv420p           3         12")
    /// );
    /// assert_eq!(
    ///     PixelFormat::NONE.describe(&mut buf),
    ///     Some(c"name nb_components nb_bits")
    /// );
    /// ```
    pub fn describe(self, buf: &mut [u8]) -> Option<&CStr> {
        if buf.is_empty() || self.into_raw().0 >= ffi::AVPixelFormat::AV_PIX_FMT_NB.0 {
            // C's `snprintf` writes nothing into an empty buffer, and C
            // indexes its table without a bound check.
            return None;
        }
        // A smaller bound than `buf` holds is still a sound one.
        let size = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: `buf` is `size` or more writable bytes, the bound C gives
        // `snprintf`; the value is negative (the header) or below
        // `AV_PIX_FMT_NB`, so it indexes C's table, every entry of which
        // names its format. The returned pointer is `buf`, which C does not
        // retain.
        unsafe { ffi::av_get_pix_fmt_string(buf.as_mut_ptr().cast(), size, self.into_raw()) };
        // `snprintf` NUL-terminated its output within `buf`.
        CStr::from_bytes_until_nul(buf).ok()
    }

    /// Wraps: `av_pix_fmt_count_planes`
    ///
    /// The number of planes this pixel format stores its components in: 1
    /// for packed formats, up to 4 for planar ones, 0 for hardware formats.
    /// `None` for [`NONE`](Self::NONE) and any value without a descriptor.
    ///
    /// A palettized format ([`PAL8`](Self::PAL8)) counts its one plane of
    /// indices, not the palette C keeps beside it.
    #[must_use]
    pub fn plane_count(self) -> Option<usize> {
        // SAFETY: `av_pix_fmt_count_planes` takes the enum by value and
        // looks it up through `av_pix_fmt_desc_get`, which range-checks it.
        let ret = unsafe { ffi::av_pix_fmt_count_planes(self.into_raw()) };
        usize::try_from(ret).ok()
    }

    /// Wraps: `av_pix_fmt_get_chroma_sub_sample`
    ///
    /// The chroma subsampling of this pixel format, as base-2 logarithms:
    /// `(horizontal, vertical)`, the amounts to right-shift (rounding up) a
    /// luma width and height by to get the chroma ones. `(1, 1)` for
    /// [`YUV420P`](Self::YUV420P), `(0, 0)` for formats without subsampled
    /// chroma. `None` for [`NONE`](Self::NONE) and any value without a
    /// descriptor.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the shifts are C's `log2_chroma_*` values, 0 to 3"
    )]
    pub fn chroma_subsampling(self) -> Option<(u8, u8)> {
        let (mut h_shift, mut v_shift): (c_int, c_int) = (0, 0);
        // SAFETY: both out-parameters are live, writable `int`s C writes
        // only during the call; the enum is range-checked by C.
        let ret = unsafe {
            ffi::av_pix_fmt_get_chroma_sub_sample(
                self.into_raw(),
                &raw mut h_shift,
                &raw mut v_shift,
            )
        };
        if ret < 0 {
            return None;
        }
        // C copies the descriptor's `uint8_t` fields.
        Some((h_shift as u8, v_shift as u8))
    }
}

impl PixelFormat {
    /// Wraps: `av_pix_fmt_desc_get`
    ///
    /// This pixel format's descriptor in C's table, borrowed for the whole
    /// program: the table is static and never written. `None` for
    /// [`NONE`](Self::NONE) and every other negative value, and for every
    /// value from the last format these bindings know (`AV_PIX_FMT_NB`) on.
    ///
    /// A value inside that range always has an entry, but one C's table
    /// leaves out is all-zero: it has no [`name`](PixelFormatDescriptorRef::name)
    /// and no component.
    ///
    /// The handle is shared, as C's table is never written; a descriptor to
    /// edit is a copy, [`get`](PixelFormatDescriptorRef::get):
    ///
    /// ```compile_fail,E0599
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let mut desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// desc.set_component_count(0);
    /// ```
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// assert_eq!(desc.name(), Some(c"yuv420p"));
    /// assert_eq!(desc.pixel_format(), Some(PixelFormat::YUV420P));
    /// assert!(PixelFormat::NONE.descriptor().is_none());
    /// ```
    #[must_use]
    pub fn descriptor(self) -> Option<PixelFormatDescriptorRef<'static>> {
        // For the unsafe operations below: `av_pix_fmt_desc_get` takes the enum by value, checks
        // its range, and returns NULL or the address of an entry of C's static
        // `const` table, which lives, unwritten, for the whole program and
        // whose `name` and `alias` are NULL or string literals, the type's
        // invariant; a shared `'static` handle only reads it.
        {
            // SAFETY: as the comment above this block states.
            let p = unsafe { ffi::av_pix_fmt_desc_get(self.into_raw()) };
            // SAFETY: as the comment above this block states.
            unsafe { PixelFormatDescriptorRef::from_ptr(p.cast_mut()) }
        }
    }
}

impl PixelFormatDescriptorRef<'_> {
    /// Wraps: `av_pix_fmt_desc_get_id`
    ///
    /// The pixel format whose entry in C's table this handle addresses;
    /// `None` for a descriptor anywhere else, such as a copy of an entry
    /// held in Rust. C compares the address only, never reading through it.
    #[must_use]
    pub fn pixel_format(&self) -> Option<PixelFormat> {
        // SAFETY: C compares `desc` against the bounds of its static table
        // and subtracts the table's base from it only when it lies inside,
        // where a handle to an entry points at an entry's start; it never
        // dereferences `desc`.
        let raw = unsafe { ffi::av_pix_fmt_desc_get_id(self.as_ptr()) };
        Some(PixelFormat::from_raw(raw)).filter(|&f| f != PixelFormat::NONE)
    }

    /// Wraps: `av_pix_fmt_desc_next`
    ///
    /// The next named descriptor in C's table after the entry this handle
    /// addresses, skipping the entries the table leaves out; `None` past the
    /// last one, and for a descriptor that is not an entry of C's table.
    #[must_use]
    pub fn next_in_table(&self) -> Option<PixelFormatDescriptorRef<'static>> {
        next_descriptor(Some(*self))
    }
}

/// The named C table entry after `prev`, or the first entry for `None`, as a
/// `'static` handle; `None` past the last one, and after a descriptor that
/// is not an entry of C's table.
fn next_descriptor(
    prev: Option<PixelFormatDescriptorRef<'_>>,
) -> Option<PixelFormatDescriptorRef<'static>> {
    let prev = match prev {
        // C walks forward from `prev` inside its own table, so only one of
        // its entries may be passed.
        Some(d) => {
            d.pixel_format()?;
            d.as_ptr()
        }
        None => ptr::null(),
    };
    // SAFETY: as the comment below states, for this part of the operation.
    let pix_fmt_desc_next = unsafe { ffi::av_pix_fmt_desc_next(prev) };
    // SAFETY: `prev` is NULL, from which C starts at its first entry, or an
    // entry of C's static table (`pixel_format` found its address there),
    // from which C steps through the following entries. C returns NULL or
    // the address of one of them, which lives, unwritten, for the whole
    // program with `name` and `alias` NULL or literals, the type's
    // invariant; a shared `'static` handle only reads it.
    unsafe { PixelFormatDescriptorRef::from_ptr(pix_fmt_desc_next.cast_mut()) }
}

/// Wraps: `av_pix_fmt_desc_next`
///
/// Every named pixel format descriptor in C's table, in the table's order:
/// the order of [`PixelFormat`]'s values, skipping the ones the table leaves
/// out. Each is borrowed for the whole program.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::{PixelFormat, descriptors};
/// let first = descriptors().next().unwrap();
/// assert_eq!(first.pixel_format(), Some(PixelFormat::YUV420P));
/// assert!(descriptors().any(|d| d.name() == Some(c"rgb24")));
/// ```
#[must_use]
pub fn descriptors() -> Descriptors {
    Descriptors {
        prev: None,
        done: false,
    }
}

/// The iterator over C's pixel format descriptors that [`descriptors`]
/// returns.
#[derive(Clone, Copy, Debug)]
pub struct Descriptors {
    /// The descriptor returned last, `None` before the first.
    prev: Option<PixelFormatDescriptorRef<'static>>,
    /// Whether C reported the end of its table.
    done: bool,
}

#[expect(
    clippy::copy_iterator,
    reason = "a copied iterator restarts where the copy was taken, as a C cursor does"
)]
impl Iterator for Descriptors {
    type Item = PixelFormatDescriptorRef<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let next = next_descriptor(self.prev);
        self.done = next.is_none();
        self.prev = next;
        next
    }
}

impl core::iter::FusedIterator for Descriptors {}

/// Why a whole-descriptor reader refused a descriptor: a field C would read
/// out of bounds with, or whose arithmetic would overflow `int` or shift by
/// more than its width. C's own table never has one; only a descriptor
/// built or edited in Rust can.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidDescriptor;

impl fmt::Display for InvalidDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("pixel format descriptor outside C's arithmetic range")
    }
}

impl std::error::Error for InvalidDescriptor {}

/// `(value << shift)` as C computes it on an `int`, when that is defined: a
/// non-negative value, a shift below 32 and a result in range.
fn c_shl(value: c_int, shift: c_int) -> Option<c_int> {
    if value < 0 || !(0..32).contains(&shift) {
        return None;
    }
    c_int::try_from(i64::from(value) << shift).ok()
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the conventions' getters and setters take `&self` and `&mut self` on the handles"
)]
impl PixelFormatDescriptorRef<'_> {
    /// The number of components C's bit counters read, and the sum of the
    /// chroma shifts they scale by, when C can index its four component
    /// slots with the one and shift an `int` by the other.
    fn counted_components(&self) -> Result<(usize, c_int), InvalidDescriptor> {
        let count = usize::from(self.component_count());
        let log2_pixels = c_int::from(self.log2_chroma_w()) + c_int::from(self.log2_chroma_h());
        if count > 4 || log2_pixels >= 32 {
            return Err(InvalidDescriptor);
        }
        Ok((count, log2_pixels))
    }

    /// `av_get_bits_per_pixel`'s arithmetic, refused where C's is undefined.
    fn checked_bits_per_pixel(&self) -> Result<c_int, InvalidDescriptor> {
        let (count, log2_pixels) = self.counted_components()?;
        let mut bits: c_int = 0;
        for (c, comp) in self.components().iter().take(count).enumerate() {
            let s = if c == 1 || c == 2 { 0 } else { log2_pixels };
            let scaled = c_shl(comp.depth(), s).ok_or(InvalidDescriptor)?;
            bits = bits.checked_add(scaled).ok_or(InvalidDescriptor)?;
        }
        Ok(bits >> log2_pixels)
    }

    /// `av_get_padded_bits_per_pixel`'s arithmetic, refused where C's is
    /// undefined.
    fn checked_padded_bits_per_pixel(&self) -> Result<c_int, InvalidDescriptor> {
        let (count, log2_pixels) = self.counted_components()?;
        let mut steps: [c_int; 4] = [0; 4];
        for (c, comp) in self.components().iter().take(count).enumerate() {
            let s = if c == 1 || c == 2 { 0 } else { log2_pixels };
            // C stores into its own `int steps[4]` by plane.
            let plane = usize::try_from(comp.plane())
                .ok()
                .filter(|&p| p < 4)
                .ok_or(InvalidDescriptor)?;
            steps[plane] = c_shl(comp.step(), s).ok_or(InvalidDescriptor)?;
        }
        let mut bits = steps
            .iter()
            .try_fold(0, |sum: c_int, &s| sum.checked_add(s))
            .ok_or(InvalidDescriptor)?;
        if !self.flags().contains(PixelFormatFlags::BITSTREAM) {
            bits = bits.checked_mul(8).ok_or(InvalidDescriptor)?;
        }
        Ok(bits >> log2_pixels)
    }

    /// Wraps: `av_get_bits_per_pixel`
    ///
    /// The number of bits that represent a pixel, averaged over the chroma
    /// subsampling and excluding padding: 12 for
    /// [`YUV420P`](PixelFormat::YUV420P), 24 for
    /// [`RGB24`](PixelFormat::RGB24) and [`RGB0`](PixelFormat::RGB0).
    ///
    /// # Errors
    ///
    /// [`InvalidDescriptor`] for a descriptor with more than four
    /// components, chroma shifts summing to 32 or more, or a component depth
    /// that is negative or overflows C's `int` sum, on which C would read
    /// past its component slots or overflow. C's own table has none.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// assert_eq!(desc.bits_per_pixel(), Ok(12));
    /// ```
    pub fn bits_per_pixel(&self) -> Result<u32, InvalidDescriptor> {
        self.checked_bits_per_pixel()?;
        // SAFETY: the handle addresses a live, initialised descriptor, which
        // C only reads during the call; `checked_bits_per_pixel` found C's
        // component count, shifts and sums in range for it.
        let bits = unsafe { ffi::av_get_bits_per_pixel(self.as_ptr()) };
        // Non-negative: a sum of non-negative terms, shifted right.
        Ok(bits.cast_unsigned())
    }

    /// Wraps: `av_get_padded_bits_per_pixel`
    ///
    /// The number of bits a pixel occupies, padding and unused bits
    /// included, averaged over the chroma subsampling: 12 for
    /// [`YUV420P`](PixelFormat::YUV420P), 32 for [`RGB0`](PixelFormat::RGB0).
    /// C counts the largest step of each plane among the used components
    /// (in bytes, or bits for a [bitstream](PixelFormatFlags::BITSTREAM)
    /// format).
    ///
    /// # Errors
    ///
    /// [`InvalidDescriptor`] for a descriptor with more than four
    /// components, chroma shifts summing to 32 or more, a used component in
    /// a plane outside 0 to 3 (C would write past its four-plane array), or a
    /// step that is negative or overflows C's `int` sum. C's own table has
    /// none.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
    /// let desc = PixelFormat::RGB0.descriptor().unwrap();
    /// assert_eq!(desc.padded_bits_per_pixel(), Ok(32));
    /// ```
    pub fn padded_bits_per_pixel(&self) -> Result<u32, InvalidDescriptor> {
        self.checked_padded_bits_per_pixel()?;
        // SAFETY: the handle addresses a live, initialised descriptor, which
        // C only reads during the call; `checked_padded_bits_per_pixel` found
        // the used components' planes inside C's `steps[4]` and its shifts
        // and sums in range.
        let bits = unsafe { ffi::av_get_padded_bits_per_pixel(self.as_ptr()) };
        // Non-negative: a sum of non-negative terms, shifted right.
        Ok(bits.cast_unsigned())
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
}

/// An element of the lines [`read_image_line`] fills and
/// [`write_image_line`] reads: `u16` or `u32`, the two sizes C takes.
///
/// Sealed: C knows no other element size, so a line of any other element
/// does not compile:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::Planes;
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::read_image_line;
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;
/// let desc = PixelFormat::GRAY8.descriptor().unwrap();
/// let mut bytes = [0u8; 4];
/// let _ = read_image_line(&mut bytes, Planes::default(), desc, 0, 0, 0, false);
/// ```
pub trait LineElement: sealed::Sealed + Copy {
    /// The element size C is told, in bytes.
    const SIZE: c_int;
}

impl LineElement for u16 {
    const SIZE: c_int = 2;
}

impl LineElement for u32 {
    const SIZE: c_int = 4;
}

/// Why [`read_image_line`] or [`write_image_line`] refused a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageLineError {
    /// The component is not one of the descriptor's four slots.
    Component,
    /// A position, linesize or line length beyond `i32`, or C's `int`
    /// arithmetic on them overflowing.
    OutOfRange,
    /// The component's layout makes C shift by more than a value's width or
    /// overflow `int`. C's own table has no such component.
    InvalidDescriptor,
    /// Part of the line lies outside its plane's slice.
    OutsidePlane,
    /// Plane 1 is too short for every palette entry the read may index.
    OutsidePalette,
}

impl fmt::Display for ImageLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Component => "component index past the descriptor's four slots",
            Self::OutOfRange => "line position, linesize or length out of range",
            Self::InvalidDescriptor => "component layout outside C's arithmetic range",
            Self::OutsidePlane => "line lies outside its plane",
            Self::OutsidePalette => "palette shorter than the entries the line may index",
        })
    }
}

impl std::error::Error for ImageLineError {}

/// Whether a line is read, with or without palette lookups, or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineAccess {
    Read { palette: bool },
    Write,
}

/// The bytes C touches for one line: the byte offsets `span` from row 0 of
/// the component's plane, and, for palette lookups, the bytes of plane 1
/// from its row 0 the lookups may index.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinePlan {
    plane: usize,
    span: Range<i64>,
    palette: Option<u64>,
}

/// What `av_read_image_line2` or `av_write_image_line2` touches when it
/// moves `w > 0` values of component `c`, laid out as `comp` (`[plane,
/// step, offset, shift, depth]`), from pixel `x` of row `y`, mirroring C's
/// `int` arithmetic and refusing where it is undefined; with the plane's
/// linesize as C takes it, or `None` when C returns before touching the
/// image.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a component depth is at most 32 bits; mirrors C's computation over a component of at most 8 bytes"
)]
#[expect(
    clippy::many_single_char_names,
    reason = "the names follow the C variables they mirror"
)]
fn line_plan(
    [plane, step, offset, shift, depth]: [c_int; 5],
    flags: u64,
    (x, y, c, w): (c_int, c_int, c_int, c_int),
    linesizes: &[isize; 4],
    access: LineAccess,
) -> Result<Option<(LinePlan, c_int)>, ImageLineError> {
    use ImageLineError::{InvalidDescriptor as Invalid, OutOfRange};
    let reading = access != LineAccess::Write;
    // The reader computes `(1ULL << depth) - 1` before anything else.
    if reading && !(0..64).contains(&depth) {
        return Err(Invalid);
    }
    if depth == 0 {
        return Ok(None);
    }
    // C indexes `data[4]` and `linesize[4]` by the plane.
    let plane = usize::try_from(plane)
        .ok()
        .filter(|&p| p < 4)
        .ok_or(Invalid)?;
    let linesize = c_int::try_from(linesizes[plane]).map_err(|_| OutOfRange)?;
    let row = i64::from(y.checked_mul(linesize).ok_or(OutOfRange)?);
    // The mask the reader applies, as C's `unsigned`.
    let mask = if reading {
        ((1u64 << depth) - 1) as u32
    } else {
        0
    };
    let bitstream = flags & u64::from(ffi::AV_PIX_FMT_FLAG_BITSTREAM) != 0;
    let big_endian = flags & u64::from(ffi::AV_PIX_FMT_FLAG_BE) != 0;
    let shift_ok = |s: c_int| (0..32).contains(&s);
    // The span, and the largest value a palette lookup may index with.
    let (span, max_val) = if bitstream && step > 8 {
        // Whole 32-bit words from the row's start: `val >> offset`, or
        // `((1ULL << depth) - 1) << offset` and `val << offset`.
        if !shift_ok(offset) || !(0..64).contains(&depth) {
            return Err(Invalid);
        }
        (row..row + 4 * i64::from(w), u64::from(mask))
    } else if bitstream {
        let skip = x
            .checked_mul(step)
            .and_then(|s| s.checked_add(offset))
            .ok_or(OutOfRange)?;
        let mut shift = 8_i32
            .checked_sub(depth)
            .and_then(|s| s.checked_sub(skip & 7))
            .ok_or(Invalid)?;
        let mut p = row + i64::from(skip >> 3);
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for _ in 0..w {
            // `*p >> shift` or `val << shift`.
            if !shift_ok(shift) {
                return Err(Invalid);
            }
            (lo, hi) = (lo.min(p), hi.max(p + 1));
            shift = shift.checked_sub(step).ok_or(Invalid)?;
            p -= i64::from(shift >> 3);
            shift &= 7;
        }
        // A byte, shifted and masked.
        (lo..hi, u64::from(mask.min(0xFF)))
    } else {
        let base = row + i64::from(x.checked_mul(step).ok_or(OutOfRange)?) + i64::from(offset);
        let width = shift.checked_add(depth).ok_or(Invalid)?;
        // `val >> shift` or `val << shift` on an `unsigned`.
        if !shift_ok(shift) {
            return Err(Invalid);
        }
        let (size, base): (i64, i64) = if width <= 8 {
            (1, base + i64::from(big_endian))
        } else if width <= 16 {
            (2, base)
        } else {
            (4, base)
        };
        let last = i64::from(w - 1) * i64::from(step);
        let read_max = (u64::MAX >> (64 - 8 * size as u32)) >> shift;
        (
            base + last.min(0)..base + last.max(0) + size,
            u64::from(mask).min(read_max),
        )
    };
    let palette = match access {
        LineAccess::Read { palette: true } => {
            // `data[1][4 * val + c]`, with `val` an `int` in the bitstream
            // readers.
            let top = 4 * max_val + c as u64;
            if top > c_int::MAX as u64 {
                return Err(Invalid);
            }
            Some(top + 1)
        }
        _ => None,
    };
    Ok(Some((
        LinePlan {
            plane,
            span,
            palette,
        },
        linesize,
    )))
}

/// The offset of row 0 in a plane slice of `len` bytes, rows `linesize`
/// apart: its start for a non-negative linesize, and the start of the last
/// whole row for a negative one, as the [`imgutils`](super::imgutils)
/// module docs lay a plane out.
fn row0(len: usize, linesize: isize) -> usize {
    if linesize >= 0 || len == 0 {
        return 0;
    }
    let l = linesize.unsigned_abs();
    (len - 1) / l * l
}

/// Whether `range`, in bytes from offset `at` of a slice of `len` bytes,
/// lies inside it.
#[expect(
    clippy::cast_possible_wrap,
    reason = "slice positions, below `isize::MAX`"
)]
fn inside(len: usize, at: usize, range: &Range<i64>) -> bool {
    let at = at as i64;
    at.checked_add(range.start).is_some_and(|s| s >= 0)
        && at.checked_add(range.end).is_some_and(|e| e <= len as i64)
}

/// The checked arguments of one line read or write: C's `int`s, and the
/// offsets of row 0 in the planes C reaches (`None` for the others).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineArgs {
    at: [Option<usize>; 4],
    linesizes: [c_int; 4],
    x: c_int,
    y: c_int,
    c: c_int,
    w: c_int,
}

impl LineArgs {
    /// The arguments for moving `len` values of `component` of `desc` at
    /// pixel `x` of row `y` of planes of `lens` bytes, rows `linesizes`
    /// apart; `None` when C touches nothing.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "a component index, below 4"
    )]
    #[expect(
        clippy::many_single_char_names,
        reason = "the names follow the C variables they mirror"
    )]
    fn new(
        desc: PixelFormatDescriptorRef<'_>,
        (lens, linesizes): ([usize; 4], &[isize; 4]),
        (x, y, component, len): (usize, usize, usize, usize),
        access: LineAccess,
    ) -> Result<Option<Self>, ImageLineError> {
        let comp = desc
            .components()
            .get(component)
            .ok_or(ImageLineError::Component)?;
        let int = |v: usize| c_int::try_from(v).map_err(|_| ImageLineError::OutOfRange);
        let (x, y, c, w) = (int(x)?, int(y)?, component as c_int, int(len)?);
        if w == 0 {
            return Ok(None);
        }
        let fields = [
            comp.plane(),
            comp.step(),
            comp.offset(),
            comp.shift(),
            comp.depth(),
        ];
        let Some((plan, linesize)) =
            line_plan(fields, desc.flags().bits(), (x, y, c, w), linesizes, access)?
        else {
            return Ok(None);
        };
        let mut args = Self {
            at: [None; 4],
            linesizes: [0; 4],
            x,
            y,
            c,
            w,
        };
        let p = plan.plane;
        let at = row0(lens[p], linesizes[p]);
        if !inside(lens[p], at, &plan.span) {
            return Err(ImageLineError::OutsidePlane);
        }
        (args.at[p], args.linesizes[p]) = (Some(at), linesize);
        if let Some(need) = plan.palette {
            let at = row0(lens[1], linesizes[1]);
            if ((lens[1] - at) as u64) < need {
                return Err(ImageLineError::OutsidePalette);
            }
            args.at[1] = Some(at);
        }
        Ok(Some(args))
    }
}

/// Reads a line as [`read_image_line`] describes, calling `read` with the
/// checked arguments and C's plane pointers.
fn read_line_with(
    len: usize,
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    (x, y, component): (usize, usize, usize),
    palette: bool,
    read: impl FnOnce(&LineArgs, &mut [*const u8; 4]),
) -> Result<(), ImageLineError> {
    let lens = planes.data.map(<[u8]>::len);
    let Some(args) = LineArgs::new(
        desc,
        (lens, &planes.linesizes),
        (x, y, component, len),
        LineAccess::Read { palette },
    )?
    else {
        return Ok(());
    };
    let mut data = [ptr::null(); 4];
    for ((p, plane), at) in data.iter_mut().zip(planes.data).zip(args.at) {
        if let Some(at) = at {
            // `row0` is at most the slice's length; the pointer derives from
            // the whole slice, so C may reach rows on either side of row 0.
            *p = plane.as_ptr().wrapping_add(at);
        }
    }
    read(&args, &mut data);
    Ok(())
}

/// Wraps: `av_read_image_line2`
///
/// Reads `dst.len()` values of component `component` (0 to 3, in the
/// order of [`components`](PixelFormatDescriptorRef::components)) of the
/// pixels from `x` on of row `y` of an image in `planes`, laid out as
/// `desc` describes, into `dst`, each as a `u16` or `u32`, truncated to the
/// element. With `palette`, each value read is an index, and the byte C
/// stores is the component's entry of plane 1's palette (four bytes per
/// entry, at `4 * index + component`), as for [`PAL8`](PixelFormat::PAL8).
///
/// Row `y` lies `y * linesize` bytes from row 0 of the component's plane,
/// laid out as the [`imgutils`](super::imgutils) module docs describe: row
/// 0 starts the slice for a non-negative linesize, and is its last whole
/// row (the last offset that is a multiple of the linesize) for a negative
/// one. A component with a depth of 0 (absent) reads nothing and leaves
/// `dst` as it is, as does an empty `dst`.
///
/// For a [bitstream](PixelFormatFlags::BITSTREAM) format whose component
/// step exceeds 8 bits ([`XV30BE`](PixelFormat::XV30BE) and
/// [`V30XBE`](PixelFormat::V30XBE)), C takes every component of a pixel
/// from one big-endian 32-bit word per pixel and ignores `x`: the line
/// starts at the row's first word.
///
/// # Errors
///
/// Without touching `dst`: [`ImageLineError::Component`] for a component
/// past 3, [`ImageLineError::OutOfRange`] for a position, linesize or
/// length beyond `i32` or C's `int` arithmetic on them overflowing,
/// [`ImageLineError::InvalidDescriptor`] for a component whose layout makes
/// C's arithmetic undefined, and [`ImageLineError::OutsidePlane`] or
/// [`ImageLineError::OutsidePalette`] when the bytes C would read are not
/// all inside `planes`. With `palette`, plane 1 must hold an entry for every
/// value the component's depth can hold (1024 bytes for an 8-bit index).
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::Planes;
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::{PixelFormat, read_image_line};
/// let desc = PixelFormat::RGB24.descriptor().unwrap();
/// let row = [1, 2, 3, 4, 5, 6];
/// let planes = Planes {
///     data: [&row[..], &[], &[], &[]],
///     linesizes: [6, 0, 0, 0],
/// };
/// let mut green = [0u16; 2];
/// read_image_line(&mut green, planes, desc, 0, 0, 1, false).unwrap();
/// assert_eq!(green, [2, 5]);
/// ```
pub fn read_image_line<T: LineElement>(
    dst: &mut [T],
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
    palette: bool,
) -> Result<(), ImageLineError> {
    let len = dst.len();
    read_line_with(
        len,
        planes,
        desc,
        (x, y, component),
        palette,
        |args, data| {
            // SAFETY: `LineArgs::new` mirrored C's arithmetic for this component
            // and found it defined, and every byte C reads (row `y` from pixel
            // `x` for `w` values, and the palette entries) inside the planes
            // whose row-0 pointers `data` holds; C reads no other plane. `dst`
            // holds `w` elements of `T::SIZE` bytes, the size C writes, and is
            // borrowed apart from the planes; the descriptor is live and only
            // read. C keeps no pointer.
            unsafe {
                ffi::av_read_image_line2(
                    dst.as_mut_ptr().cast(),
                    data.as_mut_ptr(),
                    args.linesizes.as_ptr(),
                    desc.as_ptr(),
                    args.x,
                    args.y,
                    args.c,
                    args.w,
                    c_int::from(palette),
                    T::SIZE,
                );
            }
        },
    )
}

/// Wraps: `av_read_image_line`
///
/// [`read_image_line`] into `u16`s, through C's original entry point.
///
/// # Errors
///
/// As for [`read_image_line`].
pub fn read_image_line_u16(
    dst: &mut [u16],
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
    palette: bool,
) -> Result<(), ImageLineError> {
    let len = dst.len();
    read_line_with(
        len,
        planes,
        desc,
        (x, y, component),
        palette,
        |args, data| {
            // SAFETY: as in `read_image_line`, for `w` 2-byte elements, the size
            // `av_read_image_line` passes on.
            unsafe {
                ffi::av_read_image_line(
                    dst.as_mut_ptr(),
                    data.as_mut_ptr(),
                    args.linesizes.as_ptr(),
                    desc.as_ptr(),
                    args.x,
                    args.y,
                    args.c,
                    args.w,
                    c_int::from(palette),
                );
            }
        },
    )
}

/// Writes a line as [`write_image_line`] describes, calling `write` with
/// the checked arguments and C's plane pointers.
fn write_line_with(
    len: usize,
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    (x, y, component): (usize, usize, usize),
    write: impl FnOnce(&LineArgs, &mut [*mut u8; 4]),
) -> Result<(), ImageLineError> {
    let lens = planes.data.each_ref().map(|d| d.len());
    let Some(args) = LineArgs::new(
        desc,
        (lens, &planes.linesizes),
        (x, y, component, len),
        LineAccess::Write,
    )?
    else {
        return Ok(());
    };
    let mut data = [ptr::null_mut(); 4];
    for ((p, plane), at) in data.iter_mut().zip(&mut planes.data).zip(args.at) {
        if let Some(at) = at {
            // As in `read_line_with`.
            *p = plane.as_mut_ptr().wrapping_add(at);
        }
    }
    write(&args, &mut data);
    Ok(())
}

/// Wraps: `av_write_image_line2`
///
/// Writes the values of `src` to component `component` (0 to 3) of the
/// pixels from `x` on of row `y` of an image in `planes`, laid out as
/// `desc` describes and as [`read_image_line`] reads them (including the
/// 32-bit-word bitstream formats, for which C ignores `x`).
///
/// C ORs each value, shifted into place, into the bytes already there, as
/// it expects a zeroed image: the bits of earlier values stay set, and
/// value bits beyond the component's depth spill into its neighbours.
/// Nothing is written for a component with a depth of 0 or an empty `src`.
///
/// # Errors
///
/// As for [`read_image_line`], without touching `planes`, except that no
/// palette is involved.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::{Planes, PlanesMut};
/// use ffmpeg_bindings::libavutil::libavutil::pixdesc::{
///     PixelFormat, read_image_line, write_image_line,
/// };
/// let desc = PixelFormat::RGB565LE.descriptor().unwrap();
/// let mut row = [0u8; 4];
/// let mut planes = PlanesMut {
///     data: [&mut row[..], &mut [], &mut [], &mut []],
///     linesizes: [4, 0, 0, 0],
/// };
/// write_image_line(&[31u32, 1], &mut planes, desc, 0, 0, 0).unwrap();
/// assert_eq!(row, [0x00, 0xF8, 0x00, 0x08]);
/// ```
pub fn write_image_line<T: LineElement>(
    src: &[T],
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
) -> Result<(), ImageLineError> {
    write_line_with(src.len(), planes, desc, (x, y, component), |args, data| {
        // SAFETY: `LineArgs::new` mirrored C's arithmetic for this component
        // and found it defined, and every byte C reads and writes (row `y`
        // from pixel `x` for `w` values) inside the plane whose row-0
        // pointer `data` holds, borrowed exclusively; C touches no other
        // plane. `src` holds `w` elements of `T::SIZE` bytes, the size C
        // reads; the descriptor is live and only read. C keeps no pointer.
        unsafe {
            ffi::av_write_image_line2(
                src.as_ptr().cast(),
                data.as_mut_ptr(),
                args.linesizes.as_ptr(),
                desc.as_ptr(),
                args.x,
                args.y,
                args.c,
                args.w,
                T::SIZE,
            );
        }
    })
}

/// Wraps: `av_write_image_line`
///
/// [`write_image_line`] from `u16`s, through C's original entry point.
///
/// # Errors
///
/// As for [`write_image_line`].
pub fn write_image_line_u16(
    src: &[u16],
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
) -> Result<(), ImageLineError> {
    write_line_with(src.len(), planes, desc, (x, y, component), |args, data| {
        // SAFETY: as in `write_image_line`, for `w` 2-byte elements, the
        // size `av_write_image_line` passes on.
        unsafe {
            ffi::av_write_image_line(
                src.as_ptr(),
                data.as_mut_ptr(),
                args.linesizes.as_ptr(),
                desc.as_ptr(),
                args.x,
                args.y,
                args.c,
                args.w,
            );
        }
    })
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#[expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]
mod unit_tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;

    fn raw(
        plane: i32,
        step: i32,
        offset: i32,
        shift: i32,
        depth: i32,
    ) -> ffi::AVComponentDescriptor {
        ffi::AVComponentDescriptor {
            plane,
            step,
            offset,
            shift,
            depth,
        }
    }

    fn hash_of(d: &ComponentDescriptor) -> u64 {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    }

    #[test]
    fn component_descriptor_new_and_default_are_all_zero() {
        let d = ComponentDescriptor::new();
        assert_eq!(d.as_ref().fields(), [0; 5]);
        assert_eq!(ComponentDescriptor::default(), d);
        let back = d.into_raw();
        assert_eq!(
            (back.plane, back.step, back.offset, back.shift, back.depth),
            (0, 0, 0, 0, 0)
        );
    }

    #[test]
    fn component_descriptor_raw_round_trip_keeps_every_field() {
        let d = ComponentDescriptor::from(raw(3, 32, -1, 7, 32));
        assert_eq!(d.as_ref().fields(), [3, 32, -1, 7, 32]);
        let back: ffi::AVComponentDescriptor = d.into();
        assert_eq!(
            (back.plane, back.step, back.offset, back.shift, back.depth),
            (3, 32, -1, 7, 32)
        );
    }

    #[test]
    fn component_descriptor_equality_and_hash_are_field_wise() {
        let a = ComponentDescriptor::from_raw(raw(1, 2, 3, 4, 5));
        let b = ComponentDescriptor::from_raw(raw(1, 2, 3, 4, 5));
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        for i in 0..5 {
            let mut f = [1, 2, 3, 4, 5];
            f[i] += 1;
            let c = ComponentDescriptor::from_raw(raw(f[0], f[1], f[2], f[3], f[4]));
            assert_ne!(a, c, "field {i}");
        }
    }

    #[test]
    fn component_descriptor_debug_names_every_field() {
        let d = ComponentDescriptor::from_raw(raw(0, 2, -1, 3, 5));
        let s = format!("{d:?}");
        assert_eq!(
            s,
            "ComponentDescriptor { plane: 0, step: 2, offset: -1, shift: 3, depth: 5 }"
        );
        assert_eq!(format!("{:?}", d.as_ref()), s);
    }

    #[test]
    fn component_descriptor_setters_write_one_field_each() {
        let mut d = ComponentDescriptor::new();
        let mut m = d.as_mut();
        m.set_plane(i32::MIN);
        m.set_step(i32::MAX);
        m.set_offset(-1);
        m.set_shift(7);
        m.set_depth(32);
        assert_eq!(m.as_ref().fields(), [i32::MIN, i32::MAX, -1, 7, 32]);
        m.set(ComponentDescriptor::new());
        assert_eq!(d.as_ref().fields(), [0; 5]);
    }

    #[test]
    fn component_descriptor_get_copies_out_of_the_place() {
        let mut d = ComponentDescriptor::from_raw(raw(1, 1, 1, 1, 1));
        let copy = d.as_ref().get();
        d.as_mut().set_depth(9);
        assert_eq!(copy.as_ref().fields(), [1, 1, 1, 1, 1]);
        assert_eq!(d.as_ref().fields(), [1, 1, 1, 1, 9]);
    }

    #[test]
    fn from_name_reads_c_errors_as_not_found() {
        assert_eq!(found(0), Some(0));
        assert_eq!(found(256), Some(256));
        assert_eq!(found(-(ffi::EINVAL.cast_signed())), None);
        assert_eq!(found(c_int::MIN), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn from_name_finds_each_property() {
        assert_eq!(ColorRange::from_name(c"pc"), Some(ColorRange::JPEG));
        assert_eq!(
            ColorPrimaries::from_name(c"bt2020"),
            Some(ColorPrimaries::BT2020)
        );
        assert_eq!(
            ColorTransfer::from_name(c"smpte2084"),
            Some(ColorTransfer::SMPTE2084)
        );
        assert_eq!(ColorSpace::from_name(c"gbr"), Some(ColorSpace::RGB));
        assert_eq!(
            ChromaLocation::from_name(c"topleft"),
            Some(ChromaLocation::TOPLEFT)
        );
        assert_eq!(ColorRange::from_name(c"PC"), None);
        assert_eq!(ChromaLocation::from_name(c""), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn swap_endianness_pairs_be_and_le() {
        let pairs = [
            (PixelFormat::RGB48BE, PixelFormat::RGB48LE),
            (PixelFormat::GRAY16BE, PixelFormat::GRAY16LE),
            (PixelFormat::YUV420P10BE, PixelFormat::YUV420P10LE),
        ];
        for (be, le) in pairs {
            assert_eq!(be.swap_endianness(), Some(le));
            assert_eq!(le.swap_endianness(), Some(be));
        }
        for f in [PixelFormat::YUV420P, PixelFormat::RGB24, PixelFormat::NONE] {
            assert_eq!(f.swap_endianness(), None, "{f:?}");
        }
        for raw in [c_int::MIN, -2, 100_000, c_int::MAX] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(f.swap_endianness(), None, "{raw}");
        }
    }

    #[test]
    fn static_name_reads_null_as_none() {
        // For the unsafe operations below: NULL, and a string literal, which lives for the process.
        {
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(static_name(ptr::null()), None) };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(static_name(c"tv".as_ptr()), Some(c"tv")) };
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn property_names_come_from_c_tables() {
        assert_eq!(ColorRange::MPEG.name(), Some(c"tv"));
        assert_eq!(ColorPrimaries::RESERVED.name(), Some(c"reserved"));
        assert_eq!(ColorPrimaries::V_GAMUT.name(), Some(c"vgamut"));
        assert_eq!(ColorTransfer::V_LOG.name(), Some(c"vlog"));
        assert_eq!(ColorSpace::RGB.name(), Some(c"gbr"));
        assert_eq!(ChromaLocation::TOPLEFT.name(), Some(c"topleft"));
        assert_eq!(AlphaMode::STRAIGHT.name(), Some(c"straight"));
        assert_eq!(PixelFormat::RGB48LE.name(), Some(c"rgb48le"));
        assert_eq!(
            ColorPrimaries::from_raw(ffi::AVColorPrimaries(13)).name(),
            None
        );
        assert_eq!(
            ColorRange::from_raw(ffi::AVColorRange(u32::MAX)).name(),
            None
        );
        assert_eq!(
            PixelFormat::from_raw(ffi::AVPixelFormat(i32::MIN)).name(),
            None
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_read_cs_sentinels_as_not_found() {
        assert_eq!(
            AlphaMode::from_name(c"premultiplied"),
            Some(AlphaMode::PREMULTIPLIED)
        );
        assert_eq!(
            AlphaMode::from_name(c"unspecified"),
            Some(AlphaMode::UNSPECIFIED)
        );
        assert_eq!(AlphaMode::from_name(c"Straight"), None);
        assert_eq!(
            PixelFormat::from_name(c"rgb48le"),
            Some(PixelFormat::RGB48LE)
        );
        assert_eq!(PixelFormat::from_name(c"none"), None);
        assert_eq!(PixelFormat::from_name(c""), None);
        assert_eq!(
            ChromaLocation::from_position(128, 128),
            Some(ChromaLocation::CENTER)
        );
        // C's "not found" is `UNSPECIFIED`, which no position maps to.
        assert_eq!(ChromaLocation::from_position(0, 64), None);
        assert_eq!(ChromaLocation::UNSPECIFIED.position(), None);
        assert_eq!(ChromaLocation::BOTTOM.position(), Some((128, 256)));
    }

    #[test]
    fn loss_reads_cs_errors_and_sets() {
        assert_eq!(loss_or_error(0), Ok(Loss::empty()));
        assert_eq!(loss_or_error(0x24), Ok(Loss::COLORSPACE | Loss::CHROMA));
        for e in [-1, -2, -3, -4, i32::MIN] {
            assert_eq!(loss_or_error(e), Err(e));
        }
    }

    #[test]
    fn loss_set_operations_carry_every_bit() {
        let all = Loss::KNOWN
            .iter()
            .fold(Loss::empty(), |acc, &(k, _)| acc | k);
        assert_eq!(all.bits(), 0xFF);
        assert!(Loss::empty().is_empty() && Loss::default().is_empty());
        assert!(all.contains(Loss::ALPHA | Loss::DEPTH));
        assert!(!Loss::ALPHA.contains(Loss::ALPHA | Loss::DEPTH));
        assert!(all.contains(Loss::empty()));
        let mut l = Loss::RESOLUTION;
        l |= Loss::from_bits_retain(0x100);
        assert_eq!(l.bits(), 0x101);
        assert_eq!((l & Loss::RESOLUTION), Loss::RESOLUTION);
        assert_eq!(Loss::from_bits_retain(-1).bits(), -1);
    }

    #[test]
    fn loss_debug_names_the_kinds() {
        assert_eq!(format!("{:?}", Loss::empty()), "Loss(0x0)");
        assert_eq!(format!("{:?}", Loss::ALPHA), "Loss(ALPHA)");
        assert_eq!(
            format!("{:?}", Loss::RESOLUTION | Loss::EXCESS_DEPTH),
            "Loss(RESOLUTION | EXCESS_DEPTH)"
        );
        assert_eq!(
            format!("{:?}", Loss::CHROMA | Loss::from_bits_retain(0x300)),
            "Loss(CHROMA | 0x300)"
        );
        assert_eq!(
            format!("{:?}", Loss::from_bits_retain(0x400)),
            "Loss(0x400)"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn conversion_loss_and_choices() {
        assert_eq!(
            PixelFormat::YUV420P.conversion_loss(PixelFormat::YUV420P, true),
            Ok(Loss::empty())
        );
        let loss = PixelFormat::RGBA
            .conversion_loss(PixelFormat::RGB24, true)
            .unwrap();
        assert!(loss.contains(Loss::ALPHA));
        let loss = PixelFormat::RGBA
            .conversion_loss(PixelFormat::RGB24, false)
            .unwrap();
        assert!(!loss.contains(Loss::ALPHA));
        assert_eq!(
            PixelFormat::NONE.conversion_loss(PixelFormat::RGB24, true),
            Err(-4)
        );
        assert_eq!(
            PixelFormat::VAAPI.conversion_loss(PixelFormat::VAAPI, true),
            Err(-1)
        );
        assert_eq!(
            PixelFormat::VAAPI.conversion_loss(PixelFormat::RGB24, true),
            Err(-2)
        );

        let src = PixelFormat::RGBA;
        assert_eq!(
            src.best_of_two(PixelFormat::RGB24, PixelFormat::BGRA, true),
            PixelFormat::BGRA
        );
        assert_eq!(
            src.best_of_two(PixelFormat::NONE, PixelFormat::GRAY8, true),
            PixelFormat::GRAY8
        );
        assert_eq!(
            src.best_of_two(PixelFormat::GRAY8, PixelFormat::NONE, true),
            PixelFormat::GRAY8
        );
        // An unknown `dst1` hands back `dst2`, known or not.
        assert_eq!(
            src.best_of_two(PixelFormat::NONE, PixelFormat::NONE, true),
            PixelFormat::NONE
        );
        assert_eq!(
            src.best_of_two_with_loss(PixelFormat::NONE, PixelFormat::NONE, true, Loss::empty()),
            (PixelFormat::NONE, Err(-4))
        );
        let (best, loss) =
            src.best_of_two_with_loss(PixelFormat::RGB24, PixelFormat::BGRA, true, Loss::empty());
        assert_eq!((best, loss), (PixelFormat::BGRA, Ok(Loss::empty())));
        // Ignoring the alpha loss leaves RGB24's fewer bits per pixel to win.
        let (best, loss) =
            src.best_of_two_with_loss(PixelFormat::RGB24, PixelFormat::BGRA, true, Loss::ALPHA);
        assert_eq!(best, PixelFormat::RGB24);
        assert!(loss.unwrap().contains(Loss::ALPHA));
    }

    #[test]
    fn loss_debug_passes_writer_errors_on() {
        /// A writer that fails once `room` bytes are written.
        struct Short {
            room: usize,
        }
        impl fmt::Write for Short {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.room = self.room.checked_sub(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let loss = Loss::RESOLUTION | Loss::ALPHA | Loss::from_bits_retain(0x100);
        let whole = format!("{loss:?}").len();
        for room in 0..whole {
            assert!(
                fmt::write(&mut Short { room }, format_args!("{loss:?}")).is_err(),
                "{room}"
            );
        }
        assert!(fmt::write(&mut Short { room: whole }, format_args!("{loss:?}")).is_ok());
    }

    /// A descriptor with every field set to a distinct value, built without
    /// a C call.
    fn yuv420p_like() -> PixelFormatDescriptor {
        let mut d = PixelFormatDescriptor::new();
        let mut m = d.as_mut();
        m.set_name(Some(c"yuv420p"));
        m.set_component_count(3);
        m.set_log2_chroma_w(1);
        m.set_log2_chroma_h(2);
        m.set_flags(PixelFormatFlags::PLANAR);
        {
            let mut comps = m.components_mut();
            for i in 0..3 {
                comps
                    .get_mut(i)
                    .unwrap()
                    .set(ComponentDescriptor::from_raw(raw(i as i32, 1, 0, 0, 8)));
            }
        }
        m.set_alias(Some(c"i420,yv12x"));
        d
    }

    fn pf_hash_of(d: &PixelFormatDescriptor) -> u64 {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    }

    #[test]
    fn pixel_format_flags_name_c_s_bits() {
        let named = [
            (PixelFormatFlags::BIG_ENDIAN, 1 << 0),
            (PixelFormatFlags::PALETTE, 1 << 1),
            (PixelFormatFlags::BITSTREAM, 1 << 2),
            (PixelFormatFlags::HWACCEL, 1 << 3),
            (PixelFormatFlags::PLANAR, 1 << 4),
            (PixelFormatFlags::RGB, 1 << 5),
            (PixelFormatFlags::ALPHA, 1 << 7),
            (PixelFormatFlags::BAYER, 1 << 8),
            (PixelFormatFlags::FLOAT, 1 << 9),
            (PixelFormatFlags::XYZ, 1 << 10),
        ];
        for (flag, bits) in named {
            assert_eq!(flag.bits(), bits);
            assert_eq!(PixelFormatFlags::from_bits_retain(bits), flag);
        }
        assert_eq!(
            PixelFormatFlags::KNOWN.map(|(f, _)| f),
            named.map(|(f, _)| f)
        );
    }

    #[test]
    fn pixel_format_flags_set_operations_keep_unnamed_bits() {
        let empty = PixelFormatFlags::empty();
        assert!(empty.is_empty());
        assert_eq!(PixelFormatFlags::default(), empty);
        let mut f = PixelFormatFlags::RGB | PixelFormatFlags::ALPHA;
        assert!(f.contains(PixelFormatFlags::RGB));
        assert!(f.contains(PixelFormatFlags::RGB | PixelFormatFlags::ALPHA));
        assert!(!f.contains(PixelFormatFlags::PLANAR));
        assert!(f.contains(empty));
        f |= PixelFormatFlags::from_bits_retain(1 << 6 | 1 << 63);
        assert_eq!(f.bits(), 1 << 5 | 1 << 7 | 1 << 6 | 1 << 63);
        assert_eq!(
            (f & PixelFormatFlags::from_bits_retain(1 << 63 | 1 << 5)).bits(),
            1 << 63 | 1 << 5
        );
        assert!(!f.is_empty());
    }

    #[test]
    fn pixel_format_flags_debug_names_known_bits_and_shows_the_rest() {
        assert_eq!(
            format!("{:?}", PixelFormatFlags::empty()),
            "PixelFormatFlags(0x0)"
        );
        assert_eq!(
            format!("{:?}", PixelFormatFlags::BIG_ENDIAN | PixelFormatFlags::XYZ),
            "PixelFormatFlags(BIG_ENDIAN | XYZ)"
        );
        assert_eq!(
            format!(
                "{:?}",
                PixelFormatFlags::PALETTE | PixelFormatFlags::from_bits_retain(1 << 6)
            ),
            "PixelFormatFlags(PALETTE | 0x40)"
        );
        assert_eq!(
            format!("{:?}", PixelFormatFlags::from_bits_retain(1 << 40)),
            "PixelFormatFlags(0x10000000000)"
        );
    }

    #[test]
    fn pixel_format_flags_debug_passes_on_writer_errors() {
        use core::fmt::Write;

        /// A writer that fails once `budget` bytes are spent.
        struct Budget(usize);
        impl Write for Budget {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.checked_sub(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }

        let mixed = PixelFormatFlags::RGB
            | PixelFormatFlags::ALPHA
            | PixelFormatFlags::from_bits_retain(1 << 6);
        for f in [mixed, PixelFormatFlags::empty()] {
            let full = format!("{f:?}").len();
            for budget in 0..full {
                assert!(write!(Budget(budget), "{f:?}").is_err(), "{f:?} {budget}");
            }
            assert!(write!(Budget(full), "{f:?}").is_ok());
        }
    }

    #[test]
    fn pixel_format_descriptor_new_and_default_are_all_zero() {
        let d = PixelFormatDescriptor::new();
        let r = d.as_ref();
        assert_eq!(r.name(), None);
        assert_eq!(r.alias(), None);
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (0, 0, 0)
        );
        assert!(r.flags().is_empty());
        assert_eq!(r.components().len(), 4);
        assert!(
            r.components()
                .iter()
                .all(|c| c.get() == ComponentDescriptor::new())
        );
        assert_eq!(PixelFormatDescriptor::default(), d);
        let back: ffi::AVPixFmtDescriptor = d.into();
        assert!(back.name.is_null() && back.alias.is_null());
        assert_eq!((back.nb_components, back.flags), (0, 0));
    }

    #[test]
    fn pixel_format_descriptor_setters_write_one_field_each() {
        let mut d = yuv420p_like();
        let r = d.as_ref();
        assert_eq!(r.name(), Some(c"yuv420p"));
        assert_eq!(r.alias(), Some(c"i420,yv12x"));
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (3, 1, 2)
        );
        assert_eq!(r.flags(), PixelFormatFlags::PLANAR);
        let planes: Vec<i32> = r.components().iter().map(|c| c.plane()).collect();
        assert_eq!(planes, [0, 1, 2, 0]);
        assert_eq!(r.components().get(3).unwrap().depth(), 0);
        assert!(r.components().get(4).is_none());

        let mut m = d.as_mut();
        m.set_name(None);
        assert_eq!(m.as_ref().name(), None);
        assert_eq!(m.as_ref().alias(), Some(c"i420,yv12x"));
        m.set_alias(None);
        m.set_component_count(u8::MAX);
        m.set_log2_chroma_w(u8::MAX);
        m.set_log2_chroma_h(0);
        m.set_flags(PixelFormatFlags::from_bits_retain(u64::MAX));
        m.components_mut().get_mut(3).unwrap().set_depth(-1);
        let r = m.as_ref();
        assert_eq!(r.alias(), None);
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (u8::MAX, u8::MAX, 0)
        );
        assert_eq!(r.flags().bits(), u64::MAX);
        assert_eq!(r.components().get(3).unwrap().depth(), -1);
        assert_eq!(r.components().get(0).unwrap().depth(), 8);
        assert!(m.components_mut().get_mut(4).is_none());
    }

    #[test]
    fn pixel_format_descriptor_raw_round_trip_keeps_every_field() {
        let d = yuv420p_like();
        // The pointers the setters stored, not another instance of the
        // literals: two occurrences of one literal need not share an
        // address.
        let (name, alias) = (d.as_ref().name().unwrap(), d.as_ref().alias().unwrap());
        let back = d.into_raw();
        assert_eq!(back.name, name.as_ptr());
        assert_eq!(back.alias, alias.as_ptr());
        // SAFETY: `back`'s strings are the literals `yuv420p_like` set.
        let back_name = unsafe { CStr::from_ptr(back.name) };
        // SAFETY: as above.
        let back_alias = unsafe { CStr::from_ptr(back.alias) };
        assert_eq!((back_name, back_alias), (c"yuv420p", c"i420,yv12x"));
        assert_eq!(
            (
                back.nb_components,
                back.log2_chroma_w,
                back.log2_chroma_h,
                back.flags
            ),
            (3, 1, 2, 1 << 4)
        );
        assert_eq!(back.comp[2].plane, 2);
        // SAFETY: `back`'s strings are the literals `yuv420p_like` set.
        let again = unsafe { PixelFormatDescriptor::from_raw(back) };
        assert_eq!(again, d);
        assert_eq!(again.as_ref().name().map(CStr::as_ptr), Some(back.name));
    }

    #[test]
    fn pixel_format_descriptor_equality_and_hash_are_field_wise_by_content() {
        let a = yuv420p_like();
        let b = yuv420p_like();
        assert_eq!(a, b);
        assert_eq!(pf_hash_of(&a), pf_hash_of(&b));

        // Same bytes at another address compare equal.
        static OTHER: [u8; 8] = *b"yuv420p\0";
        let other = CStr::from_bytes_with_nul(&OTHER).unwrap();
        let mut c = a;
        c.as_mut().set_name(Some(other));
        assert_ne!(a.as_ref().name().unwrap().as_ptr(), other.as_ptr());
        assert_eq!(a, c);
        assert_eq!(pf_hash_of(&a), pf_hash_of(&c));

        let changes: [fn(&mut PixelFormatDescriptorMut<'_>); 8] = [
            |m| m.set_name(Some(c"yuv420P")),
            |m| m.set_name(None),
            |m| m.set_component_count(4),
            |m| m.set_log2_chroma_w(0),
            |m| m.set_log2_chroma_h(1),
            |m| m.set_flags(PixelFormatFlags::PLANAR | PixelFormatFlags::BIG_ENDIAN),
            |m| m.components_mut().get_mut(3).unwrap().set_shift(1),
            |m| m.set_alias(None),
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut d = a;
            change(&mut d.as_mut());
            assert_ne!(a, d, "change {i}");
        }
    }

    #[test]
    fn pixel_format_descriptor_debug_names_every_field() {
        let mut d = PixelFormatDescriptor::new();
        d.as_mut().set_name(Some(c"gray"));
        d.as_mut().set_component_count(1);
        d.as_mut()
            .components_mut()
            .get_mut(0)
            .unwrap()
            .set(ComponentDescriptor::from_raw(raw(0, 1, 0, 0, 8)));
        d.as_mut().set_alias(Some(c"gray8,y8"));
        let zero = "ComponentDescriptor { plane: 0, step: 0, offset: 0, shift: 0, depth: 0 }";
        let s = format!("{d:?}");
        assert_eq!(
            s,
            format!(
                "PixelFormatDescriptor {{ name: Some(\"gray\"), component_count: 1, \
                 log2_chroma_w: 0, log2_chroma_h: 0, flags: PixelFormatFlags(0x0), \
                 components: [ComponentDescriptor {{ plane: 0, step: 1, offset: 0, \
                 shift: 0, depth: 8 }}, {zero}, {zero}, {zero}], alias: Some(\"gray8,y8\") }}"
            )
        );
        assert_eq!(format!("{:?}", d.as_ref()), s);
    }

    #[test]
    fn pixel_format_descriptor_get_and_set_copy_whole_values() {
        let mut d = yuv420p_like();
        let copy = d.as_ref().get();
        d.as_mut().set_log2_chroma_h(0);
        assert_eq!(copy, yuv420p_like());
        assert_eq!(d.as_ref().log2_chroma_h(), 0);
        // The copy shares the source's strings.
        assert_eq!(
            copy.as_ref().name().unwrap().as_ptr(),
            d.as_ref().name().unwrap().as_ptr()
        );
        d.as_mut().set(PixelFormatDescriptor::new());
        assert_eq!(d, PixelFormatDescriptor::new());
        d.as_mut().set(copy);
        assert_eq!(d, copy);
    }

    #[test]
    fn pixel_format_descriptor_component_runs_cover_the_embedded_array() {
        let mut d = yuv420p_like();
        let base = d.as_ref().as_ptr() as usize;
        let first = d.as_ref().components().as_ptr() as usize;
        assert_eq!(
            first - base,
            core::mem::offset_of!(ffi::AVPixFmtDescriptor, comp)
        );
        {
            let mut comps = d.as_mut().into_components_mut();
            let (mut lo, mut hi) = comps.split_at_mut(2).unwrap();
            lo.get_mut(1).unwrap().set_offset(-1);
            hi.get_mut(1).unwrap().set_offset(7);
        }
        let offsets: Vec<i32> = d.as_ref().components().iter().map(|c| c.offset()).collect();
        assert_eq!(offsets, [0, -1, 0, 7]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_refuses_values_past_the_table_untouched() {
        let mut buf = [b'x'; 40];
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        for raw in [nb, nb + 1, c_int::MAX] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(f.describe(&mut buf), None);
        }
        assert_eq!(PixelFormat::YUV420P.describe(&mut []), None);
        assert_eq!(buf, [b'x'; 40]);
        let last = PixelFormat::from_raw(ffi::AVPixelFormat(nb - 1));
        assert!(last.describe(&mut buf).is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_prints_headers_for_negatives_and_truncates() {
        let mut buf = [b'x'; 40];
        for raw in [c_int::MIN, -2, -1] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(f.describe(&mut buf), Some(c"name nb_components nb_bits"));
        }
        assert_eq!(PixelFormat::RGB24.describe(&mut buf[..4]), Some(c"rgb"));
        assert_eq!(&buf[..5], b"rgb\0 ");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_count_and_chroma_subsampling_of_known_formats() {
        assert_eq!(PixelFormat::YUV420P.plane_count(), Some(3));
        assert_eq!(PixelFormat::NV12.plane_count(), Some(2));
        assert_eq!(PixelFormat::RGB24.plane_count(), Some(1));
        assert_eq!(PixelFormat::PAL8.plane_count(), Some(1));
        assert_eq!(PixelFormat::YUVA420P.plane_count(), Some(4));
        assert_eq!(PixelFormat::VAAPI.plane_count(), Some(0));
        assert_eq!(PixelFormat::NONE.plane_count(), None);
        assert_eq!(PixelFormat::YUV420P.chroma_subsampling(), Some((1, 1)));
        assert_eq!(PixelFormat::YUV422P.chroma_subsampling(), Some((1, 0)));
        assert_eq!(PixelFormat::YUV410P.chroma_subsampling(), Some((2, 2)));
        assert_eq!(PixelFormat::RGB24.chroma_subsampling(), Some((0, 0)));
        assert_eq!(PixelFormat::NONE.chroma_subsampling(), None);
        let nb = PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_NB);
        assert_eq!(nb.plane_count(), None);
        assert_eq!(nb.chroma_subsampling(), None);
    }

    const READ: LineAccess = LineAccess::Read { palette: false };
    const PAL: LineAccess = LineAccess::Read { palette: true };
    const WRITE: LineAccess = LineAccess::Write;
    const BE: u64 = ffi::AV_PIX_FMT_FLAG_BE as u64;
    const BITSTREAM: u64 = ffi::AV_PIX_FMT_FLAG_BITSTREAM as u64;

    /// `line_plan` for the span alone.
    fn span(
        comp: [c_int; 5],
        flags: u64,
        at: (c_int, c_int, c_int, c_int),
        linesize: isize,
        access: LineAccess,
    ) -> Result<Option<Range<i64>>, ImageLineError> {
        line_plan(comp, flags, at, &[linesize; 4], access).map(|p| p.map(|(p, _)| p.span))
    }

    #[test]
    fn line_plan_spans_bytes_words_and_steps() {
        // rgb24's green: byte 1 of 3-byte pixels, row 1 of 10-byte rows.
        let plan = line_plan([0, 3, 1, 0, 8], 0, (2, 1, 1, 4), &[10; 4], READ)
            .unwrap()
            .unwrap();
        assert_eq!(
            plan,
            (
                LinePlan {
                    plane: 0,
                    span: 17..27,
                    palette: None
                },
                10
            )
        );
        // gray16: one 2-byte value a pixel; a 32-bit value past 16 bits.
        assert_eq!(
            span([0, 2, 0, 0, 16], 0, (1, 0, 0, 3), 8, READ),
            Ok(Some(2..8))
        );
        assert_eq!(
            span([0, 4, 0, 4, 20], 0, (0, 0, 0, 2), 8, WRITE),
            Ok(Some(0..8))
        );
        // A negative step walks backwards from pixel `x`.
        assert_eq!(
            span([0, -2, 0, 0, 8], 0, (3, 0, 0, 3), 8, READ),
            Ok(Some(-10..-5))
        );
        // rgb565be's red is at offset -1, read from the byte after it.
        assert_eq!(
            span([0, 2, -1, 3, 5], BE, (0, 0, 0, 1), 2, READ),
            Ok(Some(0..1))
        );
        assert_eq!(
            span([0, 2, -1, 3, 5], 0, (0, 0, 0, 1), 2, READ),
            Ok(Some(-1..0))
        );
        // Bit-packed: monob's pixels 3 to 12 are bits 3 to 12.
        assert_eq!(
            span([0, 1, 0, 0, 1], BITSTREAM, (3, 0, 0, 10), 2, READ),
            Ok(Some(0..2))
        );
        assert_eq!(
            span([0, 4, 0, 0, 4], BITSTREAM, (1, 0, 0, 2), 2, WRITE),
            Ok(Some(0..2))
        );
        // Packed into 32-bit words from the row's start, whatever `x` is.
        assert_eq!(
            span([0, 32, 4, 0, 10], BITSTREAM, (9, 2, 0, 3), 16, READ),
            Ok(Some(32..44))
        );
    }

    #[test]
    fn line_plan_returns_nothing_for_absent_components() {
        for access in [READ, PAL, WRITE] {
            assert_eq!(span([7, 0, 0, 99, 0], 0, (0, 0, 0, 1), 8, access), Ok(None));
        }
        // Only the reader shifts by the depth before returning.
        assert_eq!(
            span([0, 1, 0, 0, 64], 0, (0, 0, 0, 1), 8, WRITE),
            Ok(Some(0..4))
        );
        assert_eq!(
            span([0, 1, 0, 0, 64], 0, (0, 0, 0, 1), 8, READ),
            Err(ImageLineError::InvalidDescriptor)
        );
        assert_eq!(
            span([0, 1, 0, 0, -1], 0, (0, 0, 0, 1), 8, READ),
            Err(ImageLineError::InvalidDescriptor)
        );
    }

    #[test]
    fn line_plan_refuses_what_c_cannot_compute() {
        use ImageLineError::{InvalidDescriptor as Invalid, OutOfRange};
        let one = (0, 0, 0, 1);
        // Planes C cannot index its arrays with.
        assert_eq!(span([4, 1, 0, 0, 8], 0, one, 8, READ), Err(Invalid));
        assert_eq!(span([-1, 1, 0, 0, 8], 0, one, 8, WRITE), Err(Invalid));
        // Shifts by a value's width or more, or by a negative amount.
        assert_eq!(span([0, 4, 0, 32, 1], 0, one, 8, READ), Err(Invalid));
        assert_eq!(span([0, 4, 0, -1, 1], 0, one, 8, WRITE), Err(Invalid));
        assert_eq!(span([0, 1, 0, 0, 9], BITSTREAM, one, 8, READ), Err(Invalid));
        assert_eq!(
            span([0, 32, 32, 0, 8], BITSTREAM, one, 8, READ),
            Err(Invalid)
        );
        assert_eq!(
            span([0, 32, 0, 0, -1], BITSTREAM, one, 8, WRITE),
            Err(Invalid)
        );
        // `shift + depth` and `shift - step` overflowing `int`.
        assert_eq!(
            span([0, 1, 0, 8, c_int::MAX], 0, one, 8, WRITE),
            Err(Invalid)
        );
        assert_eq!(
            span([0, c_int::MIN, 0, 0, 1], BITSTREAM, (0, 0, 0, 2), 8, READ),
            Err(Invalid)
        );
        assert_eq!(
            span([0, 1, 0, 0, c_int::MIN], BITSTREAM, one, 8, WRITE),
            Err(Invalid)
        );
        // `y * linesize`, `x * step` and `skip` overflowing `int`.
        assert_eq!(
            span([0, 1, 0, 0, 8], 0, (0, 2, 0, 1), 1 << 30, READ),
            Err(OutOfRange)
        );
        assert_eq!(
            span([0, 1 << 30, 0, 0, 8], 0, (2, 0, 0, 1), 8, READ),
            Err(OutOfRange)
        );
        assert_eq!(
            span([0, 1, c_int::MAX, 0, 1], BITSTREAM, (1, 0, 0, 1), 8, READ),
            Err(OutOfRange)
        );
        // A linesize beyond `int`.
        assert_eq!(
            span([0, 1, 0, 0, 8], 0, one, 1 << 31, READ),
            Err(OutOfRange)
        );
    }

    #[test]
    fn line_plan_sizes_palettes_by_the_largest_index() {
        let pal = |comp, flags, c| {
            line_plan(comp, flags, (0, 0, c, 1), &[8; 4], PAL).map(|p| p.unwrap().0.palette)
        };
        // pal8: 256 entries of 4 bytes, the last read at `4 * 255 + c`.
        assert_eq!(pal([0, 1, 0, 0, 8], 0, 0), Ok(Some(1021)));
        assert_eq!(pal([0, 1, 0, 0, 8], 0, 3), Ok(Some(1024)));
        // A shift leaves fewer bits of the byte read.
        assert_eq!(pal([0, 1, 0, 4, 4], 0, 0), Ok(Some(4 * 15 + 1)));
        // A bit-packed byte holds at most 255.
        assert_eq!(pal([0, 1, 0, 0, 1], BITSTREAM, 2), Ok(Some(4 + 3)));
        // `4 * val + c` must stay in `int` where `val` is one.
        assert_eq!(
            pal([0, 32, 0, 0, 29], BITSTREAM, 3),
            Ok(Some(4 * ((1 << 29) - 1) + 4))
        );
        assert_eq!(
            pal([0, 32, 0, 0, 30], BITSTREAM, 0),
            Err(ImageLineError::InvalidDescriptor)
        );
        assert_eq!(
            line_plan([0, 1, 0, 0, 8], 0, (0, 0, 0, 1), &[8; 4], READ)
                .map(|p| p.unwrap().0.palette),
            Ok(None)
        );
    }

    #[test]
    fn row0_is_the_first_or_last_whole_row() {
        assert_eq!(row0(0, -4), 0);
        assert_eq!(row0(10, 4), 0);
        assert_eq!(row0(10, 0), 0);
        assert_eq!(row0(10, -4), 8);
        assert_eq!(row0(12, -4), 8);
        assert_eq!(row0(13, -4), 12);
        assert_eq!(row0(3, -4), 0);
        assert_eq!(row0(5, isize::MIN), 0);
    }

    #[test]
    fn inside_checks_both_ends() {
        assert!(inside(10, 0, &(0..10)));
        assert!(inside(10, 4, &(-4..6)));
        assert!(!inside(10, 4, &(-5..6)));
        assert!(!inside(10, 4, &(0..7)));
        assert!(inside(0, 0, &(0..0)));
        assert!(!inside(10, 0, &(i64::MIN..0)));
        assert!(!inside(10, 9, &(0..i64::MAX)));
    }

    /// A descriptor with one component, `comp`, in slot 0.
    fn one_component(comp: [c_int; 5], flags: PixelFormatFlags) -> PixelFormatDescriptor {
        let mut d = PixelFormatDescriptor::new();
        let mut m = d.as_mut();
        m.set_component_count(1);
        m.set_flags(flags);
        let [plane, step, offset, shift, depth] = comp;
        m.components_mut()
            .get_mut(0)
            .unwrap()
            .set(ComponentDescriptor::from_raw(raw(
                plane, step, offset, shift, depth,
            )));
        d
    }

    #[test]
    fn line_args_check_positions_and_planes() {
        use ImageLineError::{Component, OutOfRange, OutsidePalette, OutsidePlane};
        let d = one_component([1, 1, 0, 0, 8], PixelFormatFlags::empty());
        let desc = d.as_ref();
        let geometry = |lens| (lens, &[4isize, -4, 4, 4]);
        let args = |lens, at, access| LineArgs::new(desc, geometry(lens), at, access);
        // Plane 1 holds three rows of four bytes, bottom-up: row 0 is last.
        let lens = [0, 12, 0, 0];
        let ok = args(lens, (1, 2, 0, 3), READ).unwrap().unwrap();
        assert_eq!(ok.at, [None, Some(8), None, None]);
        assert_eq!(ok.linesizes, [0, -4, 0, 0]);
        assert_eq!((ok.x, ok.y, ok.c, ok.w), (1, 2, 0, 3));
        // Row 2 is the slice's first; the line may run on into row 1.
        assert!(args(lens, (6, 2, 0, 3), READ).is_ok());
        assert_eq!(args(lens, (10, 2, 0, 3), READ), Err(OutsidePlane));
        assert_eq!(args(lens, (0, 3, 0, 1), WRITE), Err(OutsidePlane));
        assert_eq!(args(lens, (0, 0, 4, 1), READ), Err(Component));
        assert_eq!(args(lens, (1 << 31, 0, 0, 1), READ), Err(OutOfRange));
        assert_eq!(args(lens, (0, 1 << 31, 0, 1), READ), Err(OutOfRange));
        assert_eq!(args(lens, (0, 0, 0, 1 << 31), READ), Err(OutOfRange));
        // Nothing to move, or an absent component: no plane is reached.
        assert_eq!(args([0; 4], (99, 99, 0, 0), READ), Ok(None));
        assert_eq!(args([0; 4], (0, 0, 2, 1), WRITE), Ok(None));
        // The palette is plane 1 from its row 0: the last row of a bottom-up
        // plane, from which the entries run on.
        assert_eq!(args(lens, (0, 0, 0, 1), PAL), Err(OutsidePalette));
        let pal = args([0, 8 + 1021, 0, 0], (0, 0, 0, 1), PAL);
        assert_eq!(pal, Err(OutsidePalette));
        // pal8's indices in plane 0, and its palette in plane 1.
        let d = one_component([0, 1, 0, 0, 8], PixelFormatFlags::PALETTE);
        let pal8 = |lens| LineArgs::new(d.as_ref(), (lens, &[4, 0, 0, 0]), (0, 0, 0, 1), PAL);
        assert_eq!(pal8([4, 1020, 0, 0]), Err(OutsidePalette));
        let ok = pal8([4, 1021, 0, 0]).unwrap().unwrap();
        assert_eq!(ok.at, [Some(0), Some(0), None, None]);
        assert_eq!(ok.linesizes, [4, 0, 0, 0]);
    }

    #[test]
    fn whole_descriptor_checks_mirror_c_arithmetic() {
        let d = yuv420p_like();
        // 8 + 8/8 + 8/8 bits a pixel: chroma 1 x 2 subsampled.
        assert_eq!(d.as_ref().checked_bits_per_pixel(), Ok(10));
        assert_eq!(d.as_ref().checked_padded_bits_per_pixel(), Ok(10));
        let refused = |edit: &dyn Fn(&mut PixelFormatDescriptorMut<'_>)| {
            let mut d = yuv420p_like();
            edit(&mut d.as_mut());
            (
                d.as_ref().checked_bits_per_pixel(),
                d.as_ref().checked_padded_bits_per_pixel(),
            )
        };
        let both = (Err(InvalidDescriptor), Err(InvalidDescriptor));
        assert_eq!(refused(&|m| m.set_component_count(5)), both);
        assert_eq!(refused(&|m| m.set_log2_chroma_w(31)), both);
        let comp = |i: usize, f: [i32; 5]| {
            move |m: &mut PixelFormatDescriptorMut<'_>| {
                let [p, s, o, sh, d] = f;
                m.components_mut()
                    .get_mut(i)
                    .unwrap()
                    .set(ComponentDescriptor::from_raw(raw(p, s, o, sh, d)));
            }
        };
        // A negative depth, or one whose scaling overflows `int`.
        assert_eq!(
            refused(&comp(0, [0, 1, 0, 0, -1])).0,
            Err(InvalidDescriptor)
        );
        assert_eq!(
            refused(&comp(0, [0, 1, 0, 0, 1 << 28])).0,
            Err(InvalidDescriptor)
        );
        // Chroma depths are not scaled, but their sum may overflow.
        assert_eq!(
            refused(&comp(1, [1, 1, 0, 0, 1 << 28])).0,
            Ok((64 + (1 << 28) + 8) >> 3)
        );
        assert_eq!(
            refused(&comp(1, [1, 1, 0, 0, c_int::MAX])).0,
            Err(InvalidDescriptor)
        );
        // A used component's plane indexes C's `steps[4]`.
        assert_eq!(refused(&comp(2, [4, 1, 0, 0, 8])).1, Err(InvalidDescriptor));
        assert_eq!(
            refused(&comp(2, [-1, 1, 0, 0, 8])).1,
            Err(InvalidDescriptor)
        );
        assert_eq!(
            refused(&comp(0, [0, -1, 0, 0, 8])).1,
            Err(InvalidDescriptor)
        );
        // `bits * 8` overflowing, unless the format is bit-packed.
        let wide = comp(1, [1, 1 << 28, 0, 0, 8]);
        assert_eq!(refused(&wide).1, Err(InvalidDescriptor));
        let packed = |m: &mut PixelFormatDescriptorMut<'_>| {
            wide(m);
            m.set_flags(PixelFormatFlags::BITSTREAM);
        };
        assert_eq!(refused(&packed).1, Ok((8 + (1 << 28) + 1) >> 3));
        // Unused slots past the count are never read.
        assert_eq!(refused(&comp(3, [9, -1, 0, 0, -1])), (Ok(10), Ok(10)));
    }

    #[test]
    fn c_shl_is_defined_shifts_only() {
        assert_eq!(c_shl(1, 0), Some(1));
        assert_eq!(c_shl(1, 30), Some(1 << 30));
        assert_eq!(c_shl(1, 31), None);
        assert_eq!(c_shl(0, 31), Some(0));
        assert_eq!(c_shl(3, 32), None);
        assert_eq!(c_shl(3, -1), None);
        assert_eq!(c_shl(-1, 0), None);
    }

    #[test]
    fn line_errors_display_their_cause() {
        let all = [
            ImageLineError::Component,
            ImageLineError::OutOfRange,
            ImageLineError::InvalidDescriptor,
            ImageLineError::OutsidePlane,
            ImageLineError::OutsidePalette,
        ];
        let texts: Vec<String> = all.iter().map(ToString::to_string).collect();
        for (i, t) in texts.iter().enumerate() {
            assert!(!t.is_empty());
            assert!(!texts[..i].contains(t), "{t}");
        }
        assert!(!InvalidDescriptor.to_string().is_empty());
        let _: &dyn std::error::Error = &InvalidDescriptor;
        let _: &dyn std::error::Error = &ImageLineError::Component;
    }

    #[test]
    fn line_wrappers_return_early_without_reaching_c() {
        // No element to move: neither the planes nor `dst` are looked at.
        let d = one_component([0, 1, 0, 0, 8], PixelFormatFlags::empty());
        let mut dst: [u32; 0] = [];
        assert_eq!(
            read_image_line(&mut dst, Planes::default(), d.as_ref(), 7, 7, 0, true),
            Ok(())
        );
        let mut planes = PlanesMut::default();
        assert_eq!(
            write_image_line::<u16>(&[], &mut planes, d.as_ref(), 0, 0, 0),
            Ok(())
        );
        // An absent component reads nothing, and leaves `dst` as it is.
        let mut dst = [9u16; 3];
        assert_eq!(
            read_image_line_u16(&mut dst, Planes::default(), d.as_ref(), 0, 0, 1, false),
            Ok(())
        );
        assert_eq!(dst, [9; 3]);
        assert_eq!(
            write_image_line_u16(&[1], &mut planes, d.as_ref(), 0, 0, 4),
            Err(ImageLineError::Component)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptors_iterator_is_fused_and_walks_the_table() {
        let mut it = descriptors();
        let first = it.next().unwrap();
        assert_eq!(first.pixel_format(), Some(PixelFormat::YUV420P));
        let rest = it.by_ref().count();
        assert!(rest > 100, "{rest}");
        assert!(it.next().is_none());
        assert!(it.next().is_none());
        // Every one has a name, and the walk from any entry matches.
        assert!(descriptors().all(|d| d.name().is_some()));
        let mut from = first;
        for d in descriptors().skip(1) {
            let next = from.next_in_table().unwrap();
            assert_eq!(next.as_ptr(), d.as_ptr());
            from = next;
        }
        assert!(from.next_in_table().is_none());
        // A copy is not in the table.
        let copy = first.get();
        assert_eq!(copy.as_ref().pixel_format(), None);
        assert!(copy.as_ref().next_in_table().is_none());
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
    )]
    /// The pixel format and color enums of `pixfmt.h`.
    mod enums {
        use core::ffi::{c_int, c_uint};
        use core::mem::{align_of, size_of};
        use std::collections::HashSet;

        use super::super::*;

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
            let ext_nb =
                ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_NB);
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
                let name = PixelFormat::from_raw(ffi::AVPixelFormat(raw)).constant_name();
                let name = name.unwrap_or_else(|| panic!("raw {raw} has no name"));
                assert!(names.insert(name), "{name} names two values");
            }
            assert_eq!(names.len(), (nb + 1) as usize);
            assert_eq!(
                PixelFormat::from_raw(ffi::AVPixelFormat(nb)).constant_name(),
                None
            );
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
                assert_eq!(loc.constant_name(), Some(name));
            }
        }

        #[test]
        fn chroma_location_debug_shows_unnamed_values_raw() {
            for raw in CHROMA_LOCATION_UNNAMED {
                let loc = ChromaLocation::from_raw(ffi::AVChromaLocation(raw));
                assert_eq!(loc.constant_name(), None);
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
        #[expect(clippy::clone_on_copy, reason = "exercises the explicit `Clone` impl")]
        fn chroma_location_clone_and_hash_follow_the_encoding() {
            use std::collections::HashSet;
            let all: Vec<ChromaLocation> = CHROMA_LOCATION_NAMED
                .map(|(v, _)| v)
                .into_iter()
                .chain(
                    CHROMA_LOCATION_UNNAMED
                        .map(|r| ChromaLocation::from_raw(ffi::AVChromaLocation(r))),
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
}
