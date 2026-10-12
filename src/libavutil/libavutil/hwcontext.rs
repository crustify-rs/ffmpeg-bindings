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

use core::ffi::c_int;
use core::fmt;
use core::ptr::{self, addr_of};

use ffibox::CPlainElem;

use super::buffer::{BufferReferenceRef, ENOMEM};
use super::ffi;
use super::frame::{Frame, FrameBox, FrameMut, FrameRef, PALETTE_SIZE, Rows, image_reach};
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

#[cfg(test)]
#[path = "../tests/common/hwframes.rs"]
mod fixture;

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
}
