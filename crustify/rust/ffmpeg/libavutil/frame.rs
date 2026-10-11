//! Safe wrappers over `libavutil/frame.h`.
//!
//! [`SideDataDescriptor`] is `AVSideDataDescriptor`: a side data type's
//! human-readable name and its [`SideDataProps`]. C keeps its descriptors in
//! one static, never-written table indexed by side data type, so a
//! descriptor has no owner: a Rust-held one is a value, and its `name` is
//! borrowed for the whole program. [`SideDataProps`] is `AVSideDataProps`, the
//! set of property bits C stores in a descriptor's `props` and filters side
//! data arrays by; it is a plain `Copy` value.
//!
//! [`RegionOfInterest`] is `AVRegionOfInterest`, one element of a
//! `REGIONS_OF_INTEREST` side data array: a rectangle of the frame and the
//! encoder quality to give it. Nothing allocates one on its own, so it is a
//! value type, reached as a [`RegionOfInterestRef`] or
//! [`RegionOfInterestMut`], its quantisation offset as a [`RationalRef`] or
//! [`RationalMut`] over its own.

use core::ffi::{CStr, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Sub, SubAssign};
use core::ptr::{self, addr_of, addr_of_mut};

use ffibox::{CPlainElem, CZeroable, define_ctype};

use super::ffi;
use super::rational::{Rational, RationalMut, RationalRef};

/// Wraps: AVFrameSideDataType
/// Handwritten
///
/// The kind of side data attached to a frame, which decides how its payload
/// is laid out: a C struct (`AVStereo3D`, `AVMasteringDisplayMetadata`, ...),
/// an array of them, a scalar, or opaque bytes.
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// the public `AVFrameSideData.type` field its users write directly, so it may
/// hold any `unsigned`, and a Rust view of that memory must accept every one.
/// C itself accepts every value: its descriptor and name lookups return
/// nothing for values it does not name, and its side-data lists compare types
/// for equality only. Match on the associated constants, with a `_` arm for
/// the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::frame::SideDataType;
/// fn is_hdr_metadata(t: SideDataType) -> bool {
///     matches!(
///         t,
///         SideDataType::DYNAMIC_HDR_PLUS
///             | SideDataType::DYNAMIC_HDR_VIVID
///             | SideDataType::DYNAMIC_HDR_SMPTE_2094_APP5
///     )
/// }
/// assert!(is_hdr_metadata(SideDataType::DYNAMIC_HDR_VIVID));
/// assert!(!is_hdr_metadata(SideDataType::ICC_PROFILE));
/// ```
///
/// C has no count sentinel for this enum, so there is no `COUNT`, and no
/// value stands for "no side data", so there is no `Default`.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SideDataType(ffi::AVFrameSideDataType);

impl SideDataType {
    /// The `AVPanScan` struct defined in libavcodec.
    pub const PAN_SCAN: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_PANSCAN);
    /// ATSC A53 Part 4 closed captions: the CC bitstream as bytes.
    pub const A53_CC: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_A53_CC);
    /// Stereoscopic 3D metadata, an `AVStereo3D` (`libavutil/stereo3d.h`).
    pub const STEREO_3D: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_STEREO3D);
    /// An `AVMatrixEncoding` value
    /// ([`MatrixEncoding`](super::channel_layout::MatrixEncoding)).
    pub const MATRIX_ENCODING: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_MATRIXENCODING);
    /// Metadata for a downmix procedure, an `AVDownmixInfo`
    /// (`libavutil/downmix_info.h`).
    pub const DOWNMIX_INFO: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DOWNMIX_INFO);
    /// ReplayGain information, an `AVReplayGain`.
    pub const REPLAY_GAIN: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_REPLAYGAIN);
    /// A 3x3 affine transformation matrix to apply for correct presentation
    /// (`libavutil/display.h`).
    pub const DISPLAY_MATRIX: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DISPLAYMATRIX);
    /// Active Format Description: one byte holding an
    /// `AVActiveFormatDescription`, as ETSI TS 101 154 specifies.
    pub const AFD: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_AFD);
    /// Motion vectors exported by some codecs, `AVMotionVector`s
    /// (`libavutil/motion_vector.h`).
    pub const MOTION_VECTORS: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_MOTION_VECTORS);
    /// The number of samples to skip from the start and end of the frame,
    /// laid out as `AV_PKT_DATA_SKIP_SAMPLES`.
    pub const SKIP_SAMPLES: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_SKIP_SAMPLES);
    /// An audio frame's `AVAudioServiceType` (`libavcodec/avcodec.h`).
    pub const AUDIO_SERVICE_TYPE: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_AUDIO_SERVICE_TYPE);
    /// Mastering display color volume, an `AVMasteringDisplayMetadata`.
    pub const MASTERING_DISPLAY_METADATA: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_MASTERING_DISPLAY_METADATA);
    /// The GOP timecode in 25-bit timecode format, as a 64-bit integer.
    pub const GOP_TIMECODE: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_GOP_TIMECODE);
    /// An `AVSphericalMapping` (`libavutil/spherical.h`).
    pub const SPHERICAL: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_SPHERICAL);
    /// Content light level (CTA-861.3), an `AVContentLightMetadata`.
    pub const CONTENT_LIGHT_LEVEL: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_CONTENT_LIGHT_LEVEL);
    /// An ICC profile (ISO 15076-1) as opaque bytes, optionally named by the
    /// side data's `name` metadata entry.
    pub const ICC_PROFILE: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_ICC_PROFILE);
    /// SMPTE ST 12-1 timecodes: four `u32`, the first counting how many (1-3)
    /// of the others are used.
    pub const S12M_TIMECODE: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_S12M_TIMECODE);
    /// HDR10+ dynamic metadata (SMPTE 2094-40), an `AVDynamicHDRPlus`.
    pub const DYNAMIC_HDR_PLUS: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DYNAMIC_HDR_PLUS);
    /// Regions of interest, an array of `AVRegionOfInterest` whose length is
    /// the payload size over the first element's `self_size`.
    pub const REGIONS_OF_INTEREST: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_REGIONS_OF_INTEREST);
    /// Encoding parameters of a video frame, an `AVVideoEncParams`.
    pub const VIDEO_ENC_PARAMS: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_VIDEO_ENC_PARAMS);
    /// An H.264 or H.265 user data unregistered SEI message: a 16-byte UUID
    /// followed by the payload bytes.
    pub const SEI_UNREGISTERED: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_SEI_UNREGISTERED);
    /// Film grain parameters, an `AVFilmGrainParams`; a frame may carry
    /// several alternative sets.
    pub const FILM_GRAIN_PARAMS: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_FILM_GRAIN_PARAMS);
    /// Object detection and classification bounding boxes, an
    /// `AVDetectionBBoxHeader`.
    pub const DETECTION_BBOXES: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DETECTION_BBOXES);
    /// Raw Dolby Vision RPU bytes, NAL emulation bytes intact.
    pub const DOVI_RPU_BUFFER: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DOVI_RPU_BUFFER);
    /// Parsed Dolby Vision metadata, an `AVDOVIMetadata`
    /// (`libavutil/dovi_meta.h`).
    pub const DOVI_METADATA: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DOVI_METADATA);
    /// HDR Vivid dynamic metadata (CUVA 005.1-2021), an `AVDynamicHDRVivid`.
    pub const DYNAMIC_HDR_VIVID: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DYNAMIC_HDR_VIVID);
    /// Ambient viewing environment metadata, as H.274 defines it.
    pub const AMBIENT_VIEWING_ENVIRONMENT: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT);
    /// Encoder hints about the changed and unchanged parts of a frame.
    pub const VIDEO_HINT: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_VIDEO_HINT);
    /// Raw LCEVC payload bytes, NAL emulation bytes intact.
    pub const LCEVC: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_LCEVC);
    /// The view ID of a multi-view video frame, as an `int`.
    pub const VIEW_ID: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_VIEW_ID);
    /// 3D reference display widths, viewing distances and stereo pairs, an
    /// `AV3DReferenceDisplaysInfo` (`libavutil/tdrdi.h`;
    /// `AV_FRAME_DATA_3D_REFERENCE_DISPLAYS`).
    pub const REFERENCE_DISPLAYS_3D: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_3D_REFERENCE_DISPLAYS);
    /// EXIF metadata bytes, starting with a TIFF header's byte-order mark.
    pub const EXIF: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_EXIF);
    /// HDR dynamic metadata (SMPTE 2094-50), an `AVDynamicHDRSmpte2094App5`.
    pub const DYNAMIC_HDR_SMPTE_2094_APP5: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DYNAMIC_HDR_SMPTE_2094_APP5);
    /// IAMF mix gain parameter data, an `AVIAMFParamDefinition`.
    pub const IAMF_MIX_GAIN_PARAM: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_IAMF_MIX_GAIN_PARAM);
    /// IAMF demixing info parameter data, an `AVIAMFParamDefinition`.
    pub const IAMF_DEMIXING_INFO_PARAM: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_IAMF_DEMIXING_INFO_PARAM);
    /// IAMF recon gain info parameter data, an `AVIAMFParamDefinition`.
    pub const IAMF_RECON_GAIN_INFO_PARAM: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_IAMF_RECON_GAIN_INFO_PARAM);
    /// Color information from a RAW camera codec, an `AVRawColorParams`
    /// (`libavutil/raw_color_params.h`).
    pub const RAW_COLOR_PARAMS: Self =
        Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_RAW_COLOR_PARAMS);
    /// A downmix remixing matrix, an `AVDownmixMatrix`
    /// (`libavutil/downmix_info.h`).
    pub const DOWNMIX_MATRIX: Self = Self(ffi::AVFrameSideDataType::AV_FRAME_DATA_DOWNMIX_MATRIX);

    /// The side data type C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVFrameSideDataType) -> Self {
        Self(raw)
    }

    /// The C encoding of this side data type.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVFrameSideDataType {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
        Some(match self {
            Self::PAN_SCAN => "PAN_SCAN",
            Self::A53_CC => "A53_CC",
            Self::STEREO_3D => "STEREO_3D",
            Self::MATRIX_ENCODING => "MATRIX_ENCODING",
            Self::DOWNMIX_INFO => "DOWNMIX_INFO",
            Self::REPLAY_GAIN => "REPLAY_GAIN",
            Self::DISPLAY_MATRIX => "DISPLAY_MATRIX",
            Self::AFD => "AFD",
            Self::MOTION_VECTORS => "MOTION_VECTORS",
            Self::SKIP_SAMPLES => "SKIP_SAMPLES",
            Self::AUDIO_SERVICE_TYPE => "AUDIO_SERVICE_TYPE",
            Self::MASTERING_DISPLAY_METADATA => "MASTERING_DISPLAY_METADATA",
            Self::GOP_TIMECODE => "GOP_TIMECODE",
            Self::SPHERICAL => "SPHERICAL",
            Self::CONTENT_LIGHT_LEVEL => "CONTENT_LIGHT_LEVEL",
            Self::ICC_PROFILE => "ICC_PROFILE",
            Self::S12M_TIMECODE => "S12M_TIMECODE",
            Self::DYNAMIC_HDR_PLUS => "DYNAMIC_HDR_PLUS",
            Self::REGIONS_OF_INTEREST => "REGIONS_OF_INTEREST",
            Self::VIDEO_ENC_PARAMS => "VIDEO_ENC_PARAMS",
            Self::SEI_UNREGISTERED => "SEI_UNREGISTERED",
            Self::FILM_GRAIN_PARAMS => "FILM_GRAIN_PARAMS",
            Self::DETECTION_BBOXES => "DETECTION_BBOXES",
            Self::DOVI_RPU_BUFFER => "DOVI_RPU_BUFFER",
            Self::DOVI_METADATA => "DOVI_METADATA",
            Self::DYNAMIC_HDR_VIVID => "DYNAMIC_HDR_VIVID",
            Self::AMBIENT_VIEWING_ENVIRONMENT => "AMBIENT_VIEWING_ENVIRONMENT",
            Self::VIDEO_HINT => "VIDEO_HINT",
            Self::LCEVC => "LCEVC",
            Self::VIEW_ID => "VIEW_ID",
            Self::REFERENCE_DISPLAYS_3D => "REFERENCE_DISPLAYS_3D",
            Self::EXIF => "EXIF",
            Self::DYNAMIC_HDR_SMPTE_2094_APP5 => "DYNAMIC_HDR_SMPTE_2094_APP5",
            Self::IAMF_MIX_GAIN_PARAM => "IAMF_MIX_GAIN_PARAM",
            Self::IAMF_DEMIXING_INFO_PARAM => "IAMF_DEMIXING_INFO_PARAM",
            Self::IAMF_RECON_GAIN_INFO_PARAM => "IAMF_RECON_GAIN_INFO_PARAM",
            Self::RAW_COLOR_PARAMS => "RAW_COLOR_PARAMS",
            Self::DOWNMIX_MATRIX => "DOWNMIX_MATRIX",
            _ => return None,
        })
    }
}

impl fmt::Debug for SideDataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.constant_name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("SideDataType").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVFrameSideDataType> for SideDataType {
    #[inline]
    fn from(raw: ffi::AVFrameSideDataType) -> Self {
        Self::from_raw(raw)
    }
}

impl From<SideDataType> for ffi::AVFrameSideDataType {
    #[inline]
    fn from(t: SideDataType) -> Self {
        t.into_raw()
    }
}

// SAFETY: `SideDataType` is `#[repr(transparent)]` over
// `ffi::AVFrameSideDataType`, itself `#[repr(transparent)]` over a `c_uint`,
// so every bit pattern is a valid value, and it is a value with no C-side
// identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for SideDataType {}

/// Wraps: AVActiveFormatDescription
/// Handwritten
///
/// An Active Format Description (ETSI TS 101 154): which part of a coded
/// picture carries the image the producer means to be shown, and which part
/// is safe to crop when the picture is displayed at another aspect ratio.
///
/// A newtype over the C enum rather than a Rust `enum`: no C signature takes
/// or returns the enum, and its values travel as the single byte of
/// `AV_FRAME_DATA_AFD` frame and `AV_PKT_DATA_AFD` packet side data, which
/// decoders fill with the 4-bit code they parsed. The reserved codes (0 to 7,
/// and 12) and any other byte reach readers of that payload, so a Rust view of
/// it must accept every value. Match on the associated constants, with a `_`
/// arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::frame::ActiveFormatDescription as Afd;
/// fn is_pillarboxed_4_3(afd: Afd) -> bool {
///     matches!(afd, Afd::ASPECT_4_3 | Afd::ASPECT_4_3_SP_14_9)
/// }
/// assert!(is_pillarboxed_4_3(Afd::from_byte(9)));
/// assert!(!is_pillarboxed_4_3(Afd::from_byte(0)));
/// ```
///
/// The C names `AV_AFD_4_3`, `AV_AFD_16_9`, `AV_AFD_14_9`,
/// `AV_AFD_4_3_SP_14_9` and `AV_AFD_16_9_SP_14_9` are not Rust identifiers,
/// so their constants carry an `ASPECT_` prefix. There is no `Default`: the
/// zeroed value is a reserved code, and C has no default description.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActiveFormatDescription(ffi::AVActiveFormatDescription);

impl ActiveFormatDescription {
    /// The image fills the coded frame (code `1000`).
    pub const SAME: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_SAME);
    /// A 4:3 image, centred in the coded frame (code `1001`).
    pub const ASPECT_4_3: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_4_3);
    /// A 16:9 image, centred in the coded frame (code `1010`).
    pub const ASPECT_16_9: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_16_9);
    /// A 14:9 image, centred in the coded frame (code `1011`).
    pub const ASPECT_14_9: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_14_9);
    /// A 4:3 image whose centred 14:9 part is safe to show alone (code
    /// `1101`).
    pub const ASPECT_4_3_SP_14_9: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_4_3_SP_14_9);
    /// A 16:9 image whose centred 14:9 part is safe to show alone (code
    /// `1110`).
    pub const ASPECT_16_9_SP_14_9: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_16_9_SP_14_9);
    /// A 16:9 image whose centred 4:3 part is safe to show alone (code
    /// `1111`).
    pub const SP_4_3: Self = Self(ffi::AVActiveFormatDescription::AV_AFD_SP_4_3);

    /// The description C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVActiveFormatDescription) -> Self {
        Self(raw)
    }

    /// The C encoding of this description.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVActiveFormatDescription {
        self.0
    }

    /// The description stored as `byte`, the payload of `AV_FRAME_DATA_AFD`
    /// and `AV_PKT_DATA_AFD` side data, whatever its value.
    #[inline]
    pub const fn from_byte(byte: u8) -> Self {
        Self(ffi::AVActiveFormatDescription(byte as _))
    }

    /// This description as a side-data payload byte, or `None` for a value
    /// above 255 that no byte holds.
    #[inline]
    pub const fn to_byte(self) -> Option<u8> {
        if self.0.0 <= u8::MAX as _ {
            Some(self.0.0 as u8)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
        Some(match self {
            Self::SAME => "SAME",
            Self::ASPECT_4_3 => "ASPECT_4_3",
            Self::ASPECT_16_9 => "ASPECT_16_9",
            Self::ASPECT_14_9 => "ASPECT_14_9",
            Self::ASPECT_4_3_SP_14_9 => "ASPECT_4_3_SP_14_9",
            Self::ASPECT_16_9_SP_14_9 => "ASPECT_16_9_SP_14_9",
            Self::SP_4_3 => "SP_4_3",
            _ => return None,
        })
    }
}

impl fmt::Debug for ActiveFormatDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.constant_name() {
            Some(name) => f.write_str(name),
            None => f
                .debug_tuple("ActiveFormatDescription")
                .field(&self.0.0)
                .finish(),
        }
    }
}

impl From<ffi::AVActiveFormatDescription> for ActiveFormatDescription {
    #[inline]
    fn from(raw: ffi::AVActiveFormatDescription) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ActiveFormatDescription> for ffi::AVActiveFormatDescription {
    #[inline]
    fn from(afd: ActiveFormatDescription) -> Self {
        afd.into_raw()
    }
}

impl From<u8> for ActiveFormatDescription {
    #[inline]
    fn from(byte: u8) -> Self {
        Self::from_byte(byte)
    }
}

// SAFETY: `ActiveFormatDescription` is `#[repr(transparent)]` over
// `ffi::AVActiveFormatDescription`, itself `#[repr(transparent)]` over a
// `c_uint`, so every bit pattern is a valid value, and it is a value with no
// C-side identity: a copy read out of C memory is the whole of it.
unsafe impl CPlainElem for ActiveFormatDescription {}

/// Wraps: AVSideDataProps
/// Handwritten
///
/// A set of `AVSideDataProps` bits: what a side data type's payload depends
/// on (the video dimensions, its color encoding, the channel layout), whether
/// it may also live in stream-global structures, and whether several entries
/// of the type may share one side data array.
///
/// C keeps the set in an `unsigned` (a descriptor's `props`) or an `int` (the
/// filter argument of `av_frame_side_data_remove_by_props`) and combines the
/// flags with bitwise operators, so any bit pattern is a valid value:
/// [`from_bits_retain`] keeps bits without a constant, and the set operations
/// carry them along.
///
/// [`from_bits_retain`]: SideDataProps::from_bits_retain
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SideDataProps(ffi::AVSideDataProps);

impl SideDataProps {
    /// The side data type can be used in stream-global structures; types
    /// without it are only meaningful per frame.
    pub const GLOBAL: Self = Self(ffi::AVSideDataProps::AV_SIDE_DATA_PROP_GLOBAL);
    /// Several entries of the side data type can meaningfully be present in
    /// one side data array.
    pub const MULTI: Self = Self(ffi::AVSideDataProps::AV_SIDE_DATA_PROP_MULTI);
    /// The side data depends on the video dimensions, and loses its meaning
    /// when the image is rescaled or cropped unless recomputed or adjusted.
    pub const SIZE_DEPENDENT: Self = Self(ffi::AVSideDataProps::AV_SIDE_DATA_PROP_SIZE_DEPENDENT);
    /// The side data depends on the video color space, and loses its meaning
    /// when the color encoding changes (other primaries or transfer
    /// characteristics).
    pub const COLOR_DEPENDENT: Self = Self(ffi::AVSideDataProps::AV_SIDE_DATA_PROP_COLOR_DEPENDENT);
    /// The side data depends on the channel layout, and loses its meaning
    /// when the audio is downmixed or upmixed unless recomputed or adjusted.
    pub const CHANNEL_DEPENDENT: Self =
        Self(ffi::AVSideDataProps::AV_SIDE_DATA_PROP_CHANNEL_DEPENDENT);

    /// Every named flag with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 5] = [
        (Self::GLOBAL, "GLOBAL"),
        (Self::MULTI, "MULTI"),
        (Self::SIZE_DEPENDENT, "SIZE_DEPENDENT"),
        (Self::COLOR_DEPENDENT, "COLOR_DEPENDENT"),
        (Self::CHANNEL_DEPENDENT, "CHANNEL_DEPENDENT"),
    ];

    /// The set with no flag.
    #[inline]
    pub const fn empty() -> Self {
        Self::from_bits_retain(0)
    }

    /// The set of every named flag.
    pub const fn all() -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < Self::KNOWN.len() {
            bits |= Self::KNOWN[i].0.bits();
            i += 1;
        }
        Self::from_bits_retain(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    pub const fn bits(self) -> c_uint {
        self.0.0
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    pub const fn from_bits_retain(bits: c_uint) -> Self {
        Self(ffi::AVSideDataProps(bits))
    }

    /// The set with `bits`, or `None` when a bit has no constant.
    #[inline]
    pub const fn from_bits(bits: c_uint) -> Option<Self> {
        if bits & !Self::all().bits() == 0 {
            Some(Self::from_bits_retain(bits))
        } else {
            None
        }
    }

    /// The named flags among `bits`, the others dropped.
    #[inline]
    pub const fn from_bits_truncate(bits: c_uint) -> Self {
        Self::from_bits_retain(bits & Self::all().bits())
    }

    /// Whether no bit is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.bits() == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.bits() & other.bits() == other.bits()
    }

    /// Whether `self` and `other` share a bit, the test C applies when it
    /// filters side data by properties.
    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }

    /// Sets every bit of `other`.
    #[inline]
    pub fn insert(&mut self, other: Self) {
        *self |= other;
    }

    /// Clears every bit of `other`.
    #[inline]
    pub fn remove(&mut self, other: Self) {
        *self -= other;
    }

    /// Sets every bit of `other` when `value`, and clears them otherwise.
    #[inline]
    pub fn set(&mut self, other: Self, value: bool) {
        if value {
            self.insert(other)
        } else {
            self.remove(other)
        }
    }
}

impl Default for SideDataProps {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for SideDataProps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SideDataProps(")?;
        let mut rest = self.bits();
        let mut first = true;
        for &(flag, name) in &Self::KNOWN {
            if self.contains(flag) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                rest &= !flag.bits();
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

impl BitOr for SideDataProps {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for SideDataProps {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl BitAnd for SideDataProps {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for SideDataProps {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

/// The bits of `self` not in `rhs`.
impl Sub for SideDataProps {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() & !rhs.bits())
    }
}

impl SubAssign for SideDataProps {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl From<ffi::AVSideDataProps> for SideDataProps {
    #[inline]
    fn from(raw: ffi::AVSideDataProps) -> Self {
        Self(raw)
    }
}

impl From<SideDataProps> for ffi::AVSideDataProps {
    #[inline]
    fn from(props: SideDataProps) -> Self {
        props.0
    }
}

define_ctype!(
    /// Wraps: AVSideDataDescriptor
    ///
    /// What a side data type is: its human-readable name and its
    /// [`SideDataProps`]. Its getters are on [`SideDataDescriptorRef`]
    /// (`as_ref()`), its setters on [`SideDataDescriptorMut`] (`as_mut()`).
    ///
    /// C's descriptors live in libavutil's static, never-written table, one
    /// per side data type; no C routine allocates, frees or copies one, so a
    /// Rust-held descriptor is a value with no owner, and copying one shares
    /// its name.
    ///
    /// Its `name` is borrowed for the whole program: it is NULL or a
    /// NUL-terminated string that is never freed or written, as C's table
    /// holds string literals, the setter takes only `&'static CStr`, and
    /// [`from_raw`](Self::from_raw) requires it. That is what lets the getter
    /// hand it out as a `&'static CStr`, which outlives the descriptor and can
    /// be set into another one. The `props` bits are not range-checked, by C
    /// or here.
    #[derive(Clone, Copy)]
    SideDataDescriptor,
    SideDataDescriptorRef,
    SideDataDescriptorMut,
    ffi::AVSideDataDescriptor
);

// SAFETY: all-zero is a NULL `name`, which the getter reads as `None`, and no
// property bit; every accessor takes it, and the type has no teardown to
// reject it.
unsafe impl CZeroable for SideDataDescriptor {}

// SAFETY: a descriptor is an `unsigned` and a pointer to a string that, by
// the type's invariant, lives for the whole program and is never written;
// nothing in it is tied to the thread that wrote it, so a value, or an
// exclusive handle to one, moves between threads as a `(u32, &'static CStr)`
// does.
unsafe impl Send for SideDataDescriptor {}
// SAFETY: as for `SideDataDescriptor`: the handle is the only access to the
// descriptor's fields, and the string it reaches is never written.
unsafe impl Send for SideDataDescriptorMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `SideDataDescriptorRef` has getters only, the struct has no interior
// mutability, and the string is never written by anyone, so concurrent shared
// access reads unchanging bytes, as through a `&(u32, &'static CStr)`.
unsafe impl Sync for SideDataDescriptor {}
// SAFETY: as for `Sync for SideDataDescriptor`: copies of a shared handle only
// read.
unsafe impl Send for SideDataDescriptorRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for SideDataDescriptorRef<'_> {}
// SAFETY: `&SideDataDescriptorMut` reaches the fields only through `as_ref`,
// a shared handle; the setters need `&mut`.
unsafe impl Sync for SideDataDescriptorMut<'_> {}

impl SideDataDescriptor {
    /// The all-zero descriptor: no name and no property.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::zeroed()
    }

    /// The descriptor named `name` with the properties `props`.
    #[inline]
    #[must_use]
    pub const fn named(name: &'static CStr, props: SideDataProps) -> Self {
        Self(
            ffi::AVSideDataDescriptor {
                name: name.as_ptr(),
                props: props.bits(),
            },
            core::marker::PhantomData,
        )
    }

    /// The descriptor C encodes as `raw`.
    ///
    /// # Safety
    ///
    /// `raw.name` must be NULL or point to a NUL-terminated string that is
    /// never freed or written for the rest of the program, such as a string
    /// literal or the name of a descriptor in C's own table.
    #[inline]
    #[must_use]
    pub const unsafe fn from_raw(raw: ffi::AVSideDataDescriptor) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this descriptor, for storing it in a C struct by
    /// value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVSideDataDescriptor {
        self.0
    }
}

impl Default for SideDataDescriptor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl From<SideDataDescriptor> for ffi::AVSideDataDescriptor {
    #[inline]
    fn from(d: SideDataDescriptor) -> Self {
        d.into_raw()
    }
}

/// Field by field: two descriptors are equal when their names hold the same
/// bytes (or are both absent) and their properties are equal.
impl PartialEq for SideDataDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().fields() == other.as_ref().fields()
    }
}

impl Eq for SideDataDescriptor {}

impl Hash for SideDataDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().fields().hash(state);
    }
}

impl fmt::Debug for SideDataDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for SideDataDescriptorRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SideDataDescriptor")
            .field("name", &self.name())
            .field("props", &self.props())
            .finish()
    }
}

impl SideDataDescriptorRef<'_> {
    /// Field: AVSideDataDescriptor.name
    ///
    /// The side data type's human-readable description, such as
    /// `Stereo 3D`; `None` for a descriptor without one, such as the
    /// all-zero descriptor. Every descriptor in C's table has one.
    #[must_use]
    pub fn name(&self) -> Option<&'static CStr> {
        // SAFETY: a read of the `name` pointer projected through the shared
        // handle's pointer to a live, initialised `AVSideDataDescriptor`.
        let p = unsafe { addr_of!((*self.as_ptr()).name).read() };
        // SAFETY: by the type's invariant, a non-null `name` addresses a
        // NUL-terminated string that is never freed or written for the rest
        // of the program.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// Field: AVSideDataDescriptor.props
    ///
    /// The side data type's [`SideDataProps`], unnamed bits included.
    #[must_use]
    pub fn props(&self) -> SideDataProps {
        // SAFETY: a read of the `props` field projected through the shared
        // handle's pointer to a live, initialised `AVSideDataDescriptor`;
        // every bit pattern is a valid `unsigned`.
        SideDataProps::from_bits_retain(unsafe { addr_of!((*self.as_ptr()).props).read() })
    }

    /// The fields in declaration order, the name by content, for comparison
    /// and hashing.
    fn fields(&self) -> (Option<&'static CStr>, c_uint) {
        (self.name(), self.props().bits())
    }

    /// A copy of the whole descriptor, independent of the place it was read
    /// from; its name is shared, as it lives for the whole program.
    #[must_use]
    pub fn get(&self) -> SideDataDescriptor {
        // SAFETY: a read of the whole live, initialised
        // `AVSideDataDescriptor` through the shared handle's pointer; it is
        // plain data, and its name pointer keeps the type's invariant in the
        // copy, since the string lives, unwritten, for the whole program.
        SideDataDescriptor(unsafe { self.as_ptr().read() }, core::marker::PhantomData)
    }
}

impl SideDataDescriptorMut<'_> {
    /// Field: AVSideDataDescriptor.name
    ///
    /// Sets the side data type's description, or clears it with `None`,
    /// leaving the properties as they are. The string is borrowed for the
    /// whole program, as C's own names are.
    pub fn set_name(&mut self, name: Option<&'static CStr>) {
        let p = name.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: a write of the `name` pointer projected through the
        // exclusive handle's pointer to a live `AVSideDataDescriptor`; `p` is
        // NULL or a `'static` string, never written, which keeps the type's
        // invariant, and the old value is borrowed, so nothing leaks.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).name).write(p) }
    }

    /// Field: AVSideDataDescriptor.props
    ///
    /// Sets the side data type's properties, unnamed bits included, leaving
    /// the name as it is.
    pub fn set_props(&mut self, props: SideDataProps) {
        // SAFETY: a write of the `props` field projected through the
        // exclusive handle's pointer to a live `AVSideDataDescriptor`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).props).write(props.bits()) }
    }

    /// Overwrites both fields with `d`'s.
    pub fn set(&mut self, d: SideDataDescriptor) {
        // SAFETY: a write of the whole `AVSideDataDescriptor` through the
        // exclusive handle's pointer to a live one; `d` keeps the type's
        // invariant, and the old value owns nothing to leak.
        unsafe { self.as_mut_ptr().write(d.into_raw()) }
    }
}

impl SideDataType {
    /// Wraps: av_frame_side_data_desc
    ///
    /// This side data type's descriptor in C's table, borrowed for the whole
    /// program: the table is static and never written, and its names are
    /// string literals, the descriptor's invariant. `None` for a value the
    /// table has no named entry for, such as one past the last type these
    /// bindings know.
    ///
    /// The handle is shared, as C's table is never written; a descriptor to
    /// edit is a copy, [`get`](SideDataDescriptorRef::get):
    ///
    /// ```compile_fail,E0599
    /// use ffmpeg::libavutil::frame::SideDataType;
    /// let mut desc = SideDataType::STEREO_3D.descriptor().unwrap();
    /// desc.set_name(None);
    /// ```
    ///
    /// ```
    /// use ffmpeg::libavutil::frame::{SideDataProps, SideDataType};
    /// let desc = SideDataType::DISPLAY_MATRIX.descriptor().unwrap();
    /// assert_eq!(desc.name(), Some(c"3x3 displaymatrix"));
    /// assert!(desc.props().contains(SideDataProps::GLOBAL));
    /// ```
    #[must_use]
    pub fn descriptor(self) -> Option<SideDataDescriptorRef<'static>> {
        // SAFETY: `av_frame_side_data_desc` takes the enum by value, checks
        // it against the bounds of its static `const` table, and returns NULL
        // or the address of a named entry, which lives, unwritten, for the
        // whole program; its `name` is a string literal, the descriptor's
        // invariant, and a shared `'static` handle only reads it.
        unsafe {
            let p = ffi::av_frame_side_data_desc(self.into_raw());
            SideDataDescriptorRef::from_ptr(p.cast_mut())
        }
    }

    /// Wraps: av_frame_side_data_name
    ///
    /// This side data type's human-readable name, the
    /// [`name`](SideDataDescriptorRef::name) of its
    /// [`descriptor`](Self::descriptor), such as `Stereo 3D`; `None` when the
    /// table has no named entry for it.
    ///
    /// ```
    /// use ffmpeg::libavutil::frame::SideDataType;
    /// assert_eq!(SideDataType::STEREO_3D.name(), Some(c"Stereo 3D"));
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: `av_frame_side_data_name` takes the enum by value and
        // returns NULL or the `name` of an entry of the same static table,
        // a string literal.
        let name = unsafe { ffi::av_frame_side_data_name(self.into_raw()) };
        // SAFETY: a non-NULL `name` is a NUL-terminated string literal,
        // immutable for the life of the process.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }
}

define_ctype!(
    /// Wraps: AVRegionOfInterest
    ///
    /// A rectangle of a video frame and the encoder quality to give it: one
    /// element of `REGIONS_OF_INTEREST` frame side data
    /// ([`SideDataType::REGIONS_OF_INTEREST`]). Its getters are on
    /// [`RegionOfInterestRef`] (`as_ref()`), its setters on
    /// [`RegionOfInterestMut`] (`as_mut()`).
    ///
    /// The rectangle is given by the distances in pixels from the frame's top
    /// edge to its top and bottom edges, and from the frame's left edge to its
    /// left and right edges. The quantisation offset ranges from -1 (best
    /// quality) to +1 (worst), 0 asking for no change.
    ///
    /// C stores these as an array in side data and nothing allocates, frees
    /// or copies one on its own, so it is a value type with no owner. Each
    /// element records its own size, `self_size`, which readers of the array
    /// take as its stride and as the divisor of the payload size, and reject
    /// when it is 0; [`new`](Self::new) sets it to the size of this struct.
    /// No field is checked, by libavutil or here: encoders reject regions
    /// they cannot use, such as a quantisation offset with a zero
    /// denominator.
    ///
    /// The offset's exclusive handle freezes the region's while it lives:
    ///
    /// ```compile_fail,E0499
    /// use ffmpeg::libavutil::frame::RegionOfInterest;
    /// let mut roi = RegionOfInterest::default();
    /// let mut region = roi.as_mut();
    /// let mut offset = region.qoffset_mut();
    /// region.set_top(1);
    /// offset.set_num(1);
    /// ```
    #[derive(Clone, Copy)]
    RegionOfInterest,
    RegionOfInterestRef,
    RegionOfInterestMut,
    ffi::AVRegionOfInterest
);

// SAFETY: all-zero is an empty region with a `0/0` offset and a `self_size`
// of 0, all plain integers every accessor reads as they are; the type has no
// teardown to reject it, and C's readers of side data reject a zero
// `self_size` with an error, not by misbehaving.
unsafe impl CZeroable for RegionOfInterest {}

// SAFETY: an `AVRegionOfInterest` is 28 bytes of plain data, a `uint32_t`
// and six `int`s, with no pointer, sub-allocation or tie to the thread that
// wrote them, so a value, or an exclusive handle to one, moves between
// threads as a `(u32, [i32; 6])` does.
unsafe impl Send for RegionOfInterest {}
// SAFETY: as for `RegionOfInterest`: the handle is the only access to the
// fields, and the offset handle it lends borrows it.
unsafe impl Send for RegionOfInterestMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `RegionOfInterestRef` has getters only, which lend a getter-only
// `RationalRef`, and the struct has no interior mutability, so concurrent
// shared access reads unchanging bytes, as through `&(u32, [i32; 6])`.
unsafe impl Sync for RegionOfInterest {}
// SAFETY: as for `Sync for RegionOfInterest`: copies of a shared handle only
// read.
unsafe impl Send for RegionOfInterestRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for RegionOfInterestRef<'_> {}
// SAFETY: `&RegionOfInterestMut` reaches the fields only through `as_ref`, a
// shared handle; the setters and `qoffset_mut` need `&mut`.
unsafe impl Sync for RegionOfInterestMut<'_> {}

impl RegionOfInterest {
    /// The `self_size` C requires of every element: the size of
    /// `AVRegionOfInterest` in bytes.
    pub const SELF_SIZE: u32 = size_of::<ffi::AVRegionOfInterest>() as u32;

    /// The region whose edges lie `top`, `bottom`, `left` and `right` pixels
    /// from the frame's top and left edges, encoded with the quantisation
    /// offset `qoffset`; its `self_size` is [`SELF_SIZE`](Self::SELF_SIZE).
    #[inline]
    #[must_use]
    pub const fn new(top: i32, bottom: i32, left: i32, right: i32, qoffset: Rational) -> Self {
        Self::from_raw(ffi::AVRegionOfInterest {
            self_size: Self::SELF_SIZE,
            top,
            bottom,
            left,
            right,
            qoffset: qoffset.into_raw(),
        })
    }

    /// The region C encodes as `raw`, whatever its fields.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVRegionOfInterest) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this region, for storing it in side data.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVRegionOfInterest {
        self.0
    }
}

/// An empty region at the frame's top left corner with no quality change (a
/// `0/1` offset), and the `self_size` C requires.
impl Default for RegionOfInterest {
    #[inline]
    fn default() -> Self {
        Self::new(
            0,
            0,
            0,
            0,
            Rational::from_raw(ffi::AVRational { num: 0, den: 1 }),
        )
    }
}

impl From<ffi::AVRegionOfInterest> for RegionOfInterest {
    #[inline]
    fn from(raw: ffi::AVRegionOfInterest) -> Self {
        Self::from_raw(raw)
    }
}

impl From<RegionOfInterest> for ffi::AVRegionOfInterest {
    #[inline]
    fn from(roi: RegionOfInterest) -> Self {
        roi.into_raw()
    }
}

/// Field by field, the offset by [`Rational`]'s value equality: `1/2` equals
/// `2/4`, and a `0/0` offset equals nothing, itself included.
impl PartialEq for RegionOfInterest {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.edges() == b.edges() && a.qoffset().get() == b.qoffset().get()
    }
}

impl fmt::Debug for RegionOfInterest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for RegionOfInterestRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegionOfInterest")
            .field("self_size", &self.self_size())
            .field("top", &self.top())
            .field("bottom", &self.bottom())
            .field("left", &self.left())
            .field("right", &self.right())
            .field("qoffset", &self.qoffset())
            .finish()
    }
}

impl<'a> RegionOfInterestRef<'a> {
    /// Field: AVRegionOfInterest.self_size
    ///
    /// The size this element claims, which readers of a side data array take
    /// as its stride.
    #[must_use]
    pub fn self_size(&self) -> u32 {
        // SAFETY: a read of the `self_size` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVRegionOfInterest`.
        unsafe { addr_of!((*self.as_ptr()).self_size).read() }
    }

    /// Field: AVRegionOfInterest.top
    ///
    /// The distance in pixels from the frame's top edge to the region's.
    #[must_use]
    pub fn top(&self) -> i32 {
        // SAFETY: as in `self_size`, for the `top` field.
        unsafe { addr_of!((*self.as_ptr()).top).read() }
    }

    /// Field: AVRegionOfInterest.bottom
    ///
    /// The distance in pixels from the frame's top edge to the region's
    /// bottom edge.
    #[must_use]
    pub fn bottom(&self) -> i32 {
        // SAFETY: as in `self_size`, for the `bottom` field.
        unsafe { addr_of!((*self.as_ptr()).bottom).read() }
    }

    /// Field: AVRegionOfInterest.left
    ///
    /// The distance in pixels from the frame's left edge to the region's.
    #[must_use]
    pub fn left(&self) -> i32 {
        // SAFETY: as in `self_size`, for the `left` field.
        unsafe { addr_of!((*self.as_ptr()).left).read() }
    }

    /// Field: AVRegionOfInterest.right
    ///
    /// The distance in pixels from the frame's left edge to the region's
    /// right edge.
    #[must_use]
    pub fn right(&self) -> i32 {
        // SAFETY: as in `self_size`, for the `right` field.
        unsafe { addr_of!((*self.as_ptr()).right).read() }
    }

    /// Field: AVRegionOfInterest.qoffset
    ///
    /// The quantisation offset, as a handle over it that lives as long as
    /// this one.
    #[must_use]
    pub fn qoffset(&self) -> RationalRef<'a> {
        // SAFETY: `qoffset` is projected through the shared handle's pointer
        // to a live, initialised `AVRegionOfInterest`, so it is non-NULL,
        // aligned and initialised; the struct stays live and unwritten for
        // `'a`, which the returned getter-only handle cannot outlive.
        unsafe { RationalRef::from_ptr(addr_of!((*self.as_ptr()).qoffset).cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// The integer fields in declaration order, for comparison.
    fn edges(&self) -> (u32, i32, i32, i32, i32) {
        (
            self.self_size(),
            self.top(),
            self.bottom(),
            self.left(),
            self.right(),
        )
    }

    /// A copy of the whole region, independent of the place it was read from.
    #[must_use]
    pub fn get(&self) -> RegionOfInterest {
        // SAFETY: a read of the whole live, initialised `AVRegionOfInterest`
        // through the shared handle's pointer; it is seven integers, so the
        // byte copy is a complete, independent value.
        RegionOfInterest::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl RegionOfInterestMut<'_> {
    /// Field: AVRegionOfInterest.self_size
    ///
    /// Sets the size this element claims; C's readers need
    /// [`RegionOfInterest::SELF_SIZE`].
    pub fn set_self_size(&mut self, self_size: u32) {
        // SAFETY: a write of the `self_size` field projected through the
        // exclusive handle's pointer to a live `AVRegionOfInterest`; the old
        // value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).self_size).write(self_size) }
    }

    /// Field: AVRegionOfInterest.top
    ///
    /// Sets the distance from the frame's top edge to the region's.
    pub fn set_top(&mut self, top: i32) {
        // SAFETY: as in `set_self_size`, for the `top` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).top).write(top) }
    }

    /// Field: AVRegionOfInterest.bottom
    ///
    /// Sets the distance from the frame's top edge to the region's bottom
    /// edge.
    pub fn set_bottom(&mut self, bottom: i32) {
        // SAFETY: as in `set_self_size`, for the `bottom` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).bottom).write(bottom) }
    }

    /// Field: AVRegionOfInterest.left
    ///
    /// Sets the distance from the frame's left edge to the region's.
    pub fn set_left(&mut self, left: i32) {
        // SAFETY: as in `set_self_size`, for the `left` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).left).write(left) }
    }

    /// Field: AVRegionOfInterest.right
    ///
    /// Sets the distance from the frame's left edge to the region's right
    /// edge.
    pub fn set_right(&mut self, right: i32) {
        // SAFETY: as in `set_self_size`, for the `right` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).right).write(right) }
    }

    /// Field: AVRegionOfInterest.qoffset
    ///
    /// An exclusive handle over the quantisation offset, for writing its
    /// parts; this handle is frozen while it lives.
    #[must_use]
    pub fn qoffset_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: `qoffset` is projected through the exclusive handle's
        // pointer to a live `AVRegionOfInterest`, so it is non-NULL, aligned
        // and initialised; the result borrows `self` exclusively, so it is
        // the only access to the field while it lives.
        unsafe { RationalMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).qoffset)) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVRegionOfInterest.qoffset
    ///
    /// Sets the quantisation offset.
    pub fn set_qoffset(&mut self, qoffset: Rational) {
        // SAFETY: as in `set_self_size`, for the `qoffset` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).qoffset).write(qoffset.into_raw()) }
    }

    /// Overwrites every field with `roi`'s.
    pub fn set(&mut self, roi: RegionOfInterest) {
        // SAFETY: a write of the whole `AVRegionOfInterest` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(roi.into_raw()) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::{CStr, c_uint};
    use core::hash::BuildHasher;
    use core::mem::{align_of, offset_of, size_of};
    use core::ptr;
    use std::collections::hash_map::RandomState;

    const NAMED: [(SideDataType, &str); 38] = [
        (SideDataType::PAN_SCAN, "PAN_SCAN"),
        (SideDataType::A53_CC, "A53_CC"),
        (SideDataType::STEREO_3D, "STEREO_3D"),
        (SideDataType::MATRIX_ENCODING, "MATRIX_ENCODING"),
        (SideDataType::DOWNMIX_INFO, "DOWNMIX_INFO"),
        (SideDataType::REPLAY_GAIN, "REPLAY_GAIN"),
        (SideDataType::DISPLAY_MATRIX, "DISPLAY_MATRIX"),
        (SideDataType::AFD, "AFD"),
        (SideDataType::MOTION_VECTORS, "MOTION_VECTORS"),
        (SideDataType::SKIP_SAMPLES, "SKIP_SAMPLES"),
        (SideDataType::AUDIO_SERVICE_TYPE, "AUDIO_SERVICE_TYPE"),
        (
            SideDataType::MASTERING_DISPLAY_METADATA,
            "MASTERING_DISPLAY_METADATA",
        ),
        (SideDataType::GOP_TIMECODE, "GOP_TIMECODE"),
        (SideDataType::SPHERICAL, "SPHERICAL"),
        (SideDataType::CONTENT_LIGHT_LEVEL, "CONTENT_LIGHT_LEVEL"),
        (SideDataType::ICC_PROFILE, "ICC_PROFILE"),
        (SideDataType::S12M_TIMECODE, "S12M_TIMECODE"),
        (SideDataType::DYNAMIC_HDR_PLUS, "DYNAMIC_HDR_PLUS"),
        (SideDataType::REGIONS_OF_INTEREST, "REGIONS_OF_INTEREST"),
        (SideDataType::VIDEO_ENC_PARAMS, "VIDEO_ENC_PARAMS"),
        (SideDataType::SEI_UNREGISTERED, "SEI_UNREGISTERED"),
        (SideDataType::FILM_GRAIN_PARAMS, "FILM_GRAIN_PARAMS"),
        (SideDataType::DETECTION_BBOXES, "DETECTION_BBOXES"),
        (SideDataType::DOVI_RPU_BUFFER, "DOVI_RPU_BUFFER"),
        (SideDataType::DOVI_METADATA, "DOVI_METADATA"),
        (SideDataType::DYNAMIC_HDR_VIVID, "DYNAMIC_HDR_VIVID"),
        (
            SideDataType::AMBIENT_VIEWING_ENVIRONMENT,
            "AMBIENT_VIEWING_ENVIRONMENT",
        ),
        (SideDataType::VIDEO_HINT, "VIDEO_HINT"),
        (SideDataType::LCEVC, "LCEVC"),
        (SideDataType::VIEW_ID, "VIEW_ID"),
        (SideDataType::REFERENCE_DISPLAYS_3D, "REFERENCE_DISPLAYS_3D"),
        (SideDataType::EXIF, "EXIF"),
        (
            SideDataType::DYNAMIC_HDR_SMPTE_2094_APP5,
            "DYNAMIC_HDR_SMPTE_2094_APP5",
        ),
        (SideDataType::IAMF_MIX_GAIN_PARAM, "IAMF_MIX_GAIN_PARAM"),
        (
            SideDataType::IAMF_DEMIXING_INFO_PARAM,
            "IAMF_DEMIXING_INFO_PARAM",
        ),
        (
            SideDataType::IAMF_RECON_GAIN_INFO_PARAM,
            "IAMF_RECON_GAIN_INFO_PARAM",
        ),
        (SideDataType::RAW_COLOR_PARAMS, "RAW_COLOR_PARAMS"),
        (SideDataType::DOWNMIX_MATRIX, "DOWNMIX_MATRIX"),
    ];

    #[test]
    fn layout_matches_the_c_enum() {
        assert_eq!(
            size_of::<SideDataType>(),
            size_of::<ffi::AVFrameSideDataType>()
        );
        assert_eq!(
            align_of::<SideDataType>(),
            align_of::<ffi::AVFrameSideDataType>()
        );
        assert_eq!(size_of::<SideDataType>(), size_of::<c_uint>());
    }

    #[test]
    fn constants_are_the_contiguous_c_values() {
        // C numbers its enumerators from 0 in declaration order; the table
        // lists them in that order, so each constant's value is its position.
        for (i, (t, _)) in NAMED.into_iter().enumerate() {
            assert_eq!(t.into_raw().0 as usize, i, "{t:?}");
        }
    }

    #[test]
    fn debug_names_the_constants() {
        for (t, name) in NAMED {
            assert_eq!(format!("{t:?}"), name);
        }
    }

    #[test]
    fn debug_shows_unnamed_values_raw() {
        for raw in [38, 39, 1 << 20, i32::MAX as u32, u32::MAX - 1, u32::MAX] {
            let t = SideDataType::from_raw(ffi::AVFrameSideDataType(raw));
            assert_eq!(format!("{t:?}"), format!("SideDataType({raw})"));
        }
    }

    #[test]
    fn raw_conversions_round_trip_every_value() {
        for raw in [0, 1, 37, 38, 39, i32::MAX as u32, i32::MIN as u32, u32::MAX] {
            let c = ffi::AVFrameSideDataType(raw);
            assert_eq!(SideDataType::from(c).into_raw(), c);
            assert_eq!(ffi::AVFrameSideDataType::from(SideDataType::from_raw(c)), c);
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
    fn hash_follows_equality() {
        use std::collections::HashSet;
        let set: HashSet<SideDataType> = NAMED
            .iter()
            .map(|&(t, _)| t)
            .chain(
                NAMED
                    .iter()
                    .map(|&(t, _)| SideDataType::from_raw(t.into_raw())),
            )
            .collect();
        assert_eq!(set.len(), NAMED.len());
        assert!(!set.contains(&SideDataType::from_raw(ffi::AVFrameSideDataType(38))));
    }

    /// Every named constant beside its name and its 4-bit code.
    const AFD_NAMED: [(ActiveFormatDescription, &str, u8); 7] = [
        (ActiveFormatDescription::SAME, "SAME", 0b1000),
        (ActiveFormatDescription::ASPECT_4_3, "ASPECT_4_3", 0b1001),
        (ActiveFormatDescription::ASPECT_16_9, "ASPECT_16_9", 0b1010),
        (ActiveFormatDescription::ASPECT_14_9, "ASPECT_14_9", 0b1011),
        (
            ActiveFormatDescription::ASPECT_4_3_SP_14_9,
            "ASPECT_4_3_SP_14_9",
            0b1101,
        ),
        (
            ActiveFormatDescription::ASPECT_16_9_SP_14_9,
            "ASPECT_16_9_SP_14_9",
            0b1110,
        ),
        (ActiveFormatDescription::SP_4_3, "SP_4_3", 0b1111),
    ];

    #[test]
    fn active_format_description_constants_are_the_named_codes() {
        for (afd, name, code) in AFD_NAMED {
            assert_eq!(afd.into_raw().0, u32::from(code), "{name}");
            assert_eq!(afd.to_byte(), Some(code), "{name}");
            assert_eq!(ActiveFormatDescription::from_byte(code), afd, "{name}");
            assert_eq!(format!("{afd:?}"), name);
        }
        for (i, (a, ..)) in AFD_NAMED.iter().enumerate() {
            for (b, ..) in &AFD_NAMED[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn active_format_description_reserved_codes_have_no_name() {
        for code in (0..8).chain([12]) {
            let afd = ActiveFormatDescription::from_byte(code);
            assert_eq!(afd.constant_name(), None);
            assert_eq!(
                format!("{afd:?}"),
                format!("ActiveFormatDescription({code})")
            );
            assert_eq!(afd.to_byte(), Some(code));
        }
    }

    #[test]
    fn active_format_description_bytes_round_trip() {
        for byte in 0..=u8::MAX {
            let afd = ActiveFormatDescription::from(byte);
            assert_eq!(afd, ActiveFormatDescription::from_byte(byte));
            assert_eq!(afd.into_raw().0, u32::from(byte));
            assert_eq!(afd.to_byte(), Some(byte));
            assert_eq!(
                afd.constant_name().is_some(),
                AFD_NAMED.iter().any(|n| n.0 == afd)
            );
        }
    }

    #[test]
    fn active_format_description_values_past_a_byte_have_no_byte() {
        for raw in [256, 0x1_08, i32::MAX as u32, u32::MAX] {
            let afd = ActiveFormatDescription::from_raw(ffi::AVActiveFormatDescription(raw));
            assert_eq!(afd.to_byte(), None, "{raw}");
            assert_eq!(afd.constant_name(), None, "{raw}");
            assert_eq!(afd.into_raw().0, raw);
            let back: ffi::AVActiveFormatDescription = afd.into();
            assert_eq!(ActiveFormatDescription::from(back), afd);
            assert_eq!(
                format!("{afd:?}"),
                format!("ActiveFormatDescription({raw})")
            );
        }
    }

    #[test]
    fn active_format_description_is_layout_compatible_with_c() {
        assert_eq!(
            size_of::<ActiveFormatDescription>(),
            size_of::<ffi::AVActiveFormatDescription>()
        );
        assert_eq!(
            align_of::<ActiveFormatDescription>(),
            align_of::<ffi::AVActiveFormatDescription>()
        );
        assert_eq!(
            size_of::<ffi::AVActiveFormatDescription>(),
            size_of::<c_uint>()
        );
    }

    #[test]
    fn layouts_match_the_c_types() {
        assert_eq!(
            size_of::<SideDataProps>(),
            size_of::<ffi::AVSideDataProps>()
        );
        assert_eq!(
            align_of::<SideDataProps>(),
            align_of::<ffi::AVSideDataProps>()
        );
        assert_eq!(size_of::<SideDataProps>(), size_of::<c_uint>());
        assert_eq!(
            size_of::<SideDataDescriptor>(),
            size_of::<ffi::AVSideDataDescriptor>()
        );
        assert_eq!(
            align_of::<SideDataDescriptor>(),
            align_of::<ffi::AVSideDataDescriptor>()
        );
        assert_eq!(offset_of!(ffi::AVSideDataDescriptor, name), 0);
        // Handles are one pointer wide, as `define_ctype!` promises.
        assert_eq!(size_of::<SideDataDescriptorRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<SideDataDescriptorMut<'_>>(), size_of::<usize>());
    }

    #[test]
    fn props_constants_carry_the_c_values() {
        let bits: Vec<c_uint> = SideDataProps::KNOWN.iter().map(|(p, _)| p.bits()).collect();
        assert_eq!(bits, [1, 2, 4, 8, 16]);
        assert_eq!(SideDataProps::all().bits(), 0x1f);
        assert_eq!(
            SideDataProps::GLOBAL.bits(),
            ffi::AVSideDataProps::AV_SIDE_DATA_PROP_GLOBAL.0
        );
        assert_eq!(
            SideDataProps::CHANNEL_DEPENDENT.bits(),
            ffi::AVSideDataProps::AV_SIDE_DATA_PROP_CHANNEL_DEPENDENT.0
        );
    }

    #[test]
    fn props_bits_and_constructors() {
        assert_eq!(SideDataProps::default(), SideDataProps::empty());
        assert!(SideDataProps::empty().is_empty());
        assert!(!SideDataProps::MULTI.is_empty());
        assert_eq!(SideDataProps::from_bits(0x1f), Some(SideDataProps::all()));
        assert_eq!(SideDataProps::from_bits(0), Some(SideDataProps::empty()));
        assert_eq!(SideDataProps::from_bits(0x20), None);
        assert_eq!(SideDataProps::from_bits(c_uint::MAX), None);
        assert_eq!(
            SideDataProps::from_bits_truncate(c_uint::MAX),
            SideDataProps::all()
        );
        assert_eq!(
            SideDataProps::from_bits_retain(c_uint::MAX).bits(),
            c_uint::MAX
        );
        let raw: ffi::AVSideDataProps = SideDataProps::from_bits_retain(0x8000_0005).into();
        assert_eq!(raw.0, 0x8000_0005);
        assert_eq!(SideDataProps::from(raw).bits(), 0x8000_0005);
    }

    #[test]
    fn props_set_operations_keep_unnamed_bits() {
        let size = SideDataProps::SIZE_DEPENDENT;
        let other = SideDataProps::from_bits_retain(0x100);

        let both = size | other;
        assert_eq!(both.bits(), 0x104);
        assert!(both.contains(size) && both.contains(other));
        assert!(both.intersects(size));
        assert!(!size.intersects(other));
        assert!(!size.intersects(SideDataProps::empty()));
        assert!(!size.contains(both));
        assert!(size.contains(SideDataProps::empty()));
        assert_eq!(both & size, size);
        assert_eq!(both - size, other);

        let mut f = SideDataProps::empty();
        f.insert(size);
        assert_eq!(f, size);
        f |= other;
        assert_eq!(f, both);
        f.remove(size);
        assert_eq!(f, other);
        f.set(size, true);
        assert_eq!(f, both);
        f.set(size, false);
        assert_eq!(f, other);
        f -= other;
        assert!(f.is_empty());
        f = both;
        f &= size;
        assert_eq!(f, size);
    }

    #[test]
    fn props_debug() {
        assert_eq!(
            format!("{:?}", SideDataProps::empty()),
            "SideDataProps(0x0)"
        );
        assert_eq!(
            format!("{:?}", SideDataProps::GLOBAL),
            "SideDataProps(GLOBAL)"
        );
        assert_eq!(
            format!("{:?}", SideDataProps::all()),
            "SideDataProps(GLOBAL | MULTI | SIZE_DEPENDENT | COLOR_DEPENDENT | CHANNEL_DEPENDENT)"
        );
        assert_eq!(
            format!("{:?}", SideDataProps::from_bits_retain(0x40)),
            "SideDataProps(0x40)"
        );
        assert_eq!(
            format!(
                "{:?}",
                SideDataProps::COLOR_DEPENDENT | SideDataProps::from_bits_retain(0x8000_0000)
            ),
            "SideDataProps(COLOR_DEPENDENT | 0x80000000)"
        );
    }

    #[test]
    fn descriptor_zeroed_named_and_default() {
        let d = SideDataDescriptor::new();
        assert_eq!(d.as_ref().name(), None);
        assert!(d.as_ref().props().is_empty());
        assert_eq!(d, SideDataDescriptor::default());
        let raw = d.into_raw();
        assert!(raw.name.is_null());
        assert_eq!(raw.props, 0);

        const STEREO: SideDataDescriptor =
            SideDataDescriptor::named(c"Stereo 3D", SideDataProps::GLOBAL);
        assert_eq!(STEREO.as_ref().name(), Some(c"Stereo 3D"));
        assert_eq!(STEREO.as_ref().props(), SideDataProps::GLOBAL);
        let raw: ffi::AVSideDataDescriptor = STEREO.into();
        assert_eq!(raw.name, c"Stereo 3D".as_ptr());
        assert_eq!(raw.props, 1);
    }

    #[test]
    fn descriptor_from_raw_adopts_and_round_trips() {
        let names: [*const core::ffi::c_char; 3] =
            [ptr::null(), c"".as_ptr(), c"ICC profile".as_ptr()];
        for (k, &name) in names.iter().enumerate() {
            let raw = ffi::AVSideDataDescriptor {
                name,
                props: c_uint::MAX >> k,
            };
            // SAFETY: `name` is NULL or a literal, never freed or written.
            let d = unsafe { SideDataDescriptor::from_raw(raw) };
            let back = d.into_raw();
            assert_eq!((back.name, back.props), (raw.name, raw.props));
            assert_eq!(d.as_ref().name().map_or(ptr::null(), CStr::as_ptr), name);
            assert_eq!(d.as_ref().props().bits(), c_uint::MAX >> k);
        }
    }

    #[test]
    fn descriptor_setters_and_whole_sets() {
        let mut d = SideDataDescriptor::new();
        let mut m = d.as_mut();
        m.set_name(Some(c"Downmix Matrix"));
        m.set_props(SideDataProps::CHANNEL_DEPENDENT | SideDataProps::from_bits_retain(0x40));
        assert_eq!(m.as_ref().name(), Some(c"Downmix Matrix"));
        assert_eq!(m.as_ref().props().bits(), 0x50);
        m.set_name(None);
        assert_eq!(m.as_ref().props().bits(), 0x50);
        assert_eq!(d.as_ref().name(), None);

        let copy = SideDataDescriptor::named(c"AVPanScan", SideDataProps::SIZE_DEPENDENT);
        d.as_mut().set(copy);
        assert_eq!(d, copy);
        assert_eq!(d.as_ref().get(), copy);
    }

    #[test]
    fn descriptor_equality_and_hash_compare_names_by_content() {
        // Two distinct statics with the same bytes.
        static A: [u8; 8] = *b"View ID\0";
        static B: [u8; 8] = *b"View ID\0";
        let a_name = CStr::from_bytes_with_nul(&A).unwrap();
        let b_name = CStr::from_bytes_with_nul(&B).unwrap();
        assert_ne!(a_name.as_ptr(), b_name.as_ptr());
        let a = SideDataDescriptor::named(a_name, SideDataProps::empty());
        let b = SideDataDescriptor::named(b_name, SideDataProps::empty());
        assert_eq!(a, b);
        let s = RandomState::new();
        assert_eq!(s.hash_one(a), s.hash_one(b));
        assert_ne!(a, SideDataDescriptor::named(a_name, SideDataProps::MULTI));
        assert_ne!(
            a,
            SideDataDescriptor::named(c"View", SideDataProps::empty())
        );
        assert_ne!(a, SideDataDescriptor::new());
        assert_eq!(SideDataDescriptor::new(), SideDataDescriptor::new());
        assert_eq!(
            s.hash_one(SideDataDescriptor::new()),
            s.hash_one(SideDataDescriptor::default())
        );
    }

    #[test]
    fn descriptor_debug() {
        assert_eq!(
            format!("{:?}", SideDataDescriptor::new()),
            "SideDataDescriptor { name: None, props: SideDataProps(0x0) }"
        );
        let d = SideDataDescriptor::named(
            c"ICC profile",
            SideDataProps::GLOBAL | SideDataProps::COLOR_DEPENDENT,
        );
        assert_eq!(
            format!("{d:?}"),
            "SideDataDescriptor { name: Some(\"ICC profile\"), props: SideDataProps(GLOBAL | COLOR_DEPENDENT) }"
        );
        assert_eq!(format!("{:?}", d.as_ref()), format!("{d:?}"));
    }

    /// Buffer-frame wave 1, batch 0: `RegionOfInterest`. Tests without
    /// `equality` in their name make no C call and also run under Miri.
    #[test]
    fn region_of_interest_layout_matches_c() {
        type C = ffi::AVRegionOfInterest;
        assert_eq!(size_of::<RegionOfInterest>(), size_of::<C>());
        assert_eq!(align_of::<RegionOfInterest>(), align_of::<C>());
        assert_eq!(size_of::<C>(), 28);
        assert_eq!(RegionOfInterest::SELF_SIZE, 28);
        assert_eq!(
            [
                offset_of!(C, self_size),
                offset_of!(C, top),
                offset_of!(C, bottom),
                offset_of!(C, left),
                offset_of!(C, right),
                offset_of!(C, qoffset),
            ],
            [0, 4, 8, 12, 16, 20]
        );
        assert_eq!(size_of::<RegionOfInterestRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<RegionOfInterestMut<'_>>(), size_of::<usize>());
        fn send_sync<T: Send + Sync>() {}
        send_sync::<RegionOfInterest>();
        send_sync::<RegionOfInterestRef<'_>>();
        send_sync::<RegionOfInterestMut<'_>>();
    }

    fn q(num: i32, den: i32) -> Rational {
        Rational::from_raw(ffi::AVRational { num, den })
    }

    /// Every field, `qoffset` as its parts.
    fn region_fields(r: RegionOfInterestRef<'_>) -> [i64; 7] {
        let o = r.qoffset();
        [
            r.self_size().into(),
            r.top().into(),
            r.bottom().into(),
            r.left().into(),
            r.right().into(),
            o.num().into(),
            o.den().into(),
        ]
    }

    #[test]
    fn region_of_interest_constructors_and_raw_round_trip() {
        let r = RegionOfInterest::new(1, 2, 3, 4, q(-1, 10));
        assert_eq!(region_fields(r.as_ref()), [28, 1, 2, 3, 4, -1, 10]);
        assert_eq!(
            region_fields(RegionOfInterest::default().as_ref()),
            [28, 0, 0, 0, 0, 0, 1]
        );
        assert_eq!(region_fields(RegionOfInterest::zeroed().as_ref()), [0; 7]);
        let raw = ffi::AVRegionOfInterest {
            self_size: u32::MAX,
            top: i32::MIN,
            bottom: i32::MAX,
            left: -1,
            right: 0,
            qoffset: ffi::AVRational {
                num: i32::MAX,
                den: i32::MIN,
            },
        };
        let r = RegionOfInterest::from(raw);
        let back: ffi::AVRegionOfInterest = r.into();
        assert_eq!(
            region_fields(r.as_ref()),
            [
                u32::MAX.into(),
                i32::MIN.into(),
                i32::MAX.into(),
                -1,
                0,
                i32::MAX.into(),
                i32::MIN.into()
            ]
        );
        assert_eq!(
            (back.self_size, back.top, back.bottom, back.left, back.right),
            (raw.self_size, raw.top, raw.bottom, raw.left, raw.right)
        );
        assert_eq!((back.qoffset.num, back.qoffset.den), (i32::MAX, i32::MIN));
        let copy = r.as_ref().get();
        assert_eq!(region_fields(copy.as_ref()), region_fields(r.as_ref()));
    }

    #[test]
    fn region_of_interest_setters_write_one_field_each() {
        let mut r = RegionOfInterest::zeroed();
        let mut m = r.as_mut();
        m.set_self_size(RegionOfInterest::SELF_SIZE);
        m.set_top(-5);
        m.set_bottom(6);
        m.set_left(i32::MIN);
        m.set_right(i32::MAX);
        m.set_qoffset(q(1, 3));
        assert_eq!(
            region_fields(m.as_ref()),
            [28, -5, 6, i32::MIN.into(), i32::MAX.into(), 1, 3]
        );
        let mut o = m.qoffset_mut();
        o.set_num(-2);
        o.set_den(7);
        assert_eq!(region_fields(r.as_ref())[5..], [-2, 7]);
        r.as_mut().set(RegionOfInterest::new(9, 8, 7, 6, q(0, 1)));
        assert_eq!(region_fields(r.as_ref()), [28, 9, 8, 7, 6, 0, 1]);
    }

    #[test]
    fn region_of_interest_debug() {
        assert_eq!(
            format!("{:?}", RegionOfInterest::new(1, 2, 3, 4, q(-1, 10))),
            "RegionOfInterest { self_size: 28, top: 1, bottom: 2, left: 3, right: 4, \
             qoffset: Rational { num: -1, den: 10 } }"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn region_of_interest_equality_compares_offsets_by_value() {
        let a = RegionOfInterest::new(1, 2, 3, 4, q(1, 2));
        assert_eq!(a, RegionOfInterest::new(1, 2, 3, 4, q(2, 4)));
        assert_ne!(a, RegionOfInterest::new(1, 2, 3, 4, q(1, 3)));
        for field in 0..4 {
            let mut b = a;
            let mut m = b.as_mut();
            match field {
                0 => m.set_top(100),
                1 => m.set_bottom(100),
                2 => m.set_left(100),
                _ => m.set_right(100),
            }
            assert_ne!(a, b, "field {field}");
        }
        let mut b = a;
        b.as_mut().set_self_size(0);
        assert_ne!(a, b);
        // A `0/0` offset equals nothing, itself included.
        let z = RegionOfInterest::zeroed();
        assert_ne!(z, z);
    }
}
