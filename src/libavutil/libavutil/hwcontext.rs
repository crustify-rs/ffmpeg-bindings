//! Safe wrappers over `libavutil/hwcontext.h`.
//!
//! [`DeviceType`] is `AVHWDeviceType`, the hardware API a device context
//! drives, and [`TransferDirection`] is `AVHWFrameTransferDirection`, which
//! side of a transfer between hardware and software frames a format query
//! asks about. Both are plain `Copy` values.

use core::fmt;

use ffibox::CPlainElem;

use super::ffi;

/// Wraps: AVHWDeviceType
/// Handwritten
///
/// The hardware API behind a device context: VAAPI, CUDA, Vulkan and so on,
/// or [`NONE`](Self::NONE) for no device.
///
/// A newtype over the C enum rather than a Rust `enum`, because the values C
/// hands out are an open set: FFmpeg appends enumerators in minor releases,
/// so a newer `libavutil` loaded at run time can return one this crate has
/// no constant for (`av_hwdevice_find_type_by_name`,
/// `av_hwdevice_iterate_types`, a device context's `type`), and a Rust view
/// of C memory holding one must accept it. C itself accepts every value: its
/// name lookup returns nothing for a type it does not name, and device
/// creation fails for a type the library was built without. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::hwcontext::DeviceType;
/// fn is_windows_api(t: DeviceType) -> bool {
///     matches!(
///         t,
///         DeviceType::DXVA2 | DeviceType::D3D11VA | DeviceType::D3D12VA
///     )
/// }
/// assert!(is_windows_api(DeviceType::D3D11VA));
/// assert!(!is_windows_api(DeviceType::VAAPI));
/// ```
///
/// C stores a device type in `AVHWDeviceContext.type`, which the library
/// sets when it allocates the context and documents as never changed
/// afterwards: the backends cast the context's `hwctx` by it, so it is a
/// value to read, not to write. libavcodec's `AVCodecHWConfig.device_type`
/// lives in read-only codec tables.
///
/// A constant names a type whatever backends the library was built with;
/// which of them it can create is a property of the build. C has no count
/// sentinel for this enum, so there is no `COUNT`. [`Default`] is
/// [`NONE`](Self::NONE), the zeroed value.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceType(ffi::AVHWDeviceType);

impl DeviceType {
    /// No hardware device; what C's lookups return when they find none.
    pub const NONE: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE);
    /// NVIDIA VDPAU (`vdpau`).
    pub const VDPAU: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VDPAU);
    /// NVIDIA CUDA (`cuda`).
    pub const CUDA: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA);
    /// VA-API (`vaapi`).
    pub const VAAPI: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI);
    /// DirectX Video Acceleration 2 (`dxva2`).
    pub const DXVA2: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DXVA2);
    /// Intel Quick Sync Video (`qsv`).
    pub const QSV: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_QSV);
    /// Apple VideoToolbox (`videotoolbox`).
    pub const VIDEOTOOLBOX: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX);
    /// Direct3D 11 video acceleration (`d3d11va`).
    pub const D3D11VA: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA);
    /// Linux DRM PRIME buffers (`drm`).
    pub const DRM: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM);
    /// OpenCL (`opencl`).
    pub const OPENCL: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_OPENCL);
    /// Android MediaCodec (`mediacodec`).
    pub const MEDIACODEC: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_MEDIACODEC);
    /// Vulkan (`vulkan`).
    pub const VULKAN: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN);
    /// Direct3D 12 video acceleration (`d3d12va`).
    pub const D3D12VA: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D12VA);
    /// AMD Advanced Media Framework (`amf`).
    pub const AMF: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_AMF);
    /// OpenHarmony codec device (`ohcodec`).
    pub const OHCODEC: Self = Self(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_OHCODEC);

    /// The device type C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVHWDeviceType) -> Self {
        Self(raw)
    }

    /// The C encoding of this device type.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVHWDeviceType {
        self.0
    }

    /// The name of the constant this value equals, if any.
    const fn constant_name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::VDPAU => "VDPAU",
            Self::CUDA => "CUDA",
            Self::VAAPI => "VAAPI",
            Self::DXVA2 => "DXVA2",
            Self::QSV => "QSV",
            Self::VIDEOTOOLBOX => "VIDEOTOOLBOX",
            Self::D3D11VA => "D3D11VA",
            Self::DRM => "DRM",
            Self::OPENCL => "OPENCL",
            Self::MEDIACODEC => "MEDIACODEC",
            Self::VULKAN => "VULKAN",
            Self::D3D12VA => "D3D12VA",
            Self::AMF => "AMF",
            Self::OHCODEC => "OHCODEC",
            _ => return None,
        })
    }
}

impl Default for DeviceType {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.constant_name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("DeviceType").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVHWDeviceType> for DeviceType {
    #[inline]
    fn from(raw: ffi::AVHWDeviceType) -> Self {
        Self::from_raw(raw)
    }
}

impl From<DeviceType> for ffi::AVHWDeviceType {
    #[inline]
    fn from(t: DeviceType) -> Self {
        t.into_raw()
    }
}

// SAFETY: `DeviceType` is `#[repr(transparent)]` over `ffi::AVHWDeviceType`,
// itself `#[repr(transparent)]` over a `c_uint`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for DeviceType {}

/// Wraps: AVHWFrameTransferDirection
/// Handwritten
///
/// Which side of a transfer between a hardware frame and a software frame a
/// format query asks about: the formats data can be downloaded into
/// ([`From`](Self::From) the hardware frame) or uploaded from
/// ([`To`](Self::To) it).
///
/// A Rust `enum`: the direction only ever flows from the caller into C as an
/// argument (`av_hwframe_transfer_get_formats`, which hands it to the frames
/// context's backend), and C never stores or returns one, so the safe surface
/// names the two documented directions only. C has no default direction, so
/// there is no `Default`.
///
/// Not every bit pattern is a direction, so C memory never reads as one: the
/// plain-element arrays reject it.
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::{hwcontext::TransferDirection, mem::alloc_array_zeroed};
/// let _ = alloc_array_zeroed::<TransferDirection>(1);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransferDirection {
    /// Transfer the data from the queried hardware frame.
    From,
    /// Transfer the data to the queried hardware frame.
    To,
}

impl TransferDirection {
    /// The C encoding of this direction.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVHWFrameTransferDirection {
        match self {
            Self::From => ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_FROM,
            Self::To => ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_TO,
        }
    }
}

impl From<TransferDirection> for ffi::AVHWFrameTransferDirection {
    #[inline]
    fn from(dir: TransferDirection) -> Self {
        dir.into_raw()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_uint;
    use core::mem::{align_of, size_of};
    use std::collections::HashSet;

    use crate::libavutil::mem::{HeapVec, alloc_array_zeroed};

    const NAMED: [(DeviceType, &str); 15] = [
        (DeviceType::NONE, "NONE"),
        (DeviceType::VDPAU, "VDPAU"),
        (DeviceType::CUDA, "CUDA"),
        (DeviceType::VAAPI, "VAAPI"),
        (DeviceType::DXVA2, "DXVA2"),
        (DeviceType::QSV, "QSV"),
        (DeviceType::VIDEOTOOLBOX, "VIDEOTOOLBOX"),
        (DeviceType::D3D11VA, "D3D11VA"),
        (DeviceType::DRM, "DRM"),
        (DeviceType::OPENCL, "OPENCL"),
        (DeviceType::MEDIACODEC, "MEDIACODEC"),
        (DeviceType::VULKAN, "VULKAN"),
        (DeviceType::D3D12VA, "D3D12VA"),
        (DeviceType::AMF, "AMF"),
        (DeviceType::OHCODEC, "OHCODEC"),
    ];

    /// Values C names none of: just past the enumerators, and the `unsigned`
    /// extremes a negative `int` written into C storage reads back as.
    const UNNAMED: [c_uint; 6] = [
        15,
        16,
        1 << 20,
        i32::MAX as c_uint,
        i32::MIN as c_uint,
        c_uint::MAX,
    ];

    #[test]
    fn device_type_layout_matches_the_c_enum() {
        assert_eq!(size_of::<DeviceType>(), size_of::<ffi::AVHWDeviceType>());
        assert_eq!(align_of::<DeviceType>(), align_of::<ffi::AVHWDeviceType>());
        assert_eq!(size_of::<DeviceType>(), size_of::<c_uint>());
    }

    #[test]
    fn device_type_constants_are_the_enumerators_in_declaration_order() {
        for (i, (t, _)) in NAMED.iter().enumerate() {
            assert_eq!(t.into_raw().0, i as c_uint, "{t:?}");
        }
        let distinct: HashSet<DeviceType> = NAMED.iter().map(|&(t, _)| t).collect();
        assert_eq!(distinct.len(), NAMED.len());
    }

    #[test]
    fn device_type_debug_names_the_constants() {
        for (t, name) in NAMED {
            assert_eq!(format!("{t:?}"), name);
            assert_eq!(t.constant_name(), Some(name));
        }
    }

    #[test]
    fn device_type_debug_shows_unnamed_values_raw() {
        for raw in UNNAMED {
            let t = DeviceType::from_raw(ffi::AVHWDeviceType(raw));
            assert_eq!(t.constant_name(), None);
            assert_eq!(format!("{t:?}"), format!("DeviceType({raw})"));
        }
    }

    #[test]
    fn device_type_default_is_none_and_zero() {
        assert_eq!(DeviceType::default(), DeviceType::NONE);
        assert_eq!(DeviceType::default().into_raw().0, 0);
    }

    #[test]
    fn device_type_raw_conversions_round_trip_every_value() {
        let all = NAMED.iter().map(|&(t, _)| t.into_raw().0).chain(UNNAMED);
        for raw in all {
            let c = ffi::AVHWDeviceType(raw);
            assert_eq!(DeviceType::from_raw(c).into_raw(), c);
            assert_eq!(ffi::AVHWDeviceType::from(DeviceType::from(c)), c);
        }
    }

    #[test]
    fn device_type_arrays_are_plain_elements() {
        let zeroed = alloc_array_zeroed::<DeviceType>(4).unwrap();
        assert!(zeroed.as_slice().iter().all(|&t| t == DeviceType::NONE));
        let values: Vec<DeviceType> = UNNAMED
            .iter()
            .map(|&raw| DeviceType::from_raw(ffi::AVHWDeviceType(raw)))
            .chain(NAMED.iter().map(|&(t, _)| t))
            .collect();
        let a = HeapVec::from_slice(&values).unwrap();
        assert_eq!(a.as_slice(), &values[..]);
    }

    #[test]
    fn transfer_direction_encodes_as_the_c_enumerators() {
        assert_eq!(
            TransferDirection::From.into_raw(),
            ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_FROM
        );
        assert_eq!(
            TransferDirection::To.into_raw(),
            ffi::AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_TO
        );
        assert_eq!(TransferDirection::From.into_raw().0, 0);
        assert_eq!(TransferDirection::To.into_raw().0, 1);
        for dir in [TransferDirection::From, TransferDirection::To] {
            assert_eq!(ffi::AVHWFrameTransferDirection::from(dir), dir.into_raw());
        }
    }

    #[test]
    fn transfer_direction_debug_and_equality() {
        assert_eq!(format!("{:?}", TransferDirection::From), "From");
        assert_eq!(format!("{:?}", TransferDirection::To), "To");
        assert_ne!(TransferDirection::From, TransferDirection::To);
        let set: HashSet<TransferDirection> = [
            TransferDirection::From,
            TransferDirection::To,
            TransferDirection::From,
        ]
        .into_iter()
        .collect();
        assert_eq!(set.len(), 2);
    }
}
