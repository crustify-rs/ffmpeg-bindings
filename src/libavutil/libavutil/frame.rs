//! Safe wrappers over `libavutil/frame.h`.

use core::fmt;

use ffibox::CPlainElem;

use super::ffi;

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

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_uint;
    use core::mem::{align_of, size_of};

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
}
