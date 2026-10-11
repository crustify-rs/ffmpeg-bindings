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
//!
//! [`SideData`] is `AVFrameSideData`, one piece of side data: a payload of
//! some [`SideDataType`], held by a buffer reference, and a metadata
//! dictionary. Entries live in side data arrays, which own them: a
//! [`SideDataArray`] (C's `AVFrameSideData **` and `int` count pair, freed
//! with `av_frame_side_data_free`), or a frame's list. An entry is only
//! borrowed, as a [`SideDataRef`] or a [`SideDataMut`] lent by the array's
//! [`SideDataArrayRef`] or [`SideDataArrayMut`]. Its payload is read as
//! initialised bytes, and written only while its buffer has no other
//! reference.
//!
//! [`Frame`] is `AVFrame`, decoded audio or video: its planes, the buffer
//! references holding them, what describes them, its timing, color
//! properties, side data and metadata. Its owner is a [`FrameBox`], made by
//! [`Frame::new`] (`av_frame_alloc`) and freed with `av_frame_free`;
//! [`FrameMut::reset`] (`av_frame_unref`) empties it and
//! [`FrameRef::try_clone`] (`av_frame_clone`) copies it, sharing its
//! buffers. Rust reads a plane only through the buffer reference holding it
//! ([`FrameRef::plane`]), and writes it only while that buffer has no other
//! reference ([`FrameMut::plane_mut`]); the format, dimensions, sample count
//! and line sizes are plain values. Its [`flags`](FrameRef::flags) and
//! [`decode_error_flags`](FrameRef::decode_error_flags) are a
//! [`FrameFlags`] and a [`DecodeErrorFlags`].

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit, size_of};
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Range, Sub, SubAssign};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use core::slice;

use ffibox::{CBox, CNew, CPlainElem, CZeroable, define_ctype, impl_cdrop};

use super::avutil::PictureType;
use super::buffer::{
    BufferReference, BufferReferenceBox, BufferReferenceRef, ENOMEM, may_add_reference,
};
use super::channel_layout::{ChannelLayout, ChannelLayoutMut, ChannelLayoutRef, ChannelLayoutVal};
use super::dict::{Dict, DictionaryBox, DictionaryRef};
use super::ffi;
use super::pixfmt::{
    AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer, PixelFormat,
};
use super::rational::{Rational, RationalMut, RationalRef};
use super::refstruct;
use super::samplefmt::SampleFormat;

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
    /// element records its own size, `self_size`, which C requires to be
    /// [`SELF_SIZE`](Self::SELF_SIZE), the size of this struct; [`new`](Self::new)
    /// and [`Default`] set it so. Readers of the array take the first
    /// element's `self_size` as its stride and as the divisor of the payload
    /// size, and check only that it is not 0 and divides the size: then they
    /// read a whole struct at every stride. Any other `self_size` therefore
    /// misleads them. A smaller one passes their check and makes them read
    /// past the payload's end (`vf_showinfo`, `vf_addroi`, and the encoders'
    /// ROI readers), and 0 is an error to most of them but aborts the process
    /// in `vf_addroi` (`av_assert0`). Nothing here stores regions in side
    /// data, so a wrapper that hands such a payload to C must check every
    /// element's `self_size` first.
    ///
    /// No other field is checked, by libavutil or here: encoders reject
    /// regions they cannot use, such as a quantisation offset with a zero
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
// of 0, all plain integers every accessor reads as they are, and the type has
// no teardown to reject it. No safe wrapper hands a region to C: a zero
// `self_size` is a C error, or an abort in vf_addroi, once side data carries
// it, as the type's docs say.
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
    /// Sets the size this element claims; C requires
    /// [`RegionOfInterest::SELF_SIZE`], and its readers read past the side
    /// data payload for a smaller nonzero size (see [`RegionOfInterest`]).
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

define_ctype!(
    /// Wraps: AVFrameSideData
    ///
    /// One piece of side data: a payload of bytes whose layout its
    /// [`kind`](SideDataRef::kind) decides, the [`BufferReference`](super::buffer::BufferReference) that holds
    /// them, and a metadata [`Dict`]. Its getters are on [`SideDataRef`]
    /// (`as_ref()`), its setters on [`SideDataMut`] (`as_mut()`).
    ///
    /// An entry lives in a side data array, a [`SideDataArray`] or a frame's
    /// own list, which owns it: C allocates and frees entries only inside the
    /// array routines (`av_frame_side_data_new`, `_add`, `_clone`, `_remove`
    /// and `_free`), and keeps the struct's size out of the ABI. So an entry
    /// is never held by value or owned on its own; it is only borrowed from
    /// its array.
    ///
    /// Every entry a handle reaches holds one reference to a buffer in `buf`,
    /// never NULL; its `data` and `size` describe `size` initialised bytes
    /// inside the data of that buffer (a NULL `data` only when `size` is 0),
    /// which its reference keeps alive; and its `metadata` is NULL or a
    /// non-empty dictionary the entry alone owns. Every C routine that builds
    /// or changes an entry keeps that, save the two that allocate its payload
    /// themselves: `av_frame_side_data_new` and the frame-level
    /// `av_frame_new_side_data` take it from `av_buffer_alloc` and leave it
    /// uninitialised for their caller to fill. Other references to the same
    /// buffer (an entry's clone, a frame's copy) share the bytes, so they are
    /// written only through an entry whose reference is the buffer's only one
    /// and not read-only ([`data_mut`](SideDataMut::data_mut)), as C's
    /// convention has it.
    ///
    /// The payload's bytes are borrowed from the entry, so they cannot
    /// outlive its array:
    ///
    /// ```compile_fail,E0505
    /// use ffmpeg::libavutil::frame::SideDataArray;
    /// let array = SideDataArray::new();
    /// let bytes = array.as_ref().get(0).unwrap().data();
    /// drop(array);
    /// assert_eq!(bytes.len(), 0);
    /// ```
    ///
    /// nor survive a change of the buffer that holds them:
    ///
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::buffer::BufferReference;
    /// use ffmpeg::libavutil::frame::SideDataArray;
    /// let mut array = SideDataArray::new();
    /// let mut entries = array.as_mut();
    /// let mut entry = entries.get_mut(0).unwrap();
    /// let bytes = entry.as_ref().data();
    /// entry.set_buffer(BufferReference::alloc_zeroed(4).unwrap());
    /// assert_eq!(bytes.len(), 0);
    /// ```
    ///
    /// `Send` and `Sync`: the entry is plain data, its buffer reference and
    /// dictionary are both, and the shared bytes are written only through a
    /// sole reference.
    SideData,
    SideDataRef,
    SideDataMut,
    ffi::AVFrameSideData
);

// SAFETY: an entry is a C enum, a pointer into the bytes of the buffer its
// own reference keeps alive, that `AVBufferRef` (`Send`: C counts references
// atomically and its data release may run on any thread) and an
// `AVDictionary` (`Send`: plain `av_malloc` blocks). `av_frame_side_data_free`
// releases all of it with `av_free`, `av_buffer_unref` and `av_dict_free`,
// none of which is tied to the allocating thread.
unsafe impl Send for SideData {}
// SAFETY: as for `Send for SideData`: the handle is the only access to the
// entry, and every write it makes to the shared bytes is checked sole.
unsafe impl Send for SideDataMut<'_> {}

// SAFETY: through a shared handle, Rust reads the entry's fields, its bytes,
// its buffer reference (a `Sync` `BufferReference`, whose count C updates
// atomically) and its dictionary (a `Sync` `Dictionary`, read only). The
// bytes are written only through an exclusive handle whose buffer has a
// single reference, so not while another reference, or a shared handle to
// this entry, exists.
unsafe impl Sync for SideData {}
// SAFETY: as for `Sync for SideData`: copies of a shared handle only read.
unsafe impl Send for SideDataRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for SideDataRef<'_> {}
// SAFETY: `&SideDataMut` reaches the entry only through `as_ref`, a shared
// handle; the setters and `data_mut` need `&mut`.
unsafe impl Sync for SideDataMut<'_> {}

impl<'a> SideDataRef<'a> {
    /// Field: AVFrameSideData.type
    ///
    /// The kind of side data this is (C's `type`), which tells how its
    /// payload is laid out.
    #[must_use]
    pub fn kind(&self) -> SideDataType {
        // SAFETY: a read of the `type` field projected through the shared
        // handle's pointer to a live, initialised `AVFrameSideData`; every
        // `unsigned` is a valid `SideDataType`.
        SideDataType::from_raw(unsafe { addr_of!((*self.as_ptr()).type_).read() })
    }

    /// The `data` and `size` fields, an empty payload as a dangling pointer
    /// and 0, so that every pair addresses a valid run.
    fn parts(&self) -> (NonNull<u8>, usize) {
        // SAFETY: reads of two fields projected through the shared handle's
        // pointer to a live, initialised `AVFrameSideData`.
        let (data, len) = unsafe {
            (
                addr_of!((*self.as_ptr()).data).read(),
                addr_of!((*self.as_ptr()).size).read(),
            )
        };
        match NonNull::new(data) {
            Some(data) if len > 0 => (data, len),
            _ => (NonNull::dangling(), 0),
        }
    }

    /// Field: AVFrameSideData.data
    ///
    /// The payload, borrowed for as long as the entry is. Nothing writes it
    /// meanwhile: a writer needs the only reference to its buffer, and the
    /// entry holds one.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        let (data, len) = self.parts();
        // SAFETY: by the type's invariant, `data` addresses `len` initialised
        // bytes of the buffer the entry's reference keeps alive for `'a` (or
        // is dangling for 0); they are written only through a sole reference,
        // which the entry's, held for `'a`, rules out.
        unsafe { slice::from_raw_parts(data.as_ptr(), len) }
    }

    /// Field: AVFrameSideData.size
    ///
    /// The number of bytes in the payload.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parts().1
    }

    /// Whether the payload has no byte.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Field: AVFrameSideData.buf
    ///
    /// The reference to the buffer holding the payload, borrowed from the
    /// entry. Its own window may differ from the payload's only where C code
    /// moved one of them; its [`try_clone`](BufferReferenceRef::try_clone)
    /// shares the bytes, which makes neither copy writable while both live.
    #[must_use]
    pub fn buffer(&self) -> BufferReferenceRef<'a> {
        // SAFETY: a read of the `buf` pointer projected through the shared
        // handle's pointer to a live `AVFrameSideData`.
        let buf = unsafe { addr_of!((*self.as_ptr()).buf).read() };
        // SAFETY: by the type's invariant, `buf` is a live reference the entry
        // owns, which nothing releases or replaces while it is borrowed.
        unsafe { BufferReferenceRef::from_ptr(buf) }.expect("an entry holds a buffer reference")
    }

    /// Field: AVFrameSideData.metadata
    ///
    /// The entry's metadata, borrowed from it; `None` when it has none.
    #[must_use]
    pub fn metadata(&self) -> Option<DictionaryRef<'a>> {
        // SAFETY: a read of the `metadata` pointer projected through the
        // shared handle's pointer to a live `AVFrameSideData`.
        let dict = unsafe { addr_of!((*self.as_ptr()).metadata).read() };
        // SAFETY: by the type's invariant, `dict` is NULL or a non-empty
        // dictionary the entry owns, which nothing frees or writes while the
        // entry is borrowed.
        unsafe { DictionaryRef::from_ptr(dict) }
    }
}

impl fmt::Debug for SideDataRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SideData")
            .field("kind", &self.kind())
            .field("len", &self.len())
            .field("metadata", &self.metadata().map_or(0, |m| m.len()))
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for SideDataMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl SideDataMut<'_> {
    /// Field: AVFrameSideData.type
    ///
    /// Makes this entry side data of `kind`, leaving the payload as it is.
    ///
    /// C does not check a payload against its kind: readers of a kind with a
    /// struct payload trust its size to varying degrees, as they do for
    /// `av_frame_side_data_new`'s, so a payload must be laid out as its kind
    /// says before the entry reaches them. Nor does C keep kinds unique: the
    /// lookups return the first entry of a kind, and the removals take them
    /// all.
    pub fn set_kind(&mut self, kind: SideDataType) {
        // SAFETY: a write of the `type` field projected through the exclusive
        // handle's pointer to a live `AVFrameSideData`; the old value owns
        // nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).type_).write(kind.into_raw()) }
    }

    /// Field: AVFrameSideData.data
    ///
    /// The payload, writable, when the entry's buffer has no other reference
    /// and is not read-only (`av_buffer_is_writable`); `None` otherwise.
    /// Cloning the entry's reference, or the entry into another array, makes
    /// it unwritable until the copy is gone.
    #[must_use]
    pub fn data_mut(&mut self) -> Option<&mut [u8]> {
        let shared = self.as_ref();
        if !shared.buffer().is_writable() {
            return None;
        }
        let (data, len) = shared.parts();
        // SAFETY: by the type's invariant, `data` addresses `len` initialised
        // bytes of the buffer (or is dangling for 0), writable as it is not
        // read-only. The entry holds its only reference (the acquire load
        // ordered every released one before us), and the exclusive borrow of
        // `self` stops it being cloned while the slice lives.
        Some(unsafe { slice::from_raw_parts_mut(data.as_ptr(), len) })
    }

    /// Field: AVFrameSideData.buf
    ///
    /// Makes `with` the entry's buffer reference and its window the payload,
    /// as C does when it replaces an entry's buffer, and returns the
    /// reference the entry held, as an owner. The metadata and kind stay.
    #[must_use = "dropping the old reference releases it; use `set_buffer` for that"]
    pub fn replace_buffer(&mut self, with: BufferReferenceBox) -> BufferReferenceBox {
        let new = with.into_c();
        // SAFETY: reads of two fields of the live reference `new`, which this
        // function now owns.
        let (data, size) = unsafe { (addr_of!((*new).data).read(), addr_of!((*new).size).read()) };
        let p = self.as_mut_ptr();
        // SAFETY: a read and three writes of fields projected through the
        // exclusive handle's pointer to a live `AVFrameSideData`. The entry
        // takes `new`'s one reference, and `data`/`size` are that
        // reference's own window, initialised bytes of its buffer by
        // `BufferReference`'s invariant, so the entry keeps its own.
        let old = unsafe {
            let old = addr_of!((*p).buf).read();
            addr_of_mut!((*p).buf).write(new);
            addr_of_mut!((*p).data).write(data);
            addr_of_mut!((*p).size).write(size);
            old
        };
        // SAFETY: by the type's invariant, `old` was the entry's own
        // reference, which no longer reaches the entry; the box takes it over.
        unsafe { BufferReferenceBox::from_c(old) }.expect("an entry holds a buffer reference")
    }

    /// Field: AVFrameSideData.buf
    ///
    /// Makes `with` the entry's buffer reference and its window the payload,
    /// and releases the reference the entry held, as
    /// [`replace_buffer`](Self::replace_buffer) and a drop.
    pub fn set_buffer(&mut self, with: BufferReferenceBox) {
        drop(self.replace_buffer(with));
    }

    /// Field: AVFrameSideData.metadata
    ///
    /// Makes `with` the entry's metadata, and returns the metadata the entry
    /// held.
    #[must_use = "dropping the old metadata frees it; use `set_metadata` for that"]
    pub fn replace_metadata(&mut self, with: Dict) -> Dict {
        let new = Option::<DictionaryBox>::from(with).map_or(ptr::null_mut(), CBox::into_c);
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of the `metadata` pointer projected
        // through the exclusive handle's pointer to a live `AVFrameSideData`;
        // `new` is NULL or a non-empty dictionary nothing else owns, which
        // keeps the type's invariant.
        let old = unsafe {
            let old = addr_of!((*p).metadata).read();
            addr_of_mut!((*p).metadata).write(new);
            old
        };
        // SAFETY: by the type's invariant, `old` is NULL or a non-empty
        // dictionary the entry owned, which no longer reaches the entry.
        Dict::from(unsafe { DictionaryBox::from_c(old) })
    }

    /// Field: AVFrameSideData.metadata
    ///
    /// Makes `with` the entry's metadata, and frees the metadata the entry
    /// held, as [`replace_metadata`](Self::replace_metadata) and a drop.
    pub fn set_metadata(&mut self, with: Dict) {
        drop(self.replace_metadata(with));
    }

    /// Field: AVFrameSideData.metadata
    ///
    /// Takes the entry's metadata out, leaving it none.
    #[must_use]
    pub fn take_metadata(&mut self) -> Dict {
        self.replace_metadata(Dict::new())
    }
}

/// Wraps: av_frame_side_data_free
/// Handwritten
///
/// An owned array of side data: C's `AVFrameSideData **sd` and `int nb_sd`
/// pair, empty as NULL and 0, as `AVCodecContext` keeps its
/// `decoded_side_data`. Dropping it frees every entry with its buffer
/// reference and metadata, and the array (`av_frame_side_data_free`).
///
/// Its entries are reached through a [`SideDataArrayRef`] (`as_ref()`) and a
/// [`SideDataArrayMut`] (`as_mut()`), the same handles a frame's own list
/// hands out. The former looks them up by kind
/// ([`find`](SideDataArrayRef::find)); the latter adds them
/// ([`add`](SideDataArrayMut::add), [`alloc_zeroed`](SideDataArrayMut::alloc_zeroed),
/// [`add_clone`](SideDataArrayMut::add_clone)) and removes them
/// ([`remove`](SideDataArrayMut::remove)). An array C built is taken over
/// with [`from_raw_parts`](Self::from_raw_parts).
///
/// Hand-written rather than a `CVec`: the C routines that grow, shrink and
/// free an array take the two variables by address and update both, the
/// count as an `int`, which a `CVec`'s `(pointer, usize)` cannot lend.
///
/// `Send` and `Sync` with [`SideData`].
pub struct SideDataArray {
    entries: *mut *mut ffi::AVFrameSideData,
    len: c_int,
}

// SAFETY: the array owns its `av_malloc` block of pointers and every entry
// behind them, all `Send` per `Send for SideData`; `av_frame_side_data_free`
// may release them on any thread.
unsafe impl Send for SideDataArray {}
// SAFETY: `&SideDataArray` reaches the entries only through
// `SideDataArrayRef`, which reads the pointer block and lends shared entry
// handles, `Sync` per `Sync for SideData`.
unsafe impl Sync for SideDataArray {}

impl SideDataArray {
    /// The empty array, C's NULL and 0.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: ptr::null_mut(),
            len: 0,
        }
    }

    /// Takes over the array C built in `entries` and `len`, as
    /// `av_frame_side_data_free` would free it.
    ///
    /// # Safety
    ///
    /// `entries` and `len` must be an array the `av_frame_side_data_*`
    /// routines built (or NULL and 0) that nothing else uses or frees from
    /// now on. That is, `len` is not negative, and `entries` is an
    /// `av_malloc`-family block of `len` pointers to distinct entries those
    /// routines allocated, every one satisfying [`SideData`]'s invariant: in
    /// particular, a payload `av_frame_side_data_new` returned must have
    /// been written in full.
    #[must_use]
    pub unsafe fn from_raw_parts(entries: *mut *mut ffi::AVFrameSideData, len: c_int) -> Self {
        Self { entries, len }
    }

    /// Gives the array to C as its `AVFrameSideData **` and count, which
    /// `av_frame_side_data_free` releases; NULL and 0 when it is empty.
    #[must_use]
    pub fn into_raw_parts(self) -> (*mut *mut ffi::AVFrameSideData, c_int) {
        let this = ManuallyDrop::new(self);
        (this.entries, this.len)
    }

    /// Shared handle to the array.
    #[inline]
    #[must_use]
    pub fn as_ref(&self) -> SideDataArrayRef<'_> {
        // SAFETY: by the type's invariant, the fields are an array of `len`
        // live entries this owner holds; the shared borrow of `self` keeps
        // it unchanged for the handle's lifetime.
        unsafe { SideDataArrayRef::from_raw_parts(self.entries, self.len) }
    }

    /// Exclusive handle to the array.
    #[inline]
    #[must_use]
    pub fn as_mut(&mut self) -> SideDataArrayMut<'_> {
        // The two variables of an array of live entries this owner holds,
        // exclusively borrowed for the handle's lifetime.
        SideDataArrayMut {
            entries: NonNull::from(&mut self.entries),
            len: NonNull::from(&mut self.len),
            _borrow: PhantomData,
        }
    }

    /// The number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Whether the array has no entry.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for SideDataArray {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SideDataArray {
    fn drop(&mut self) {
        // SAFETY: the owner's array, which C frees with every entry, their
        // buffer references and metadata, NULLing and zeroing the variables.
        unsafe { ffi::av_frame_side_data_free(&raw mut self.entries, &raw mut self.len) }
    }
}

impl fmt::Debug for SideDataArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> IntoIterator for &'a SideDataArray {
    type Item = SideDataRef<'a>;
    type IntoIter = SideDataIter<'a>;

    fn into_iter(self) -> SideDataIter<'a> {
        self.as_ref().into_iter()
    }
}

impl<'a> IntoIterator for &'a mut SideDataArray {
    type Item = SideDataMut<'a>;
    type IntoIter = SideDataIterMut<'a>;

    fn into_iter(self) -> SideDataIterMut<'a> {
        self.as_mut().into_iter()
    }
}

/// Handwritten
///
/// Shared borrow of a side data array: a [`SideDataArray`], or a frame's
/// list. `Copy`, like `&[T]`; it lends shared handles to the entries.
#[derive(Clone, Copy)]
pub struct SideDataArrayRef<'a> {
    entries: *const *mut ffi::AVFrameSideData,
    len: usize,
    _borrow: PhantomData<&'a SideData>,
}

// SAFETY: the handle reads the pointer block, which nothing writes while it
// is borrowed, and lends shared entry handles, `Send` and `Sync` per
// `Sync for SideData`.
unsafe impl Send for SideDataArrayRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for SideDataArrayRef<'_> {}

impl<'a> SideDataArrayRef<'a> {
    /// The handle of the array of `len` entries at `entries`; NULL and 0
    /// (or a negative count, which C reads as none) is the empty array.
    ///
    /// # Safety
    ///
    /// `entries` and `len` must be an array of distinct live entries, each
    /// satisfying [`SideData`]'s invariant, which nothing changes or frees
    /// for `'a`.
    pub(crate) unsafe fn from_raw_parts(
        entries: *const *mut ffi::AVFrameSideData,
        len: c_int,
    ) -> Self {
        Self {
            entries,
            len: usize::try_from(len).unwrap_or(0),
            _borrow: PhantomData,
        }
    }

    /// The pointer to entry `i`, which is in bounds.
    fn entry(&self, i: usize) -> *mut ffi::AVFrameSideData {
        debug_assert!(i < self.len);
        // SAFETY: `i < len`, so the read is inside the live pointer block,
        // which nothing writes for `'a`.
        unsafe { self.entries.add(i).read() }
    }

    /// The number of entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the array has no entry.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Entry `i`; `None` past the end.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<SideDataRef<'a>> {
        (i < self.len).then(|| {
            // SAFETY: the array's entry `i`, live and unchanged for `'a`.
            unsafe { SideDataRef::from_ptr(self.entry(i)) }.expect("an array entry is not NULL")
        })
    }

    /// The entries, in array order.
    #[must_use]
    pub fn iter(&self) -> SideDataIter<'a> {
        SideDataIter {
            array: *self,
            range: 0..self.len,
        }
    }
}

impl fmt::Debug for SideDataArrayRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for SideDataArrayRef<'a> {
    type Item = SideDataRef<'a>;
    type IntoIter = SideDataIter<'a>;

    fn into_iter(self) -> SideDataIter<'a> {
        self.iter()
    }
}

/// Handwritten
///
/// Exclusive borrow of a side data array: a [`SideDataArray`], or a frame's
/// list. It lends exclusive handles to the entries, and reaches the shared
/// ones with [`as_ref`](Self::as_ref).
///
/// Entries it lends exclude one another, as `&mut` elements of a slice do:
///
/// ```compile_fail,E0499
/// use ffmpeg::libavutil::frame::SideDataArray;
/// let mut array = SideDataArray::new();
/// let mut entries = array.as_mut();
/// let first = entries.get_mut(0).unwrap();
/// let second = entries.get_mut(1).unwrap();
/// drop((first, second));
/// ```
///
/// and none outlives the array's exclusive borrow:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::frame::SideDataArray;
/// let mut array = SideDataArray::new();
/// let entry = array.as_mut().into_iter().next().unwrap();
/// drop(array);
/// drop(entry);
/// ```
pub struct SideDataArrayMut<'a> {
    // The variables holding the array's pointer and count, which hold an
    // array as `SideDataArrayRef::from_raw_parts` requires, both borrowed
    // exclusively for `'a`.
    entries: NonNull<*mut *mut ffi::AVFrameSideData>,
    len: NonNull<c_int>,
    _borrow: PhantomData<&'a mut SideData>,
}

// SAFETY: the handle is the only access to the array and its entries, which
// are `Send` per `Send for SideData`.
unsafe impl Send for SideDataArrayMut<'_> {}
// SAFETY: `&SideDataArrayMut` reaches the array only through `as_ref`, a
// shared handle, `Sync` per `Sync for SideDataArrayRef`.
unsafe impl Sync for SideDataArrayMut<'_> {}

impl<'a> SideDataArrayMut<'a> {
    /// A shared handle to the array, borrowing this one.
    #[must_use]
    pub fn as_ref(&self) -> SideDataArrayRef<'_> {
        // SAFETY: reads of the two variables, live and exclusively borrowed
        // for `'a`; the array they hold stays unchanged while `self` is
        // borrowed shared.
        unsafe { SideDataArrayRef::from_raw_parts(self.entries.read(), self.len.read()) }
    }

    /// An exclusive handle to the array, borrowing this one.
    #[must_use]
    pub fn as_mut(&mut self) -> SideDataArrayMut<'_> {
        SideDataArrayMut {
            entries: self.entries,
            len: self.len,
            _borrow: PhantomData,
        }
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Whether the array has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entry `i`, writable; `None` past the end.
    #[must_use]
    pub fn get_mut(&mut self, i: usize) -> Option<SideDataMut<'_>> {
        let array = self.as_ref();
        (i < array.len()).then(|| {
            // SAFETY: the array's entry `i`, live, and exclusively borrowed
            // with the array through `&mut self`.
            unsafe { SideDataMut::from_ptr(array.entry(i)) }.expect("an array entry is not NULL")
        })
    }

    /// The entries, writable, in array order.
    #[must_use]
    pub fn iter_mut(&mut self) -> SideDataIterMut<'_> {
        self.as_mut().into_iter()
    }
}

/// The `AV_FRAME_SIDE_DATA_FLAG_*` flags of an addition to a side data array
/// ([`SideDataArrayMut::add`]).
///
/// C's `AV_FRAME_SIDE_DATA_FLAG_NEW_REF`, which makes C add its own
/// reference to the caller's buffer, is not a flag here: an addition takes an
/// owned reference, so a caller keeping one passes a clone
/// ([`try_clone`](BufferReferenceRef::try_clone)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SideDataFlags(c_uint);

impl SideDataFlags {
    /// No flag: append, unless the kind is not `MULTI` and already present.
    pub const NONE: Self = Self(0);
    /// Remove every entry of the kind first.
    pub const UNIQUE: Self = Self(ffi::AV_FRAME_SIDE_DATA_FLAG_UNIQUE);
    /// For a kind that is not `MULTI`, replace the buffer of the entry of the
    /// kind already present, if any, freeing its metadata, rather than
    /// refuse.
    pub const REPLACE: Self = Self(ffi::AV_FRAME_SIDE_DATA_FLAG_REPLACE);

    /// The flags as C's `unsigned` mask.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> c_uint {
        self.0
    }

    /// Whether every flag of `other` is set in `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for SideDataFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for SideDataFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl SideDataArrayMut<'_> {
    /// Wraps: av_frame_side_data_add
    ///
    /// Adds side data of `kind` whose payload is `buf`'s bytes, and returns
    /// the entry holding it; the entry takes `buf` over. `buf` comes back
    /// when C adds nothing: `kind` is not `MULTI`, an entry of it is present
    /// and [`REPLACE`](SideDataFlags::REPLACE) is not set, or C runs out of
    /// memory.
    ///
    /// With [`UNIQUE`](SideDataFlags::UNIQUE), C first removes every entry
    /// of `kind`, even when it then fails to add. With `REPLACE`, an entry of
    /// a kind that is not `MULTI` already present takes `buf` in place of its
    /// buffer, and loses its metadata; that is the entry returned. Otherwise
    /// the new entry is the array's last, with no metadata.
    ///
    /// ```
    /// use ffmpeg::libavutil::buffer::BufferReference;
    /// use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataType};
    /// let mut array = SideDataArray::new();
    /// let mut entries = array.as_mut();
    /// let afd = BufferReference::alloc(1).unwrap().fill(8);
    /// let entry = entries.add(SideDataType::AFD, afd, SideDataFlags::NONE).unwrap();
    /// assert_eq!(entry.as_ref().data(), [8]);
    /// let again = BufferReference::alloc(1).unwrap().fill(9);
    /// let again = entries.add(SideDataType::AFD, again, SideDataFlags::NONE).unwrap_err();
    /// let entry = entries.add(SideDataType::AFD, again, SideDataFlags::REPLACE).unwrap();
    /// assert_eq!(entry.as_ref().data(), [9]);
    /// assert_eq!(array.len(), 1);
    /// ```
    pub fn add(
        &mut self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'_>, BufferReferenceBox> {
        self.as_mut().into_added(kind, buf, flags)
    }
}

impl<'a> SideDataArrayMut<'a> {
    /// [`add`](Self::add), lending the entry for the array's whole borrow.
    fn into_added(
        self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'a>, BufferReferenceBox> {
        let mut slot = buf.into_c();
        // Without NEW_REF, C stores the slot's reference in an entry and
        // NULLs the slot on success, and leaves both alone on failure. A new
        // entry is av_mallocz'd with that reference and its window, a
        // replaced one gets them and a NULL metadata.
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive; `slot` holds a
        // live reference whose window is initialised bytes, so the array
        // keeps `SideData`'s invariant.
        let entry = unsafe {
            ffi::av_frame_side_data_add(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
                &mut slot,
                flags.bits(),
            )
        };
        if entry.is_null() {
            // SAFETY: C failed, so the slot still holds the caller's
            // reference, which the box takes back.
            let buf = unsafe { BufferReferenceBox::from_c(slot) };
            return Err(buf.expect("C keeps the reference it did not add"));
        }
        debug_assert!(slot.is_null());
        // SAFETY: an entry of the array, live and exclusively borrowed with
        // it for `'a`, which `self`, consumed, no longer reaches.
        Ok(unsafe { SideDataMut::from_ptr(entry) }.expect("C returned an entry"))
    }
}

impl SideDataArray {
    /// Adds side data of `kind` whose payload is `buf`'s bytes, and returns
    /// the entry holding it, as [`SideDataArrayMut::add`] does, borrowing
    /// the array.
    pub fn add(
        &mut self,
        kind: SideDataType,
        buf: BufferReferenceBox,
        flags: SideDataFlags,
    ) -> Result<SideDataMut<'_>, BufferReferenceBox> {
        self.as_mut().into_added(kind, buf, flags)
    }
}

impl fmt::Debug for SideDataArrayMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> IntoIterator for SideDataArrayMut<'a> {
    type Item = SideDataMut<'a>;
    type IntoIter = SideDataIterMut<'a>;

    fn into_iter(self) -> SideDataIterMut<'a> {
        let (entries, len) = (self.entries, self.len);
        // SAFETY: reads of the two variables, live and exclusively borrowed
        // for `'a`, which `self`, consumed, no longer reaches; the array
        // stays unchanged for `'a`.
        let array = unsafe { SideDataArrayRef::from_raw_parts(entries.read(), len.read()) };
        SideDataIterMut {
            range: 0..array.len(),
            array,
            _borrow: PhantomData,
        }
    }
}

/// The entries of a side data array, as shared handles, in array order.
#[derive(Clone, Debug)]
pub struct SideDataIter<'a> {
    array: SideDataArrayRef<'a>,
    range: Range<usize>,
}

impl<'a> Iterator for SideDataIter<'a> {
    type Item = SideDataRef<'a>;

    fn next(&mut self) -> Option<SideDataRef<'a>> {
        self.array.get(self.range.next()?)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for SideDataIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.array.get(self.range.next_back()?)
    }
}

impl ExactSizeIterator for SideDataIter<'_> {}

impl FusedIterator for SideDataIter<'_> {}

/// The entries of a side data array, as exclusive handles, in array order.
pub struct SideDataIterMut<'a> {
    array: SideDataArrayRef<'a>,
    range: Range<usize>,
    _borrow: PhantomData<&'a mut SideData>,
}

impl<'a> SideDataIterMut<'a> {
    /// The handle to entry `i`, which the range has just yielded.
    fn handle(&self, i: usize) -> SideDataMut<'a> {
        // SAFETY: the array's entry `i`, live and exclusively borrowed for
        // `'a`; the entries are distinct and the range yields each index
        // once, so no two handles reach one entry.
        unsafe { SideDataMut::from_ptr(self.array.entry(i)) }.expect("an array entry is not NULL")
    }
}

impl fmt::Debug for SideDataIterMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SideDataIterMut")
            .field("remaining", &self.range.len())
            .finish()
    }
}

impl<'a> Iterator for SideDataIterMut<'a> {
    type Item = SideDataMut<'a>;

    fn next(&mut self) -> Option<SideDataMut<'a>> {
        let i = self.range.next()?;
        Some(self.handle(i))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl DoubleEndedIterator for SideDataIterMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let i = self.range.next_back()?;
        Some(self.handle(i))
    }
}

impl ExactSizeIterator for SideDataIterMut<'_> {}

impl FusedIterator for SideDataIterMut<'_> {}

impl<'a> SideDataArrayRef<'a> {
    /// Wraps: av_frame_side_data_get_c
    ///
    /// The first entry of `kind`, in array order; `None` when there is none.
    #[must_use]
    pub fn find(&self, kind: SideDataType) -> Option<SideDataRef<'a>> {
        // The count came from a `c_int`, so it converts back.
        let len = c_int::try_from(self.len).expect("an array counts entries in an int");
        let sd = self.entries.cast();
        // SAFETY: an array of `len` live entries, unchanged for `'a`, which C
        // only reads; it returns one of them, or NULL.
        let entry = unsafe { ffi::av_frame_side_data_get_c(sd, len, kind.into_raw()) };
        // SAFETY: NULL, or an entry of the array (const only for C's
        // const-correctness), live and unchanged for `'a`.
        unsafe { SideDataRef::from_ptr(entry.cast_mut()) }
    }

    /// Whether C's additions of `kind` with `flags` (`_new`, `_add`,
    /// `_clone`) change an entry in place rather than append or refuse:
    /// `REPLACE` without `UNIQUE`, for a kind that is not `MULTI` and has an
    /// entry here.
    fn replaces_in_place(&self, kind: SideDataType, flags: SideDataFlags) -> bool {
        let multi = kind
            .descriptor()
            .is_some_and(|d| d.props().contains(SideDataProps::MULTI));
        flags.contains(SideDataFlags::REPLACE)
            && !flags.contains(SideDataFlags::UNIQUE)
            && !multi
            && self.find(kind).is_some()
    }
}

impl SideDataMut<'_> {
    /// Makes this entry a copy of `src`, as `av_frame_side_data_clone`'s
    /// in-place replacement does and in its order: a copy of `src`'s
    /// metadata (C's flags 0, so keys equal up to ASCII case merge), then a
    /// reference to `src`'s buffer, then `src`'s window and the copy in
    /// place of the entry's own metadata. On failure the entry is as it
    /// was, and the partial copy, which C's own code leaks, is freed.
    fn assign_clone(&mut self, src: SideDataRef<'_>) -> Result<(), c_int> {
        let s = src.as_ptr();
        // SAFETY: reads of three fields projected through the shared
        // handle's pointer to a live `AVFrameSideData`.
        let (metadata, data, size) = unsafe {
            (
                addr_of!((*s).metadata).read(),
                addr_of!((*s).data).read(),
                addr_of!((*s).size).read(),
            )
        };
        let mut copy = ptr::null_mut();
        // SAFETY: `metadata` is NULL or `src`'s live dictionary, which C only
        // reads, into a local slot.
        let ret = unsafe { ffi::av_dict_copy(&mut copy, metadata, 0) };
        // SAFETY: NULL, or the non-empty copy C built (in part, on failure),
        // ours alone: an `av_dict_set` that leaves no entry frees it.
        let copy = Dict::from(unsafe { DictionaryBox::from_c(copy) });
        if ret < 0 {
            return Err(ret);
        }
        let (p, from) = (self.as_mut_ptr(), src.buffer().as_ptr());
        // SAFETY: the entry's own reference slot, exclusively borrowed, and
        // `src`'s live reference, a distinct struct as `src` is another
        // entry: C keeps the slot, or replaces its reference with a new one
        // to `src`'s buffer, and leaves it alone on failure.
        let ret = unsafe { ffi::av_buffer_replace(addr_of_mut!((*p).buf), from) };
        if ret < 0 {
            return Err(ret);
        }
        // SAFETY: writes of two fields projected through the exclusive
        // handle's pointer: `src`'s window, initialised bytes of the buffer
        // the entry now references too, by `SideData`'s invariant.
        unsafe {
            addr_of_mut!((*p).data).write(data);
            addr_of_mut!((*p).size).write(size);
        }
        self.set_metadata(copy);
        Ok(())
    }
}

impl SideDataArrayMut<'_> {
    /// Wraps: av_frame_side_data_get
    ///
    /// The first entry of `kind`, in array order, writable; `None` when
    /// there is none.
    #[must_use]
    pub fn find_mut(&mut self, kind: SideDataType) -> Option<SideDataMut<'_>> {
        self.as_mut().into_found(kind)
    }

    /// Wraps: av_frame_side_data_remove
    ///
    /// Removes every entry of `kind`, freeing each with its buffer reference
    /// and metadata. C moves the array's last entry into each freed slot, so
    /// the others may change order.
    pub fn remove(&mut self, kind: SideDataType) {
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C frees or moves is alive; C keeps the count
        // equal to the entries left, and their invariant.
        unsafe {
            ffi::av_frame_side_data_remove(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
            )
        }
    }

    /// Wraps: av_frame_side_data_remove_by_props
    ///
    /// Removes every entry whose kind has properties in common with `props`
    /// ([`SideDataProps::intersects`] its
    /// [`descriptor`](SideDataType::descriptor)'s), freeing each with its
    /// buffer reference and metadata, as [`remove`](Self::remove) does. Kinds
    /// C has no descriptor for stay.
    pub fn remove_by_props(&mut self, props: SideDataProps) {
        // C's `int` filter, the bits as they are (C ANDs it with an unsigned
        // `props`, which converts it back).
        let props = props.bits() as c_int;
        // SAFETY: as in `remove`.
        unsafe {
            ffi::av_frame_side_data_remove_by_props(self.entries.as_ptr(), self.len.as_ptr(), props)
        }
    }

    /// Wraps: av_frame_side_data_clone
    ///
    /// Adds a copy of `src`, an entry of another array: one that shares its
    /// buffer, through a new reference (so neither payload is writable while
    /// both live), and holds a copy of its metadata, made as C makes it,
    /// where keys equal up to ASCII case collapse into one entry holding
    /// the last one's value (unlike [`Dict`]'s `Clone`). Flags act as for
    /// [`add`](Self::add): [`UNIQUE`](SideDataFlags::UNIQUE) first removes
    /// every entry of `src`'s kind, even when the copy then fails; for a kind
    /// that is not `MULTI` and already present,
    /// [`REPLACE`](SideDataFlags::REPLACE) makes that entry the copy in
    /// place, and without it C refuses. Otherwise the copy is the array's
    /// last entry.
    ///
    /// ```
    /// use ffmpeg::libavutil::buffer::BufferReference;
    /// use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataType};
    /// let mut src = SideDataArray::new();
    /// let afd = BufferReference::alloc(1).unwrap().fill(8);
    /// src.add(SideDataType::AFD, afd, SideDataFlags::NONE).unwrap();
    /// let mut dst = SideDataArray::new();
    /// let entry = src.as_ref().get(0).unwrap();
    /// dst.as_mut().add_clone(entry, SideDataFlags::NONE).unwrap();
    /// assert_eq!(dst.as_ref().get(0).unwrap().data(), [8]);
    /// assert!(dst.as_mut().add_clone(entry, SideDataFlags::NONE).is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EEXIST)` for a kind present without `REPLACE`, and
    /// `AVERROR(ENOMEM)` when C cannot allocate, or `src`'s buffer counts so
    /// many references that another could near wrapping its count. The array
    /// is then as `UNIQUE` left it.
    pub fn add_clone(&mut self, src: SideDataRef<'_>, flags: SideDataFlags) -> Result<(), c_int> {
        if !may_add_reference(src.buffer().ref_count()) {
            return Err(ENOMEM);
        }
        if self.as_ref().replaces_in_place(src.kind(), flags) {
            // C's in-place path leaks a metadata copy that fails part way
            // (`side_data.c:275-277`): run it here, freeing the copy.
            let entry = self.as_mut().into_found(src.kind());
            return entry
                .expect("an entry of the kind is present")
                .assign_clone(src);
        }
        // Every entry C builds or changes holds a new reference to `src`'s
        // buffer, `src`'s window into it, and a copy of its metadata.
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive; `src` a live
        // entry of another array (borrowed shared, so not this one), which C
        // only reads. The entries C writes keep `SideData`'s invariant.
        let ret = unsafe {
            ffi::av_frame_side_data_clone(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                src.as_ptr(),
                flags.bits(),
            )
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_frame_side_data_new
    ///
    /// Adds side data of `kind` with a new payload of `size` zeroed bytes,
    /// and returns the entry holding it, as [`add`](Self::add) does with a
    /// buffer C allocates; its payload is writable
    /// ([`data_mut`](SideDataMut::data_mut)) until its buffer is shared.
    /// `None` when C adds nothing: `kind` is not `MULTI`, an entry of it is
    /// present and [`REPLACE`](SideDataFlags::REPLACE) is not set, or the
    /// allocation fails. [`UNIQUE`](SideDataFlags::UNIQUE) removes every
    /// entry of `kind` first, even then.
    ///
    /// ```
    /// use ffmpeg::libavutil::frame::{SideDataArray, SideDataFlags, SideDataType};
    /// let mut array = SideDataArray::new();
    /// let mut entry = array.alloc_zeroed(SideDataType::AFD, 1, SideDataFlags::NONE).unwrap();
    /// entry.data_mut().unwrap()[0] = 10;
    /// assert_eq!(array.as_ref().find(SideDataType::AFD).unwrap().data(), [10]);
    /// ```
    pub fn alloc_zeroed(
        &mut self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut().into_new_entry(kind, size, flags, zero_into)
    }

    /// Wraps: av_frame_side_data_new
    ///
    /// Adds side data of `kind` with a new payload copied from `bytes`, and
    /// returns the entry holding it, as [`alloc_zeroed`](Self::alloc_zeroed)
    /// does with zeroes.
    pub fn alloc_from_slice(
        &mut self,
        kind: SideDataType,
        bytes: &[u8],
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut()
            .into_new_entry(kind, bytes.len(), flags, |run| copy_into(run, bytes))
    }
}

impl<'a> SideDataArrayMut<'a> {
    /// [`find_mut`](Self::find_mut), lending the entry for the array's
    /// whole borrow.
    fn into_found(self, kind: SideDataType) -> Option<SideDataMut<'a>> {
        // SAFETY: reads of the two variables, which hold an array of live
        // entries; C only reads it, and returns one of its entry pointers
        // (const only for C's const-correctness), or NULL.
        let entry = unsafe {
            ffi::av_frame_side_data_get(self.entries.read(), self.len.read(), kind.into_raw())
        };
        // SAFETY: NULL, or an entry of the array, live and exclusively
        // borrowed with it for `'a`, which `self`, consumed, no longer
        // reaches.
        unsafe { SideDataMut::from_ptr(entry.cast_mut()) }
    }

    /// [`alloc_zeroed`](Self::alloc_zeroed) and
    /// [`alloc_from_slice`](Self::alloc_from_slice), with `init` writing
    /// every byte of the new payload before anything can read it, lending
    /// the entry for the array's whole borrow.
    fn into_new_entry(
        self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
        init: impl FnOnce(&mut [MaybeUninit<u8>]),
    ) -> Option<SideDataMut<'a>> {
        if self.as_ref().replaces_in_place(kind, flags) {
            // C's replacement path stores the buffer it allocated without
            // checking it, so a failed allocation is a NULL dereference
            // (`replace_side_data_from_buf`). Allocate first, and replace
            // through `av_frame_side_data_add`, which writes the entry's
            // fields as C's path does.
            let mut buf = BufferReference::alloc(size)?;
            init(buf.as_mut_slice());
            // SAFETY: `init` wrote every byte.
            let buf = unsafe { buf.assume_init() };
            return self.into_added(kind, buf, flags).ok();
        }
        // SAFETY: the array's two variables, exclusively borrowed, so no
        // handle to an entry C may remove or change is alive. With no entry
        // to replace, C appends an entry holding a fresh, sole reference to
        // `size` bytes, its window, or fails with the array as `UNIQUE` left
        // it; the payload is uninitialised until written below.
        let entry = unsafe {
            ffi::av_frame_side_data_new(
                self.entries.as_ptr(),
                self.len.as_ptr(),
                kind.into_raw(),
                size,
                flags.bits(),
            )
        };
        if entry.is_null() {
            return None;
        }
        // SAFETY: reads of the new entry's window, `size` bytes of a buffer
        // the entry alone references, which nothing has read or lent yet.
        let run = unsafe {
            (
                addr_of!((*entry).data).read(),
                addr_of!((*entry).size).read(),
            )
        };
        let run = match NonNull::new(run.0) {
            Some(data) if run.1 > 0 => (data, run.1),
            _ => (NonNull::dangling(), 0),
        };
        let data = run.0.cast::<MaybeUninit<u8>>().as_ptr();
        // SAFETY: the window's bytes (or nothing), writable, viewed
        // uninitialised and exclusively for `init`, which writes them all
        // before the entry is handed out, restoring `SideData`'s invariant.
        let bytes = unsafe { slice::from_raw_parts_mut(data, run.1) };
        init(bytes);
        // SAFETY: the new entry, its payload written in full, live and
        // exclusively borrowed with the array for `'a`, which `self`,
        // consumed, no longer reaches.
        unsafe { SideDataMut::from_ptr(entry) }
    }
}

/// Zeroes `dst`, in one operation.
fn zero_into(dst: &mut [MaybeUninit<u8>]) {
    // SAFETY: writes the whole exclusively borrowed run.
    unsafe { ptr::write_bytes(dst.as_mut_ptr(), 0, dst.len()) };
}

/// Copies `src` into `dst`, of the same length, in one operation.
fn copy_into(dst: &mut [MaybeUninit<u8>], src: &[u8]) {
    assert_eq!(
        dst.len(),
        src.len(),
        "source length differs from the payload's"
    );
    // SAFETY: two runs of `src.len()` bytes that cannot overlap, `src`
    // borrowed shared and `dst` exclusively.
    unsafe { ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().cast::<u8>(), src.len()) };
}

impl SideDataArray {
    /// The first entry of `kind`, in array order, writable; `None` when
    /// there is none, as [`SideDataArrayMut::find_mut`] finds it, borrowing
    /// the array.
    #[must_use]
    pub fn find_mut(&mut self, kind: SideDataType) -> Option<SideDataMut<'_>> {
        self.as_mut().into_found(kind)
    }

    /// Adds side data of `kind` with `size` zeroed bytes, and returns the
    /// entry holding it, as [`SideDataArrayMut::alloc_zeroed`] does,
    /// borrowing the array.
    pub fn alloc_zeroed(
        &mut self,
        kind: SideDataType,
        size: usize,
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut().into_new_entry(kind, size, flags, zero_into)
    }

    /// Adds side data of `kind` with a payload copied from `bytes`, and
    /// returns the entry holding it, as [`SideDataArrayMut::alloc_from_slice`]
    /// does, borrowing the array.
    pub fn alloc_from_slice(
        &mut self,
        kind: SideDataType,
        bytes: &[u8],
        flags: SideDataFlags,
    ) -> Option<SideDataMut<'_>> {
        self.as_mut()
            .into_new_entry(kind, bytes.len(), flags, |run| copy_into(run, bytes))
    }
}

/// `AVERROR(EINVAL)`.
const EINVAL: c_int = -(ffi::EINVAL as c_int);

/// The number of plane pointers, line sizes and buffer references a frame
/// holds inline (`AV_NUM_DATA_POINTERS`).
pub const NUM_DATA_POINTERS: usize = ffi::AV_NUM_DATA_POINTERS as usize;

/// A set of flag bits C keeps in an `int` field and combines with bitwise
/// operators, so that any bit pattern is a valid value, unknown bits
/// included: the set operations carry them along, and `Debug` shows them in
/// hex after the named ones.
macro_rules! int_flags {
    (
        $(#[$attr:meta])*
        $name:ident { $($(#[$fattr:meta])* $flag:ident = $bits:expr,)+ }
    ) => {
        $(#[$attr])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name(c_int);

        impl $name {
            $($(#[$fattr])* pub const $flag: Self = Self($bits as c_int);)+

            /// Every named flag with its constant's name, in declaration
            /// order.
            const KNOWN: &'static [(Self, &'static str)] = &[$((Self::$flag, stringify!($flag)),)+];

            /// The set with no flag, as a zeroed field holds.
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

            /// Sets every bit of `other`.
            #[inline]
            pub fn insert(&mut self, other: Self) {
                self.0 |= other.0;
            }

            /// Clears every bit of `other`.
            #[inline]
            pub fn remove(&mut self, other: Self) {
                self.0 &= !other.0;
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "("))?;
                let mut rest = self.0;
                let mut first = true;
                for &(flag, name) in Self::KNOWN {
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

        impl BitOr for $name {
            type Output = Self;

            #[inline]
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }

        impl BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }

        impl BitAnd for $name {
            type Output = Self;

            #[inline]
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }

        /// The bits of `self` not in `rhs`.
        impl Sub for $name {
            type Output = Self;

            #[inline]
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 & !rhs.0)
            }
        }
    };
}

int_flags! {
    /// The `AV_FRAME_FLAG_*` bits of a frame's [`flags`](FrameRef::flags):
    /// properties of its content that decoders set and filters and encoders
    /// read.
    FrameFlags {
        /// The data may be corrupted, e.g. by decoding errors.
        CORRUPT = ffi::AV_FRAME_FLAG_CORRUPT,
        /// The frame is a key frame.
        KEY = ffi::AV_FRAME_FLAG_KEY,
        /// The frame is to be decoded, but not output.
        DISCARD = ffi::AV_FRAME_FLAG_DISCARD,
        /// The content is interlaced.
        INTERLACED = ffi::AV_FRAME_FLAG_INTERLACED,
        /// The top field is displayed first, when the content is interlaced.
        TOP_FIELD_FIRST = ffi::AV_FRAME_FLAG_TOP_FIELD_FIRST,
        /// The frame was originally encoded losslessly.
        LOSSLESS = ffi::AV_FRAME_FLAG_LOSSLESS,
    }
}

int_flags! {
    /// The `FF_DECODE_ERROR_*` bits of a frame's
    /// [`decode_error_flags`](FrameRef::decode_error_flags): the errors a
    /// decoder met while still producing the frame.
    DecodeErrorFlags {
        /// The bitstream was invalid.
        INVALID_BITSTREAM = ffi::FF_DECODE_ERROR_INVALID_BITSTREAM,
        /// A reference frame was missing.
        MISSING_REFERENCE = ffi::FF_DECODE_ERROR_MISSING_REFERENCE,
        /// Error concealment was applied.
        CONCEALMENT_ACTIVE = ffi::FF_DECODE_ERROR_CONCEALMENT_ACTIVE,
        /// Slices failed to decode.
        DECODE_SLICES = ffi::FF_DECODE_ERROR_DECODE_SLICES,
    }
}

/// Why [`FrameMut::set_plane`] or [`FrameMut::set_extended_plane`] refused
/// to point a plane into a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneError {
    /// The frame has no such plane: an index of [`NUM_DATA_POINTERS`] or
    /// more, or of [`extended_plane_count`](FrameRef::extended_plane_count)
    /// or more.
    NoPlane,
    /// The frame holds no buffer reference at that index, or none at index
    /// 0, without which C reads the frame as not reference counted.
    NoBuffer,
    /// The offset is not inside the buffer reference's bytes.
    OutOfBounds,
}

impl fmt::Display for PlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoPlane => "no such plane",
            Self::NoBuffer => "no such buffer reference",
            Self::OutOfBounds => "offset outside the buffer reference",
        })
    }
}

impl std::error::Error for PlaneError {}

define_ctype!(
    /// Wraps: AVFrame
    ///
    /// Decoded audio or video: the planes of its samples or pixels, the
    /// buffer references that hold them, what describes them (format,
    /// dimensions or sample count and channel layout, line sizes), their
    /// timing and color properties, and its side data and metadata. Its
    /// getters are on [`FrameRef`] (`as_ref()`), its setters on [`FrameMut`]
    /// (`as_mut()`).
    ///
    /// C allocates a frame only through `av_frame_alloc` and keeps its size
    /// out of the ABI, so a frame is never held by value: its owner is a
    /// [`FrameBox`], built by [`Frame::new`] and freed with
    /// `av_frame_free`. [`FrameMut::reset`] (`av_frame_unref`) releases
    /// everything a frame holds and gives it back its defaults, and
    /// [`FrameRef::try_clone`] (`av_frame_clone`) makes a new frame sharing
    /// its buffers.
    ///
    /// Every frame a handle reaches owns what its pointers hold:
    ///
    /// - each non-NULL `buf[i]`, `hw_frames_ctx` and `opaque_ref` is a
    ///   reference satisfying [`BufferReference`](super::buffer::BufferReference)'s invariant, and
    ///   `hw_frames_ctx` one to a hardware frames context;
    /// - `extended_buf` is NULL with a zero `nb_extended_buf`, or an
    ///   `av_malloc`-family block of `nb_extended_buf` such references,
    ///   none NULL;
    /// - `side_data` and `nb_side_data` are a side data array as
    ///   [`SideDataArray`] holds one;
    /// - `metadata` is NULL or a non-empty dictionary;
    /// - `private_ref` is NULL or a RefStruct reference, which only the
    ///   FFmpeg library holding the frame sets;
    /// - `ch_layout` satisfies [`ChannelLayout`]'s invariant;
    /// - `extended_data` is the frame's own `data` array, or an
    ///   `av_malloc`-family block of at least `ch_layout.nb_channels`
    ///   plane pointers;
    /// - while `buf[0]` holds a reference, every non-NULL plane pointer
    ///   (`data`, and the first `nb_channels` of a separate `extended_data`)
    ///   points inside the bytes of one of `buf` and `extended_buf`; while it
    ///   holds none, which C reads as a frame whose data is not reference
    ///   counted, every plane pointer is NULL unless the caller of
    ///   [`FrameMut::set_data_ptr`] or [`FrameMut::set_extended_data_ptr`]
    ///   vouched for it.
    ///
    /// Nothing else is part of it: in particular, the format, dimensions,
    /// sample count, line sizes and cropping are plain values, which the
    /// safe setters change freely, and which need not describe planes that
    /// fit the buffers. Rust reads a plane through its buffer reference
    /// alone ([`FrameRef::plane`]); wrappers of C routines that read or
    /// write planes through those values (`av_frame_copy`,
    /// `av_frame_make_writable`, `av_frame_apply_cropping`, any filter or
    /// encoder) must check them first. So must wrappers that hand frames
    /// from hardware decoders out as `Frame`s, whose buffers do not hold
    /// bytes.
    ///
    /// Buffers shared with other frames are written by none of them: a plane
    /// is writable ([`FrameMut::plane_mut`]) only while its buffer has no
    /// other reference and is not read-only, as C's convention has it.
    ///
    /// A plane is borrowed from its frame:
    ///
    /// ```compile_fail,E0505
    /// use ffmpeg::libavutil::buffer::BufferReference;
    /// use ffmpeg::libavutil::frame::Frame;
    /// let mut frame = Frame::new().unwrap();
    /// frame.as_mut().set_buffer(0, Some(BufferReference::alloc_zeroed(8).unwrap()));
    /// frame.as_mut().set_plane(0, 0, 0).unwrap();
    /// let plane = frame.as_ref().plane(0).unwrap();
    /// drop(frame);
    /// assert_eq!(plane[0], 0);
    /// ```
    ///
    /// and frozen while a buffer change could take its bytes away:
    ///
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::buffer::BufferReference;
    /// use ffmpeg::libavutil::frame::Frame;
    /// let mut frame = Frame::new().unwrap();
    /// frame.as_mut().set_buffer(0, Some(BufferReference::alloc_zeroed(8).unwrap()));
    /// frame.as_mut().set_plane(0, 0, 0).unwrap();
    /// let plane = frame.as_ref().plane(0).unwrap();
    /// frame.as_mut().set_buffer(0, None);
    /// assert_eq!(plane[0], 0);
    /// ```
    ///
    /// `Send` and `Sync`: what a frame owns is, and the shared bytes are
    /// written only through a sole reference.
    Frame,
    FrameRef,
    FrameMut,
    ffi::AVFrame
);

// A frame is plain data and the owners its pointers hold: buffer references
// (`Send`: C counts them atomically, and their data releases may run on any
// thread), `av_malloc`-family arrays of them and of plane pointers into their
// bytes, side data entries and a dictionary (`Send`), a channel layout
// (`Send`), a RefStruct reference (atomically counted, released through
// `av_free`) and the `opaque` cookie, which nothing in FFmpeg or this crate
// dereferences.
// SAFETY: `av_frame_free` and `av_frame_unref` release all of it with
// `av_buffer_unref`, `av_dict_free`, `av_frame_side_data_free`,
// `av_refstruct_unref`, `av_channel_layout_uninit` and `av_freep`, none tied
// to the allocating thread.
unsafe impl Send for Frame {}
// SAFETY: as for `Send for Frame`: the handle is the only access to the
// frame, and every write it makes to shared bytes is checked sole.
unsafe impl Send for FrameMut<'_> {}

// Through a shared handle, Rust reads the frame's fields, its bytes through
// its buffer references (`Sync`), its side data, dictionary and channel
// layout (`Sync`, read only), and C reads it in `av_frame_clone`, which only
// reads its source while it counts new references to the shared buffers and
// RefStruct objects atomically.
// SAFETY: nothing above writes; bytes are written only through an exclusive
// handle whose buffer has a single reference, so not while another
// reference, or a shared handle to this frame, exists.
unsafe impl Sync for Frame {}
// SAFETY: as for `Sync for Frame`: copies of a shared handle only read.
unsafe impl Send for FrameRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for FrameRef<'_> {}
// SAFETY: `&FrameMut` reaches the frame only through `as_ref`, a shared
// handle; the setters and `plane_mut` need `&mut`.
unsafe impl Sync for FrameMut<'_> {}

/// `av_frame_free` on a local slot holding `frame`.
///
/// # Safety
///
/// `frame` must be a frame `av_frame_alloc` made, satisfying [`Frame`]'s
/// invariant, that nothing uses afterwards.
unsafe fn frame_free(frame: *mut ffi::AVFrame) {
    let mut slot = frame;
    // SAFETY: the caller hands over its sole claim on a live frame; C
    // releases what it holds (`av_frame_unref`), frees the struct and NULLs
    // only the local slot.
    unsafe { ffi::av_frame_free(&mut slot) }
}

/// Wraps: av_frame_free
///
/// The release policy of a [`FrameBox`]: `av_frame_free` releases every
/// buffer reference, side data entry, dictionary and array the frame holds
/// and frees the struct. Its constructor is `av_frame_alloc`
/// ([`Frame::new`]).
///
/// No `Clone`: `av_frame_clone` fails for ordinary frames (any frame without
/// a buffer reference, whose data C copies into a buffer it can only
/// allocate for a known format and size), where `Clone` would abort; the
/// copy is [`FrameRef::try_clone`].
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::frame::Frame;
/// let frame = Frame::new().unwrap();
/// let _ = frame.clone();
/// ```
///
/// Stateless: C frees with `av_free`, and the buffers' data releases may
/// run on any thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameFree;

impl_cdrop!(FrameFree, Frame, frame_free);

// SAFETY: a non-NULL result is a fresh frame C allocated with av_malloc and
// set to its defaults (get_frame_defaults: every pointer NULL, extended_data
// at its own data array, an unspecified empty channel layout), which holds
// nothing and so satisfies `Frame`'s invariant, and which `av_frame_free`
// releases.
unsafe impl CNew<Frame> for FrameFree {
    fn c_new(&self) -> Option<NonNull<Frame>> {
        // SAFETY: no precondition; NULL when C cannot allocate.
        NonNull::new(unsafe { ffi::av_frame_alloc() }.cast())
    }
}

/// A frame, owned: dropping it releases everything the frame holds and the
/// frame (`av_frame_free`). Built by [`Frame::new`] (`av_frame_alloc`);
/// copied by [`FrameRef::try_clone`]. `Send` and `Sync` with [`Frame`].
///
/// Never allocated bare:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::frame::FrameBox;
/// let _ = FrameBox::new_zeroed();
/// ```
pub type FrameBox = CBox<Frame, FrameFree>;

impl Frame {
    /// Wraps: av_frame_alloc
    ///
    /// A new frame with C's defaults: no data, buffer, side data or
    /// metadata, an unknown format (-1), no timestamps
    /// (`AV_NOPTS_VALUE`), a 0/1 time base and sample aspect ratio,
    /// unspecified color properties and an empty channel layout. `None` when
    /// C cannot allocate it.
    #[must_use]
    pub fn new() -> Option<FrameBox> {
        FrameBox::new()
    }

    /// Exchanges the planes of two frames and the buffers holding them: `buf`,
    /// `extended_buf` and its count, `data`, `linesize` and `extended_data`,
    /// which stays pointed at its own frame's `data` where it was.
    ///
    /// # Safety
    ///
    /// `a` and `b` must be distinct live frames nothing else reaches.
    unsafe fn swap_planes(a: *mut ffi::AVFrame, b: *mut ffi::AVFrame) {
        // SAFETY: two distinct live frames, by the caller's contract; each field
        // is read and written whole through raw places.
        unsafe {
            let own_a = (*a).extended_data == addr_of_mut!((*a).data).cast();
            let own_b = (*b).extended_data == addr_of_mut!((*b).data).cast();
            ptr::swap(addr_of_mut!((*a).buf), addr_of_mut!((*b).buf));
            ptr::swap(
                addr_of_mut!((*a).extended_buf),
                addr_of_mut!((*b).extended_buf),
            );
            ptr::swap(
                addr_of_mut!((*a).nb_extended_buf),
                addr_of_mut!((*b).nb_extended_buf),
            );
            ptr::swap(addr_of_mut!((*a).data), addr_of_mut!((*b).data));
            ptr::swap(addr_of_mut!((*a).linesize), addr_of_mut!((*b).linesize));
            ptr::swap(
                addr_of_mut!((*a).extended_data),
                addr_of_mut!((*b).extended_data),
            );
            if own_b {
                (*a).extended_data = addr_of_mut!((*a).data).cast();
            }
            if own_a {
                (*b).extended_data = addr_of_mut!((*b).data).cast();
            }
        }
    }
}

impl<'a> FrameRef<'a> {
    /// The `buf` array, copied out.
    fn buffer_ptrs(&self) -> [*mut ffi::AVBufferRef; NUM_DATA_POINTERS] {
        // SAFETY: a read of the `buf` array projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).buf).read() }
    }

    /// The `extended_buf` pointer and its count, 0 for a NULL array.
    fn extended_buffer_parts(&self) -> (*mut *mut ffi::AVBufferRef, usize) {
        let p = self.as_ptr();
        // SAFETY: reads of two fields projected through the shared handle's
        // pointer to a live, initialised `AVFrame`.
        let (bufs, count) = unsafe {
            (
                addr_of!((*p).extended_buf).read(),
                addr_of!((*p).nb_extended_buf).read(),
            )
        };
        if bufs.is_null() {
            (bufs, 0)
        } else {
            (bufs, usize::try_from(count).unwrap_or(0))
        }
    }

    /// The `data` array, copied out.
    fn data_ptrs(&self) -> [*mut u8; NUM_DATA_POINTERS] {
        // SAFETY: a read of the `data` array projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).data).read() }
    }

    /// The `extended_data` pointer and the number of entries Rust reads
    /// there: the frame's own `data` array when it points at it, else the
    /// channel count (the invariant's minimum).
    fn extended_parts(&self) -> (*mut *mut u8, usize) {
        let p = self.as_ptr();
        // SAFETY: a read of the `extended_data` field projected through the
        // shared handle's pointer to a live, initialised `AVFrame`; the
        // address of its `data` field, projected without a read.
        let (ext, own) = unsafe { (addr_of!((*p).extended_data).read(), addr_of!((*p).data)) };
        if ext.cast_const() == own.cast() {
            (ext, NUM_DATA_POINTERS)
        } else {
            let channels = self.channel_layout().channel_count();
            (ext, usize::try_from(channels).unwrap_or(0))
        }
    }

    /// Whether `extended_data` is a separate array rather than the frame's
    /// own `data`.
    fn has_extended_array(&self) -> bool {
        let p = self.as_ptr();
        // SAFETY: as in `extended_parts`.
        let (ext, own) = unsafe { (addr_of!((*p).extended_data).read(), addr_of!((*p).data)) };
        ext.cast_const() != own.cast()
    }

    /// Whether `buf[0]` holds a reference, which C takes for a frame whose
    /// data is reference counted.
    fn is_counted(&self) -> bool {
        !self.buffer_ptrs()[0].is_null()
    }

    /// Every buffer reference the frame holds, `buf` first, in C's search
    /// order (`av_frame_get_plane_buffer`'s, past its first NULL).
    fn buffers(&self) -> impl Iterator<Item = BufferReferenceRef<'a>> + use<'a> {
        let this = *self;
        (0..NUM_DATA_POINTERS)
            .filter_map(move |i| this.buffer(i))
            .chain((0..this.extended_buffer_count()).filter_map(move |i| this.extended_buffer(i)))
    }

    /// The buffer reference whose bytes `p` points inside, and its offset
    /// there; `None` for NULL, a frame that is not reference counted, or a
    /// pointer inside none of them.
    fn locate(&self, p: *const u8) -> Option<(BufferReferenceRef<'a>, usize)> {
        if p.is_null() || !self.is_counted() {
            return None;
        }
        self.buffers().find_map(|buf| {
            // Read from the fields, without a view of the bytes.
            let (start, len) = buf.parts();
            let start = start.as_ptr().addr();
            let offset = p.addr().wrapping_sub(start);
            (p.addr() >= start && offset < len).then_some((buf, offset))
        })
    }

    /// The bytes from `p` to the end of the buffer reference it points
    /// inside.
    fn view(&self, p: *const u8) -> Option<&'a [u8]> {
        let (buf, offset) = self.locate(p)?;
        Some(&buf.data()[offset..])
    }

    /// Field: AVFrame.data
    ///
    /// Plane `i`'s bytes: from where its `data` pointer points to the end of
    /// the buffer reference holding it, borrowed for as long as the frame
    /// is. `None` for a NULL plane, an index of [`NUM_DATA_POINTERS`] or
    /// more, and the planes of a frame that is not reference counted (no
    /// buffer reference at index 0), whose bytes no buffer bounds.
    ///
    /// A plane usually starts its buffer's bytes, or, for video, shares one
    /// buffer with the other planes; with a negative line size it may point
    /// at its last line, so the view holds only that line's bytes and
    /// whatever follows. How many of the bytes are the plane's is for the
    /// format, dimensions or sample count and line size to tell. Nothing
    /// writes them meanwhile: a writer needs the only reference to their
    /// buffer, and the frame holds one.
    #[must_use]
    pub fn plane(&self, i: usize) -> Option<&'a [u8]> {
        self.view(*self.data_ptrs().get(i)?)
    }

    /// Field: AVFrame.extended_data
    ///
    /// Plane or channel `i`'s bytes through `extended_data`, as
    /// [`plane`](Self::plane) reads `data`: where it points to the end of the
    /// buffer reference holding it. `None` for a NULL entry, an index of
    /// [`extended_plane_count`](Self::extended_plane_count) or more, and the
    /// entries of a frame that is not reference counted.
    ///
    /// For video and for audio with at most [`NUM_DATA_POINTERS`] planes,
    /// `extended_data` is the frame's own `data`, and this reads what
    /// `plane` does; planar audio with more channels lists every channel's
    /// plane here.
    #[must_use]
    pub fn extended_plane(&self, i: usize) -> Option<&'a [u8]> {
        let (ext, count) = self.extended_parts();
        if i >= count {
            return None;
        }
        // SAFETY: `ext` is the frame's `data` array of `NUM_DATA_POINTERS`
        // entries or, by the type's invariant, an array of at least the
        // channel count's; `i` is below the matching count.
        self.view(unsafe { ext.add(i).read() })
    }

    /// Field: AVFrame.extended_data
    ///
    /// The number of entries [`extended_plane`](Self::extended_plane) reads:
    /// [`NUM_DATA_POINTERS`] while `extended_data` is the frame's own `data`,
    /// and the channel layout's count (none for a negative one) while it is
    /// a separate array ([`FrameMut::alloc_extended_data`]).
    #[must_use]
    pub fn extended_plane_count(&self) -> usize {
        self.extended_parts().1
    }

    /// Whether `extended_data` is an array of its own
    /// ([`FrameMut::alloc_extended_data`]) rather than the frame's `data`.
    #[must_use]
    pub fn has_extended_data(&self) -> bool {
        self.has_extended_array()
    }

    /// Field: AVFrame.linesize
    ///
    /// The line sizes of the planes: for video, each plane's byte stride,
    /// negative for a bottom-up plane; for audio, the size of each plane in
    /// `[0]` alone.
    #[must_use]
    pub fn linesizes(&self) -> [i32; NUM_DATA_POINTERS] {
        // SAFETY: a read of the `linesize` array projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).linesize).read() }
    }

    /// Field: AVFrame.width
    ///
    /// The coded width of a video frame, in pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        // SAFETY: a read of an `int` field projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).width).read() }
    }

    /// Field: AVFrame.height
    ///
    /// The coded height of a video frame, in pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        // SAFETY: as in `width`, for the `height` field.
        unsafe { addr_of!((*self.as_ptr()).height).read() }
    }

    /// Field: AVFrame.nb_samples
    ///
    /// The number of audio samples per channel.
    #[must_use]
    pub fn sample_count(&self) -> i32 {
        // SAFETY: as in `width`, for the `nb_samples` field.
        unsafe { addr_of!((*self.as_ptr()).nb_samples).read() }
    }

    /// Field: AVFrame.format
    ///
    /// The format as C stores it: a [`PixelFormat`] for video, a
    /// [`SampleFormat`] for audio, -1 when unknown.
    #[must_use]
    pub fn format(&self) -> i32 {
        // SAFETY: as in `width`, for the `format` field.
        unsafe { addr_of!((*self.as_ptr()).format).read() }
    }

    /// The format read as a video frame's pixel format;
    /// [`PixelFormat::NONE`] when unknown.
    #[must_use]
    pub fn pixel_format(&self) -> PixelFormat {
        PixelFormat::from_raw(ffi::AVPixelFormat(self.format()))
    }

    /// The format read as an audio frame's sample format;
    /// [`SampleFormat::NONE`] when unknown.
    #[must_use]
    pub fn sample_format(&self) -> SampleFormat {
        SampleFormat::from_raw(ffi::AVSampleFormat(self.format()))
    }

    /// Field: AVFrame.pict_type
    ///
    /// The picture type of a video frame.
    #[must_use]
    pub fn picture_type(&self) -> PictureType {
        // SAFETY: a read of an enum field projected through the shared
        // handle's pointer to a live, initialised `AVFrame`; every value of
        // the C enum's integer is a valid `PictureType`.
        PictureType::from_raw(unsafe { addr_of!((*self.as_ptr()).pict_type).read() })
    }

    /// Field: AVFrame.sample_aspect_ratio
    ///
    /// The sample aspect ratio of a video frame, 0/1 when unknown, as a
    /// handle over it that lives as long as this one.
    #[must_use]
    pub fn sample_aspect_ratio(&self) -> RationalRef<'a> {
        // SAFETY: the field is projected through the shared handle's pointer
        // to a live, initialised `AVFrame`, so it is non-NULL, aligned and
        // initialised; the frame stays live and unwritten for `'a`, which
        // the returned getter-only handle cannot outlive.
        unsafe { RationalRef::from_ptr(addr_of!((*self.as_ptr()).sample_aspect_ratio).cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVFrame.pts
    ///
    /// The presentation timestamp, in [`time_base`](Self::time_base) units;
    /// `AV_NOPTS_VALUE` when unknown.
    #[must_use]
    pub fn pts(&self) -> i64 {
        // SAFETY: a read of an `int64_t` field projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).pts).read() }
    }

    /// Field: AVFrame.pkt_dts
    ///
    /// The decoding timestamp of the packet the frame was decoded from;
    /// `AV_NOPTS_VALUE` when unknown.
    #[must_use]
    pub fn packet_dts(&self) -> i64 {
        // SAFETY: as in `pts`, for the `pkt_dts` field.
        unsafe { addr_of!((*self.as_ptr()).pkt_dts).read() }
    }

    /// Field: AVFrame.time_base
    ///
    /// The time base of the frame's timestamps, as a handle over it that
    /// lives as long as this one.
    #[must_use]
    pub fn time_base(&self) -> RationalRef<'a> {
        // SAFETY: as in `sample_aspect_ratio`, for the `time_base` field.
        unsafe { RationalRef::from_ptr(addr_of!((*self.as_ptr()).time_base).cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVFrame.quality
    ///
    /// The quality, from 1 (good) to `FF_LAMBDA_MAX` (bad).
    #[must_use]
    pub fn quality(&self) -> i32 {
        // SAFETY: as in `width`, for the `quality` field.
        unsafe { addr_of!((*self.as_ptr()).quality).read() }
    }

    /// Field: AVFrame.opaque
    ///
    /// The owner's user pointer, as the address it is. FFmpeg never
    /// dereferences it (it only copies it with the frame's properties, and
    /// to and from packets when asked), so what it points at is its
    /// setter's business.
    #[must_use]
    pub fn opaque(&self) -> *mut c_void {
        // SAFETY: a read of the `opaque` field projected through the shared
        // handle's pointer to a live, initialised `AVFrame`; the pointer is
        // copied, not dereferenced.
        unsafe { addr_of!((*self.as_ptr()).opaque).read() }
    }

    /// Field: AVFrame.repeat_pict
    ///
    /// The number of fields to repeat: the frame lasts `repeat_pict + 2`
    /// field durations.
    #[must_use]
    pub fn repeat_pict(&self) -> i32 {
        // SAFETY: as in `width`, for the `repeat_pict` field.
        unsafe { addr_of!((*self.as_ptr()).repeat_pict).read() }
    }

    /// Field: AVFrame.sample_rate
    ///
    /// The sample rate of the audio, in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> i32 {
        // SAFETY: as in `width`, for the `sample_rate` field.
        unsafe { addr_of!((*self.as_ptr()).sample_rate).read() }
    }

    /// Field: AVFrame.buf
    ///
    /// Buffer reference `i` of the planes' bytes, borrowed from the frame;
    /// `None` when the frame holds none at `i` or `i` is
    /// [`NUM_DATA_POINTERS`] or more. Its
    /// [`try_clone`](BufferReferenceRef::try_clone) shares the bytes, which
    /// makes no plane in them writable while the copy lives.
    #[must_use]
    pub fn buffer(&self, i: usize) -> Option<BufferReferenceRef<'a>> {
        let buf = *self.buffer_ptrs().get(i)?;
        // SAFETY: by the type's invariant, NULL or a live reference the frame
        // owns, which nothing releases or replaces while the frame is
        // borrowed shared.
        unsafe { BufferReferenceRef::from_ptr(buf) }
    }

    /// Field: AVFrame.extended_buf
    ///
    /// Extra buffer reference `i`, for the planes of audio with more
    /// planes than [`NUM_DATA_POINTERS`], borrowed from the frame; `None`
    /// past the [`extended_buffer_count`](Self::extended_buffer_count).
    #[must_use]
    pub fn extended_buffer(&self, i: usize) -> Option<BufferReferenceRef<'a>> {
        let (bufs, count) = self.extended_buffer_parts();
        if i >= count {
            return None;
        }
        // SAFETY: by the type's invariant, `bufs` is a block of `count`
        // references, and `i < count`.
        let buf = unsafe { bufs.add(i).read() };
        // SAFETY: by the type's invariant, a live reference the frame owns,
        // which nothing releases or replaces while the frame is borrowed
        // shared.
        unsafe { BufferReferenceRef::from_ptr(buf) }
    }

    /// Field: AVFrame.nb_extended_buf
    ///
    /// The number of extra buffer references.
    #[must_use]
    pub fn extended_buffer_count(&self) -> usize {
        self.extended_buffer_parts().1
    }

    /// Field: AVFrame.side_data
    /// Field: AVFrame.nb_side_data
    ///
    /// The frame's side data, borrowed from it.
    #[must_use]
    pub fn side_data(&self) -> SideDataArrayRef<'a> {
        let p = self.as_ptr();
        // SAFETY: reads of two fields projected through the shared handle's
        // pointer to a live, initialised `AVFrame`.
        let (entries, len) = unsafe {
            (
                addr_of!((*p).side_data).read(),
                addr_of!((*p).nb_side_data).read(),
            )
        };
        // SAFETY: by the type's invariant, a side data array of live entries
        // the frame owns, which nothing changes or frees while the frame is
        // borrowed shared.
        unsafe { SideDataArrayRef::from_raw_parts(entries, len) }
    }

    /// Field: AVFrame.flags
    ///
    /// The frame's flags.
    #[must_use]
    pub fn flags(&self) -> FrameFlags {
        // SAFETY: as in `width`, for the `flags` field.
        FrameFlags::from_bits_retain(unsafe { addr_of!((*self.as_ptr()).flags).read() })
    }

    /// Field: AVFrame.color_range
    ///
    /// The color range, MPEG or JPEG.
    #[must_use]
    pub fn color_range(&self) -> ColorRange {
        // SAFETY: as in `picture_type`, for the `color_range` field.
        ColorRange::from_raw(unsafe { addr_of!((*self.as_ptr()).color_range).read() })
    }

    /// Field: AVFrame.color_primaries
    ///
    /// The color primaries.
    #[must_use]
    pub fn color_primaries(&self) -> ColorPrimaries {
        // SAFETY: as in `picture_type`, for the `color_primaries` field.
        ColorPrimaries::from_raw(unsafe { addr_of!((*self.as_ptr()).color_primaries).read() })
    }

    /// Field: AVFrame.color_trc
    ///
    /// The color transfer characteristic.
    #[must_use]
    pub fn color_transfer(&self) -> ColorTransfer {
        // SAFETY: as in `picture_type`, for the `color_trc` field.
        ColorTransfer::from_raw(unsafe { addr_of!((*self.as_ptr()).color_trc).read() })
    }

    /// Field: AVFrame.colorspace
    ///
    /// The YUV color space.
    #[must_use]
    pub fn color_space(&self) -> ColorSpace {
        // SAFETY: as in `picture_type`, for the `colorspace` field.
        ColorSpace::from_raw(unsafe { addr_of!((*self.as_ptr()).colorspace).read() })
    }

    /// Field: AVFrame.chroma_location
    ///
    /// The location of the chroma samples.
    #[must_use]
    pub fn chroma_location(&self) -> ChromaLocation {
        // SAFETY: as in `picture_type`, for the `chroma_location` field.
        ChromaLocation::from_raw(unsafe { addr_of!((*self.as_ptr()).chroma_location).read() })
    }

    /// Field: AVFrame.best_effort_timestamp
    ///
    /// The timestamp a decoder estimated by heuristics, in stream time base;
    /// `AV_NOPTS_VALUE` when unknown.
    #[must_use]
    pub fn best_effort_timestamp(&self) -> i64 {
        // SAFETY: as in `pts`, for the `best_effort_timestamp` field.
        unsafe { addr_of!((*self.as_ptr()).best_effort_timestamp).read() }
    }

    /// Field: AVFrame.metadata
    ///
    /// The frame's metadata, borrowed from it; `None` when it has none.
    #[must_use]
    pub fn metadata(&self) -> Option<DictionaryRef<'a>> {
        // SAFETY: a read of the `metadata` pointer projected through the
        // shared handle's pointer to a live, initialised `AVFrame`.
        let dict = unsafe { addr_of!((*self.as_ptr()).metadata).read() };
        // SAFETY: by the type's invariant, `dict` is NULL or a non-empty
        // dictionary the frame owns, which nothing frees or writes while the
        // frame is borrowed shared.
        unsafe { DictionaryRef::from_ptr(dict) }
    }

    /// Field: AVFrame.decode_error_flags
    ///
    /// The errors the decoder met while producing the frame.
    #[must_use]
    pub fn decode_error_flags(&self) -> DecodeErrorFlags {
        // SAFETY: as in `width`, for the `decode_error_flags` field.
        DecodeErrorFlags::from_bits_retain(unsafe {
            addr_of!((*self.as_ptr()).decode_error_flags).read()
        })
    }

    /// Field: AVFrame.hw_frames_ctx
    ///
    /// The reference to the hardware frames context of a hardware frame,
    /// borrowed from the frame; `None` for a software frame. Its bytes are
    /// the context's `AVHWFramesContext` struct.
    // A typed handle replaces this byte view once hwcontext.h's
    // AVHWFramesContext is wrapped.
    #[must_use]
    pub fn hw_frames_context(&self) -> Option<BufferReferenceRef<'a>> {
        // SAFETY: a read of the `hw_frames_ctx` pointer projected through the
        // shared handle's pointer to a live, initialised `AVFrame`.
        let buf = unsafe { addr_of!((*self.as_ptr()).hw_frames_ctx).read() };
        // SAFETY: by the type's invariant, NULL or a live reference the frame
        // owns, which nothing releases or replaces while the frame is
        // borrowed shared.
        unsafe { BufferReferenceRef::from_ptr(buf) }
    }

    /// Field: AVFrame.opaque_ref
    ///
    /// The owner's reference-counted user data, borrowed from the frame;
    /// `None` when it has none. FFmpeg only shares it with the frame's
    /// copies, and to and from packets when asked.
    #[must_use]
    pub fn opaque_ref(&self) -> Option<BufferReferenceRef<'a>> {
        // SAFETY: a read of the `opaque_ref` pointer projected through the
        // shared handle's pointer to a live, initialised `AVFrame`.
        let buf = unsafe { addr_of!((*self.as_ptr()).opaque_ref).read() };
        // SAFETY: as in `hw_frames_context`.
        unsafe { BufferReferenceRef::from_ptr(buf) }
    }

    /// Field: AVFrame.crop_top
    ///
    /// The number of pixel rows to discard from the top of a video frame for
    /// presentation.
    #[must_use]
    pub fn crop_top(&self) -> usize {
        // SAFETY: a read of a `size_t` field projected through the shared
        // handle's pointer to a live, initialised `AVFrame`.
        unsafe { addr_of!((*self.as_ptr()).crop_top).read() }
    }

    /// Field: AVFrame.crop_bottom
    ///
    /// The number of pixel rows to discard from the bottom.
    #[must_use]
    pub fn crop_bottom(&self) -> usize {
        // SAFETY: as in `crop_top`, for the `crop_bottom` field.
        unsafe { addr_of!((*self.as_ptr()).crop_bottom).read() }
    }

    /// Field: AVFrame.crop_left
    ///
    /// The number of pixel columns to discard from the left.
    #[must_use]
    pub fn crop_left(&self) -> usize {
        // SAFETY: as in `crop_top`, for the `crop_left` field.
        unsafe { addr_of!((*self.as_ptr()).crop_left).read() }
    }

    /// Field: AVFrame.crop_right
    ///
    /// The number of pixel columns to discard from the right.
    #[must_use]
    pub fn crop_right(&self) -> usize {
        // SAFETY: as in `crop_top`, for the `crop_right` field.
        unsafe { addr_of!((*self.as_ptr()).crop_right).read() }
    }

    /// Field: AVFrame.private_ref
    ///
    /// Whether the FFmpeg library holding the frame keeps a private
    /// RefStruct reference in it. That reference is the library's alone,
    /// which C clears before the frame leaves it, so a frame Rust receives
    /// has none, and nothing here sets one; [`FrameMut::reset`] and the
    /// frame's drop release it and [`try_clone`](Self::try_clone) shares it.
    #[must_use]
    pub fn has_private_ref(&self) -> bool {
        // SAFETY: a read of the `private_ref` pointer projected through the
        // shared handle's pointer to a live, initialised `AVFrame`; it is
        // compared, not dereferenced.
        !unsafe { addr_of!((*self.as_ptr()).private_ref).read() }.is_null()
    }

    /// Field: AVFrame.ch_layout
    ///
    /// The channel layout of the audio, as a handle over it that lives as
    /// long as this one.
    #[must_use]
    pub fn channel_layout(&self) -> ChannelLayoutRef<'a> {
        // SAFETY: the field is projected through the shared handle's pointer
        // to a live, initialised `AVFrame`, so it is non-NULL and aligned,
        // and it satisfies `ChannelLayout`'s invariant by the type's; the
        // frame stays live and unwritten for `'a`.
        unsafe { ChannelLayoutRef::from_ptr(addr_of!((*self.as_ptr()).ch_layout).cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVFrame.duration
    ///
    /// The duration of the frame, in [`time_base`](Self::time_base) units; 0
    /// when unknown.
    #[must_use]
    pub fn duration(&self) -> i64 {
        // SAFETY: as in `pts`, for the `duration` field.
        unsafe { addr_of!((*self.as_ptr()).duration).read() }
    }

    /// Field: AVFrame.alpha_mode
    ///
    /// How the alpha channel of the video is to be handled.
    #[must_use]
    pub fn alpha_mode(&self) -> AlphaMode {
        // SAFETY: as in `picture_type`, for the `alpha_mode` field.
        AlphaMode::from_raw(unsafe { addr_of!((*self.as_ptr()).alpha_mode).read() })
    }

    /// Whether every reference `av_frame_clone` would add stays below the
    /// counts C could wrap: one per buffer reference (planes, side data,
    /// hardware context, user data) and one to the private RefStruct
    /// object.
    fn may_clone(&self) -> bool {
        let wide = !self.has_private_ref() || refstruct::COUNT_IS_WIDE;
        let mut refs = self
            .buffers()
            .chain(self.hw_frames_context())
            .chain(self.opaque_ref())
            .chain(self.side_data().iter().map(|e| e.buffer()));
        wide && refs.all(|buf| may_add_reference(buf.ref_count()))
    }

    /// Wraps: av_frame_clone
    ///
    /// A new frame describing the same data: the same properties, side data
    /// (sharing each payload's buffer), metadata (copied) and channel
    /// layout (copied), and new references to the same buffers (so no
    /// plane in them is writable while both frames live).
    ///
    /// A frame that is not reference counted (no buffer reference at index
    /// 0) has its data copied instead, into buffers C allocates for its
    /// format and dimensions or sample count and channel layout. For a frame
    /// with no planes, such as a new one, that copy fails, and so does the
    /// clone: use C's `av_frame_copy_props` for properties alone. C leaves
    /// the bytes of those buffers outside the copied lines (padding, and
    /// alignment gaps) uninitialised; here they are zero.
    ///
    /// `None` when C fails (the copy above, or memory), or when one of the
    /// buffers' counts would near wrapping.
    #[must_use]
    pub fn try_clone(&self) -> Option<FrameBox> {
        if !self.may_clone() {
            return None;
        }
        // C allocates a frame and av_frame_ref's it: it copies the scalar
        // fields, deep-copies the metadata, side data entries (sharing their
        // buffers) and channel layout, adds references to the buffers, the
        // hardware context, the user data and the RefStruct object, and
        // copies `extended_data`'s channel count of entries when separate.
        // SAFETY: a live frame, read only, whose invariant keeps all of that
        // valid; a frame that is not reference counted has NULL planes (C
        // fails at the first one) or planes whose setter vouched for them.
        // On failure C frees the copy and returns NULL.
        let mut copy = unsafe { ffi::av_frame_clone(self.as_ptr()) };
        if !copy.is_null() && !self.is_counted() {
            // The copy's buffers come from av_buffer_alloc and hold
            // uninitialised bytes around the copied lines, which the type's
            // invariant forbids: trade them for a zeroed set holding the
            // same copy.
            let mut zeroed = self.copy_into_zeroed_buffers();
            if zeroed.is_null() {
                // SAFETY: the copy C made, which nothing else reaches; its
                // release reads none of its bytes.
                unsafe { ffi::av_frame_free(&mut copy) };
                return None;
            }
            // SAFETY: two live frames C built, which nothing else reaches;
            // `zeroed` then holds the copy's buffers, which its release frees
            // without reading their bytes.
            unsafe {
                Frame::swap_planes(copy, zeroed);
                ffi::av_frame_free(&mut zeroed);
            }
        }
        // SAFETY: NULL, or a fresh frame C built by the invariant's rules,
        // whose planes point into its own references to the same buffers, or
        // to buffers of initialised bytes holding a copy of the data.
        unsafe { FrameBox::from_c(copy) }
    }

    /// A new frame of this frame's geometry, holding buffers C allocates for
    /// it (`av_frame_get_buffer`, as `av_frame_ref` does) whose bytes are
    /// zeroed before C copies this frame's data into them
    /// (`av_frame_copy`); NULL when C fails.
    fn copy_into_zeroed_buffers(&self) -> *mut ffi::AVFrame {
        // SAFETY: no precondition; NULL when C cannot allocate.
        let mut f = unsafe { ffi::av_frame_alloc() };
        if f.is_null() {
            return f;
        }
        let src = self.as_ptr();
        // SAFETY: `f` is a fresh frame nothing else reaches and `src` a live
        // one, read only. C copies the layout (by the invariant, one C
        // accepts), allocates buffers for the geometry, whose bytes are
        // zeroed over each reference's window before C copies `src`'s planes
        // into them, through the geometry it copied the planes with in
        // `av_frame_clone`; every failure frees `f`.
        unsafe {
            (*f).format = (*src).format;
            (*f).width = (*src).width;
            (*f).height = (*src).height;
            (*f).nb_samples = (*src).nb_samples;
            let ok =
                ffi::av_channel_layout_copy(&raw mut (*f).ch_layout, &raw const (*src).ch_layout)
                    >= 0
                    && ffi::av_frame_get_buffer(f, 0) >= 0;
            if ok {
                let extra = usize::try_from((*f).nb_extended_buf).unwrap_or(0);
                let extra = (0..extra).map(|i| (*f).extended_buf.add(i).read());
                for buf in (*f).buf.into_iter().chain(extra).filter(|b| !b.is_null()) {
                    ptr::write_bytes((*buf).data, 0, (*buf).size);
                }
            }
            if !ok || ffi::av_frame_copy(f, src) < 0 {
                ffi::av_frame_free(&mut f);
            }
        }
        f
    }
}

impl fmt::Debug for FrameRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("format", &self.format())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("sample_count", &self.sample_count())
            .field("pts", &self.pts())
            .field("flags", &self.flags())
            .field("buffers", &self.buffers().count())
            .field("side_data", &self.side_data().len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for FrameMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

/// A reference field of a frame other than its plane buffers.
#[derive(Clone, Copy)]
enum ReferenceField {
    /// `hw_frames_ctx`.
    HwFramesContext,
    /// `opaque_ref`.
    OpaqueRef,
}

impl FrameMut<'_> {
    /// Clears every plane pointer the type's invariant no longer allows
    /// after a change of buffer references: all of them when `buf[0]` holds
    /// none, else those pointing inside none of the references.
    fn prune_planes(&mut self) {
        let shared = self.as_ref();
        let counted = shared.is_counted();
        let keep = |p: *mut u8| p.is_null() || (counted && shared.locate(p).is_some());
        let mut data = shared.data_ptrs();
        let (ext, count) = shared.extended_parts();
        let separate = shared.has_extended_array();
        for p in &mut data {
            if !keep(*p) {
                *p = ptr::null_mut();
            }
        }
        let entry = |i: usize| {
            // SAFETY: by the type's invariant, a separate `extended_data`
            // holds at least `count`, the channel count, entries, and `i` is
            // below it.
            unsafe { ext.add(i).read() }
        };
        let stale: Vec<usize> = if separate {
            (0..count).filter(|&i| !keep(entry(i))).collect()
        } else {
            Vec::new()
        };
        let p = self.as_mut_ptr();
        // SAFETY: a write of the `data` array projected through the exclusive
        // handle's pointer to a live `AVFrame`; its pointers own nothing.
        unsafe { addr_of_mut!((*p).data).write(data) };
        for i in stale {
            // SAFETY: as above, entry `i < count` of the separate
            // `extended_data` array the frame owns.
            unsafe { ext.add(i).write(ptr::null_mut()) };
        }
    }

    /// The pointer to `offset` bytes into buffer reference `buffer` (`buf`
    /// below [`NUM_DATA_POINTERS`], `extended_buf` from there on), checked
    /// as a plane pointer the type's invariant allows.
    fn plane_target(&self, buffer: usize, offset: usize) -> Result<*mut u8, PlaneError> {
        let shared = self.as_ref();
        if !shared.is_counted() {
            return Err(PlaneError::NoBuffer);
        }
        let buf = match buffer.checked_sub(NUM_DATA_POINTERS) {
            None => shared.buffer(buffer),
            Some(i) => shared.extended_buffer(i),
        }
        .ok_or(PlaneError::NoBuffer)?;
        let (start, len) = buf.parts();
        if offset >= len {
            return Err(PlaneError::OutOfBounds);
        }
        // SAFETY: `offset < len`, inside the reference's bytes, whose pointer
        // comes from C's own field so that writes may go through it.
        Ok(unsafe { start.add(offset) }.as_ptr())
    }

    /// The bytes from `p` to the end of the buffer reference it points
    /// inside, writable when that buffer has no other reference and is not
    /// read-only.
    ///
    /// The slice is derived from the reference's own `data` pointer, not
    /// from `p`, whose provenance may be a shared view's.
    fn view_mut(&mut self, p: *mut u8) -> Option<&mut [u8]> {
        let (buf, offset) = self.as_ref().locate(p)?;
        if !buf.is_writable() {
            return None;
        }
        let (start, len) = buf.parts();
        // SAFETY: `start + offset` is `p`, `offset < len` bytes into the
        // initialised bytes of a reference the frame owns, which are
        // writable: the buffer is not read-only and the frame holds its only
        // reference (the acquire load ordered every released one before us).
        // The exclusive borrow of `self` stops the frame being cloned, or the
        // reference released, while the slice lives.
        Some(unsafe { slice::from_raw_parts_mut(start.as_ptr().add(offset), len - offset) })
    }

    /// Field: AVFrame.data
    ///
    /// Plane `i`'s bytes, writable, as [`FrameRef::plane`] reads them, when
    /// the buffer holding them has no other reference and is not read-only;
    /// `None` otherwise. Cloning the frame or the buffer reference makes them
    /// unwritable until the copy is gone.
    #[must_use]
    pub fn plane_mut(&mut self, i: usize) -> Option<&mut [u8]> {
        let p = *self.as_ref().data_ptrs().get(i)?;
        self.view_mut(p)
    }

    /// Field: AVFrame.data
    ///
    /// Points plane `plane` `offset` bytes into buffer reference `buffer`,
    /// counted across `buf` (`0..NUM_DATA_POINTERS`) and then `extended_buf`.
    ///
    /// # Errors
    ///
    /// [`PlaneError::NoPlane`] for a plane of [`NUM_DATA_POINTERS`] or more;
    /// [`PlaneError::NoBuffer`] when the frame holds no reference at
    /// `buffer`, or none at index 0; [`PlaneError::OutOfBounds`] when
    /// `offset` is not below that reference's length. The plane is left as
    /// it was.
    pub fn set_plane(
        &mut self,
        plane: usize,
        buffer: usize,
        offset: usize,
    ) -> Result<(), PlaneError> {
        if plane >= NUM_DATA_POINTERS {
            return Err(PlaneError::NoPlane);
        }
        let target = self.plane_target(buffer, offset)?;
        let p = self.as_mut_ptr();
        // SAFETY: a write of entry `plane < NUM_DATA_POINTERS` of the `data`
        // array projected through the exclusive handle's pointer to a live
        // `AVFrame`; `target` points inside one of its references, as the
        // invariant asks of a reference-counted frame.
        unsafe {
            addr_of_mut!((*p).data)
                .cast::<*mut u8>()
                .add(plane)
                .write(target)
        };
        Ok(())
    }

    /// Field: AVFrame.data
    ///
    /// Makes plane `i` NULL, as C wants of every plane the format does not
    /// use; does nothing for an index of [`NUM_DATA_POINTERS`] or more.
    pub fn clear_plane(&mut self, i: usize) {
        if i < NUM_DATA_POINTERS {
            let p = self.as_mut_ptr();
            // SAFETY: as in `set_plane`; NULL is always allowed.
            unsafe {
                addr_of_mut!((*p).data)
                    .cast::<*mut u8>()
                    .add(i)
                    .write(ptr::null_mut())
            };
        }
    }

    /// Field: AVFrame.data
    ///
    /// Stores `data` as plane `i`'s pointer, unchecked, for a frame whose
    /// data is not reference counted: memory the caller keeps, which C reads
    /// and copies (`av_frame_clone` copies it into buffers of its own) and
    /// which [`FrameRef::plane`] does not read.
    ///
    /// # Panics
    ///
    /// For an index of [`NUM_DATA_POINTERS`] or more.
    ///
    /// # Safety
    ///
    /// `data` must be NULL, or point inside one of the frame's buffer
    /// references while it holds one at index 0 (as
    /// [`set_plane`](Self::set_plane) checks), or, while it holds none
    /// there, point at memory that, until the plane is cleared or replaced
    /// or the frame reset or dropped:
    ///
    /// - is valid for reads (and for writes, where C writes the frame) of the
    ///   whole plane as C reads it through the frame's format, dimensions or
    ///   sample count, channel layout, line sizes and `extended_data` at each
    ///   call that reads the frame's data (`av_frame_clone`'s copy among
    ///   them), whatever setter changed those since;
    /// - is not written through any other path while the frame or a C copy
    ///   of the pointer may read it.
    ///
    /// A buffer reference added later at index 0 clears every plane outside
    /// it, this one included.
    pub unsafe fn set_data_ptr(&mut self, i: usize, data: *mut u8) {
        assert!(i < NUM_DATA_POINTERS, "plane {i} out of range");
        let p = self.as_mut_ptr();
        // SAFETY: as in `set_plane`; the caller vouches for the pointer, which
        // the invariant then admits.
        unsafe { addr_of_mut!((*p).data).cast::<*mut u8>().add(i).write(data) };
    }

    /// Field: AVFrame.extended_data
    ///
    /// Entry `i` of `extended_data`'s bytes, writable, as
    /// [`FrameRef::extended_plane`] reads them, when the buffer holding them
    /// has no other reference and is not read-only; `None` otherwise.
    #[must_use]
    pub fn extended_plane_mut(&mut self, i: usize) -> Option<&mut [u8]> {
        let (ext, count) = self.as_ref().extended_parts();
        if i >= count {
            return None;
        }
        // SAFETY: as in `FrameRef::extended_plane`.
        let p = unsafe { ext.add(i).read() };
        self.view_mut(p)
    }

    /// Field: AVFrame.extended_data
    ///
    /// Points entry `i` of `extended_data` `offset` bytes into buffer
    /// reference `buffer`, as [`set_plane`](Self::set_plane) does for
    /// `data`. While `extended_data` is the frame's own `data`, that is
    /// plane `i`.
    ///
    /// # Errors
    ///
    /// As `set_plane`'s, [`PlaneError::NoPlane`] meaning an index of
    /// [`FrameRef::extended_plane_count`] or more.
    pub fn set_extended_plane(
        &mut self,
        i: usize,
        buffer: usize,
        offset: usize,
    ) -> Result<(), PlaneError> {
        let (ext, count) = self.as_ref().extended_parts();
        if i >= count {
            return Err(PlaneError::NoPlane);
        }
        let target = self.plane_target(buffer, offset)?;
        // SAFETY: entry `i` below the count the invariant guarantees of the
        // frame's own `data` array or separate block, reached through the
        // exclusive handle; `target` points inside one of its references.
        unsafe { ext.add(i).write(target) };
        Ok(())
    }

    /// Field: AVFrame.extended_data
    ///
    /// Makes entry `i` of `extended_data` NULL; does nothing for an index of
    /// [`FrameRef::extended_plane_count`] or more.
    pub fn clear_extended_plane(&mut self, i: usize) {
        let (ext, count) = self.as_ref().extended_parts();
        if i < count {
            // SAFETY: as in `set_extended_plane`; NULL is always allowed.
            unsafe { ext.add(i).write(ptr::null_mut()) };
        }
    }

    /// Field: AVFrame.extended_data
    ///
    /// Stores `data` as entry `i` of `extended_data`, unchecked, as
    /// [`set_data_ptr`](Self::set_data_ptr) does for `data`.
    ///
    /// # Panics
    ///
    /// For an index of [`FrameRef::extended_plane_count`] or more.
    ///
    /// # Safety
    ///
    /// As for `set_data_ptr`.
    pub unsafe fn set_extended_data_ptr(&mut self, i: usize, data: *mut u8) {
        let (ext, count) = self.as_ref().extended_parts();
        assert!(i < count, "extended plane {i} out of range");
        // SAFETY: as in `set_extended_plane`; the caller vouches for the
        // pointer, which the invariant then admits.
        unsafe { ext.add(i).write(data) };
    }

    /// Field: AVFrame.extended_data
    ///
    /// Gives the frame an `extended_data` array of its own, of one entry per
    /// channel of its layout, as C allocates for planar audio with more
    /// channels than [`NUM_DATA_POINTERS`]: the first entries copy `data`,
    /// the rest are NULL. An array the frame had already is freed.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a layout of no channel; `AVERROR(ENOMEM)` when
    /// the array cannot be allocated. The frame is left as it was.
    pub fn alloc_extended_data(&mut self) -> Result<(), i32> {
        let shared = self.as_ref();
        let channels = shared.channel_layout().channel_count();
        let count = match usize::try_from(channels) {
            Ok(n) if n > 0 => n,
            _ => return Err(EINVAL),
        };
        let data = shared.data_ptrs();
        // SAFETY: no precondition; a zeroed block of `count` pointers, or
        // NULL when C cannot allocate it (an overflowing size included).
        let block = unsafe { ffi::av_calloc(count, size_of::<*mut u8>()) }.cast::<*mut u8>();
        if block.is_null() {
            return Err(ENOMEM);
        }
        for (i, &plane) in data.iter().enumerate().take(count) {
            // SAFETY: `i < count`, inside the fresh block.
            unsafe { block.add(i).write(plane) };
        }
        self.replace_extended_array(block);
        Ok(())
    }

    /// Field: AVFrame.extended_data
    ///
    /// Frees the frame's own `extended_data` array, if any, and points
    /// `extended_data` back at `data`, as C's defaults have it.
    pub fn reset_extended_data(&mut self) {
        let p = self.as_mut_ptr();
        // SAFETY: the address of the `data` field, projected without a read.
        let own = unsafe { addr_of_mut!((*p).data) }.cast::<*mut u8>();
        self.replace_extended_array(own);
    }

    /// Makes `with` the frame's `extended_data`, freeing a separate array it
    /// replaces.
    fn replace_extended_array(&mut self, with: *mut *mut u8) {
        let separate = self.as_ref().has_extended_array();
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of the `extended_data` pointer projected
        // through the exclusive handle's pointer to a live `AVFrame`. `with`
        // is the frame's own `data` array or a block of at least the channel
        // count's entries the frame now owns, each NULL or copied from `data`,
        // as the invariant asks.
        let old = unsafe {
            let old = addr_of!((*p).extended_data).read();
            addr_of_mut!((*p).extended_data).write(with);
            old
        };
        if separate && old != with {
            // SAFETY: by the type's invariant, the frame's own
            // `av_malloc`-family block, which no longer reaches the frame.
            unsafe { ffi::av_free(old.cast()) };
        }
    }

    /// Field: AVFrame.linesize
    ///
    /// Sets plane `i`'s line size.
    ///
    /// # Panics
    ///
    /// For an index of [`NUM_DATA_POINTERS`] or more.
    pub fn set_linesize(&mut self, i: usize, linesize: i32) {
        assert!(i < NUM_DATA_POINTERS, "plane {i} out of range");
        let p = self.as_mut_ptr();
        // SAFETY: a write of entry `i < NUM_DATA_POINTERS` of the `linesize`
        // array projected through the exclusive handle's pointer to a live
        // `AVFrame`; Rust reads no plane through it.
        unsafe {
            addr_of_mut!((*p).linesize)
                .cast::<c_int>()
                .add(i)
                .write(linesize)
        };
    }

    /// Field: AVFrame.linesize
    ///
    /// Sets every plane's line size.
    pub fn set_linesizes(&mut self, linesizes: [i32; NUM_DATA_POINTERS]) {
        // SAFETY: as in `set_linesize`, for the whole array.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).linesize).write(linesizes) };
    }

    /// Field: AVFrame.width
    ///
    /// Sets the coded width. Rust reads no plane through it; C routines that
    /// do (copies, allocations, cropping) check it as their wrappers say.
    pub fn set_width(&mut self, width: i32) {
        // SAFETY: a write of an `int` field projected through the exclusive
        // handle's pointer to a live `AVFrame`; not part of the invariant.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).width).write(width) }
    }

    /// Field: AVFrame.height
    ///
    /// Sets the coded height, as [`set_width`](Self::set_width) does the
    /// width.
    pub fn set_height(&mut self, height: i32) {
        // SAFETY: as in `set_width`, for the `height` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).height).write(height) }
    }

    /// Field: AVFrame.nb_samples
    ///
    /// Sets the number of audio samples per channel, as
    /// [`set_width`](Self::set_width) does the width.
    pub fn set_sample_count(&mut self, count: i32) {
        // SAFETY: as in `set_width`, for the `nb_samples` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).nb_samples).write(count) }
    }

    /// Field: AVFrame.format
    ///
    /// Sets the format as C stores it, -1 for unknown, as
    /// [`set_width`](Self::set_width) does the width.
    pub fn set_format(&mut self, format: i32) {
        // SAFETY: as in `set_width`, for the `format` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).format).write(format) }
    }

    /// Sets the format to a video frame's pixel format.
    pub fn set_pixel_format(&mut self, format: PixelFormat) {
        self.set_format(format.into_raw().0);
    }

    /// Sets the format to an audio frame's sample format.
    pub fn set_sample_format(&mut self, format: SampleFormat) {
        self.set_format(format.into_raw().0);
    }

    /// Field: AVFrame.pict_type
    ///
    /// Sets the picture type.
    pub fn set_picture_type(&mut self, kind: PictureType) {
        // SAFETY: a write of an enum field projected through the exclusive
        // handle's pointer to a live `AVFrame`; the old value owns nothing.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).pict_type).write(kind.into_raw()) }
    }

    /// Field: AVFrame.sample_aspect_ratio
    ///
    /// An exclusive handle over the sample aspect ratio, for writing its
    /// parts; this handle is frozen while it lives.
    #[must_use]
    pub fn sample_aspect_ratio_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the field is projected through the exclusive handle's
        // pointer to a live `AVFrame`, so it is non-NULL, aligned and
        // initialised; the result borrows `self` exclusively, so it is the
        // only access to the field while it lives.
        unsafe { RationalMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).sample_aspect_ratio)) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVFrame.sample_aspect_ratio
    ///
    /// Sets the sample aspect ratio.
    pub fn set_sample_aspect_ratio(&mut self, ratio: Rational) {
        // SAFETY: as in `set_picture_type`, for the `sample_aspect_ratio`
        // field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).sample_aspect_ratio).write(ratio.into_raw()) }
    }

    /// Field: AVFrame.pts
    ///
    /// Sets the presentation timestamp.
    pub fn set_pts(&mut self, pts: i64) {
        // SAFETY: a write of an `int64_t` field projected through the
        // exclusive handle's pointer to a live `AVFrame`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).pts).write(pts) }
    }

    /// Field: AVFrame.pkt_dts
    ///
    /// Sets the decoding timestamp of the frame's packet.
    pub fn set_packet_dts(&mut self, dts: i64) {
        // SAFETY: as in `set_pts`, for the `pkt_dts` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).pkt_dts).write(dts) }
    }

    /// Field: AVFrame.time_base
    ///
    /// An exclusive handle over the time base, for writing its parts; this
    /// handle is frozen while it lives.
    #[must_use]
    pub fn time_base_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: as in `sample_aspect_ratio_mut`, for the `time_base` field.
        unsafe { RationalMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).time_base)) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVFrame.time_base
    ///
    /// Sets the time base.
    pub fn set_time_base(&mut self, time_base: Rational) {
        // SAFETY: as in `set_picture_type`, for the `time_base` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).time_base).write(time_base.into_raw()) }
    }

    /// Field: AVFrame.quality
    ///
    /// Sets the quality.
    pub fn set_quality(&mut self, quality: i32) {
        // SAFETY: as in `set_width`, for the `quality` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).quality).write(quality) }
    }

    /// Field: AVFrame.opaque
    ///
    /// Stores the user pointer `opaque`, NULL included. Nothing in FFmpeg or
    /// this crate dereferences or frees it, so any address is safe to store:
    /// keeping what it points at alive for as long as the caller means to
    /// read it back is the caller's concern.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // stored, never dereferenced
    pub fn set_opaque(&mut self, opaque: *mut c_void) {
        // SAFETY: a write of the `opaque` field projected through the
        // exclusive handle's pointer to a live `AVFrame`; the pointer is
        // stored, not dereferenced, and the old one owned nothing.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).opaque).write(opaque) }
    }

    /// Field: AVFrame.repeat_pict
    ///
    /// Sets the number of fields to repeat.
    pub fn set_repeat_pict(&mut self, repeat: i32) {
        // SAFETY: as in `set_width`, for the `repeat_pict` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).repeat_pict).write(repeat) }
    }

    /// Field: AVFrame.sample_rate
    ///
    /// Sets the sample rate.
    pub fn set_sample_rate(&mut self, rate: i32) {
        // SAFETY: as in `set_width`, for the `sample_rate` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).sample_rate).write(rate) }
    }

    /// Field: AVFrame.buf
    ///
    /// Makes `with` buffer reference `i` (none for `None`) and returns the
    /// reference the frame held there, as an owner. Every plane pointer
    /// left pointing inside none of the frame's references is cleared, and
    /// so is every plane when the frame is left without a reference at
    /// index 0; planes are then pointed into the new bytes with
    /// [`set_plane`](Self::set_plane).
    ///
    /// C wants `buf` filled from index 0 without a gap, and reads a frame
    /// without a reference at 0 as not reference counted.
    ///
    /// # Panics
    ///
    /// For an index of [`NUM_DATA_POINTERS`] or more.
    #[must_use = "dropping the old reference releases it; use `set_buffer` for that"]
    pub fn replace_buffer(
        &mut self,
        i: usize,
        with: Option<BufferReferenceBox>,
    ) -> Option<BufferReferenceBox> {
        assert!(i < NUM_DATA_POINTERS, "buffer {i} out of range");
        let new = with.map_or(ptr::null_mut(), BufferReferenceBox::into_c);
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of entry `i < NUM_DATA_POINTERS` of the
        // `buf` array projected through the exclusive handle's pointer to a
        // live `AVFrame`; `new` is NULL or a reference nothing else owns,
        // which keeps the invariant's first point, and `prune_planes`
        // restores its last.
        let old = unsafe {
            let slot = addr_of_mut!((*p).buf)
                .cast::<*mut ffi::AVBufferRef>()
                .add(i);
            let old = slot.read();
            slot.write(new);
            old
        };
        self.prune_planes();
        // SAFETY: by the type's invariant, `old` is NULL or a reference the
        // frame owned, which no longer reaches the frame.
        unsafe { BufferReferenceBox::from_c(old) }
    }

    /// Field: AVFrame.buf
    ///
    /// Makes `with` buffer reference `i` and releases the one the frame held,
    /// as [`replace_buffer`](Self::replace_buffer) and a drop.
    ///
    /// # Panics
    ///
    /// For an index of [`NUM_DATA_POINTERS`] or more.
    pub fn set_buffer(&mut self, i: usize, with: Option<BufferReferenceBox>) {
        drop(self.replace_buffer(i, with));
    }

    /// Field: AVFrame.buf
    ///
    /// Takes buffer reference `i` out, as [`replace_buffer`](Self::replace_buffer)
    /// with `None`; `None` when the frame held none there.
    ///
    /// # Panics
    ///
    /// For an index of [`NUM_DATA_POINTERS`] or more.
    #[must_use]
    pub fn take_buffer(&mut self, i: usize) -> Option<BufferReferenceBox> {
        self.replace_buffer(i, None)
    }

    /// Field: AVFrame.extended_buf
    ///
    /// Makes `with` extra buffer reference `i` and returns the one the frame
    /// held there, clearing the plane pointers left pointing inside no
    /// reference as [`replace_buffer`](Self::replace_buffer) does; `with`
    /// comes back as the error past the
    /// [`extended_buffer_count`](FrameRef::extended_buffer_count).
    pub fn replace_extended_buffer(
        &mut self,
        i: usize,
        with: BufferReferenceBox,
    ) -> Result<BufferReferenceBox, BufferReferenceBox> {
        let (bufs, count) = self.as_ref().extended_buffer_parts();
        if i >= count {
            return Err(with);
        }
        // SAFETY: entry `i < count` of the `extended_buf` block the frame
        // owns, reached through the exclusive handle; the new reference is
        // nothing else's and never NULL, as the invariant asks, and
        // `prune_planes` restores its last point.
        let old = unsafe {
            let old = bufs.add(i).read();
            bufs.add(i).write(with.into_c());
            old
        };
        self.prune_planes();
        // SAFETY: by the type's invariant, a reference the frame owned, which
        // no longer reaches the frame.
        Ok(unsafe { BufferReferenceBox::from_c(old) }
            .expect("an extra buffer reference is not NULL"))
    }

    /// Field: AVFrame.extended_buf
    ///
    /// Appends `with` to the extra buffer references, growing their array;
    /// `with` comes back when C cannot.
    pub fn push_extended_buffer(
        &mut self,
        with: BufferReferenceBox,
    ) -> Result<(), BufferReferenceBox> {
        let (bufs, count) = self.as_ref().extended_buffer_parts();
        let Ok(new_count) = c_int::try_from(count + 1) else {
            return Err(with);
        };
        // SAFETY: `bufs` is NULL or the frame's own `av_malloc`-family block;
        // C reallocates it to `count + 1` pointers, keeping the first
        // `count`, or returns NULL and leaves it alone.
        let grown = unsafe {
            ffi::av_realloc_array(bufs.cast(), count + 1, size_of::<*mut ffi::AVBufferRef>())
        }
        .cast::<*mut ffi::AVBufferRef>();
        if grown.is_null() {
            return Err(with);
        }
        let p = self.as_mut_ptr();
        // SAFETY: entry `count` of the grown block, and writes of the two
        // fields projected through the exclusive handle's pointer to a live
        // `AVFrame`, which keep the invariant: `count + 1` non-NULL
        // references the frame owns.
        unsafe {
            grown.add(count).write(with.into_c());
            addr_of_mut!((*p).extended_buf).write(grown);
            addr_of_mut!((*p).nb_extended_buf).write(new_count);
        }
        Ok(())
    }

    /// Field: AVFrame.extended_buf
    ///
    /// Takes the last extra buffer reference out, freeing their array with
    /// the last one, and clears the plane pointers left pointing inside no
    /// reference as [`replace_buffer`](Self::replace_buffer) does; `None`
    /// when the frame holds none.
    #[must_use]
    pub fn pop_extended_buffer(&mut self) -> Option<BufferReferenceBox> {
        let (bufs, count) = self.as_ref().extended_buffer_parts();
        let last = count.checked_sub(1)?;
        let p = self.as_mut_ptr();
        // SAFETY: entry `last < count` of the frame's `extended_buf` block,
        // and writes of the two fields projected through the exclusive
        // handle's pointer to a live `AVFrame`: the count drops to `last`,
        // and an emptied block is freed and NULLed, as the invariant asks.
        let old = unsafe {
            let old = bufs.add(last).read();
            if last == 0 {
                ffi::av_free(bufs.cast());
                addr_of_mut!((*p).extended_buf).write(ptr::null_mut());
            }
            addr_of_mut!((*p).nb_extended_buf).write(last as c_int);
            old
        };
        self.prune_planes();
        // SAFETY: by the type's invariant, a reference the frame owned, which
        // no longer reaches the frame.
        let old = unsafe { BufferReferenceBox::from_c(old) };
        Some(old.expect("an extra buffer reference is not NULL"))
    }

    /// Field: AVFrame.side_data
    /// Field: AVFrame.nb_side_data
    ///
    /// The frame's side data, writable, borrowed exclusively from it.
    #[must_use]
    pub fn side_data_mut(&mut self) -> SideDataArrayMut<'_> {
        let p = self.as_mut_ptr();
        // SAFETY: projections of two fields through the exclusive handle's
        // pointer to a live `AVFrame`, without a read.
        let (entries, len) = unsafe {
            (
                addr_of_mut!((*p).side_data),
                addr_of_mut!((*p).nb_side_data),
            )
        };
        // The frame's two side data variables, which hold an array as
        // `SideDataArrayRef::from_raw_parts` requires by the type's
        // invariant, exclusively borrowed with the frame.
        SideDataArrayMut {
            entries: NonNull::new(entries).expect("a field of a live struct is not NULL"),
            len: NonNull::new(len).expect("a field of a live struct is not NULL"),
            _borrow: PhantomData,
        }
    }

    /// Field: AVFrame.flags
    ///
    /// Sets the frame's flags.
    pub fn set_flags(&mut self, flags: FrameFlags) {
        // SAFETY: as in `set_width`, for the `flags` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).flags).write(flags.bits()) }
    }

    /// Field: AVFrame.color_range
    ///
    /// Sets the color range.
    pub fn set_color_range(&mut self, range: ColorRange) {
        // SAFETY: as in `set_picture_type`, for the `color_range` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).color_range).write(range.into_raw()) }
    }

    /// Field: AVFrame.color_primaries
    ///
    /// Sets the color primaries.
    pub fn set_color_primaries(&mut self, primaries: ColorPrimaries) {
        // SAFETY: as in `set_picture_type`, for the `color_primaries` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).color_primaries).write(primaries.into_raw()) }
    }

    /// Field: AVFrame.color_trc
    ///
    /// Sets the color transfer characteristic.
    pub fn set_color_transfer(&mut self, transfer: ColorTransfer) {
        // SAFETY: as in `set_picture_type`, for the `color_trc` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).color_trc).write(transfer.into_raw()) }
    }

    /// Field: AVFrame.colorspace
    ///
    /// Sets the YUV color space.
    pub fn set_color_space(&mut self, space: ColorSpace) {
        // SAFETY: as in `set_picture_type`, for the `colorspace` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).colorspace).write(space.into_raw()) }
    }

    /// Field: AVFrame.chroma_location
    ///
    /// Sets the location of the chroma samples.
    pub fn set_chroma_location(&mut self, location: ChromaLocation) {
        // SAFETY: as in `set_picture_type`, for the `chroma_location` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).chroma_location).write(location.into_raw()) }
    }

    /// Field: AVFrame.best_effort_timestamp
    ///
    /// Sets the estimated timestamp.
    pub fn set_best_effort_timestamp(&mut self, timestamp: i64) {
        // SAFETY: as in `set_pts`, for the `best_effort_timestamp` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).best_effort_timestamp).write(timestamp) }
    }

    /// Field: AVFrame.metadata
    ///
    /// Makes `with` the frame's metadata, and returns the metadata the frame
    /// held.
    #[must_use = "dropping the old metadata frees it; use `set_metadata` for that"]
    pub fn replace_metadata(&mut self, with: Dict) -> Dict {
        let new = Option::<DictionaryBox>::from(with).map_or(ptr::null_mut(), CBox::into_c);
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of the `metadata` pointer projected
        // through the exclusive handle's pointer to a live `AVFrame`; `new`
        // is NULL or a non-empty dictionary nothing else owns, which keeps
        // the type's invariant.
        let old = unsafe {
            let old = addr_of!((*p).metadata).read();
            addr_of_mut!((*p).metadata).write(new);
            old
        };
        // SAFETY: by the type's invariant, `old` is NULL or a non-empty
        // dictionary the frame owned, which no longer reaches the frame.
        Dict::from(unsafe { DictionaryBox::from_c(old) })
    }

    /// Field: AVFrame.metadata
    ///
    /// Makes `with` the frame's metadata, and frees the metadata the frame
    /// held, as [`replace_metadata`](Self::replace_metadata) and a drop.
    pub fn set_metadata(&mut self, with: Dict) {
        drop(self.replace_metadata(with));
    }

    /// Field: AVFrame.metadata
    ///
    /// Takes the frame's metadata out, leaving it none.
    #[must_use]
    pub fn take_metadata(&mut self) -> Dict {
        self.replace_metadata(Dict::new())
    }

    /// Field: AVFrame.decode_error_flags
    ///
    /// Sets the decode error flags.
    pub fn set_decode_error_flags(&mut self, flags: DecodeErrorFlags) {
        // SAFETY: as in `set_width`, for the `decode_error_flags` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).decode_error_flags).write(flags.bits()) }
    }

    /// Makes `with` the frame's reference in `field` (none for `None`), and
    /// returns the one it held. For [`ReferenceField::HwFramesContext`],
    /// `with` must be a hardware frames context reference, which this
    /// private helper leaves to its callers.
    fn replace_reference(
        &mut self,
        field: ReferenceField,
        with: Option<BufferReferenceBox>,
    ) -> Option<BufferReferenceBox> {
        let new = with.map_or(ptr::null_mut(), BufferReferenceBox::into_c);
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of one reference field projected through
        // the exclusive handle's pointer to a live `AVFrame`; `new` is NULL or
        // a reference nothing else owns, of the kind the field takes.
        let old = unsafe {
            let slot = match field {
                ReferenceField::HwFramesContext => addr_of_mut!((*p).hw_frames_ctx),
                ReferenceField::OpaqueRef => addr_of_mut!((*p).opaque_ref),
            };
            slot.replace(new)
        };
        // SAFETY: by the type's invariant, `old` is NULL or a reference the
        // frame owned, which no longer reaches the frame.
        unsafe { BufferReferenceBox::from_c(old) }
    }

    /// Field: AVFrame.hw_frames_ctx
    ///
    /// Makes `with` the frame's hardware frames context reference (none for
    /// `None`) and returns the one it held.
    ///
    /// # Safety
    ///
    /// `with` must be `None`, or a reference to a hardware frames context
    /// (one `av_hwframe_ctx_alloc` made, or a reference to it), whose bytes C
    /// reads as an `AVHWFramesContext` (`av_frame_make_writable`,
    /// `av_frame_copy`, the `av_hwframe_*` routines, hardware decoders,
    /// filters and encoders).
    // A safe setter taking a typed frames context replaces this seam once
    // hwcontext.h's AVHWFramesContext is wrapped.
    #[must_use = "dropping the old reference releases it"]
    pub unsafe fn replace_hw_frames_context(
        &mut self,
        with: Option<BufferReferenceBox>,
    ) -> Option<BufferReferenceBox> {
        // The caller vouches for `with`.
        self.replace_reference(ReferenceField::HwFramesContext, with)
    }

    /// Field: AVFrame.hw_frames_ctx
    ///
    /// Takes the frame's hardware frames context reference out, leaving it
    /// none; `None` when it held none.
    #[must_use]
    pub fn take_hw_frames_context(&mut self) -> Option<BufferReferenceBox> {
        self.replace_reference(ReferenceField::HwFramesContext, None)
    }

    /// Field: AVFrame.opaque_ref
    ///
    /// Makes `with` the owner's reference-counted user data (none for
    /// `None`) and returns the reference the frame held.
    #[must_use = "dropping the old reference releases it; use `set_opaque_ref` for that"]
    pub fn replace_opaque_ref(
        &mut self,
        with: Option<BufferReferenceBox>,
    ) -> Option<BufferReferenceBox> {
        // Any reference is allowed there, as FFmpeg only shares and releases
        // it.
        self.replace_reference(ReferenceField::OpaqueRef, with)
    }

    /// Field: AVFrame.opaque_ref
    ///
    /// Makes `with` the owner's reference-counted user data and releases the
    /// reference the frame held, as
    /// [`replace_opaque_ref`](Self::replace_opaque_ref) and a drop.
    pub fn set_opaque_ref(&mut self, with: Option<BufferReferenceBox>) {
        drop(self.replace_opaque_ref(with));
    }

    /// Field: AVFrame.crop_top
    ///
    /// Sets the number of rows to discard from the top. Rust reads no plane
    /// through it; `av_frame_apply_cropping` checks it against the
    /// dimensions.
    pub fn set_crop_top(&mut self, rows: usize) {
        // SAFETY: a write of a `size_t` field projected through the exclusive
        // handle's pointer to a live `AVFrame`; not part of the invariant.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).crop_top).write(rows) }
    }

    /// Field: AVFrame.crop_bottom
    ///
    /// Sets the number of rows to discard from the bottom.
    pub fn set_crop_bottom(&mut self, rows: usize) {
        // SAFETY: as in `set_crop_top`, for the `crop_bottom` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).crop_bottom).write(rows) }
    }

    /// Field: AVFrame.crop_left
    ///
    /// Sets the number of columns to discard from the left.
    pub fn set_crop_left(&mut self, columns: usize) {
        // SAFETY: as in `set_crop_top`, for the `crop_left` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).crop_left).write(columns) }
    }

    /// Field: AVFrame.crop_right
    ///
    /// Sets the number of columns to discard from the right.
    pub fn set_crop_right(&mut self, columns: usize) {
        // SAFETY: as in `set_crop_top`, for the `crop_right` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).crop_right).write(columns) }
    }

    /// Field: AVFrame.ch_layout
    ///
    /// An exclusive handle over the channel layout, for changing it in
    /// place; `None` while `extended_data` is an array of the frame's own
    /// ([`FrameRef::has_extended_data`]), whose length C knows only as the
    /// channel count, which the handle could raise past it. This handle is
    /// frozen while it lives.
    #[must_use]
    pub fn channel_layout_mut(&mut self) -> Option<ChannelLayoutMut<'_>> {
        if self.as_ref().has_extended_array() {
            return None;
        }
        // SAFETY: the field is projected through the exclusive handle's
        // pointer to a live `AVFrame`, so it is non-NULL and aligned, and it
        // satisfies `ChannelLayout`'s invariant; the result borrows `self`
        // exclusively. With `extended_data` the frame's own `data`, the
        // channel count bounds nothing the invariant reads.
        unsafe { ChannelLayoutMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).ch_layout)) }
    }

    /// Field: AVFrame.ch_layout
    ///
    /// Makes `with` the frame's channel layout and returns the layout it
    /// held. While `extended_data` is an array of the frame's own, a layout
    /// of more channels than the current one would overrun it, so `with`
    /// comes back as the error instead.
    pub fn replace_channel_layout(
        &mut self,
        with: ChannelLayoutVal,
    ) -> Result<ChannelLayoutVal, ChannelLayoutVal> {
        let shared = self.as_ref();
        if shared.has_extended_array()
            && with.as_ref().channel_count() > shared.channel_layout().channel_count()
        {
            return Err(with);
        }
        let new = with.into_inner();
        let p = self.as_mut_ptr();
        // SAFETY: a read and a write of the `ch_layout` field projected
        // through the exclusive handle's pointer to a live frame; `new` satisfies
        // `ChannelLayout`'s invariant and is nothing else's, and its count
        // keeps a separate `extended_data` long enough. The old value moves
        // out whole, so its map has one owner still.
        let old = unsafe {
            addr_of_mut!((*p).ch_layout)
                .cast::<ChannelLayout>()
                .replace(new)
        };
        Ok(ChannelLayoutVal::new(old))
    }

    /// Field: AVFrame.ch_layout
    ///
    /// Makes `with` the frame's channel layout and disposes of the layout it
    /// held, as [`replace_channel_layout`](Self::replace_channel_layout) and
    /// a drop; `with` comes back as the error where that refuses it.
    pub fn set_channel_layout(&mut self, with: ChannelLayoutVal) -> Result<(), ChannelLayoutVal> {
        self.replace_channel_layout(with).map(drop)
    }

    /// Field: AVFrame.duration
    ///
    /// Sets the duration.
    pub fn set_duration(&mut self, duration: i64) {
        // SAFETY: as in `set_pts`, for the `duration` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).duration).write(duration) }
    }

    /// Field: AVFrame.alpha_mode
    ///
    /// Sets how the alpha channel is to be handled.
    pub fn set_alpha_mode(&mut self, mode: AlphaMode) {
        // SAFETY: as in `set_picture_type`, for the `alpha_mode` field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).alpha_mode).write(mode.into_raw()) }
    }

    /// Wraps: av_frame_unref
    ///
    /// Releases everything the frame holds (buffer references, side data,
    /// metadata, `extended_data` and `extended_buf` arrays, the hardware
    /// context, user data and private references, the channel layout) and
    /// gives every field its default back, as [`Frame::new`] makes it.
    pub fn reset(&mut self) {
        // SAFETY: a live frame satisfying the type's invariant, which C
        // releases field by field (av_frame_side_data_free, av_buffer_unref,
        // av_dict_free, av_refstruct_unref, av_channel_layout_uninit,
        // av_freep of a separate extended_data) before writing the defaults
        // av_frame_alloc writes. No handle into the frame outlives the
        // exclusive borrow.
        unsafe { ffi::av_frame_unref(self.as_mut_ptr()) }
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

    /// A side data array of one entry per payload, kinds `MULTI`.
    fn side_data_array(payloads: &[&[u8]]) -> SideDataArray {
        let mut a = SideDataArray::new();
        for p in payloads {
            let buf = crate::libavutil::buffer::BufferReference::alloc(p.len())
                .unwrap()
                .copy_from_slice(p);
            let added = a.add(SideDataType::SEI_UNREGISTERED, buf, SideDataFlags::NONE);
            assert!(added.is_ok());
        }
        a
    }

    /// The count of references to the buffer `b` belongs to.
    fn count(b: &BufferReferenceBox) -> u32 {
        b.as_ref().ref_count()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_parts_read_a_null_payload_as_empty() {
        let a = side_data_array(&[b"abc"]);
        let buf = a.as_ref().get(0).unwrap().buffer().as_ptr().cast_mut();
        // An entry in Rust storage pointing at the array's buffer, with a
        // NULL payload of 0 and of 5 bytes; nothing releases it.
        for size in [0, 5] {
            let mut raw = ffi::AVFrameSideData {
                type_: SideDataType::AFD.into_raw(),
                data: ptr::null_mut(),
                size,
                metadata: ptr::null_mut(),
                buf,
            };
            // SAFETY: a live entry in this frame, borrowed for the block.
            let e = unsafe { SideDataRef::from_ptr(&raw mut raw) }.unwrap();
            assert_eq!(e.parts().1, 0);
            assert!(e.data().is_empty() && e.is_empty());
            assert_eq!(e.kind(), SideDataType::AFD);
        }
    }

    #[test]
    fn side_data_array_refs_read_negative_counts_as_empty() {
        for len in [c_int::MIN, -1, 0] {
            // SAFETY: an empty array, by C's reading of the count.
            let r = unsafe { SideDataArrayRef::from_raw_parts(ptr::null(), len) };
            assert!(r.is_empty() && r.get(0).is_none() && r.iter().next().is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_array_round_trips_through_raw_parts() {
        let a = side_data_array(&[b"x", b"", b"yz"]);
        let (sd, nb) = a.into_raw_parts();
        assert_eq!(nb, 3);
        // SAFETY: the array given up just above, unchanged.
        let a = unsafe { SideDataArray::from_raw_parts(sd, nb) };
        let lens: Vec<usize> = a.as_ref().iter().map(|e| e.len()).collect();
        assert_eq!(lens, [1, 0, 2]);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_takes_the_window_of_its_new_reference() {
        let mut a = side_data_array(&[b"old"]);
        let mut buf = crate::libavutil::buffer::BufferReference::alloc(8)
            .unwrap()
            .copy_from_slice(b"01234567");
        buf.as_mut().advance(2);
        buf.as_mut().truncate(3);
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(0).unwrap();
        let old = e.replace_buffer(buf);
        assert_eq!(old.as_ref().data(), b"old");
        assert_eq!(e.as_ref().data(), b"234");
        assert_eq!(e.as_ref().buffer().data(), b"234");
        e.data_mut().unwrap()[0] = b'x';
        assert_eq!(e.as_ref().data(), b"x34");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_array_drop_releases_every_reference_and_dictionary() {
        let mut a = side_data_array(&[b"a", b"bb"]);
        let copies: Vec<BufferReferenceBox> = a
            .as_ref()
            .iter()
            .map(|e| e.buffer().try_clone().unwrap())
            .collect();
        assert!(copies.iter().all(|c| count(c) == 2));
        let mut entries = a.as_mut();
        let mut e = entries.get_mut(1).unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        assert!(e.data_mut().is_none());
        drop(a);
        assert!(copies.iter().all(|c| count(c) == 1));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_debug_names_the_kind_and_sizes() {
        let mut a = side_data_array(&[b"abcd"]);
        a.as_mut()
            .get_mut(0)
            .unwrap()
            .set_metadata(Dict::from_entries([(c"k", c"v"), (c"l", c"w")]).unwrap());
        assert_eq!(
            format!("{a:?}"),
            "[SideData { kind: SEI_UNREGISTERED, len: 4, metadata: 2, .. }]"
        );
        let mut entries = a.as_mut();
        assert_eq!(
            format!("{entries:?}"),
            format!("{a_dbg:?}", a_dbg = entries.as_ref())
        );
        let e = entries.get_mut(0).unwrap();
        assert!(format!("{e:?}").starts_with("SideData {"));
        let it = SideDataArray::new();
        assert_eq!(format!("{it:?}"), "[]");
    }

    /// The array handles and iterators over entries in Rust storage, whose
    /// kinds alone are read and written (no C call, so Miri runs it): every
    /// exclusive handle the iterators lend reaches its own entry, and the
    /// array's variables stay readable through the shared handle after.
    #[test]
    fn side_data_array_handles_lend_each_entry_once() {
        let blank = |t: u32| ffi::AVFrameSideData {
            type_: ffi::AVFrameSideDataType(t),
            data: ptr::null_mut(),
            size: 0,
            metadata: ptr::null_mut(),
            buf: ptr::null_mut(),
        };
        let mut entries = [blank(1), blank(2), blank(3)];
        let mut pointers: [*mut ffi::AVFrameSideData; 3] =
            core::array::from_fn(|i| (&raw mut entries[i]));
        let mut sd = pointers.as_mut_ptr();
        let mut nb: c_int = 3;
        let mut array = SideDataArrayMut {
            entries: NonNull::from(&mut sd),
            len: NonNull::from(&mut nb),
            _borrow: PhantomData,
        };
        let mut lent: Vec<SideDataMut<'_>> = array.iter_mut().collect();
        for (i, e) in lent.iter_mut().enumerate().rev() {
            e.set_kind(SideDataType::from_raw(ffi::AVFrameSideDataType(
                10 + i as u32,
            )));
        }
        drop(lent);
        let mut back = array.as_mut().into_iter();
        back.next_back().unwrap().set_kind(SideDataType::AFD);
        array.get_mut(0).unwrap().set_kind(SideDataType::A53_CC);
        assert!(array.get_mut(3).is_none());
        let kinds: Vec<u32> = array
            .as_ref()
            .iter()
            .map(|e| e.kind().into_raw().0)
            .collect();
        let afd = SideDataType::AFD.into_raw().0;
        let a53 = SideDataType::A53_CC.into_raw().0;
        assert_eq!(kinds, [a53, 11, afd]);
        assert_eq!(array.len(), 3);
        assert!(array.as_ref().iter().rev().all(|e| e.data().is_empty()));
    }

    #[test]
    fn side_data_flags_are_cs_bits() {
        assert_eq!(SideDataFlags::NONE.bits(), 0);
        assert_eq!(
            SideDataFlags::UNIQUE.bits(),
            ffi::AV_FRAME_SIDE_DATA_FLAG_UNIQUE
        );
        assert_eq!(
            SideDataFlags::REPLACE.bits(),
            ffi::AV_FRAME_SIDE_DATA_FLAG_REPLACE
        );
        assert!(SideDataFlags::default() == SideDataFlags::NONE);
    }

    /// `alloc_zeroed` replacing an entry, when its allocation fails, returns
    /// `None` and leaves the entry as it was: C's `av_frame_side_data_new`
    /// would free the entry's buffer and metadata and then dereference the
    /// NULL buffer (`replace_side_data_from_buf`), so the wrapper allocates
    /// first (a deliberate correction of C).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_alloc_replacing_with_a_failed_allocation_keeps_the_entry() {
        let mut a = SideDataArray::new();
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, &[7], SideDataFlags::NONE)
            .unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        // More than `av_max_alloc`'s default `INT_MAX` limit, which fails
        // without touching the process-wide limit other tests share.
        let huge = 1 << 40;
        assert!(
            a.alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        let mut entries = a.as_mut();
        assert!(
            entries
                .alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        let e = a.as_ref().find(SideDataType::AFD).unwrap();
        assert_eq!(e.data(), [7]);
        assert_eq!(e.metadata().map(|m| m.len()), Some(1));
        assert_eq!(a.len(), 1);
        // Without an entry to replace, C's own path fails cleanly.
        let multi = SideDataType::SEI_UNREGISTERED;
        assert!(
            a.alloc_zeroed(multi, huge, SideDataFlags::REPLACE)
                .is_none()
        );
        assert!(
            a.alloc_zeroed(SideDataType::AFD, huge, SideDataFlags::UNIQUE)
                .is_none()
        );
        assert!(a.is_empty(), "UNIQUE removed the entry before failing");
    }

    /// `alloc_*` replacing an entry goes through `av_frame_side_data_add`
    /// with a buffer it filled: the entry keeps its place and kind, takes the
    /// new payload, loses its metadata and releases its old reference.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_alloc_replacing_takes_a_filled_buffer() {
        let mut a = side_data_array(&[b"m"]);
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, b"old", SideDataFlags::NONE)
            .unwrap();
        e.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        let old = e.as_ref().buffer().try_clone().unwrap();
        assert_eq!(count(&old), 2);
        let mut e = a
            .alloc_from_slice(SideDataType::AFD, b"new!", SideDataFlags::REPLACE)
            .unwrap();
        assert_eq!(e.data_mut().unwrap(), b"new!");
        assert_eq!(count(&old), 1);
        let mut entries = a.as_mut();
        let e = entries
            .alloc_zeroed(SideDataType::AFD, 3, SideDataFlags::REPLACE)
            .unwrap();
        assert_eq!(e.as_ref().data(), [0; 3]);
        assert!(e.as_ref().metadata().is_none());
        let kinds: Vec<_> = a.as_ref().iter().map(|e| e.kind()).collect();
        assert_eq!(kinds, [SideDataType::SEI_UNREGISTERED, SideDataType::AFD]);
        assert_eq!(old.as_ref().data(), b"old");
    }

    /// `add_clone` refuses at `MAX_REFERENCES` without calling C, leaving
    /// the count and the array, and clones one below it.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_clones_stop_at_the_reference_limit() {
        use crate::libavutil::buffer::MAX_REFERENCES;
        use crate::libavutil::buffer::unit_tests::{counts, set_count};
        let src = side_data_array(&[b"abc"]);
        let held = src.as_ref().get(0).unwrap().buffer().try_clone().unwrap();
        let mut dst = SideDataArray::new();
        let entry = src.as_ref().get(0).unwrap();
        set_count(&held, MAX_REFERENCES);
        assert_eq!(
            dst.as_mut().add_clone(entry, SideDataFlags::NONE),
            Err(ENOMEM)
        );
        assert_eq!(counts(&held).1, MAX_REFERENCES);
        assert!(dst.is_empty());
        set_count(&held, MAX_REFERENCES - 1);
        assert_eq!(dst.as_mut().add_clone(entry, SideDataFlags::NONE), Ok(()));
        assert_eq!(counts(&held).1, MAX_REFERENCES);
        set_count(&held, 3);
        assert_eq!(counts(&held), (3, 3));
        drop(dst);
        assert_eq!(count(&held), 2);
    }

    /// `find` on a handle whose count C stored negative reads no pointer,
    /// as C's loop does not.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn side_data_find_reads_negative_counts_as_empty() {
        // SAFETY: a dangling, never-read block, and a negative count that
        // the handle reads as empty.
        let r = unsafe { SideDataArrayRef::from_raw_parts(NonNull::dangling().as_ptr(), -3) };
        assert!(r.find(SideDataType::AFD).is_none());
    }

    #[test]
    fn side_data_payload_initialisers_write_every_byte() {
        let mut run = [MaybeUninit::new(0xa5u8); 9];
        zero_into(&mut run);
        // SAFETY: every byte is initialised, before and after.
        assert_eq!(unsafe { run.map(|b| b.assume_init()) }, [0; 9]);
        copy_into(&mut run, b"012345678");
        // SAFETY: as above.
        assert_eq!(unsafe { run.map(|b| b.assume_init()) }, *b"012345678");
        zero_into(&mut []);
        copy_into(&mut [], &[]);
    }

    #[test]
    #[should_panic(expected = "source length differs")]
    fn side_data_payload_copies_need_equal_lengths() {
        copy_into(&mut [MaybeUninit::uninit(); 2], b"abc");
    }

    /// `AVBuffer`'s private layout (`buffer_internal.h`), through which a
    /// frame test sets a count no number of live clones could reach.
    #[repr(C)]
    struct FrameBufferBody {
        data: *mut u8,
        size: usize,
        refcount: c_uint,
        free: Option<unsafe extern "C" fn(*mut c_void, *mut u8)>,
        opaque: *mut c_void,
        flags: c_int,
        flags_internal: c_int,
    }

    /// Sets the count of `buf`'s buffer behind C's back, returning the old
    /// one.
    fn frame_set_count(buf: BufferReferenceRef<'_>, count: c_uint) -> c_uint {
        assert_eq!(size_of::<FrameBufferBody>(), 48);
        let body = buf.buffer().as_ptr().cast_mut().cast::<FrameBufferBody>();
        // SAFETY: `body` is the live 48-byte body `buf` counts, which this
        // single-threaded test alone reaches; the caller restores the true
        // count before any reference is released.
        unsafe {
            let old = addr_of!((*body).refcount).read();
            addr_of_mut!((*body).refcount).write(count);
            old
        }
    }

    /// A frame holding one zeroed buffer of `len` bytes at index 0.
    fn frame_with_buffer(len: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        f.as_mut()
            .set_buffer(0, Some(BufferReference::alloc_zeroed(len).unwrap()));
        f
    }

    /// The raw plane pointers of a frame.
    fn frame_data(f: &FrameBox) -> [*mut u8; NUM_DATA_POINTERS] {
        f.as_ref().data_ptrs()
    }

    #[test]
    fn frame_flags_are_cs_bits() {
        let all = [
            (FrameFlags::CORRUPT, ffi::AV_FRAME_FLAG_CORRUPT),
            (FrameFlags::KEY, ffi::AV_FRAME_FLAG_KEY),
            (FrameFlags::DISCARD, ffi::AV_FRAME_FLAG_DISCARD),
            (FrameFlags::INTERLACED, ffi::AV_FRAME_FLAG_INTERLACED),
            (
                FrameFlags::TOP_FIELD_FIRST,
                ffi::AV_FRAME_FLAG_TOP_FIELD_FIRST,
            ),
            (FrameFlags::LOSSLESS, ffi::AV_FRAME_FLAG_LOSSLESS),
        ];
        for (flag, bits) in all {
            assert_eq!(flag.bits(), bits as c_int);
        }
        let errors = [
            (
                DecodeErrorFlags::INVALID_BITSTREAM,
                ffi::FF_DECODE_ERROR_INVALID_BITSTREAM,
            ),
            (
                DecodeErrorFlags::MISSING_REFERENCE,
                ffi::FF_DECODE_ERROR_MISSING_REFERENCE,
            ),
            (
                DecodeErrorFlags::CONCEALMENT_ACTIVE,
                ffi::FF_DECODE_ERROR_CONCEALMENT_ACTIVE,
            ),
            (
                DecodeErrorFlags::DECODE_SLICES,
                ffi::FF_DECODE_ERROR_DECODE_SLICES,
            ),
        ];
        for (flag, bits) in errors {
            assert_eq!(flag.bits(), bits as c_int);
        }
        assert_eq!(NUM_DATA_POINTERS, 8);
    }

    #[test]
    fn frame_flags_set_operations_keep_unknown_bits() {
        let mut f = FrameFlags::empty();
        assert!(f.is_empty() && f == FrameFlags::default());
        f.insert(FrameFlags::KEY | FrameFlags::LOSSLESS);
        assert!(f.contains(FrameFlags::KEY) && !f.contains(FrameFlags::CORRUPT));
        f.remove(FrameFlags::KEY);
        assert_eq!(f, FrameFlags::LOSSLESS);
        let odd = FrameFlags::from_bits_retain(i32::MIN | 0x40 | 2);
        assert_eq!(odd.bits(), i32::MIN | 0x42);
        assert_eq!((odd & FrameFlags::KEY), FrameFlags::KEY);
        assert_eq!((odd - FrameFlags::KEY).bits(), i32::MIN | 0x40);
        let mut acc = FrameFlags::empty();
        acc |= odd;
        assert_eq!(acc, odd);
        assert_eq!(format!("{:?}", FrameFlags::empty()), "FrameFlags(0x0)");
        assert_eq!(
            format!("{:?}", FrameFlags::KEY | FrameFlags::INTERLACED),
            "FrameFlags(KEY | INTERLACED)"
        );
        assert_eq!(
            format!("{:?}", FrameFlags::from_bits_retain(0x102)),
            "FrameFlags(KEY | 0x100)"
        );
        assert_eq!(
            format!("{:?}", DecodeErrorFlags::from_bits_retain(0x30)),
            "DecodeErrorFlags(0x30)"
        );
        assert_eq!(
            format!("{:?}", DecodeErrorFlags::CONCEALMENT_ACTIVE),
            "DecodeErrorFlags(CONCEALMENT_ACTIVE)"
        );
    }

    #[test]
    fn frame_plane_errors_describe_themselves() {
        for (e, s) in [
            (PlaneError::NoPlane, "no such plane"),
            (PlaneError::NoBuffer, "no such buffer reference"),
            (
                PlaneError::OutOfBounds,
                "offset outside the buffer reference",
            ),
        ] {
            assert_eq!(e.to_string(), s);
            let _: &dyn std::error::Error = &e;
        }
    }

    /// A plane pointer at the end of its buffer's window is inside none, as
    /// for C's `av_frame_get_plane_buffer`; the one before it is.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_locate_bounds_planes_by_the_window() {
        let mut f = frame_with_buffer(16);
        let (start, len) = f.as_ref().buffer(0).unwrap().parts();
        let start = start.as_ptr();
        let r = f.as_ref();
        assert!(r.locate(start.wrapping_add(len)).is_none());
        assert_eq!(r.locate(start.wrapping_add(len - 1)).unwrap().1, len - 1);
        assert_eq!(r.locate(start).unwrap().1, 0);
        assert!(r.locate(start.wrapping_sub(1)).is_none());
        assert!(r.locate(ptr::null()).is_none());
        // A frame not reference counted locates nothing, even inside a
        // buffer it holds past index 0.
        let b = f.as_mut().take_buffer(0).unwrap();
        f.as_mut().set_buffer(1, Some(b));
        let (start, _) = f.as_ref().buffer(1).unwrap().parts();
        assert!(f.as_ref().locate(start.as_ptr()).is_none());
        assert_eq!(f.as_mut().set_plane(0, 1, 0), Err(PlaneError::NoBuffer));
    }

    /// Buffer changes clear exactly the plane pointers no reference holds
    /// any more, and every one once `buf[0]` is gone.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_buffer_changes_prune_stale_planes() {
        let mut f = frame_with_buffer(32);
        f.as_mut()
            .set_buffer(1, Some(BufferReference::alloc_zeroed(8).unwrap()));
        let mut m = f.as_mut();
        m.set_plane(0, 0, 0).unwrap();
        m.set_plane(1, 0, 16).unwrap();
        m.set_plane(2, 1, 4).unwrap();
        let before = frame_data(&f);
        assert!(before[..3].iter().all(|p| !p.is_null()));
        // Replacing buf[1] clears plane 2 only.
        let old = f
            .as_mut()
            .replace_buffer(1, Some(BufferReference::alloc_zeroed(8).unwrap()));
        assert!(old.is_some());
        let after = frame_data(&f);
        assert_eq!(after[..2], before[..2]);
        assert!(after[2].is_null());
        // A second reference to buf[0]'s buffer keeps its planes when buf[0]
        // goes elsewhere, as long as buf[0] holds a reference.
        let copy = f.as_ref().buffer(0).unwrap().try_clone().unwrap();
        f.as_mut().set_buffer(1, Some(copy));
        let swapped = f
            .as_mut()
            .replace_buffer(0, Some(BufferReference::alloc_zeroed(4).unwrap()));
        assert_eq!(frame_data(&f)[..2], before[..2]);
        drop(swapped);
        // Taking buf[0] clears everything, even planes inside buf[1].
        drop(f.as_mut().take_buffer(0));
        assert!(frame_data(&f).iter().all(|p| p.is_null()));
    }

    /// The separate `extended_data` array is pruned over the channel count,
    /// and freed by `reset_extended_data` and by the frame's drop (a leak
    /// otherwise).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_extended_arrays_are_pruned_and_freed() {
        let mut f = frame_with_buffer(64);
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(10))
            .unwrap();
        f.as_mut().set_plane(0, 0, 0).unwrap();
        f.as_mut().alloc_extended_data().unwrap();
        assert!(f.as_ref().has_extended_data());
        assert_eq!(f.as_ref().extended_plane_count(), 10);
        for i in 0..10 {
            f.as_mut().set_extended_plane(i, 0, i).unwrap();
        }
        // A second allocation frees the first array (else a leak).
        f.as_mut().alloc_extended_data().unwrap();
        assert!(f.as_ref().extended_plane(0).is_some());
        assert!(f.as_ref().extended_plane(9).is_none());
        for i in 0..10 {
            f.as_mut().set_extended_plane(i, 0, i).unwrap();
        }
        let (ext, _) = f.as_ref().extended_parts();
        f.as_mut()
            .set_buffer(0, Some(BufferReference::alloc_zeroed(4).unwrap()));
        // SAFETY: the frame's own extended array of 10 entries.
        let entries: Vec<_> = (0..10).map(|i| unsafe { ext.add(i).read() }).collect();
        assert!(entries.iter().all(|p| p.is_null()));
        f.as_mut().set_extended_plane(9, 0, 3).unwrap();
        f.as_mut().reset_extended_data();
        assert!(!f.as_ref().has_extended_data());
        assert_eq!(f.as_ref().extended_plane_count(), NUM_DATA_POINTERS);
        f.as_mut().reset_extended_data();
        f.as_mut().alloc_extended_data().unwrap();
        drop(f);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_extended_arrays_need_a_channel() {
        let mut f = Frame::new().unwrap();
        assert_eq!(f.as_mut().alloc_extended_data(), Err(EINVAL));
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(-3))
            .unwrap();
        assert_eq!(f.as_mut().alloc_extended_data(), Err(EINVAL));
        assert!(!f.as_ref().has_extended_data());
        assert_eq!(f.as_ref().extended_plane_count(), NUM_DATA_POINTERS);
        // A separate array may outlive every channel: C then refuses to
        // copy it, and Rust reads no entry.
        let mut f = frame_with_buffer(8);
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(2))
            .unwrap();
        f.as_mut().alloc_extended_data().unwrap();
        f.as_mut()
            .set_channel_layout(ChannelLayout::unspecified(0))
            .unwrap();
        assert!(f.as_ref().has_extended_data());
        assert_eq!(f.as_ref().extended_plane_count(), 0);
        assert!(f.as_ref().extended_plane(0).is_none());
        assert!(f.as_ref().try_clone().is_none());
    }

    /// The extra buffer block grows, shrinks and goes with its last entry,
    /// keeping `extended_buf` NULL exactly when the count is 0.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_extended_buffer_block_tracks_its_count() {
        let mut f = Frame::new().unwrap();
        let parts = |f: &FrameBox| f.as_ref().extended_buffer_parts();
        assert!(parts(&f).0.is_null());
        for n in 1..=3 {
            f.as_mut()
                .push_extended_buffer(BufferReference::alloc_zeroed(n).unwrap())
                .unwrap();
            assert!(!parts(&f).0.is_null());
            assert_eq!(parts(&f).1, n);
        }
        let back = f
            .as_mut()
            .replace_extended_buffer(3, BufferReference::alloc_zeroed(9).unwrap())
            .unwrap_err();
        assert_eq!(back.as_ref().len(), 9);
        let old = f.as_mut().replace_extended_buffer(1, back).unwrap();
        assert_eq!(old.as_ref().len(), 2);
        assert_eq!(f.as_ref().extended_buffer(1).unwrap().len(), 9);
        for n in (0..3).rev() {
            assert!(f.as_mut().pop_extended_buffer().is_some());
            assert_eq!(parts(&f).1, n);
        }
        assert!(parts(&f).0.is_null());
        assert!(f.as_mut().pop_extended_buffer().is_none());
    }

    /// `try_clone` counts every reference `av_frame_clone` would add, and
    /// refuses before C is called when one buffer is at the limit.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_clones_stop_at_the_reference_limit() {
        let mut f = frame_with_buffer(8);
        f.as_mut().set_plane(0, 0, 0).unwrap();
        f.as_mut()
            .set_opaque_ref(Some(BufferReference::alloc_zeroed(1).unwrap()));
        f.as_mut()
            .side_data_mut()
            .add(
                SideDataType::AFD,
                BufferReference::alloc_zeroed(1).unwrap(),
                SideDataFlags::NONE,
            )
            .unwrap();
        f.as_mut()
            .push_extended_buffer(BufferReference::alloc_zeroed(2).unwrap())
            .unwrap();
        let limit = c_uint::MAX / 2;
        let holders: [fn(FrameRef<'_>) -> BufferReferenceRef<'_>; 4] = [
            |r| r.buffer(0).unwrap(),
            |r| r.opaque_ref().unwrap(),
            |r| r.side_data().get(0).unwrap().buffer(),
            |r| r.extended_buffer(0).unwrap(),
        ];
        for holder in holders {
            let old = frame_set_count(holder(f.as_ref()), limit);
            assert!(f.as_ref().try_clone().is_none());
            frame_set_count(holder(f.as_ref()), limit - 1);
            let copy = f.as_ref().try_clone().expect("one below the limit");
            assert_eq!(holder(copy.as_ref()).ref_count(), limit);
            drop(copy);
            assert_eq!(frame_set_count(holder(f.as_ref()), old), limit - 1);
        }
    }

    /// A private RefStruct reference, which only C code sets, is shared by a
    /// clone and released by `reset` and by the frame's drop.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_private_refs_are_shared_and_released() {
        // SAFETY: a zeroed 16-byte RefStruct object, or NULL.
        let obj = unsafe { ffi::av_refstruct_allocz(16) };
        assert!(!obj.is_null());
        let mut f = frame_with_buffer(4);
        assert!(!f.as_ref().has_private_ref());
        // SAFETY: the frame takes over the caller's one reference, as a
        // library's private_ref; nothing else reaches the field.
        unsafe { addr_of_mut!((*f.as_mut().as_mut_ptr()).private_ref).write(obj) };
        assert!(f.as_ref().has_private_ref());
        // SAFETY: `obj` is live while the frame holds it.
        let exclusive = || unsafe { ffi::av_refstruct_exclusive(obj) != 0 };
        assert!(exclusive());
        let copy = f.as_ref().try_clone().unwrap();
        assert!(copy.as_ref().has_private_ref() && !exclusive());
        drop(copy);
        assert!(exclusive());
        let mut copy = f.as_ref().try_clone().unwrap();
        copy.as_mut().reset();
        assert!(!copy.as_ref().has_private_ref() && exclusive());
        drop(f);
    }

    /// `reset` frees a separate `extended_data` and every extra buffer, and
    /// writes the defaults back over everything (ASan and LSan check the
    /// release).
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_reset_releases_owned_arrays() {
        let mut f = frame_with_buffer(32);
        f.as_mut()
            .set_channel_layout(ChannelLayout::custom(12).unwrap())
            .unwrap();
        f.as_mut().alloc_extended_data().unwrap();
        f.as_mut()
            .push_extended_buffer(BufferReference::alloc_zeroed(3).unwrap())
            .unwrap();
        f.as_mut().set_extended_plane(11, 8, 1).unwrap();
        f.as_mut()
            .set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        f.as_mut().set_width(7);
        f.as_mut().reset();
        let r = f.as_ref();
        assert!(!r.has_extended_data() && r.extended_buffer_count() == 0);
        assert!(r.buffer(0).is_none() && r.metadata().is_none());
        assert_eq!((r.width(), r.format()), (0, -1));
        assert_eq!(r.channel_layout().channel_count(), 0);
        assert!(r.extended_buffer_parts().0.is_null());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_debug_names_the_shape() {
        let mut f = frame_with_buffer(4);
        f.as_mut().set_pts(5);
        let s = format!("{:?}", f.as_ref());
        assert!(s.starts_with("Frame {"), "{s}");
        assert!(s.contains("pts: 5") && s.contains("buffers: 1"), "{s}");
        assert_eq!(format!("{:?}", f.as_mut()), s);
    }

    /// A frame and two buffer references laid out in Rust memory, with no C
    /// routine behind them: the plane bookkeeping (`set_plane`, `plane`,
    /// `extended_plane`, `locate`, `prune_planes`) and `swap_planes` run
    /// under Miri. The references are never released through C.
    #[test]
    fn frame_plane_bookkeeping_over_rust_memory() {
        let mut bytes = [vec![1u8; 16], vec![2u8; 8]];
        let bufs: Vec<*mut ffi::AVBufferRef> = bytes
            .iter_mut()
            .map(|b| {
                Box::into_raw(Box::new(ffi::AVBufferRef {
                    buffer: ptr::null_mut(),
                    data: b.as_mut_ptr(),
                    size: b.len(),
                }))
            })
            .collect();
        let frames = [(); 2].map(|()| Box::into_raw(Box::new(ffi::AVFrame::default())));
        for &f in &frames {
            // SAFETY: a zeroed Rust-held frame; `extended_data` at its own
            // `data`, as C's defaults have it.
            unsafe { (*f).extended_data = addr_of_mut!((*f).data).cast() };
        }
        let [a, b] = frames;
        // SAFETY: the Rust-held frame `a`, reached only through this handle
        // until the end of the test, holding references to live Rust bytes.
        let mut m = unsafe { FrameMut::from_ptr(a) }.unwrap();
        assert_eq!(m.set_plane(0, 0, 0), Err(PlaneError::NoBuffer));
        // SAFETY: as above; the frame takes the two references.
        unsafe { ((*a).buf[0], (*a).buf[1]) = (bufs[0], bufs[1]) };
        m.set_plane(0, 0, 3).unwrap();
        m.set_plane(1, 1, 7).unwrap();
        assert_eq!(m.set_plane(2, 1, 8), Err(PlaneError::OutOfBounds));
        assert_eq!(m.as_ref().plane(0).unwrap(), [1; 13]);
        assert_eq!(m.as_ref().extended_plane(1).unwrap(), [2]);
        assert_eq!(m.as_ref().locate(m.as_ref().data_ptrs()[1]).unwrap().1, 7);
        // Dropping buf[1] from the frame behind its back, then pruning,
        // clears plane 1 only; dropping buf[0] clears plane 0 too.
        // SAFETY: as above.
        unsafe { (*a).buf[1] = ptr::null_mut() };
        m.prune_planes();
        assert!(m.as_ref().plane(1).is_none() && m.as_ref().plane(0).is_some());
        // SAFETY: as above.
        unsafe { (*a).buf[1] = bufs[1] };
        m.set_plane(1, 1, 0).unwrap();
        // SAFETY: as above.
        unsafe { (*a).buf[0] = ptr::null_mut() };
        m.prune_planes();
        assert!(m.as_ref().data_ptrs().iter().all(|p| p.is_null()));
        // SAFETY: as above; `a` holds buf[1] at index 0 now.
        unsafe { (*a).buf[0] = bufs[1] };
        m.set_plane(2, 0, 5).unwrap();
        // The planes and buffers move to `b`, whose `extended_data` stays
        // its own.
        // SAFETY: two distinct Rust-held frames nothing else reaches.
        unsafe { Frame::swap_planes(a, b) };
        // SAFETY: as above.
        unsafe {
            assert!((*a).buf[0].is_null() && (*a).data[2].is_null());
            assert_eq!((*b).buf[0], bufs[1]);
            assert_eq!((*b).extended_data, addr_of_mut!((*b).data).cast());
            assert_eq!((*a).extended_data, addr_of_mut!((*a).data).cast());
        }
        // SAFETY: the Rust-held frame `b`, as `a` above.
        let r = unsafe { FrameRef::from_ptr(b) }.unwrap();
        assert_eq!(r.plane(2).unwrap(), [2; 3]);
        for p in frames {
            // SAFETY: the frames boxed above, which hold no C resource.
            drop(unsafe { Box::from_raw(p) });
        }
        for p in bufs {
            // SAFETY: the references boxed above, released here, not by C.
            drop(unsafe { Box::from_raw(p) });
        }
    }

    /// Runs the calling test alone in a child process, so that it may change
    /// process-wide state (`av_max_alloc`); true in the child.
    fn frame_isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_FRAME_ISOLATED";
        if std::env::var_os(KEY).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(KEY, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        false
    }

    /// Under allocation limits, `Frame::new`, `try_clone` (C's failure paths
    /// in `av_frame_ref`: the extra block, the `extended_data` copy, the
    /// data copy's buffers), `alloc_extended_data` and
    /// `push_extended_buffer` fail cleanly: nothing leaks (LSan), and the
    /// frames keep what they held.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn frame_allocation_failures_leave_frames_whole() {
        if !frame_isolated(
            "libavutil::frame::unit_tests::frame_allocation_failures_leave_frames_whole",
        ) {
            return;
        }
        let frame_size = size_of::<ffi::AVFrame>();
        let mut f = frame_with_buffer(16);
        let mut m = f.as_mut();
        m.set_plane(0, 0, 0).unwrap();
        m.set_channel_layout(ChannelLayout::unspecified(70))
            .unwrap();
        m.alloc_extended_data().unwrap();
        for _ in 0..62 {
            m.push_extended_buffer(BufferReference::alloc_zeroed(4).unwrap())
                .unwrap();
        }
        m.set_extended_plane(69, 69, 1).unwrap();
        m.set_metadata(Dict::from_entries([(c"k", c"v")]).unwrap());
        // 70 entries: the copy's array outgrows the smallest limits.
        let mut entries = m.side_data_mut();
        for _ in 0..70 {
            let buf = BufferReference::alloc_zeroed(1).unwrap();
            entries
                .add(SideDataType::SEI_UNREGISTERED, buf, SideDataFlags::NONE)
                .unwrap();
        }
        let mut g = Frame::new().unwrap();
        g.as_mut()
            .set_format(ffi::AVPixelFormat::AV_PIX_FMT_GRAY8.0);
        g.as_mut().set_width(64);
        g.as_mut().set_height(64);
        let set_limit = |n: usize| {
            // SAFETY: no precondition; this process runs this test alone.
            unsafe { ffi::av_max_alloc(n) }
        };
        set_limit(frame_size - 1);
        assert!(Frame::new().is_none());
        assert!(f.as_ref().try_clone().is_none());
        for limit in [frame_size, 500, 600, 2048, 1 << 16] {
            set_limit(limit);
            if let Some(copy) = f.as_ref().try_clone() {
                assert_eq!(copy.as_ref().extended_plane(69).unwrap().len(), 3);
                assert_eq!(f.as_ref().buffer(0).unwrap().ref_count(), 2);
            }
            assert_eq!(f.as_ref().buffer(0).unwrap().ref_count(), 1);
            assert_eq!(f.as_ref().extended_plane(69).unwrap().len(), 3);
            assert!(g.as_ref().try_clone().is_none());
            let mut h = Frame::new().unwrap();
            h.as_mut()
                .set_channel_layout(ChannelLayout::unspecified(200))
                .unwrap();
            let allocated = h.as_mut().alloc_extended_data();
            assert_eq!(allocated.is_ok(), 200 * size_of::<*mut u8>() <= limit);
            for n in 0..80 {
                let pushed = h
                    .as_mut()
                    .push_extended_buffer(BufferReference::alloc_zeroed(1).unwrap());
                if pushed.is_err() {
                    assert!((n + 1) * size_of::<*mut u8>() > limit);
                    break;
                }
            }
        }
        set_limit(c_int::MAX as usize);
        assert!(f.as_ref().try_clone().is_some());
    }
}
