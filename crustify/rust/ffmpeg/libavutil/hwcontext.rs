//! Safe wrappers over `libavutil/hwcontext.h`.
//!
//! [`DeviceType`] is `AVHWDeviceType`, the hardware API a device context
//! drives, and [`TransferDirection`] is `AVHWFrameTransferDirection`, which
//! side of a transfer between hardware and software frames a format query
//! asks about. Both are plain `Copy` values.
//!
//! Frames move between hardware and software through
//! [`FrameMut::transfer_data_from`] (`av_hwframe_transfer_data`), in the
//! software formats a hardware frame's context lists for each direction
//! ([`FrameRef::transfer_formats`], or [`transfer_formats`] for a context
//! reference; `av_hwframe_transfer_get_formats`). Until this module wraps
//! hardware frames contexts and their frames, the transfer and the listing
//! over a bare context reference are `unsafe`: their callers vouch for the
//! hardware side, which Rust checks the software side against.
//!
//! A device context is reached through a [`DeviceReferenceBox`], made by
//! [`DeviceReference::create`], by initialising an [`UninitDevice`] from
//! [`DeviceReference::alloc`], or by deriving it from another device
//! ([`DeviceReferenceRef::derive`]). Frames contexts on a device are
//! allocated as [`UninitFrames`] ([`DeviceReferenceRef::alloc_frames`]),
//! initialised into [`FramesReferenceBox`]es, or derived from another
//! frames context ([`FramesReferenceRef::derive`]); they allocate
//! [`HardwareFrame`]s, which map to normal memory as [`MappedFrame`]s or
//! into other frames contexts. [`DeviceType`]'s name table and the backends
//! the library was built with are [`DeviceType::name`],
//! [`DeviceType::from_name`] and [`DeviceType::supported`].

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ops::{BitAnd, BitOr, BitOrAssign, Sub};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{CBox, CDrop, CDupClone, CPlainElem, define_ctype};

use super::buffer::{BufferReferenceRef, ENOMEM, MAX_REFERENCES, may_add_reference};
use super::dict::Dict;
use super::ffi;
use super::frame::{
    Frame, FrameBox, FrameMut, FrameRef, PALETTE_SIZE, Rows, image_reach, int_flags,
};
use super::mem::HeapVec;
use super::pixdesc::PixelFormatFlags;
use super::pixfmt::PixelFormat;

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

/// `AVERROR(EINVAL)`.
const EINVAL: c_int = -(ffi::EINVAL as c_int);

/// A reference to an initialised hardware frames context: one
/// `av_hwframe_ctx_alloc` made and `av_hwframe_ctx_init` set up, whose
/// bytes are its `AVHWFramesContext` and which nothing writes while the
/// reference is borrowed.
// A private stand-in for the typed frames context handle that wrapping
// hwcontext.h's AVHWFramesContext brings; built only from a frame's
// `hw_frames_ctx` (which `Frame`'s invariant makes such a reference) and
// from callers of the unsafe entry points who vouch for one.
#[derive(Clone, Copy)]
struct HwFrames<'a>(BufferReferenceRef<'a>);

/// The public fields of a hardware frames context the transfer checks read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FramesGeometry {
    /// `sw_format`: the layout of the surfaces' data.
    sw_format: PixelFormat,
    /// `width`: the surfaces' allocated width.
    width: c_int,
    /// `height`: the surfaces' allocated height.
    height: c_int,
}

impl<'a> HwFrames<'a> {
    /// The hardware frames context `frame` holds; `None` for a software
    /// frame.
    fn of(frame: FrameRef<'a>) -> Option<Self> {
        // `Frame`'s invariant: a non-NULL `hw_frames_ctx` references an
        // initialised hardware frames context, whose bytes nothing writes
        // while the frame holds it.
        frame.hw_frames_context().map(Self)
    }

    /// The context's allocated surface size and software format.
    fn geometry(self) -> FramesGeometry {
        let ctx = self.0.parts().0.as_ptr().cast::<ffi::AVHWFramesContext>();
        // SAFETY: by the type's invariant the reference's data is a live,
        // initialised `AVHWFramesContext` (C allocates it as the leading
        // member of its private context, aligned by `av_mallocz`); three
        // plain fields are read through raw places, without a reference.
        unsafe {
            FramesGeometry {
                sw_format: PixelFormat::from_raw(addr_of!((*ctx).sw_format).read()),
                width: addr_of!((*ctx).width).read(),
                height: addr_of!((*ctx).height).read(),
            }
        }
    }

    /// `av_hwframe_transfer_get_formats` on the context.
    fn transfer_formats(self, dir: TransferDirection) -> Result<HeapVec<PixelFormat>, c_int> {
        let mut list: *mut ffi::AVPixelFormat = ptr::null_mut();
        // C hands the context to its backend's `transfer_get_formats` (or
        // refuses with `AVERROR(ENOSYS)` when the backend has none), which
        // only reads it and, on success, stores in `list` a fresh
        // `av_malloc`-family array ending in `AV_PIX_FMT_NONE`.
        // SAFETY: by the type's invariant the reference is live and its data
        // an initialised frames context; C writes nothing through the
        // reference, despite its non-const pointer; `flags` is the documented
        // 0 and `list` a writable slot.
        let ret = unsafe {
            ffi::av_hwframe_transfer_get_formats(
                self.0.as_ptr().cast_mut(),
                dir.into_raw(),
                &mut list,
                0,
            )
        };
        if ret < 0 {
            // C leaves `list` unspecified on failure: nothing to free.
            return Err(ret);
        }
        let mut len = 0;
        if !list.is_null() {
            // SAFETY: a successful call left a terminated array at `list`,
            // read up to and including its terminator.
            while unsafe { list.add(len).read() } != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
                len += 1;
            }
        }
        // SAFETY: `list` is NULL or the fresh `av_malloc`-family block of
        // `len` formats and their terminator C handed over; `PixelFormat` is
        // `#[repr(transparent)]` over `AVPixelFormat`, and `av_free`, which
        // `Heap` releases with, takes the whole block whatever its length.
        let list = unsafe { HeapVec::from_raw_parts_or_empty(list.cast::<PixelFormat>(), len) };
        Ok(list.unwrap_or_else(HeapVec::empty))
    }
}

/// Wraps: av_hwframe_transfer_get_formats
///
/// The software pixel formats a transfer with the hardware frames of the
/// context `frames_context` references can use: those data can be
/// downloaded into ([`TransferDirection::From`]) or uploaded from
/// ([`TransferDirection::To`]), in the backend's order of preference. C
/// promises at least one format on success. The list is the caller's.
///
/// [`FrameRef::transfer_formats`] is the safe form for the context of a
/// hardware frame.
///
/// # Errors
///
/// The backend's error; `AVERROR(ENOSYS)` for a backend that transfers no
/// data (MediaCodec, OpenHarmony).
///
/// # Safety
///
/// `frames_context` must reference an initialised hardware frames context
/// (one `av_hwframe_ctx_alloc` made and `av_hwframe_ctx_init` set up, or a
/// reference to it), whose bytes C reads as its `AVHWFramesContext` and
/// whose private part its backend reads; no other reference may write them
/// meanwhile.
// A method of the typed frames context replaces this seam once hwcontext.h's
// AVHWFramesContext is wrapped.
pub unsafe fn transfer_formats(
    frames_context: BufferReferenceRef<'_>,
    dir: TransferDirection,
) -> Result<HeapVec<PixelFormat>, c_int> {
    // The caller vouches for the context.
    HwFrames(frames_context).transfer_formats(dir)
}

impl FrameRef<'_> {
    /// Wraps: av_hwframe_transfer_get_formats
    ///
    /// The software pixel formats a transfer with this hardware frame can
    /// use, from its hardware frames context
    /// ([`hw_frames_context`](Self::hw_frames_context)): those its data can
    /// be downloaded into ([`TransferDirection::From`]) or uploaded from
    /// ([`TransferDirection::To`]), in the backend's order of preference. C
    /// promises at least one format on success. The list is the caller's.
    ///
    /// ```
    /// use ffmpeg::libavutil::frame::Frame;
    /// use ffmpeg::libavutil::hwcontext::TransferDirection;
    /// let software = Frame::new().unwrap();
    /// assert!(software.as_ref().transfer_formats(TransferDirection::From).is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a software frame, which has no context to ask;
    /// otherwise the backend's error, `AVERROR(ENOSYS)` for a backend that
    /// transfers no data (MediaCodec, OpenHarmony).
    pub fn transfer_formats(&self, dir: TransferDirection) -> Result<HeapVec<PixelFormat>, c_int> {
        HwFrames::of(*self).ok_or(EINVAL)?.transfer_formats(dir)
    }
}

/// Whether `frame`'s display size lies within the surfaces of `geometry`: C
/// requires it of both frames of a transfer, and backends size their
/// copies by either frame's.
fn fits_surfaces(frame: FrameRef<'_>, geometry: FramesGeometry) -> bool {
    (0..=geometry.width).contains(&frame.width()) && (0..=geometry.height).contains(&frame.height())
}

/// Whether `frame`'s planes hold a whole `width` x `height` image of
/// `format`, as a backend's transfer may reach it: every plane the format
/// uses (and a palettized format's palette) present, each with a positive
/// line size no narrower than its row, and every row of each plane lying,
/// in full line sizes, inside a buffer reference of the frame, writable when
/// `write` ([`FrameRef::reaches`]; the planes of a frame that is not
/// reference counted were vouched for by their setter). Backends differ in
/// how much of a row they copy (the image's width, or the narrower of the
/// two line sizes), so a full line size is the most any of them reaches.
fn holds_image(
    frame: FrameRef<'_>,
    format: PixelFormat,
    width: c_int,
    height: c_int,
    write: bool,
) -> bool {
    let known = format
        .descriptor()
        .is_some_and(|d| !d.flags().contains(PixelFormatFlags::HWACCEL));
    let Some(reach) = image_reach(format, width, height).filter(|_| known) else {
        return false;
    };
    let (data, linesizes) = (frame.data_ptrs(), frame.linesizes());
    let planes = reach.planes.iter().enumerate();
    let rows_fit = planes
        .filter_map(|(i, r)| r.map(|r| (i, r)))
        .all(|(i, (len, rows))| {
            let Ok(linesize) = usize::try_from(linesizes[i]) else {
                return false;
            };
            let rows = Rows {
                len: linesize,
                rows,
                linesize: linesize as isize,
            };
            !data[i].is_null() && linesize >= len.max(1) && frame.reaches(data[i], rows, write)
        });
    rows_fit
        && (!reach.palette
            || !data[1].is_null() && frame.reaches(data[1], Rows::run(PALETTE_SIZE), write))
}

/// Why a backend must not transfer between `hw`, a hardware frame of `ctx`,
/// and the software frame `sw` in `dir` (writing `sw` for
/// [`TransferDirection::From`]): either frame's display size outside the
/// context's surfaces, a format of `sw` the backend does not list for the
/// direction (`AVERROR(EINVAL)`; the listing's own error, `AVERROR(ENOSYS)`
/// from a backend that transfers nothing), or planes of `sw` that do not
/// hold an image of the surfaces' size ([`holds_image`]).
fn software_transfer_refusal(
    ctx: HwFrames<'_>,
    hw: FrameRef<'_>,
    sw: FrameRef<'_>,
    dir: TransferDirection,
) -> Option<c_int> {
    let geometry = ctx.geometry();
    if !fits_surfaces(hw, geometry) || !fits_surfaces(sw, geometry) {
        return Some(EINVAL);
    }
    let format = sw.pixel_format();
    match ctx.transfer_formats(dir) {
        Err(ret) => return Some(ret),
        Ok(listed) if !listed.as_slice().contains(&format) => return Some(EINVAL),
        Ok(_) => {}
    }
    let write = dir == TransferDirection::From;
    (!holds_image(sw, format, geometry.width, geometry.height, write)).then_some(EINVAL)
}

/// The frame C's `transfer_data_alloc` downloads into for `dst`, a frame
/// without buffers, from a hardware frame of `ctx`: a new frame given zeroed
/// buffers of the context's surface size, in `dst`'s format if it has one,
/// else in the first the context lists for downloads (`AVERROR(EINVAL)` when
/// it lists none, which C promises it never does).
fn download_target(dst: FrameRef<'_>, ctx: HwFrames<'_>) -> Result<FrameBox, c_int> {
    let geometry = ctx.geometry();
    let format = if dst.format() >= 0 {
        dst.pixel_format()
    } else {
        let listed = ctx.transfer_formats(TransferDirection::From)?;
        *listed.as_slice().first().ok_or(EINVAL)?
    };
    let mut tmp = Frame::new().ok_or(ENOMEM)?;
    let mut t = tmp.as_mut();
    t.set_pixel_format(format);
    t.set_width(geometry.width);
    t.set_height(geometry.height);
    t.alloc_buffers(0)?;
    Ok(tmp)
}

impl FrameMut<'_> {
    /// Wraps: av_hwframe_transfer_data
    ///
    /// Copies `src`'s data into this frame where at least one of them is a
    /// hardware frame ([`FrameRef::hw_frames_context`]): downloads a
    /// hardware `src` into a software frame, uploads a software `src` into
    /// a hardware frame, or copies between two hardware frames, as their
    /// backends can. Only the planes' data moves; no property is copied.
    ///
    /// A software frame on either side must use one of the formats its
    /// counterpart's context lists for the direction
    /// ([`FrameRef::transfer_formats`]) and hold an image of the context's
    /// allocated size: C requires both frames to have the allocated size of
    /// the surfaces, while their display sizes (width and height) may be
    /// smaller. A frame [`alloc_buffers`](Self::alloc_buffers) gave buffers
    /// at that size does.
    ///
    /// A frame without buffers (no `buf[0]`, as [`Frame::new`] makes one)
    /// receives new ones instead, which is how a download usually goes: it
    /// is reset and given zeroed buffers of the context's allocated size in
    /// its format if it has one, else in the first format the context lists
    /// for downloads, and then holds `src`'s data at `src`'s width and
    /// height. C's own routine leaves the bytes around the copied lines
    /// uninitialised and leaks whatever such a frame held (side data,
    /// metadata, references); this runs its steps without either.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a frame without buffers and a software `src`;
    /// `AVERROR(ENOSYS)` for two software frames, or when a context's
    /// backend transfers no data (MediaCodec, OpenHarmony) or a hardware to
    /// hardware copy is refused (a derived context on either side, or
    /// neither backend can copy between them). Refused here before C
    /// reaches any plane: `AVERROR(EINVAL)` for a frame whose width or
    /// height is negative or exceeds its counterpart context's surfaces, a
    /// software format the context does not list, or a software frame
    /// whose planes do not hold the whole image or, for a download, are
    /// shared or read-only ([`FrameRef::is_writable`]); the error of the
    /// context's format listing. Otherwise the backend's error, or
    /// [`alloc_buffers`](Self::alloc_buffers)' for new buffers. A frame
    /// receiving new buffers is unchanged on error.
    ///
    /// # Safety
    ///
    /// Every frame that holds a hardware frames context (this one, `src`,
    /// or both) must be a hardware frame of that context: its buffers and
    /// plane pointers are surfaces the context's backend allocated or
    /// mapped for it (`av_hwframe_get_buffer`, `av_hwframe_map`, a hardware
    /// decoder) and left as it made them, which the backend reads and, for
    /// a destination, writes through them. Its width and height may have
    /// changed, which is checked. This crate builds no hardware frame yet,
    /// and `Frame` cannot tell one from a software frame given a context.
    // A safe method over a typed hardware frame replaces this seam once
    // hwcontext.h's frames contexts and their frames are wrapped.
    pub unsafe fn transfer_data_from(&mut self, src: FrameRef<'_>) -> Result<(), c_int> {
        let dst = self.as_ref();
        let (dst_hw, src_hw) = (HwFrames::of(dst), HwFrames::of(src));
        let refusal = match (dst.is_counted(), dst_hw, src_hw) {
            (false, _, Some(ctx)) => {
                // C's `transfer_data_alloc`, on zeroed buffers, replacing the
                // frame only once the download succeeded.
                let mut tmp = download_target(dst, ctx)?;
                // SAFETY: `tmp` is a software frame with buffers, and the
                // caller vouches for `src`.
                unsafe { tmp.as_mut().transfer_data_from(src) }?;
                tmp.as_mut().set_width(src.width());
                tmp.as_mut().set_height(src.height());
                self.move_from(tmp.as_mut());
                return Ok(());
            }
            (true, Some(dst_ctx), Some(src_ctx)) => {
                // Every backend in FFmpeg has `transfer_get_formats`
                // exactly when it has `transfer_data_to` and
                // `transfer_data_from`, which C calls unchecked: the listing
                // tells a backend without them, which it refuses with
                // `AVERROR(ENOSYS)`.
                let listing = |ctx: HwFrames<'_>, dir| ctx.transfer_formats(dir).err();
                if !fits_surfaces(src, src_ctx.geometry())
                    || !fits_surfaces(dst, dst_ctx.geometry())
                {
                    Some(EINVAL)
                } else {
                    listing(src_ctx, TransferDirection::From)
                        .or_else(|| listing(dst_ctx, TransferDirection::To))
                }
            }
            (true, None, Some(ctx)) => {
                software_transfer_refusal(ctx, src, dst, TransferDirection::From)
            }
            (true, Some(ctx), None) => {
                software_transfer_refusal(ctx, dst, src, TransferDirection::To)
            }
            // C refuses, reading only `buf[0]` and `hw_frames_ctx`: a frame
            // without buffers and a software `src` with `AVERROR(EINVAL)`,
            // two software frames with `AVERROR(ENOSYS)`.
            (false, _, None) | (true, None, None) => None,
        };
        if let Some(ret) = refusal {
            return Err(ret);
        }
        // C hands a hardware frame's context its backend's transfer, which
        // reaches the hardware frames' surfaces (the caller's contract) and
        // the software frame's planes within the checked image: rows inside
        // its buffer references, written only where they are this frame's
        // alone.
        // SAFETY: the frame, reached through the exclusive handle, and `src`,
        // a distinct live frame read only; each frame's context is
        // initialised (`Frame`'s invariant), each hardware frame one of its
        // context's (the caller's contract), and every software plane C
        // reaches was checked above.
        let ret = unsafe { ffi::av_hwframe_transfer_data(self.as_mut_ptr(), src.as_ptr(), 0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

// Hwcontext wave 1, batch 1: the device type's name table and backend list,
// device and frames contexts, and hardware frames.

impl DeviceType {
    /// Wraps: av_hwdevice_get_type_name
    ///
    /// The name C gives this device type, as FFmpeg's options spell it
    /// (`cuda`, `vaapi`, ...); `None` for [`NONE`](Self::NONE) and any value
    /// C does not name.
    ///
    /// ```
    /// use ffmpeg::libavutil::hwcontext::DeviceType;
    /// assert_eq!(DeviceType::VULKAN.name(), Some(c"vulkan"));
    /// assert_eq!(DeviceType::NONE.name(), None);
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: `av_hwdevice_get_type_name` takes the enum by value,
        // bounds-checks it and returns NULL or an entry of its static
        // `hw_type_names` table.
        let name = unsafe { ffi::av_hwdevice_get_type_name(self.0) };
        // SAFETY: a non-NULL `name` is a string literal of that immutable
        // table, alive for the process.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }

    /// Wraps: av_hwdevice_find_type_by_name
    ///
    /// The device type C names `name` (see [`name`](Self::name)), or `None`
    /// for any other string. Names are matched exactly, case included:
    /// `hwcontext.h` calls the lookup case-insensitive, but C compares with
    /// `strcmp`.
    ///
    /// ```
    /// use ffmpeg::libavutil::hwcontext::DeviceType;
    /// assert_eq!(DeviceType::from_name(c"cuda"), Some(DeviceType::CUDA));
    /// assert_eq!(DeviceType::from_name(c"CUDA"), None);
    /// ```
    #[must_use]
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only
        // compares with its table's names during the call.
        let found = Self(unsafe { ffi::av_hwdevice_find_type_by_name(name.as_ptr()) });
        // No name maps to NONE, C's "not found".
        (found != Self::NONE).then_some(found)
    }

    /// Wraps: av_hwdevice_iterate_types
    ///
    /// The device types this build of the library has a backend for, so
    /// that [`DeviceReference::alloc`] can succeed, in increasing order of
    /// their values. Empty for a build without hardware libraries (FFmpeg
    /// configured with `--disable-autodetect` and no `--enable-<api>`).
    #[must_use]
    pub fn supported() -> SupportedDeviceTypes {
        SupportedDeviceTypes {
            prev: Some(Self::NONE),
        }
    }
}

/// The device types the library has a backend for, from
/// [`DeviceType::supported`].
#[derive(Clone, Debug)]
pub struct SupportedDeviceTypes {
    /// The last type yielded (`NONE` before the first), `None` once C
    /// reported none after it.
    prev: Option<DeviceType>,
}

impl Iterator for SupportedDeviceTypes {
    type Item = DeviceType;

    fn next(&mut self) -> Option<DeviceType> {
        let prev = self.prev?;
        // SAFETY: `av_hwdevice_iterate_types` takes the enum by value and
        // only compares it with the compiled-in backends' types.
        let next = DeviceType(unsafe { ffi::av_hwdevice_iterate_types(prev.0) });
        // C returns the smallest backend type above `prev`, so the walk
        // ends, with NONE.
        self.prev = (next != DeviceType::NONE).then_some(next);
        self.prev
    }
}

impl FusedIterator for SupportedDeviceTypes {}

int_flags! {
    /// The `AV_HWFRAME_MAP_*` flags of a hardware frame mapping
    /// ([`HardwareFrame::map`], [`HardwareFrame::map_to`]) and of the
    /// mappings a derived frames context makes
    /// ([`FramesReferenceRef::derive`]). Backends refuse a mapping they
    /// cannot make as asked, with `AVERROR(ENOSYS)`.
    MapFlags {
        /// The mapping must be readable.
        READ = ffi::AV_HWFRAME_MAP_READ.0,
        /// The mapping must be writable.
        WRITE = ffi::AV_HWFRAME_MAP_WRITE.0,
        /// The mapped frame will be overwritten whole, so its current data
        /// need not be loaded; what is not overwritten is unspecified.
        OVERWRITE = ffi::AV_HWFRAME_MAP_OVERWRITE.0,
        /// The mapping must be direct: no copy in the map or unmap steps.
        DIRECT = ffi::AV_HWFRAME_MAP_DIRECT.0,
    }
}

/// A hwcontext.h status as a `Result`.
fn status(ret: c_int) -> Result<(), c_int> {
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Whether a buffer counting `count` references (as C's
/// `av_buffer_get_ref_count` reports it, converted from `unsigned` to `int`)
/// may take another: its count is below [`MAX_REFERENCES`], as any wrapper
/// adding a reference checks (see [`may_add_reference`]).
fn count_allows_another(count: c_int) -> bool {
    may_add_reference(count as c_uint)
}

/// Whether a buffer counting `count` references is within
/// [`MAX_REFERENCES`]: C adds some references unchecked (to the device or
/// frames context it returns from a chain of derived ones), and a result
/// past the limit is released instead.
fn count_within_limit(count: c_int) -> bool {
    count as c_uint <= MAX_REFERENCES
}

/// Wraps: av_buffer_unref
/// Handwritten
///
/// Releases a hardware device or frames context reference
/// ([`DeviceReferenceBox`], [`FramesReferenceBox`]): C frees the
/// `AVBufferRef` and drops one count of the context's buffer. The last one
/// runs the context's release (`hwdevice_ctx_free`, `hwframe_ctx_free`):
/// the backend's uninit, the context's `free` callback, the references the
/// context holds (its device, the context it was derived from) and its
/// memory. Its copy adds a reference to the same context (`av_buffer_ref`).
///
/// Hand-written, as [`BufferUnref`](super::buffer::BufferUnref) is: to pass
/// the reference's slot to `av_buffer_unref`, and to check the count before
/// `av_buffer_ref`. Stateless.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextUnref;

// SAFETY: av_buffer_unref releases exactly the one reference a box holds
// and reads nothing of the box afterwards; the context's release runs with
// its last reference, which keeps everything it releases alive until then.
unsafe impl CDrop<DeviceReference> for ContextUnref {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<DeviceReference>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVBufferRef>();
        // SAFETY: the caller hands over the box's sole claim on a live
        // reference; C av_frees the struct (NULLing only the local slot) and
        // drops one count of the buffer with acq_rel ordering, releasing the
        // context with the last one.
        unsafe { ffi::av_buffer_unref(&mut slot) }
    }
}

/// Wraps: av_buffer_ref
// SAFETY: a non-null result is a fresh struct, independent of `ptr` and
// released by `c_drop`, referring to the same initialised device context,
// which no handle writes. NULL means C failed, or the count would near
// wrapping.
unsafe impl CDupClone<DeviceReference> for ContextUnref {
    #[inline]
    unsafe fn c_dup(&self, ptr: NonNull<DeviceReference>) -> Option<NonNull<DeviceReference>> {
        // SAFETY: the caller passes a live reference, borrowed for the call.
        let src = unsafe { DeviceReferenceRef::from_ptr(ptr.as_ptr().cast()) }?;
        NonNull::new(src.try_clone()?.into_raw())
    }
}

// SAFETY: as for `CDrop<DeviceReference>`.
unsafe impl CDrop<FramesReference> for ContextUnref {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<FramesReference>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVBufferRef>();
        // SAFETY: as for a device reference.
        unsafe { ffi::av_buffer_unref(&mut slot) }
    }
}

/// Wraps: av_buffer_ref
// SAFETY: as for `CDupClone<DeviceReference>`, of a frames context.
unsafe impl CDupClone<FramesReference> for ContextUnref {
    #[inline]
    unsafe fn c_dup(&self, ptr: NonNull<FramesReference>) -> Option<NonNull<FramesReference>> {
        // SAFETY: the caller passes a live reference, borrowed for the call.
        let src = unsafe { FramesReferenceRef::from_ptr(ptr.as_ptr().cast()) }?;
        NonNull::new(src.try_clone()?.into_raw())
    }
}

define_ctype!(
    /// A reference to a hardware device context: the `AVBufferRef` whose
    /// data is the context's `AVHWDeviceContext`, an open device of one
    /// hardware API ([`DeviceType`]). Device and frames contexts are only
    /// ever reached through such references, and every reference keeps the
    /// context alive: so do the frames contexts and devices made from it.
    /// Its routines are on [`DeviceReferenceRef`] (`as_ref()`); the
    /// reference itself is owned by a [`DeviceReferenceBox`].
    ///
    /// Every reference a handle reaches holds one count of a device context
    /// that `av_hwdevice_ctx_alloc` allocated (hwcontext.c's private
    /// context, which starts with the public struct and names the backend
    /// driving it), and that was initialised: by `av_hwdevice_ctx_init`
    /// ([`UninitDevice::init`]) or by the routines creating initialised
    /// devices. Its public fields are not written after that, as
    /// `hwcontext.h` asks of callers. The one reference to a context not
    /// yet initialised is held by an [`UninitDevice`], which lends no
    /// handle.
    ///
    /// Neither `Send` nor `Sync`: a context is shared by all its references
    /// and the frames contexts and frames made from it, and FFmpeg promises
    /// no thread safety for its backends (libavcodec serializes the
    /// hardware accelerators that do not declare it), so they all stay on
    /// one thread.
    DeviceReference,
    DeviceReferenceRef,
    DeviceReferenceMut,
    ffi::AVBufferRef
);

/// One reference to an initialised hardware device context, owned: the
/// reference is released when the box drops, and the context with its last
/// reference. `clone` adds a reference to the same context
/// (`av_buffer_ref`), and aborts the process where C cannot allocate one,
/// or the context's count would near wrapping; [`DeviceReferenceRef::try_clone`]
/// returns `None` there.
///
/// Made by [`DeviceReference::create`], [`UninitDevice::init`] and
/// [`DeviceReferenceRef::derive`]; never allocated bare:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::hwcontext::DeviceReferenceBox;
/// let _ = DeviceReferenceBox::new();
/// ```
///
/// Its handles are borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::hwcontext::DeviceReferenceBox;
/// fn use_after_drop(device: DeviceReferenceBox) {
///     let handle = device.as_ref();
///     drop(device);
///     let _ = handle.alloc_frames();
/// }
/// ```
///
/// and it stays on its thread:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::hwcontext::DeviceReferenceBox;
/// fn send<T: Send>(_: T) {}
/// fn move_away(device: DeviceReferenceBox) {
///     send(device);
/// }
/// ```
pub type DeviceReferenceBox = CBox<DeviceReference, ContextUnref>;

impl DeviceReference {
    /// Wraps: av_hwdevice_ctx_alloc
    ///
    /// A new, uninitialised device context of `device_type`, for the caller
    /// to fill in and [`init`](UninitDevice::init); `None` when the library
    /// has no backend for the type (see [`DeviceType::supported`]) or
    /// cannot allocate the context.
    #[must_use]
    pub fn alloc(device_type: DeviceType) -> Option<UninitDevice> {
        // SAFETY: `av_hwdevice_ctx_alloc` takes the enum by value and
        // returns NULL or the only reference to a fresh, uninitialised
        // device context of a compiled-in backend.
        let buf = unsafe { ffi::av_hwdevice_ctx_alloc(device_type.0) };
        // SAFETY: NULL, or the only reference to an allocated context that
        // nothing initialised.
        unsafe { UninitDevice::from_c(buf) }
    }

    /// Wraps: av_hwdevice_ctx_create
    ///
    /// Opens the device `device` names, in the backend's own syntax (a DRM
    /// render node for VA-API, a GPU index for CUDA; `None` for the
    /// backend's default), and returns an initialised device context of
    /// `device_type` over it, set up by the backend-specific `options`,
    /// which C only reads. The backend sets the context's `free` and
    /// `user_opaque` fields itself.
    ///
    /// # Errors
    ///
    /// C's error: `AVERROR(ENOMEM)` when the library has no backend for the
    /// type, as when memory runs out; `AVERROR(ENOSYS)` for a backend that
    /// cannot open devices itself; the backend's own errors.
    pub fn create(
        device_type: DeviceType,
        device: Option<&CStr>,
        options: &Dict,
    ) -> Result<DeviceReferenceBox, c_int> {
        let mut out = ptr::null_mut();
        let options = options.as_ref().map_or(ptr::null(), |d| d.as_ptr());
        // SAFETY: `out` is a local slot C writes once; `device` is NULL or a
        // live string and `options` NULL or a live dictionary, both only read
        // during the call (backends look options up with `av_dict_get`). On
        // failure C releases what it built and writes NULL.
        let ret = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut out,
                device_type.0,
                device.map_or(ptr::null(), CStr::as_ptr),
                options.cast_mut(),
                0,
            )
        };
        status(ret)?;
        // SAFETY: on success, the only reference to a device context C
        // allocated and initialised (`av_hwdevice_ctx_init`).
        unsafe { DeviceReferenceBox::from_c(out) }.ok_or(ENOMEM)
    }
}

/// `device`, a reference C returned from a chain of derived devices,
/// unless that left the device's count past [`MAX_REFERENCES`].
fn within_limit_device(device: DeviceReferenceBox) -> Result<DeviceReferenceBox, c_int> {
    if count_within_limit(device.as_ref().ref_count()) {
        Ok(device)
    } else {
        Err(ENOMEM)
    }
}

impl<'a> DeviceReferenceRef<'a> {
    /// The address of the device context, which identifies it.
    fn context_addr(&self) -> usize {
        // SAFETY: a read of the `data` field of a live reference.
        unsafe { addr_of!((*self.as_ptr()).data).read() }.addr()
    }

    /// The device type of the context, as C set it at allocation.
    // The public getter is AVHWDeviceContext's.
    fn kind(&self) -> DeviceType {
        // SAFETY: a read of the `data` field of a live reference.
        let data = unsafe { addr_of!((*self.as_ptr()).data).read() };
        let ctx = data.cast::<ffi::AVHWDeviceContext>();
        // SAFETY: by the type's invariant, `data` is the context's public
        // struct, whose `type` C never changes after allocation.
        DeviceType(unsafe { addr_of!((*ctx).type_).read() })
    }

    /// The number of references to the device context, as C reads it.
    fn ref_count(&self) -> c_int {
        // SAFETY: a live reference; C atomically loads its buffer's count.
        unsafe { ffi::av_buffer_get_ref_count(self.as_ptr()) }
    }

    /// Whether C may add a reference to the device context.
    fn may_reference(&self) -> bool {
        count_allows_another(self.ref_count())
    }

    /// Whether `self` and `other` refer to the same device context.
    #[must_use]
    pub fn ptr_eq(&self, other: DeviceReferenceRef<'_>) -> bool {
        self.context_addr() == other.context_addr()
    }

    /// A new, owned reference to the same device context (`av_buffer_ref`,
    /// as [`DeviceReferenceBox`]'s `clone`); `None` when C cannot allocate
    /// it, or the context's count of references would near wrapping.
    #[must_use]
    pub fn try_clone(&self) -> Option<DeviceReferenceBox> {
        if !self.may_reference() {
            return None;
        }
        // SAFETY: a live reference; C copies the struct into a fresh
        // av_mallocz block and adds one count to the buffer, or returns NULL
        // with the count untouched.
        let copy = unsafe { ffi::av_buffer_ref(self.as_ptr()) };
        // SAFETY: NULL, or a new reference to the same initialised context.
        unsafe { DeviceReferenceBox::from_c(copy) }
    }

    /// Wraps: av_hwdevice_ctx_create_derived
    ///
    /// A device context of `device_type` on the hardware this one opened. If
    /// this device is of that type, or was derived from one (through any
    /// chain of devices of other types), that device itself, as a new
    /// reference; otherwise a new device, derived by the target backend from
    /// this device or, failing that, from each device this one was derived
    /// from in turn. A new device holds a reference to this one.
    ///
    /// # Errors
    ///
    /// C's error: `AVERROR(ENOMEM)` when the library has no backend for the
    /// type, as when memory runs out; `AVERROR(ENOSYS)` when no device of
    /// the chain derives to the type; the backend's own errors.
    /// `AVERROR(ENOMEM)`, before C, when the device's count of references
    /// would near wrapping, and for a result whose count would.
    pub fn derive(&self, device_type: DeviceType) -> Result<DeviceReferenceBox, c_int> {
        if !self.may_reference() {
            return Err(ENOMEM);
        }
        let mut out = ptr::null_mut();
        // SAFETY: `out` is a local slot C writes once; the source is a live
        // reference to an initialised device context, whose type and chain
        // of sources C reads (both fixed since they were set), whose
        // backends' `device_derive` read them, and which C references. On
        // failure C releases what it built and writes NULL.
        let ret = unsafe {
            ffi::av_hwdevice_ctx_create_derived(
                &mut out,
                device_type.0,
                self.as_ptr().cast_mut(),
                0,
            )
        };
        status(ret)?;
        // SAFETY: on success, a reference the caller owns to an initialised
        // device context: a new one, or one of the source's chain.
        let device = unsafe { DeviceReferenceBox::from_c(out) }.ok_or(ENOMEM)?;
        within_limit_device(device)
    }

    /// Wraps: av_hwdevice_ctx_create_derived_opts
    ///
    /// As [`derive`](Self::derive), handing the backend-specific `options`
    /// (those of [`DeviceReference::create`]) to the target backend when it
    /// derives a new device; C only reads them.
    ///
    /// # Errors
    ///
    /// As [`derive`](Self::derive).
    pub fn derive_with_options(
        &self,
        device_type: DeviceType,
        options: &Dict,
    ) -> Result<DeviceReferenceBox, c_int> {
        if !self.may_reference() {
            return Err(ENOMEM);
        }
        let mut out = ptr::null_mut();
        let options = options.as_ref().map_or(ptr::null(), |d| d.as_ptr());
        // SAFETY: as in `derive`; `options` is NULL or a live dictionary,
        // which the backends only read during the call.
        let ret = unsafe {
            ffi::av_hwdevice_ctx_create_derived_opts(
                &mut out,
                device_type.0,
                self.as_ptr().cast_mut(),
                options.cast_mut(),
                0,
            )
        };
        status(ret)?;
        // SAFETY: on success, a reference the caller owns to an initialised
        // device context: a new one, or one of the source's chain.
        let device = unsafe { DeviceReferenceBox::from_c(out) }.ok_or(ENOMEM)?;
        within_limit_device(device)
    }

    /// Wraps: av_hwframe_ctx_alloc
    ///
    /// A new, uninitialised frames context on this device, for the caller
    /// to fill in and [`init`](UninitFrames::init): no hardware or software
    /// format (`AV_PIX_FMT_NONE`), size or pool. It holds a reference to
    /// this device. `None` when C cannot allocate it, or the device's count
    /// of references would near wrapping.
    #[must_use]
    pub fn alloc_frames(&self) -> Option<UninitFrames> {
        if !self.may_reference() {
            return None;
        }
        // SAFETY: a live reference to an initialised device context, whose
        // backend C reads and which it references.
        let buf = unsafe { ffi::av_hwframe_ctx_alloc(self.as_ptr().cast_mut()) };
        // SAFETY: NULL, or the only reference to a fresh, uninitialised
        // frames context on this device.
        unsafe { UninitFrames::from_c(buf) }
    }

    /// Wraps: av_hwdevice_hwconfig_alloc
    ///
    /// A zeroed hardware configuration for this device's backend, which
    /// describes a configuration of the hardware to query the frames
    /// constraints of; `None` when the backend has none (most do not), or C
    /// cannot allocate it.
    #[must_use]
    pub fn alloc_hw_config(&self) -> Option<HwConfig> {
        // SAFETY: a live reference to a device context, whose backend's
        // configuration size C reads; it returns NULL or a fresh av_mallocz
        // block of that size.
        let ptr = unsafe { ffi::av_hwdevice_hwconfig_alloc(self.as_ptr().cast_mut()) };
        Some(HwConfig {
            ptr: NonNull::new(ptr)?,
            device_type: self.kind(),
        })
    }
}

/// Handwritten
///
/// A device context that is allocated but not yet initialised, from
/// [`DeviceReference::alloc`]: the only reference to it. Fill in what its
/// backend needs (`hwctx`, the backend's device struct from its
/// `hwcontext_<api>.h`, and optionally `free` and `user_opaque`) through
/// [`as_mut_ptr`](Self::as_mut_ptr), then [`init`](Self::init) it.
///
/// Hand-written: a [`DeviceReferenceBox`] whose handles are withheld until
/// the context is initialised, since every routine taking a device needs an
/// initialised one. Dropping it releases the context, as C does with one
/// that failed to initialise. Neither `Send` nor `Sync`, as
/// [`DeviceReference`]:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::hwcontext::UninitDevice;
/// fn send<T: Send>(_: T) {}
/// fn move_away(device: UninitDevice) {
///     send(device);
/// }
/// ```
pub struct UninitDevice(DeviceReferenceBox);

impl UninitDevice {
    /// Adopts `ptr`, the only reference to an uninitialised device context;
    /// `None` for NULL.
    ///
    /// # Safety
    ///
    /// `ptr` must be NULL or a reference the caller owns and hands over, to
    /// a device context `av_hwdevice_ctx_alloc` allocated (or built exactly
    /// as it does) that has no other reference and was not initialised.
    pub(crate) unsafe fn from_c(ptr: *mut ffi::AVBufferRef) -> Option<Self> {
        // SAFETY: by the caller's contract; the box lends no handle to the
        // context until `init` initialised it.
        unsafe { DeviceReferenceBox::from_c(ptr) }.map(Self)
    }

    /// The context's `AVHWDeviceContext`, for filling in its `hwctx` (the
    /// backend's device struct, which C allocated), `free` and
    /// `user_opaque` before [`init`](Self::init), as the backend's header
    /// asks.
    ///
    /// A raw seam until hwcontext.h's `AVHWDeviceContext` is wrapped (its
    /// `hwctx` stays backend-specific). Writes through it must keep
    /// `av_class`, `type` and the `hwctx` pointer as C set them; the pointer
    /// is valid while the box lives.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut ffi::AVHWDeviceContext {
        // SAFETY: a read of the `data` field of the live reference the box
        // holds; its data is the context's public struct.
        unsafe { addr_of!((*self.0.as_c_ptr()).data).read() }.cast()
    }

    /// Wraps: av_hwdevice_ctx_init
    ///
    /// Finalizes the context once it is filled in: the backend checks and
    /// completes it (its `device_init`, when it has one). The initialised
    /// context can then be used and shared.
    ///
    /// # Errors
    ///
    /// The backend's error; the context is released then.
    pub fn init(self) -> Result<DeviceReferenceBox, c_int> {
        // SAFETY: the only reference to an allocated device context, never
        // initialised before (`init` consumes it); C runs the backend's
        // `device_init` on it, which reads what the caller filled in.
        let ret = unsafe { ffi::av_hwdevice_ctx_init(self.0.as_c_ptr()) };
        status(ret)?;
        Ok(self.0)
    }
}

impl fmt::Debug for UninitDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only the type C set at allocation is read through the handle.
        f.debug_tuple("UninitDevice")
            .field(&self.0.as_ref().kind())
            .finish()
    }
}

/// Handwritten
///
/// A hardware configuration of a device's backend, from
/// [`DeviceReferenceRef::alloc_hw_config`]: an `av_mallocz` block, freed
/// with `av_free` on drop, laid out as the backend's `AV<API>HWConfig`
/// struct (VA-API's names the `VAConfigID` to query frames constraints
/// for). Its size is the backend's and only C knows it, so it is reached
/// through raw pointers only: [`as_mut_ptr`](Self::as_mut_ptr) to fill it
/// in, as the backend's header asks.
///
/// Hand-written: an owner of an untyped C block whose size and type
/// depend on the backend, for later routines to pass back with the
/// device type it belongs to.
pub struct HwConfig {
    ptr: NonNull<c_void>,
    device_type: DeviceType,
}

impl HwConfig {
    /// The device type whose backend allocated the configuration.
    #[must_use]
    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    /// The configuration, for reading through the backend's struct. A raw
    /// seam: the backend headers (`hwcontext_<api>.h`) are not wrapped.
    #[must_use]
    pub fn as_ptr(&self) -> *const c_void {
        self.ptr.as_ptr()
    }

    /// The configuration, for filling in through the backend's struct. A
    /// raw seam, as [`as_ptr`](Self::as_ptr); the block is zeroed when
    /// allocated and valid while `self` lives.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut c_void {
        self.ptr.as_ptr()
    }
}

impl Drop for HwConfig {
    fn drop(&mut self) {
        // SAFETY: the av_mallocz block `av_hwdevice_hwconfig_alloc`
        // returned, which `self` owns and nothing else frees.
        unsafe { ffi::av_free(self.ptr.as_ptr()) }
    }
}

impl fmt::Debug for HwConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HwConfig")
            .field("device_type", &self.device_type)
            .finish_non_exhaustive()
    }
}

define_ctype!(
    /// A reference to a hardware frames context: the `AVBufferRef` whose
    /// data is the context's `AVHWFramesContext`, a pool of interchangeable
    /// hardware frames of one format and size on one device. Every
    /// reference keeps the context alive, and the context keeps its device
    /// alive: so do the frames allocated from it. Its routines are on
    /// [`FramesReferenceRef`] (`as_ref()`); the reference itself is owned by
    /// a [`FramesReferenceBox`].
    ///
    /// Every reference a handle reaches holds one count of a frames context
    /// that `av_hwframe_ctx_alloc` allocated on an initialised device
    /// context (hwcontext_internal.h's private context, which starts with
    /// the public struct), and that was initialised: by
    /// `av_hwframe_ctx_init` ([`UninitFrames::init`]) or as a derived
    /// context (`av_hwframe_ctx_create_derived`). Its public fields are not
    /// written after that. The one reference to a context not yet
    /// initialised is held by an [`UninitFrames`], which lends no handle.
    ///
    /// Neither `Send` nor `Sync`, as [`DeviceReference`], whose device it
    /// shares.
    FramesReference,
    FramesReferenceRef,
    FramesReferenceMut,
    ffi::AVBufferRef
);

/// One reference to an initialised hardware frames context, owned, as
/// [`DeviceReferenceBox`] owns one to a device: released on drop, `clone`
/// adds a reference (`av_buffer_ref`) or aborts where
/// [`FramesReferenceRef::try_clone`] returns `None`.
///
/// Made by [`UninitFrames::init`] and [`FramesReferenceRef::derive`]; never
/// allocated bare:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::hwcontext::FramesReferenceBox;
/// let _ = FramesReferenceBox::new();
/// ```
///
/// Its handles are borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::hwcontext::FramesReferenceBox;
/// fn use_after_drop(frames: FramesReferenceBox) {
///     let handle = frames.as_ref();
///     drop(frames);
///     let _ = handle.get_buffer();
/// }
/// ```
///
/// and it stays on its thread:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::hwcontext::FramesReferenceBox;
/// fn send<T: Send>(_: T) {}
/// fn move_away(frames: FramesReferenceBox) {
///     send(frames);
/// }
/// ```
pub type FramesReferenceBox = CBox<FramesReference, ContextUnref>;

/// `frames`, a reference C returned from a derivation, unless that left the
/// context's count past [`MAX_REFERENCES`].
fn within_limit_frames(frames: FramesReferenceBox) -> Result<FramesReferenceBox, c_int> {
    if count_within_limit(frames.as_ref().ref_count()) {
        Ok(frames)
    } else {
        Err(ENOMEM)
    }
}

impl<'a> FramesReferenceRef<'a> {
    /// The address of the frames context, which identifies it.
    fn context_addr(&self) -> usize {
        // SAFETY: a read of the `data` field of a live reference.
        unsafe { addr_of!((*self.as_ptr()).data).read() }.addr()
    }

    /// The context's hardware pixel format, which its frames have.
    // The public getter is AVHWFramesContext's.
    fn hw_format(&self) -> ffi::AVPixelFormat {
        // SAFETY: a read of the `data` field of a live reference.
        let data = unsafe { addr_of!((*self.as_ptr()).data).read() };
        let ctx = data.cast::<ffi::AVHWFramesContext>();
        // SAFETY: by the type's invariant, `data` is the initialised
        // context's public struct, which nothing writes any more.
        unsafe { addr_of!((*ctx).format).read() }
    }

    /// The number of references to the frames context, as C reads it.
    fn ref_count(&self) -> c_int {
        // SAFETY: a live reference; C atomically loads its buffer's count.
        unsafe { ffi::av_buffer_get_ref_count(self.as_ptr()) }
    }

    /// Whether C may add a reference to the frames context.
    fn may_reference(&self) -> bool {
        count_allows_another(self.ref_count())
    }

    /// Whether `self` and `other` refer to the same frames context.
    #[must_use]
    pub fn ptr_eq(&self, other: FramesReferenceRef<'_>) -> bool {
        self.context_addr() == other.context_addr()
    }

    /// A new, owned reference to the same frames context (`av_buffer_ref`,
    /// as [`FramesReferenceBox`]'s `clone`); `None` when C cannot allocate
    /// it, or the context's count of references would near wrapping.
    #[must_use]
    pub fn try_clone(&self) -> Option<FramesReferenceBox> {
        if !self.may_reference() {
            return None;
        }
        // SAFETY: as for a device reference.
        let copy = unsafe { ffi::av_buffer_ref(self.as_ptr()) };
        // SAFETY: NULL, or a new reference to the same initialised context.
        unsafe { FramesReferenceBox::from_c(copy) }
    }

    /// Wraps: av_hwframe_get_buffer
    ///
    /// A new hardware frame from the context's pool, of its format and
    /// size, holding a reference to the context. A derived context
    /// allocates the frame in the context it was derived from and maps it
    /// into its own.
    ///
    /// # Errors
    ///
    /// C's error: `AVERROR(ENOSYS)` for a backend that allocates no frames,
    /// `AVERROR(EINVAL)` for a context without a pool, `AVERROR(ENOMEM)`,
    /// the backend's own errors and, for a derived context, those of the
    /// allocation and mapping. `AVERROR(ENOMEM)`, before C, when the
    /// context's count of references would near wrapping.
    pub fn get_buffer(&self) -> Result<HardwareFrame, c_int> {
        if !self.may_reference() {
            return Err(ENOMEM);
        }
        let frame = Frame::new().ok_or(ENOMEM)?;
        let (ctx, dst) = (self.as_ptr().cast_mut(), frame.as_c_ptr());
        // SAFETY: a live reference to an initialised frames context, which
        // C reads and references, and a fresh frame nothing else reaches,
        // which C fills (buffers, planes, format, size, frames context).
        // On failure the frame holds at most a frames context reference and
        // its format, which its release handles.
        let ret = unsafe { ffi::av_hwframe_get_buffer(ctx, dst, 0) };
        status(ret)?;
        // A frame `av_hwframe_get_buffer` filled: the type's invariant.
        Ok(HardwareFrame {
            frame,
            _context: PhantomData,
        })
    }

    /// Wraps: av_hwframe_ctx_create_derived
    ///
    /// A new, initialised frames context of hardware format `format` on
    /// `device`, which maps the frames of this one: its frames are
    /// allocated here and mapped there with `flags`. Its software format
    /// and size are this context's. When this context was itself derived
    /// from one on `device`, the result is that context, as a new reference
    /// (an unmapping).
    ///
    /// # Errors
    ///
    /// C's error: `AVERROR(ENOMEM)`, the backends' own errors (a backend
    /// that cannot derive is not one: C then leaves the mapping to each
    /// allocation). `AVERROR(ENOMEM)`, before C, when this context's or the
    /// device's count of references would near wrapping, and for a result
    /// whose count would.
    pub fn derive(
        &self,
        format: PixelFormat,
        device: DeviceReferenceRef<'_>,
        flags: MapFlags,
    ) -> Result<FramesReferenceBox, c_int> {
        if !(self.may_reference() && device.may_reference()) {
            return Err(ENOMEM);
        }
        let mut out = ptr::null_mut();
        // SAFETY: `out` is a local slot C writes at most once; `device` and
        // the source are live references to initialised device and frames
        // contexts, which C reads and references, and whose backends'
        // `frames_derive_from`/`frames_derive_to` read them. On failure C
        // releases what it built.
        let ret = unsafe {
            ffi::av_hwframe_ctx_create_derived(
                &mut out,
                format.into_raw(),
                device.as_ptr().cast_mut(),
                self.as_ptr().cast_mut(),
                flags.bits(),
            )
        };
        status(ret)?;
        // SAFETY: on success, a reference the caller owns to an initialised
        // frames context: a new derived one, or the source's own source.
        let frames = unsafe { FramesReferenceBox::from_c(out) }.ok_or(ENOMEM)?;
        within_limit_frames(frames)
    }
}

/// Handwritten
///
/// A frames context that is allocated but not yet initialised, from
/// [`DeviceReferenceRef::alloc_frames`]: the only reference to it. Fill in
/// its hardware and software formats, size and, optionally, initial pool
/// size, pool, `hwctx` (the backend's frames struct), `free` and
/// `user_opaque` through [`as_mut_ptr`](Self::as_mut_ptr), then
/// [`init`](Self::init) it.
///
/// Hand-written, as [`UninitDevice`] is. Dropping it releases the context.
/// Neither `Send` nor `Sync`, as [`FramesReference`].
pub struct UninitFrames(FramesReferenceBox);

impl UninitFrames {
    /// Adopts `ptr`, the only reference to an uninitialised frames context;
    /// `None` for NULL.
    ///
    /// # Safety
    ///
    /// `ptr` must be NULL or a reference the caller owns and hands over, to
    /// a frames context `av_hwframe_ctx_alloc` allocated that has no other
    /// reference and was not initialised.
    pub(crate) unsafe fn from_c(ptr: *mut ffi::AVBufferRef) -> Option<Self> {
        // SAFETY: by the caller's contract; the box lends no handle to the
        // context until `init` initialised it.
        unsafe { FramesReferenceBox::from_c(ptr) }.map(Self)
    }

    /// The context's `AVHWFramesContext`, for filling in its formats, size,
    /// pool and backend struct before [`init`](Self::init), as
    /// `hwcontext.h` and the backend's header ask.
    ///
    /// A raw seam until hwcontext.h's `AVHWFramesContext` is wrapped.
    /// Writes through it must keep `av_class`, `device_ref`, `device_ctx`
    /// and the `hwctx` pointer as C set them, and leave `pool` NULL or a
    /// pool of buffers the backend can use as its surfaces, which the
    /// context then holds a reference to until it is released; the pointer
    /// is valid while `self` lives.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut ffi::AVHWFramesContext {
        // SAFETY: a read of the `data` field of the live reference the box
        // holds; its data is the context's public struct.
        unsafe { addr_of!((*self.0.as_c_ptr()).data).read() }.cast()
    }

    /// Wraps: av_hwframe_ctx_init
    ///
    /// Finalizes the context once it is filled in: C checks that the
    /// backend supports its hardware format (logging an error and failing
    /// with `AVERROR(ENOSYS)` otherwise, as for the format C leaves unset)
    /// and that its size is valid, the backend completes it, and the first
    /// initial pool size frames are allocated and released, to fill the
    /// pool. The initialised context can then allocate frames and be
    /// shared.
    ///
    /// # Errors
    ///
    /// C's error, as above, or the backend's or allocation's; the context
    /// is released then.
    pub fn init(self) -> Result<FramesReferenceBox, c_int> {
        // SAFETY: the only reference to an allocated frames context, never
        // initialised before (`init` consumes it), whose fields the caller
        // wrote through `as_mut_ptr` under its contract; C validates the
        // format and size, runs the backend's `frames_init` and
        // preallocates frames through `av_hwframe_get_buffer`.
        let ret = unsafe { ffi::av_hwframe_ctx_init(self.0.as_c_ptr()) };
        status(ret)?;
        Ok(self.0)
    }
}

impl fmt::Debug for UninitFrames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UninitFrames").finish_non_exhaustive()
    }
}

/// Handwritten
///
/// A hardware frame: a frame whose data are surfaces of a hardware frames
/// context, from [`FramesReferenceRef::get_buffer`] or a mapping
/// ([`map_to`](Self::map_to)). It owns a reference to its frames context
/// ([`frames`](Self::frames)), and so to its device; drop it to return the
/// surface to the pool.
///
/// Not a [`Frame`] to safe code, which lends no
/// [`FrameRef`]: a hardware frame's buffers hold the
/// backend's surface handles where a frame's hold bytes (VA-API makes a
/// surface ID the data pointer of a 4-byte buffer), and its planes point
/// into no buffer. Its data is reached by mapping it to normal memory
/// ([`map`](Self::map)), or by copying it out (`av_hwframe_transfer_data`).
///
/// Every value holds a frame that `av_hwframe_get_buffer` or
/// `av_hwframe_map` filled, with a frames context, which nothing else
/// reaches. Hand-written: an owner of an `AVFrame` that keeps it out of the
/// frame API, whose readers would take its surface handles for bytes:
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::hwcontext::HardwareFrame;
/// fn planes(frame: &HardwareFrame) {
///     let _ = frame.as_ref().plane(0);
/// }
/// ```
///
/// Its frames context is borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::hwcontext::HardwareFrame;
/// fn outlive(frame: HardwareFrame) {
///     let frames = frame.frames();
///     drop(frame);
///     let _ = frames.get_buffer();
/// }
/// ```
///
/// Neither `Send` nor `Sync`, as [`FramesReference`]:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::hwcontext::HardwareFrame;
/// fn send<T: Send>(_: T) {}
/// fn move_away(frame: HardwareFrame) {
///     send(frame);
/// }
/// ```
pub struct HardwareFrame {
    frame: FrameBox,
    /// Withholds `Send` and `Sync`, as the frames context's.
    _context: PhantomData<*const ()>,
}

impl HardwareFrame {
    /// Whether C may add a reference to the frame's frames context and to
    /// each of its buffers, as a mapping of the frame does.
    fn may_reference_all(&self) -> bool {
        let p = self.frame.as_c_ptr();
        // SAFETY: reads of the `buf`, `extended_buf`, `nb_extended_buf` and
        // `hw_frames_ctx` fields of the live frame `self` owns.
        let (bufs, extended, count, frames) = unsafe {
            (
                addr_of!((*p).buf).read(),
                addr_of!((*p).extended_buf).read(),
                addr_of!((*p).nb_extended_buf).read(),
                addr_of!((*p).hw_frames_ctx).read(),
            )
        };
        let count = if extended.is_null() {
            0
        } else {
            usize::try_from(count).unwrap_or(0)
        };
        let may = |buf: *const ffi::AVBufferRef| {
            // SAFETY: NULL, or a live reference the frame owns; C atomically
            // loads its buffer's count.
            buf.is_null() || count_allows_another(unsafe { ffi::av_buffer_get_ref_count(buf) })
        };
        // SAFETY: `extended` holds `count` live entries when not NULL.
        let extended_entry = |i: usize| unsafe { extended.add(i).read() };
        may(frames) && bufs.iter().all(|&b| may(b)) && (0..count).all(|i| may(extended_entry(i)))
    }

    /// The frame's pixel format: its frames context's hardware format.
    // Only the frame's scalar fields are read through its handle.
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.frame.as_ref().pixel_format()
    }

    /// The frame's width, in pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        self.frame.as_ref().width()
    }

    /// The frame's height, in pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        self.frame.as_ref().height()
    }

    /// The frames context the frame belongs to, borrowed from the frame.
    #[must_use]
    pub fn frames(&self) -> FramesReferenceRef<'_> {
        // SAFETY: a read of the `hw_frames_ctx` field of the live frame
        // `self` owns.
        let buf = unsafe { addr_of!((*self.frame.as_c_ptr()).hw_frames_ctx).read() };
        // SAFETY: by the type's invariant, a live reference the frame owns
        // to an initialised frames context, which nothing releases or
        // replaces while `self` is borrowed.
        unsafe { FramesReferenceRef::from_ptr(buf) }
            .expect("a hardware frame holds its frames context")
    }

    /// Wraps: av_hwframe_map
    ///
    /// Maps the frame to normal memory, as a software frame of `format`
    /// (with [`PixelFormat::NONE`], the format the backend finds most
    /// appropriate, usually the frames context's software format), with
    /// the access `flags` ask for. The mapped frame holds a reference to
    /// this one, whose surface it shows, and the mapping is undone when it
    /// is dropped.
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOSYS)` when the backend cannot map the frame as asked,
    /// other C errors when the mapping failed. `AVERROR(ENOMEM)`, before C,
    /// when the count of references to the frames context or one of the
    /// frame's buffers would near wrapping.
    pub fn map(&self, format: PixelFormat, flags: MapFlags) -> Result<MappedFrame, c_int> {
        if !self.may_reference_all() {
            return Err(ENOMEM);
        }
        let mut dst = Frame::new().ok_or(ENOMEM)?;
        dst.as_mut().set_format(format.into_raw().0);
        let (dst_ptr, src) = (dst.as_c_ptr(), self.frame.as_c_ptr());
        // SAFETY: `dst` is a fresh frame with only a format, which C fills
        // as a mapping (on failure it releases what it set and keeps the
        // format); the source is the live hardware frame `self` owns, which
        // C and the backend's `map_from` read and reference.
        let ret = unsafe { ffi::av_hwframe_map(dst_ptr, src, flags.bits()) };
        status(ret)?;
        // A frame `av_hwframe_map` filled: the type's invariant.
        Ok(MappedFrame {
            frame: dst,
            _context: PhantomData,
        })
    }

    /// Wraps: av_hwframe_map
    ///
    /// Maps the frame into another frames context, `frames`, as a hardware
    /// frame of that context's format, with `flags`: the contexts' devices
    /// must be compatible, as for a context derived from this frame's
    /// ([`FramesReferenceRef::derive`]). When this frame's context was
    /// derived from `frames`, the result is the frame this one was mapped
    /// from, as a new reference (an unmapping). The new frame holds a
    /// reference to this one, or to the frame it was mapped from.
    ///
    /// # Errors
    ///
    /// As [`map`](Self::map), and `AVERROR(EINVAL)`, before C, when
    /// `frames` is the frame's own context: C reads the frame as one it
    /// mapped itself there, which a frame of the context's pool is not.
    pub fn map_to(
        &self,
        frames: FramesReferenceRef<'_>,
        flags: MapFlags,
    ) -> Result<HardwareFrame, c_int> {
        if frames.ptr_eq(self.frames()) {
            return Err(EINVAL);
        }
        if !self.may_reference_all() {
            return Err(ENOMEM);
        }
        let target = frames.try_clone().ok_or(ENOMEM)?;
        let dst = Frame::new().ok_or(ENOMEM)?;
        let p = dst.as_c_ptr();
        // SAFETY: writes of two fields of the fresh frame `dst` owns, which
        // nothing else reaches: the format, and its frames context
        // reference, which the frame then owns and releases.
        unsafe {
            addr_of_mut!((*p).format).write(frames.hw_format().0);
            addr_of_mut!((*p).hw_frames_ctx).write(target.into_c());
        }
        // The contexts differ, so C unmaps only when this frame's context
        // was derived from `frames`, whose frames are all mappings C made,
        // as its unmapping expects of them.
        // SAFETY: `dst` is a blank frame with a format and frames context,
        // which C keeps on failure and fills as a mapping (or, unmapping,
        // as a reference to the frame this one was mapped from) on success;
        // the source is the live hardware frame `self` owns, which C and
        // the backends' `map_from`/`map_to` read and reference.
        let ret = unsafe { ffi::av_hwframe_map(p, self.frame.as_c_ptr(), flags.bits()) };
        status(ret)?;
        // A frame `av_hwframe_map` filled, holding `frames`' context (or,
        // unmapping, the source frame's, which is `frames`'): the type's
        // invariant.
        Ok(HardwareFrame {
            frame: dst,
            _context: PhantomData,
        })
    }
}

impl fmt::Debug for HardwareFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HardwareFrame")
            .field("format", &self.format())
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}

/// Handwritten
///
/// A hardware frame mapped to normal memory, from [`HardwareFrame::map`]:
/// a software frame whose planes show the surface, as the backend mapped
/// it. It holds a reference to the hardware frame, and the mapping is
/// undone when it is dropped.
///
/// Not a [`Frame`] to safe code either: its planes
/// point into the mapping rather than into its buffers, which a frame's
/// readers rely on, and how many of the bytes there may be read or written
/// depends on the access the mapping was made with. Its planes are left to
/// the wrappers of the C routines that consume frames.
///
/// Hand-written, as [`HardwareFrame`] is. Neither `Send` nor `Sync`, as
/// [`FramesReference`]:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::hwcontext::MappedFrame;
/// fn send<T: Send>(_: T) {}
/// fn move_away(frame: MappedFrame) {
///     send(frame);
/// }
/// ```
pub struct MappedFrame {
    frame: FrameBox,
    /// Withholds `Send` and `Sync`, as the source frames context's.
    _context: PhantomData<*const ()>,
}

impl MappedFrame {
    /// The mapping's pixel format.
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.frame.as_ref().pixel_format()
    }

    /// The mapping's width, in pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        self.frame.as_ref().width()
    }

    /// The mapping's height, in pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        self.frame.as_ref().height()
    }
}

impl fmt::Debug for MappedFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MappedFrame")
            .field("format", &self.format())
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "../tests/common/hwframes.rs"]
mod fixture;

#[cfg(test)]
#[path = "../tests/common/hwbackend.rs"]
pub(crate) mod backend;

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_uint;
    use core::mem::{align_of, size_of};
    use std::collections::HashSet;

    use crate::libavutil::buffer::BufferReference;
    use crate::libavutil::dict::Dict;
    use crate::libavutil::mem::alloc_array_zeroed;

    /// `AVERROR(ENOSYS)`.
    const ENOSYS: c_int = -(ffi::ENOSYS as c_int);

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

    /// The fake backend's hardware frames contexts and frames
    /// (`tests/common/hwframes.rs`), adopted by the crate's owners.
    mod hw {
        use super::super::fixture;
        use super::*;
        use crate::libavutil::buffer::BufferReferenceBox;

        pub(super) use fixture::{
            EMPTY, GRAY_FIRST, INERT, LISTING_FAILS, NO_LIST, TRANSFER_FAILS, YUV, pattern,
        };

        /// Adopts a reference the fixture made.
        pub(super) fn adopt_context(buf: *mut ffi::AVBufferRef) -> BufferReferenceBox {
            // SAFETY: a fresh reference the fixture handed over, satisfying
            // `BufferReference`'s invariant (its bytes, the zeroed context,
            // are initialised).
            unsafe { BufferReferenceBox::from_c(buf) }.unwrap()
        }

        /// A fake YUV 4:2:0 context behaving as `behaviour`.
        pub(super) fn context(
            behaviour: &'static fixture::Behaviour,
            width: c_int,
            height: c_int,
        ) -> BufferReferenceBox {
            adopt_context(fixture::yuv_context(behaviour, width, height))
        }

        /// A fake context of a backend without transfers.
        pub(super) fn inert_context() -> BufferReferenceBox {
            adopt_context(fixture::new_context(
                &INERT,
                &YUV,
                ffi::AVPixelFormat::AV_PIX_FMT_MEDIACODEC,
                ffi::AVPixelFormat::AV_PIX_FMT_YUV420P,
                16,
                8,
            ))
        }

        /// A hardware frame of `ctx` whose surface holds `pattern(seed)`.
        pub(super) fn hw_frame(ctx: &BufferReferenceBox, seed: u8) -> FrameBox {
            // SAFETY: `ctx` is a live fake context; the fresh frame satisfies
            // `Frame`'s invariant: `hw_frames_ctx` references that context,
            // and `data[3]` is the start of `buf[0]`'s window, the surface.
            unsafe { FrameBox::from_c(fixture::new_hw_frame(ctx.as_c_ptr(), pattern(seed))) }
                .unwrap()
        }

        /// The image of the surface of `frame`, a hardware frame of a fake
        /// context.
        pub(super) fn surface(frame: &FrameBox) -> Vec<u8> {
            // SAFETY: `frame` came from `hw_frame`.
            unsafe { fixture::surface_image(frame.as_c_ptr()) }
        }

        /// The `width` x `height` image of `format` in software `frame`.
        pub(super) fn image(
            frame: &FrameBox,
            format: PixelFormat,
            width: c_int,
            height: c_int,
        ) -> Vec<u8> {
            // SAFETY: callers pass frames whose planes hold that image.
            unsafe { fixture::image(frame.as_c_ptr(), format.into_raw(), width, height) }
        }
    }

    /// A software frame of `format` at `width` x `height` with zeroed
    /// buffers.
    fn software(format: PixelFormat, width: c_int, height: c_int) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(format);
        m.set_width(width);
        m.set_height(height);
        m.alloc_buffers(0).unwrap();
        f
    }

    /// A 16 x 8 YUV 4:2:0 frame whose planes lie back to back, with line
    /// sizes of their row widths, in one zeroed buffer of `size` bytes.
    fn packed(size: usize) -> FrameBox {
        let mut f = Frame::new().unwrap();
        let mut m = f.as_mut();
        m.set_pixel_format(YUV420P);
        m.set_width(16);
        m.set_height(8);
        m.set_buffer(0, Some(BufferReference::alloc_zeroed(size).unwrap()));
        m.set_linesizes([16, 8, 8, 0, 0, 0, 0, 0]);
        for (plane, offset) in [(0, 0), (1, 128), (2, 160)] {
            m.set_plane(plane, 0, offset).unwrap();
        }
        f
    }

    const YUV420P: PixelFormat = PixelFormat::YUV420P;
    const GRAY8: PixelFormat = PixelFormat::GRAY8;

    #[test]
    fn holds_image_accepts_buffers_allocated_at_the_size() {
        let f = software(YUV420P, 16, 8);
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, false));
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, true));
        // A smaller image fits too; the luma alone of the same planes.
        assert!(holds_image(f.as_ref(), YUV420P, 1, 1, true));
        assert!(holds_image(f.as_ref(), GRAY8, 16, 8, true));
        // An empty image reaches no row.
        assert!(holds_image(f.as_ref(), YUV420P, 0, 0, true));
    }

    #[test]
    fn holds_image_refuses_images_past_the_buffers() {
        let f = software(YUV420P, 16, 8);
        // av_frame_get_buffer pads the height to 32 rows: past that, and
        // past the line size, the rows leave the buffer or the line.
        assert!(!holds_image(f.as_ref(), YUV420P, 16, 64, false));
        assert!(!holds_image(f.as_ref(), YUV420P, 4096, 8, false));
    }

    #[test]
    fn holds_image_refuses_shared_planes_for_writes_only() {
        let f = software(YUV420P, 16, 8);
        let copy = f.as_ref().try_clone().unwrap();
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, false));
        assert!(!holds_image(f.as_ref(), YUV420P, 16, 8, true));
        drop(copy);
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, true));
    }

    #[test]
    fn holds_image_refuses_unusable_line_sizes_and_planes() {
        let base = software(YUV420P, 16, 8);
        let ls = base.as_ref().linesizes();
        for (plane, linesize) in [(0, -ls[0]), (0, 0), (1, 7), (2, ls[2] * 64)] {
            let mut f = software(YUV420P, 16, 8);
            f.as_mut().set_linesize(plane, linesize);
            assert!(
                !holds_image(f.as_ref(), YUV420P, 16, 8, false),
                "{plane}: {linesize}"
            );
        }
        let mut f = software(YUV420P, 16, 8);
        f.as_mut().clear_plane(2);
        assert!(!holds_image(f.as_ref(), YUV420P, 16, 8, false));
        assert!(holds_image(f.as_ref(), GRAY8, 16, 8, false));
    }

    #[test]
    fn holds_image_refuses_unknown_and_hardware_formats() {
        let f = software(YUV420P, 16, 8);
        for format in [
            PixelFormat::NONE,
            PixelFormat::VAAPI,
            PixelFormat::from_raw(ffi::AVPixelFormat(100_000)),
        ] {
            assert!(!holds_image(f.as_ref(), format, 16, 8, false), "{format:?}");
        }
    }

    #[test]
    fn holds_image_checks_a_palette() {
        let f = software(PixelFormat::PAL8, 16, 8);
        assert!(holds_image(f.as_ref(), PixelFormat::PAL8, 16, 8, true));
        let mut f = software(PixelFormat::PAL8, 16, 8);
        f.as_mut().clear_plane(1);
        assert!(!holds_image(f.as_ref(), PixelFormat::PAL8, 16, 8, false));
    }

    #[test]
    fn holds_image_trusts_planes_vouched_for() {
        let mut f = Frame::new().unwrap();
        let mut mem = vec![0u8; 16];
        // SAFETY: `mem` outlives the frame's use below and holds 16 bytes;
        // nothing reads the plane through the frame.
        unsafe { f.as_mut().set_data_ptr(0, mem.as_mut_ptr()) };
        f.as_mut().set_linesize(0, 16);
        assert!(holds_image(f.as_ref(), GRAY8, 16, 1, false));
        drop(f);
        drop(mem);
    }

    #[test]
    fn fits_surfaces_bounds_both_dimensions() {
        let g = FramesGeometry {
            sw_format: YUV420P,
            width: 16,
            height: 8,
        };
        let mut f = Frame::new().unwrap();
        for (w, h, fits) in [
            (0, 0, true),
            (16, 8, true),
            (1, 8, true),
            (17, 8, false),
            (16, 9, false),
            (-1, 8, false),
            (16, -1, false),
            (c_int::MIN, c_int::MAX, false),
        ] {
            f.as_mut().set_width(w);
            f.as_mut().set_height(h);
            assert_eq!(fits_surfaces(f.as_ref(), g), fits, "{w}x{h}");
        }
    }

    #[test]
    fn hw_frames_reads_the_context_geometry() {
        let ctx = hw::context(&hw::YUV, 32, 18);
        let frame = hw::hw_frame(&ctx, 1);
        let hw = HwFrames::of(frame.as_ref()).unwrap();
        assert_eq!(
            hw.geometry(),
            FramesGeometry {
                sw_format: YUV420P,
                width: 32,
                height: 18
            }
        );
        assert!(HwFrames::of(Frame::new().unwrap().as_ref()).is_none());
    }

    #[test]
    fn hw_frames_adopts_listings() {
        let ctx = hw::context(&hw::YUV, 16, 8);
        let hw = HwFrames(ctx.as_ref());
        let from = hw.transfer_formats(TransferDirection::From).unwrap();
        assert_eq!(from.as_slice(), [YUV420P, GRAY8]);
        let to = hw.transfer_formats(TransferDirection::To).unwrap();
        assert_eq!(to.as_slice(), [YUV420P]);
        // Each call hands out a new list.
        let again = hw.transfer_formats(TransferDirection::From).unwrap();
        assert_ne!(again.as_slice().as_ptr(), from.as_slice().as_ptr());
        let empty = hw::context(&hw::EMPTY, 16, 8);
        assert!(
            HwFrames(empty.as_ref())
                .transfer_formats(TransferDirection::From)
                .unwrap()
                .as_slice()
                .is_empty()
        );
        let none = hw::context(&hw::NO_LIST, 16, 8);
        for dir in [TransferDirection::From, TransferDirection::To] {
            let listed = HwFrames(none.as_ref()).transfer_formats(dir).unwrap();
            assert!(listed.as_slice().is_empty());
        }
        let failing = hw::context(&hw::LISTING_FAILS, 16, 8);
        assert_eq!(
            HwFrames(failing.as_ref())
                .transfer_formats(TransferDirection::To)
                .unwrap_err(),
            ENOMEM
        );
        let inert = hw::inert_context();
        assert_eq!(
            HwFrames(inert.as_ref())
                .transfer_formats(TransferDirection::From)
                .unwrap_err(),
            ENOSYS
        );
    }

    #[test]
    fn software_transfer_refusal_checks_sizes_formats_and_planes() {
        let ctx = hw::context(&hw::YUV, 16, 8);
        let hw_frame = hw::hw_frame(&ctx, 2);
        let hw = HwFrames::of(hw_frame.as_ref()).unwrap();
        let (src, from, to) = (
            hw_frame.as_ref(),
            TransferDirection::From,
            TransferDirection::To,
        );
        let ok = software(YUV420P, 16, 8);
        assert_eq!(software_transfer_refusal(hw, src, ok.as_ref(), from), None);
        assert_eq!(software_transfer_refusal(hw, src, ok.as_ref(), to), None);
        // Luma only is listed for downloads alone.
        let gray = software(GRAY8, 16, 8);
        assert_eq!(
            software_transfer_refusal(hw, src, gray.as_ref(), from),
            None
        );
        assert_eq!(
            software_transfer_refusal(hw, src, gray.as_ref(), to),
            Some(EINVAL)
        );
        // Planes packed in one buffer of exactly the surfaces' image, then
        // one byte short of it, though the display size matches.
        let exact = packed(16 * 8 + 2 * 8 * 4);
        assert_eq!(
            software_transfer_refusal(hw, src, exact.as_ref(), from),
            None
        );
        let short = packed(16 * 8 + 2 * 8 * 4 - 1);
        assert_eq!(
            software_transfer_refusal(hw, src, short.as_ref(), from),
            Some(EINVAL)
        );
        let mut tall = packed(16 * 8 + 2 * 8 * 4);
        tall.as_mut().set_linesize(0, 32);
        assert_eq!(
            software_transfer_refusal(hw, src, tall.as_ref(), to),
            Some(EINVAL)
        );
        // A display size past the surfaces, on either side.
        let mut wide = software(YUV420P, 16, 8);
        wide.as_mut().set_width(17);
        assert_eq!(
            software_transfer_refusal(hw, src, wide.as_ref(), to),
            Some(EINVAL)
        );
        let mut hw_tall = hw::hw_frame(&ctx, 2);
        hw_tall.as_mut().set_height(9);
        assert_eq!(
            software_transfer_refusal(hw, hw_tall.as_ref(), ok.as_ref(), to),
            Some(EINVAL)
        );
        // Shared planes are read, never written.
        let copy = ok.as_ref().try_clone().unwrap();
        assert_eq!(
            software_transfer_refusal(hw, src, ok.as_ref(), from),
            Some(EINVAL)
        );
        assert_eq!(software_transfer_refusal(hw, src, ok.as_ref(), to), None);
        drop(copy);
        // The listing's own error.
        let failing = hw::context(&hw::LISTING_FAILS, 16, 8);
        let failing = HwFrames(failing.as_ref());
        assert_eq!(
            software_transfer_refusal(failing, src, ok.as_ref(), to),
            Some(ENOMEM)
        );
    }

    #[test]
    fn transfer_without_contexts_is_refused_by_c() {
        let mut clean = Frame::new().unwrap();
        let counted = software(GRAY8, 4, 4);
        let mut counted_dst = software(GRAY8, 4, 4);
        // SAFETY: no frame holds a hardware frames context.
        unsafe {
            assert_eq!(
                clean.as_mut().transfer_data_from(counted.as_ref()),
                Err(EINVAL)
            );
            assert_eq!(
                counted_dst.as_mut().transfer_data_from(counted.as_ref()),
                Err(ENOSYS)
            );
            assert_eq!(
                clean
                    .as_mut()
                    .transfer_data_from(Frame::new().unwrap().as_ref()),
                Err(EINVAL)
            );
        }
    }

    /// C's `transfer_data_alloc` leaks what the frame held and leaves the
    /// bytes around the copied lines uninitialised; the Rust composition
    /// releases it (LeakSanitizer sees no leak) and zeroes them.
    #[test]
    fn download_into_new_buffers_releases_the_old_contents_and_zeroes_padding() {
        let ctx = hw::context(&hw::YUV, 16, 8);
        let mut src = hw::hw_frame(&ctx, 3);
        src.as_mut().set_width(10);
        src.as_mut().set_height(6);
        let mut dst = Frame::new().unwrap();
        let mut d = dst.as_mut();
        let mut dict = Dict::new();
        dict.set(c"k", c"v", Default::default()).unwrap();
        d.set_metadata(dict);
        d.set_opaque_ref(Some(BufferReference::alloc_zeroed(4).unwrap()));
        d.set_pts(9);
        // SAFETY: `src` is a hardware frame of the fake context.
        unsafe { dst.as_mut().transfer_data_from(src.as_ref()) }.unwrap();
        let r = dst.as_ref();
        assert!(r.metadata().is_none());
        assert!(r.opaque_ref().is_none());
        assert_eq!(r.pts(), Frame::new().unwrap().as_ref().pts());
        assert_eq!((r.pixel_format(), r.width(), r.height()), (YUV420P, 10, 6));
        assert!(r.hw_frames_context().is_none());
        assert!(r.is_writable());
        let mut copied = software(YUV420P, 16, 8);
        // SAFETY: `src` is a hardware frame of the fake context.
        unsafe { copied.as_mut().transfer_data_from(src.as_ref()) }.unwrap();
        assert_eq!(
            hw::image(&dst, YUV420P, 10, 6),
            hw::image(&copied, YUV420P, 10, 6)
        );
        // Rows past the copied ones, and bytes past each copied row's line,
        // are the zeroes the buffers were given.
        let luma = r.plane(0).unwrap();
        let ls = r.linesizes()[0] as usize;
        assert!(luma[6 * ls..7 * ls].iter().all(|&b| b == 0));
        assert_ne!(luma[..10], [0; 10]);
    }

    #[test]
    fn download_into_new_buffers_picks_the_format_as_c_does() {
        let ctx = hw::context(&hw::GRAY_FIRST, 16, 8);
        let src = hw::hw_frame(&ctx, 4);
        let mut dst = Frame::new().unwrap();
        // SAFETY: `src` is a hardware frame of the fake context.
        unsafe { dst.as_mut().transfer_data_from(src.as_ref()) }.unwrap();
        assert_eq!(dst.as_ref().pixel_format(), GRAY8);
        let mut dst = Frame::new().unwrap();
        dst.as_mut().set_pixel_format(YUV420P);
        // SAFETY: as above.
        unsafe { dst.as_mut().transfer_data_from(src.as_ref()) }.unwrap();
        assert_eq!(dst.as_ref().pixel_format(), YUV420P);
        assert_eq!(hw::image(&dst, YUV420P, 16, 8), hw::surface(&src));
    }

    #[test]
    fn download_into_new_buffers_leaves_the_frame_on_error() {
        for (behaviour, format, err) in [
            (&hw::EMPTY, PixelFormat::NONE, EINVAL),
            (&hw::LISTING_FAILS, PixelFormat::NONE, ENOMEM),
            (&hw::YUV, PixelFormat::RGB24, EINVAL),
            (&hw::YUV, PixelFormat::VAAPI, EINVAL),
            (&hw::TRANSFER_FAILS, PixelFormat::NONE, -5),
        ] {
            let ctx = hw::context(behaviour, 16, 8);
            let src = hw::hw_frame(&ctx, 5);
            let mut dst = Frame::new().unwrap();
            dst.as_mut().set_pixel_format(format);
            dst.as_mut().set_pts(11);
            // SAFETY: `src` is a hardware frame of the fake context.
            let ret = unsafe { dst.as_mut().transfer_data_from(src.as_ref()) };
            assert_eq!(ret, Err(err), "{format:?}");
            assert_eq!(dst.as_ref().pts(), 11);
            assert_eq!(dst.as_ref().pixel_format(), format);
            assert!(dst.as_ref().buffer(0).is_none());
        }
    }

    /// Runs the calling test alone in a child process, so that it may change
    /// process-wide state (`av_max_alloc`); true in the child.
    fn hwcontext_isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_HWCONTEXT_ISOLATED";
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

    /// When the frame receiving new buffers cannot be allocated, the
    /// transfer fails with `AVERROR(ENOMEM)` as C's does, and the frame is
    /// left as it was.
    #[test]
    fn download_into_new_buffers_out_of_memory() {
        if !hwcontext_isolated(
            "libavutil::hwcontext::unit_tests::download_into_new_buffers_out_of_memory",
        ) {
            return;
        }
        let ctx = hw::context(&hw::YUV, 16, 8);
        let src = hw::hw_frame(&ctx, 1);
        let mut dst = Frame::new().unwrap();
        dst.as_mut().set_pixel_format(YUV420P);
        dst.as_mut().set_pts(3);
        let c_dst = Frame::new().unwrap();
        // SAFETY: a fresh frame C owns through `c_dst`, given a format.
        unsafe { (*c_dst.as_c_ptr()).format = YUV420P.into_raw().0 };
        // An `AVFrame` takes more than this; every other allocation is done.
        assert!(size_of::<ffi::AVFrame>() > 256);
        crate::libavutil::mem::set_max_alloc(256);
        // SAFETY: `src` is a hardware frame of the fake context.
        let ret = unsafe { dst.as_mut().transfer_data_from(src.as_ref()) };
        // SAFETY: as above, through C's own routine.
        let c_ret = unsafe { ffi::av_hwframe_transfer_data(c_dst.as_c_ptr(), src.as_c_ptr(), 0) };
        crate::libavutil::mem::set_max_alloc(c_int::MAX as usize);
        assert_eq!(ret, Err(ENOMEM));
        assert_eq!(c_ret, ENOMEM);
        assert_eq!(dst.as_ref().pts(), 3);
        assert!(dst.as_ref().buffer(0).is_none());
    }

    #[test]
    fn inert_backends_are_refused_before_c_calls_their_null_transfers() {
        let inert = hw::inert_context();
        let src = hw::hw_frame(&inert, 6);
        let mut sw = software(YUV420P, 16, 8);
        let mut clean = Frame::new().unwrap();
        clean.as_mut().set_pixel_format(YUV420P);
        let mut hw_dst = hw::hw_frame(&inert, 7);
        let yuv = hw::context(&hw::YUV, 16, 8);
        let mut other = hw::hw_frame(&yuv, 8);
        // SAFETY: every hardware frame is one of its fake context.
        unsafe {
            assert_eq!(sw.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS));
            assert_eq!(clean.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS));
            assert_eq!(hw_dst.as_mut().transfer_data_from(sw.as_ref()), Err(ENOSYS));
            assert_eq!(
                hw_dst.as_mut().transfer_data_from(other.as_ref()),
                Err(ENOSYS)
            );
            assert_eq!(other.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS));
        }
    }
    /// Wave 1, batch 1: names, the backend list, map flags, and the device
    /// and frames contexts and hardware frames over the test backends of
    /// `tests/common/hwbackend.rs`, adopted through the crate's seams.
    mod contexts {
        use super::*;
        use core::ffi::CStr;
        use core::ptr::{addr_of, addr_of_mut};

        use crate::libavutil::buffer::MAX_REFERENCES;
        use crate::libavutil::dict::Dict;
        use crate::libavutil::hwcontext::backend::{
            self as fx, Event, FramesHwctx, HWCONFIG_SIZE, Kind, averror,
        };

        const ENOSYS: c_int = averror(ffi::ENOSYS);
        const ENOMEM_: c_int = averror(ffi::ENOMEM);
        const EINVAL_: c_int = averror(ffi::EINVAL);

        /// The prefix of `AVBuffer`'s private layout (`buffer_internal.h`)
        /// up to its count.
        #[repr(C)]
        struct BufferCount {
            data: *mut u8,
            size: usize,
            refcount: c_uint,
        }

        /// Sets the count of `buf`'s buffer behind C's back; the caller
        /// restores the true count before any reference is released.
        fn set_count(buf: *const ffi::AVBufferRef, count: c_uint) {
            // SAFETY: a live reference, whose private buffer body starts
            // with the mirrored prefix; this test alone references it.
            unsafe {
                let body = addr_of!((*buf).buffer).read().cast::<BufferCount>();
                addr_of_mut!((*body).refcount).write(count);
            }
        }

        /// The count of `buf`'s buffer.
        fn count(buf: *const ffi::AVBufferRef) -> c_int {
            // SAFETY: a live reference.
            unsafe { fx::ref_count(buf) }
        }

        /// An initialised test device of `kind`, adopted as a box.
        fn device(kind: Kind) -> DeviceReferenceBox {
            // SAFETY: the only reference to an initialised test device
            // context, built as `av_hwdevice_ctx_alloc` builds one.
            unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap()
        }

        /// An uninitialised test device of `kind`.
        fn uninit_device(kind: Kind) -> UninitDevice {
            // SAFETY: the only reference to a fresh test device context.
            unsafe { UninitDevice::from_c(fx::alloc_device(kind)) }.unwrap()
        }

        /// A frames context of `kind` on `device`, filled with
        /// `fx::fill_frames` and `hwctx`, initialised through the wrapper.
        fn frames_with(
            device: &DeviceReferenceBox,
            kind: Kind,
            hwctx: FramesHwctx,
            pool: c_int,
        ) -> Result<FramesReferenceBox, c_int> {
            let mut frames = device.as_ref().alloc_frames().unwrap();
            let p = frames.as_mut_ptr();
            // SAFETY: the uninitialised context `frames` owns.
            unsafe {
                fx::fill_frames(p, kind, 64, 32, pool);
                fx::set_frames_hwctx(p, hwctx);
            }
            frames.init()
        }

        fn frames(device: &DeviceReferenceBox, kind: Kind) -> FramesReferenceBox {
            let frames = frames_with(device, kind, FramesHwctx::default(), 0).unwrap();
            assert_eq!(fx::take_events(), [Event::FramesInit(kind)]);
            frames
        }

        #[test]
        fn names_round_trip_and_unknown_names_are_none() {
            for (t, _) in NAMED {
                match t.name() {
                    None => assert_eq!(t, DeviceType::NONE),
                    Some(name) => assert_eq!(DeviceType::from_name(name), Some(t)),
                }
            }
            for raw in UNNAMED {
                assert_eq!(DeviceType::from_raw(ffi::AVHWDeviceType(raw)).name(), None);
            }
            assert_eq!(DeviceType::CUDA.name(), Some(c"cuda"));
            for unknown in [c"", c"none", c"Vaapi", c"vaapi ", c"cuda\t"] {
                assert_eq!(DeviceType::from_name(unknown), None, "{unknown:?}");
            }
        }

        /// The campaign builds compile no backend: the walk is empty, and
        /// stays so.
        #[test]
        fn no_backend_is_supported_in_this_build() {
            let mut types = DeviceType::supported();
            assert_eq!(types.next(), None);
            assert_eq!(types.next(), None);
            assert_eq!(format!("{types:?}"), "SupportedDeviceTypes { prev: None }");
            for (t, _) in NAMED {
                assert!(DeviceReference::alloc(t).is_none(), "{t:?}");
                assert_eq!(
                    DeviceReference::create(t, None, &Dict::new()).err(),
                    Some(ENOMEM_),
                    "{t:?}"
                );
            }
        }

        #[test]
        fn map_flags_are_the_c_bits() {
            assert_eq!(MapFlags::READ.bits(), 1);
            assert_eq!(MapFlags::WRITE.bits(), 2);
            assert_eq!(MapFlags::OVERWRITE.bits(), 4);
            assert_eq!(MapFlags::DIRECT.bits(), 8);
            let rw = MapFlags::READ | MapFlags::WRITE;
            assert!(rw.contains(MapFlags::READ) && !rw.contains(MapFlags::DIRECT));
            assert_eq!(rw - MapFlags::READ, MapFlags::WRITE);
            assert_eq!(format!("{rw:?}"), "MapFlags(READ | WRITE)");
            assert_eq!(
                format!("{:?}", MapFlags::from_bits_retain(0x30)),
                "MapFlags(0x30)"
            );
            assert_eq!(MapFlags::default(), MapFlags::empty());
        }

        #[test]
        fn uninit_device_inits_once_and_releases_on_drop() {
            assert!(
                // SAFETY: NULL adopts nothing.
                unsafe { UninitDevice::from_c(ptr::null_mut()) }.is_none()
            );
            let mut uninit = uninit_device(Kind::A);
            assert_eq!(format!("{uninit:?}"), "UninitDevice(VAAPI)");
            let ctx = uninit.as_mut_ptr();
            // SAFETY: the public struct of the context `uninit` owns.
            unsafe {
                assert_eq!((*ctx).type_, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI);
                assert!(!(*ctx).hwctx.is_null());
            }
            assert!(fx::take_events().is_empty());
            let device = uninit.init().unwrap();
            assert_eq!(fx::take_events(), [Event::DeviceInit(Kind::A)]);
            assert_eq!(count(device.as_c_ptr()), 1);
            drop(device);
            assert_eq!(
                fx::take_events(),
                [Event::DeviceUninit(Kind::A), Event::DeviceFreed(Kind::A)]
            );
            // Never initialised: released as C releases a failed one.
            drop(uninit_device(Kind::B));
            assert_eq!(
                fx::take_events(),
                [Event::DeviceUninit(Kind::B), Event::DeviceFreed(Kind::B)]
            );
        }

        #[test]
        fn failed_device_init_releases_the_context() {
            let mut uninit = uninit_device(Kind::B);
            let ctx = uninit.as_mut_ptr();
            // SAFETY: the backend struct of the context `uninit` owns.
            unsafe { (*(*ctx).hwctx.cast::<fx::DeviceHwctx>()).init_ret = -77 };
            assert_eq!(uninit.init().err(), Some(-77));
            assert_eq!(
                fx::take_events(),
                [
                    Event::DeviceInit(Kind::B),
                    Event::DeviceUninit(Kind::B),
                    Event::DeviceFreed(Kind::B)
                ]
            );
        }

        #[test]
        fn device_clones_share_the_context_and_stop_at_the_limit() {
            let a = device(Kind::A);
            let b = a.clone();
            assert!(a.as_ref().ptr_eq(b.as_ref()));
            assert!(!a.as_ref().ptr_eq(device(Kind::A).as_ref()));
            assert_eq!(
                fx::take_events(),
                [Event::DeviceUninit(Kind::A), Event::DeviceFreed(Kind::A)]
            );
            assert_eq!(count(a.as_c_ptr()), 2);
            drop(b);
            assert_eq!(count(a.as_c_ptr()), 1);
            set_count(a.as_c_ptr(), MAX_REFERENCES);
            assert!(a.as_ref().try_clone().is_none());
            assert!(a.as_ref().alloc_frames().is_none());
            assert_eq!(a.as_ref().derive(DeviceType::VAAPI).err(), Some(ENOMEM_));
            assert_eq!(
                a.as_ref()
                    .derive_with_options(DeviceType::VAAPI, &Dict::new())
                    .err(),
                Some(ENOMEM_)
            );
            set_count(a.as_c_ptr(), MAX_REFERENCES - 1);
            let last = a.as_ref().try_clone().unwrap();
            assert!(last.as_ref().try_clone().is_none());
            set_count(a.as_c_ptr(), 2);
            drop(last);
            assert!(fx::take_events().is_empty());
            drop(a);
            assert_eq!(
                fx::take_events(),
                [Event::DeviceUninit(Kind::A), Event::DeviceFreed(Kind::A)]
            );
        }

        /// A device derived from another finds the source of the asked type
        /// in its chain; any other type needs a backend, which this build
        /// lacks.
        #[test]
        fn derive_returns_the_device_of_the_type_in_the_chain() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            // SAFETY: two live test devices; `b` is not derived yet.
            unsafe { fx::set_source_device(b.as_c_ptr(), a.as_c_ptr()) };
            assert_eq!(count(a.as_c_ptr()), 2);
            let found = b.as_ref().derive(DeviceType::VAAPI).unwrap();
            assert!(found.as_ref().ptr_eq(a.as_ref()));
            assert_eq!(count(a.as_c_ptr()), 3);
            let options = Dict::from_entries([(c"k", c"v")]).unwrap();
            let itself = b
                .as_ref()
                .derive_with_options(DeviceType::DRM, &options)
                .unwrap();
            assert!(itself.as_ref().ptr_eq(b.as_ref()));
            assert_eq!(a.as_ref().derive(DeviceType::DRM).err(), Some(ENOMEM_));
            assert_eq!(
                a.as_ref()
                    .derive_with_options(DeviceType::CUDA, &options)
                    .err(),
                Some(ENOMEM_)
            );
            // C references the chain's device unchecked: a result past the
            // limit is released.
            set_count(a.as_c_ptr(), MAX_REFERENCES);
            assert_eq!(b.as_ref().derive(DeviceType::VAAPI).err(), Some(ENOMEM_));
            assert_eq!(count(a.as_c_ptr()) as c_uint, MAX_REFERENCES);
            set_count(a.as_c_ptr(), 3);
            drop((found, itself, b));
            assert_eq!(count(a.as_c_ptr()), 1);
            drop(a);
            fx::take_events();
        }

        #[test]
        fn hw_configs_follow_the_backend() {
            let a = device(Kind::A);
            let mut config = a.as_ref().alloc_hw_config().unwrap();
            assert_eq!(config.device_type(), DeviceType::VAAPI);
            assert_eq!(config.as_ptr(), config.as_mut_ptr().cast_const());
            let start = config.as_ptr().cast::<u8>();
            // SAFETY: the backend's configuration size, zeroed by C.
            let bytes = unsafe { core::slice::from_raw_parts(start, HWCONFIG_SIZE) };
            assert!(bytes.iter().all(|&b| b == 0));
            assert_eq!(format!("{config:?}"), "HwConfig { device_type: VAAPI, .. }");
            drop(a);
            assert_eq!(
                fx::take_events(),
                [Event::DeviceUninit(Kind::A), Event::DeviceFreed(Kind::A)]
            );
            // The configuration outlives its device.
            drop(config);
            assert!(device(Kind::B).as_ref().alloc_hw_config().is_none());
            fx::take_events();
        }

        #[test]
        fn frames_init_checks_format_size_and_backend() {
            let a = device(Kind::A);
            fx::take_events();
            // Not filled in: no format.
            let uninit = a.as_ref().alloc_frames().unwrap();
            assert_eq!(format!("{uninit:?}"), "UninitFrames(..)");
            assert_eq!(count(a.as_c_ptr()), 2);
            assert_eq!(uninit.init().err(), Some(ENOSYS));
            assert_eq!(count(a.as_c_ptr()), 1);
            // C uninits a context it releases, initialised or not.
            assert_eq!(fx::take_events(), [Event::FramesUninit(Kind::A)]);
            // Another backend's format.
            let mut uninit = a.as_ref().alloc_frames().unwrap();
            // SAFETY: the uninitialised context `uninit` owns.
            unsafe { fx::fill_frames(uninit.as_mut_ptr(), Kind::B, 8, 8, 0) };
            assert_eq!(uninit.init().err(), Some(ENOSYS));
            // A size C refuses.
            let mut uninit = a.as_ref().alloc_frames().unwrap();
            // SAFETY: as above.
            unsafe { fx::fill_frames(uninit.as_mut_ptr(), Kind::A, 0, 8, 0) };
            assert_eq!(uninit.init().err(), Some(EINVAL_));
            assert_eq!(
                fx::take_events(),
                [Event::FramesUninit(Kind::A), Event::FramesUninit(Kind::A)]
            );
            // The backend's failure.
            let failing = FramesHwctx {
                init_ret: -9,
                ..FramesHwctx::default()
            };
            assert_eq!(frames_with(&a, Kind::A, failing, 0).err(), Some(-9));
            assert_eq!(
                fx::take_events(),
                [Event::FramesInit(Kind::A), Event::FramesUninit(Kind::A)]
            );
            // Preallocation runs `get_buffer` for each frame.
            let frames = frames_with(&a, Kind::A, FramesHwctx::default(), 3).unwrap();
            assert_eq!(
                fx::take_events(),
                [
                    Event::FramesInit(Kind::A),
                    Event::GetBuffer(Kind::A),
                    Event::GetBuffer(Kind::A),
                    Event::GetBuffer(Kind::A)
                ]
            );
            assert_eq!(count(frames.as_c_ptr()), 1);
            // A preallocation failure fails the init.
            let failing = FramesHwctx {
                get_buffer_ret: -4,
                ..FramesHwctx::default()
            };
            assert_eq!(frames_with(&a, Kind::A, failing, 2).err(), Some(-4));
            assert_eq!(
                fx::take_events(),
                [
                    Event::FramesInit(Kind::A),
                    Event::GetBuffer(Kind::A),
                    Event::FramesUninit(Kind::A)
                ]
            );
            drop((frames, a));
            fx::take_events();
        }

        #[test]
        fn hardware_frames_hold_their_context_and_outlive_its_box() {
            let a = device(Kind::A);
            let frames = frames(&a, Kind::A);
            let frame = frames.as_ref().get_buffer().unwrap();
            assert_eq!(fx::take_events(), [Event::GetBuffer(Kind::A)]);
            assert_eq!(frame.format(), PixelFormat::VAAPI);
            assert_eq!((frame.width(), frame.height()), (64, 32));
            assert!(frame.frames().ptr_eq(frames.as_ref()));
            assert_eq!(count(frames.as_c_ptr()), 2);
            let copy = frames.clone();
            assert!(copy.as_ref().ptr_eq(frame.frames()));
            assert_eq!(count(frames.as_c_ptr()), 3);
            drop(copy);
            assert_eq!(
                format!("{frame:?}"),
                "HardwareFrame { format: VAAPI, width: 64, height: 32, .. }"
            );
            // SAFETY: a read of the frame's fields, which C filled.
            let shape = unsafe { fx::shape(frame.frame.as_c_ptr()) };
            assert!(shape.bufs[0] && shape.data[3] && shape.has_frames && shape.own_extended);
            // The parents go first; the frame keeps them.
            drop((frames, a));
            assert!(fx::take_events().is_empty());
            let again = frame.frames().get_buffer().unwrap();
            drop(frame);
            drop(again);
            assert_eq!(
                fx::take_events(),
                [
                    Event::GetBuffer(Kind::A),
                    Event::FramesUninit(Kind::A),
                    Event::DeviceUninit(Kind::A),
                    Event::DeviceFreed(Kind::A)
                ]
            );
        }

        #[test]
        fn get_buffer_reports_c_failures() {
            let a = device(Kind::A);
            let failing = FramesHwctx {
                get_buffer_ret: -12345,
                ..FramesHwctx::default()
            };
            let frames = frames_with(&a, Kind::A, failing, 0).unwrap();
            assert_eq!(frames.as_ref().get_buffer().err(), Some(-12345));
            assert_eq!(count(frames.as_c_ptr()), 1);
            let poolless = FramesHwctx {
                no_pool: 1,
                ..FramesHwctx::default()
            };
            let frames = frames_with(&a, Kind::A, poolless, 0).unwrap();
            assert_eq!(frames.as_ref().get_buffer().err(), Some(EINVAL_));
            set_count(frames.as_c_ptr(), MAX_REFERENCES);
            assert_eq!(frames.as_ref().get_buffer().err(), Some(ENOMEM_));
            set_count(frames.as_c_ptr(), 1);
            drop((frames, a));
            fx::take_events();
        }

        #[test]
        fn maps_to_memory_hold_the_source_frame() {
            let a = device(Kind::A);
            let frames = frames(&a, Kind::A);
            let frame = frames.as_ref().get_buffer().unwrap();
            fx::take_events();
            let mapped = frame.map(PixelFormat::NONE, MapFlags::READ).unwrap();
            assert_eq!(fx::take_events(), [Event::MapFrom(Kind::A)]);
            assert_eq!(mapped.format(), PixelFormat::NV12);
            assert_eq!((mapped.width(), mapped.height()), (64, 32));
            assert_eq!(
                format!("{mapped:?}"),
                "MappedFrame { format: NV12, width: 64, height: 32, .. }"
            );
            let explicit = frame
                .map(PixelFormat::NV12, MapFlags::READ | MapFlags::WRITE)
                .unwrap();
            assert_eq!(
                frame.map(PixelFormat::YUV420P, MapFlags::empty()).err(),
                Some(ENOSYS)
            );
            assert_eq!(
                fx::take_events(),
                [Event::MapFrom(Kind::A), Event::MapFrom(Kind::A)]
            );
            // The mappings keep the frame, its context and device alive.
            drop((frame, frames, a, explicit));
            assert_eq!(fx::take_events(), [Event::Unmap(Kind::A)]);
            drop(mapped);
            assert_eq!(
                fx::take_events(),
                [
                    Event::Unmap(Kind::A),
                    Event::FramesUninit(Kind::A),
                    Event::DeviceUninit(Kind::A),
                    Event::DeviceFreed(Kind::A)
                ]
            );
        }

        #[test]
        fn maps_refuse_the_own_context_and_wrapping_counts() {
            let a = device(Kind::A);
            let frames = frames(&a, Kind::A);
            let frame = frames.as_ref().get_buffer().unwrap();
            assert_eq!(
                frame.map_to(frames.as_ref(), MapFlags::READ).err(),
                Some(EINVAL_)
            );
            assert_eq!(
                frame.map_to(frame.frames(), MapFlags::READ).err(),
                Some(EINVAL_)
            );
            // SAFETY: the frame's first buffer, a live reference it owns.
            let surface = unsafe { addr_of!((*frame.frame.as_c_ptr()).buf[0]).read() };
            set_count(surface, MAX_REFERENCES);
            assert_eq!(
                frame.map(PixelFormat::NONE, MapFlags::READ).err(),
                Some(ENOMEM_)
            );
            set_count(surface, 1);
            let failing = FramesHwctx {
                map_ret: -3,
                ..FramesHwctx::default()
            };
            let other = frames_with(&a, Kind::A, failing, 0).unwrap();
            let failing_frame = other.as_ref().get_buffer().unwrap();
            fx::take_events();
            assert_eq!(
                failing_frame.map(PixelFormat::NONE, MapFlags::READ).err(),
                Some(-3)
            );
            // A's `map_from` fails; no `map_to` is tried after a real error.
            assert_eq!(
                failing_frame.map_to(frames.as_ref(), MapFlags::READ).err(),
                Some(-3)
            );
            assert_eq!(
                fx::take_events(),
                [Event::MapFrom(Kind::A), Event::MapFrom(Kind::A)]
            );
            drop((failing_frame, other, frame, frames, a));
            fx::take_events();
        }

        #[test]
        fn maps_into_another_context_and_back() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            let fa = frames(&a, Kind::A);
            let fb = frames(&b, Kind::B);
            let frame = fa.as_ref().get_buffer().unwrap();
            fx::take_events();
            // SAFETY: the frame's first buffer, a live reference it owns.
            let surface = unsafe { addr_of!((*frame.frame.as_c_ptr()).buf[0]).read() };
            set_count(surface, MAX_REFERENCES);
            assert_eq!(
                frame.map_to(fb.as_ref(), MapFlags::READ).err(),
                Some(ENOMEM_)
            );
            set_count(surface, 1);
            assert!(fx::take_events().is_empty());
            // A maps the frame into B's context itself.
            let into_b = frame.map_to(fb.as_ref(), MapFlags::READ).unwrap();
            assert_eq!(fx::take_events(), [Event::MapFrom(Kind::A)]);
            assert_eq!(into_b.format(), PixelFormat::DRM_PRIME);
            assert!(into_b.frames().ptr_eq(fb.as_ref()));
            // When A declines, B's `map_to` maps it.
            let declining = FramesHwctx {
                map_ret: ENOSYS,
                ..FramesHwctx::default()
            };
            let fa2 = frames_with(&a, Kind::A, declining, 0).unwrap();
            let frame2 = fa2.as_ref().get_buffer().unwrap();
            fx::take_events();
            let into_b2 = frame2.map_to(fb.as_ref(), MapFlags::WRITE).unwrap();
            assert_eq!(
                fx::take_events(),
                [Event::MapFrom(Kind::A), Event::MapTo(Kind::B)]
            );
            // B maps nothing out of its own frames.
            assert_eq!(
                into_b.map(PixelFormat::NONE, MapFlags::READ).err(),
                Some(ENOSYS)
            );
            assert_eq!(
                into_b.map_to(fa.as_ref(), MapFlags::READ).err(),
                Some(ENOSYS)
            );
            assert!(fx::take_events().is_empty());
            drop((into_b, into_b2, frame, frame2, fa, fa2, fb, a, b));
            fx::take_events();
        }

        #[test]
        fn derived_frames_map_their_allocations_and_unmap() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            let fa = frames(&a, Kind::A);
            let derived = fa
                .as_ref()
                .derive(
                    PixelFormat::DRM_PRIME,
                    b.as_ref(),
                    MapFlags::READ | MapFlags::WRITE,
                )
                .unwrap();
            assert_eq!(fx::take_events(), [Event::DeriveFrom(Kind::A)]);
            assert!(!derived.as_ref().ptr_eq(fa.as_ref()));
            assert_eq!(count(fa.as_c_ptr()), 2);
            // Allocated in A's context, mapped into B's.
            let frame = derived.as_ref().get_buffer().unwrap();
            assert_eq!(
                fx::take_events(),
                [Event::GetBuffer(Kind::A), Event::MapFrom(Kind::A)]
            );
            assert_eq!(frame.format(), PixelFormat::DRM_PRIME);
            assert!(frame.frames().ptr_eq(derived.as_ref()));
            // Mapping it back into A's context unmaps it: the source frame.
            let source = frame.map_to(fa.as_ref(), MapFlags::READ).unwrap();
            assert!(fx::take_events().is_empty());
            assert!(source.frames().ptr_eq(fa.as_ref()));
            assert_eq!(source.format(), PixelFormat::VAAPI);
            // Deriving the derived context back onto A's device unmaps it.
            let back = derived
                .as_ref()
                .derive(PixelFormat::VAAPI, a.as_ref(), MapFlags::empty())
                .unwrap();
            assert!(back.as_ref().ptr_eq(fa.as_ref()));
            assert!(fx::take_events().is_empty());
            // ...and past the limit, that reference is released.
            set_count(fa.as_c_ptr(), MAX_REFERENCES);
            assert_eq!(
                derived
                    .as_ref()
                    .derive(PixelFormat::VAAPI, a.as_ref(), MapFlags::empty())
                    .err(),
                Some(ENOMEM_)
            );
            set_count(fa.as_c_ptr(), MAX_REFERENCES);
            assert_eq!(
                fa.as_ref()
                    .derive(PixelFormat::DRM_PRIME, b.as_ref(), MapFlags::empty())
                    .err(),
                Some(ENOMEM_)
            );
            // The box, the derived context's source, the mapping's source
            // frame, the unmapped frame and the unmapping's reference.
            set_count(fa.as_c_ptr(), 5);
            drop((source, frame));
            // The mapping was made in the derived context, of kind B.
            assert_eq!(fx::take_events(), [Event::Unmap(Kind::B)]);
            drop((back, derived));
            assert_eq!(fx::take_events(), [Event::FramesUninit(Kind::B)]);
            assert_eq!(count(fa.as_c_ptr()), 1);
            drop((fa, a, b));
            fx::take_events();
        }

        #[test]
        fn frames_derivations_fall_back_and_fail_as_the_backends_say() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            let declining = FramesHwctx {
                derive_ret: ENOSYS,
                ..FramesHwctx::default()
            };
            let fa = frames_with(&a, Kind::A, declining, 0).unwrap();
            fx::take_events();
            let derived = fa
                .as_ref()
                .derive(PixelFormat::DRM_PRIME, b.as_ref(), MapFlags::READ)
                .unwrap();
            assert_eq!(
                fx::take_events(),
                [Event::DeriveFrom(Kind::A), Event::DeriveTo(Kind::B)]
            );
            // A context neither backend derives is still made: C leaves the
            // mapping to each allocation.
            let derived2 = fa
                .as_ref()
                .derive(PixelFormat::VAAPI, a.as_ref(), MapFlags::READ)
                .unwrap();
            assert_eq!(fx::take_events(), [Event::DeriveFrom(Kind::A)]);
            let failing = FramesHwctx {
                derive_ret: -21,
                ..FramesHwctx::default()
            };
            let fa3 = frames_with(&a, Kind::A, failing, 0).unwrap();
            fx::take_events();
            assert_eq!(
                fa3.as_ref()
                    .derive(PixelFormat::DRM_PRIME, b.as_ref(), MapFlags::READ)
                    .err(),
                Some(-21)
            );
            assert_eq!(
                fx::take_events(),
                [Event::DeriveFrom(Kind::A), Event::FramesUninit(Kind::B)]
            );
            set_count(b.as_c_ptr(), MAX_REFERENCES);
            assert_eq!(
                fa.as_ref()
                    .derive(PixelFormat::DRM_PRIME, b.as_ref(), MapFlags::READ)
                    .err(),
                Some(ENOMEM_)
            );
            set_count(b.as_c_ptr(), 2);
            drop((derived, derived2, fa, fa3, a, b));
            fx::take_events();
        }

        thread_local! {
            static FREED: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
        }

        /// A frames context's `free` callback: counts the call and checks
        /// the user data it was given.
        unsafe extern "C" fn count_free(ctx: *mut ffi::AVHWFramesContext) {
            // SAFETY: C passes the live context being released.
            let opaque = unsafe { (*ctx).user_opaque };
            assert_eq!(opaque.addr(), 0x5eed);
            FREED.with(|f| f.set(f.get() + 1));
        }

        /// C runs the `free` filled in through the seam once, after the
        /// backend's uninit, when the last reference goes.
        #[test]
        fn frames_free_callbacks_run_at_the_last_release() {
            let a = device(Kind::A);
            let mut uninit = a.as_ref().alloc_frames().unwrap();
            let p = uninit.as_mut_ptr();
            // SAFETY: the uninitialised context `uninit` owns.
            unsafe {
                fx::fill_frames(p, Kind::A, 4, 4, 0);
                (*p).free = Some(count_free);
                (*p).user_opaque = ptr::without_provenance_mut(0x5eed);
            }
            let frames = uninit.init().unwrap();
            let frame = frames.as_ref().get_buffer().unwrap();
            drop(frames);
            assert_eq!(FREED.with(core::cell::Cell::get), 0);
            fx::take_events();
            drop(frame);
            assert_eq!(FREED.with(core::cell::Cell::get), 1);
            assert_eq!(fx::take_events(), [Event::FramesUninit(Kind::A)]);
            drop(a);
            fx::take_events();
        }

        /// A derived context reports its source's allocation failure, and
        /// a target backend's mapping failure ends the mapping.
        #[test]
        fn derived_allocations_and_target_mappings_report_failures() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            let failing = FramesHwctx {
                get_buffer_ret: -6,
                map_ret: ENOSYS,
                ..FramesHwctx::default()
            };
            let fa = frames_with(&a, Kind::A, failing, 0).unwrap();
            let derived = fa
                .as_ref()
                .derive(PixelFormat::DRM_PRIME, b.as_ref(), MapFlags::READ)
                .unwrap();
            fx::take_events();
            assert_eq!(derived.as_ref().get_buffer().err(), Some(-6));
            assert_eq!(fx::take_events(), [Event::GetBuffer(Kind::A)]);
            let refusing = FramesHwctx {
                map_ret: -7,
                ..FramesHwctx::default()
            };
            let fb = frames_with(&b, Kind::B, refusing, 0).unwrap();
            let fa2 = frames_with(
                &a,
                Kind::A,
                FramesHwctx {
                    map_ret: ENOSYS,
                    ..FramesHwctx::default()
                },
                0,
            )
            .unwrap();
            let frame = fa2.as_ref().get_buffer().unwrap();
            fx::take_events();
            assert_eq!(frame.map_to(fb.as_ref(), MapFlags::READ).err(), Some(-7));
            assert_eq!(
                fx::take_events(),
                [Event::MapFrom(Kind::A), Event::MapTo(Kind::B)]
            );
            drop((frame, fa2, fb, derived, fa, a, b));
            fx::take_events();
        }

        #[test]
        fn name_lookups_take_any_c_string() {
            let long = std::ffi::CString::new("x".repeat(4096)).unwrap();
            assert_eq!(DeviceType::from_name(&long), None);
            let names: Vec<&CStr> = NAMED.iter().filter_map(|&(t, _)| t.name()).collect();
            assert_eq!(names.len(), 14);
        }
    }
}
