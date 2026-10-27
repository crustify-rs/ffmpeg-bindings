//! Safe wrappers over `libavutil/hwcontext.h`.
//!
//! [`DeviceType`] is `AVHWDeviceType`, the hardware API a device context
//! drives, and [`TransferDirection`] is `AVHWFrameTransferDirection`, which
//! side of a transfer between hardware and software frames a format query
//! asks about. Both are plain `Copy` values.
//!
//! Frames move between hardware and software through
//! `av_hwframe_transfer_data`, in the software formats a hardware frames
//! context lists for each direction ([`FramesReferenceRef::transfer_formats`],
//! or [`FrameRef::transfer_formats`] for the context a frame holds;
//! `av_hwframe_transfer_get_formats`). A [`HardwareFrame`] downloads into a
//! software frame ([`HardwareFrame::download_to`]), uploads from one
//! ([`HardwareFrame::upload_from`]) or copies another hardware frame
//! ([`HardwareFrame::copy_from`]); Rust checks the software side against its
//! context. [`FrameMut::transfer_data_from`] is the same transfer over plain
//! frames, `unsafe`, for hardware frames C hands over as a [`Frame`] (a
//! hardware decoder's), which only their caller can vouch for.
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
//!
//! A device context's own fields are [`DeviceContext`]'s, reached through
//! [`DeviceReferenceRef::context`] once initialised and
//! [`UninitDevice::context_mut`] while it is filled in; its constraints on
//! frames are a [`FramesConstraintsBox`]
//! ([`DeviceReferenceRef::frames_constraints`]). A frames context's own
//! fields are [`FramesContext`]'s, reached likewise through
//! [`FramesReferenceRef::context`] and [`UninitFrames::context_mut`].

use core::any::Any;
use core::ffi::{CStr, c_int, c_void};
use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ops::{BitAnd, BitOr, BitOrAssign, Sub};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{
    CBox, CCell, CDrop, CDupClone, CMutSeam, CNew, CPlainElem, CRefSeam, CSlice, define_ctype,
};

use super::buffer::{
    BufferPoolBox, BufferPoolRef, BufferReferenceRef, ENOMEM, MAX_REFERENCES, may_add_reference,
};
use super::dict::Dict;
use super::ffi;
use super::frame::{
    Frame, FrameBox, FrameMut, FrameRef, PALETTE_SIZE, Rows, image_reach, int_flags,
};
use super::log::ClassRef;
use super::mem::{HeapVec, alloc_array_from_fn};
use super::opt::impl_class_object;
use super::pixdesc::{PixelFormat, PixelFormatFlags};

/// Wraps: `AVHWDeviceType`
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceType;
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
    /// AMD Advanced Media Framework (`amf`).
    pub const AMF: Self = Self(ffi::AV_HWDEVICE_TYPE_AMF);
    /// NVIDIA CUDA (`cuda`).
    pub const CUDA: Self = Self(ffi::AV_HWDEVICE_TYPE_CUDA);
    /// Direct3D 11 video acceleration (`d3d11va`).
    pub const D3D11VA: Self = Self(ffi::AV_HWDEVICE_TYPE_D3D11VA);
    /// Direct3D 12 video acceleration (`d3d12va`).
    pub const D3D12VA: Self = Self(ffi::AV_HWDEVICE_TYPE_D3D12VA);
    /// Linux DRM PRIME buffers (`drm`).
    pub const DRM: Self = Self(ffi::AV_HWDEVICE_TYPE_DRM);
    /// DirectX Video Acceleration 2 (`dxva2`).
    pub const DXVA2: Self = Self(ffi::AV_HWDEVICE_TYPE_DXVA2);
    /// Android MediaCodec (`mediacodec`).
    pub const MEDIACODEC: Self = Self(ffi::AV_HWDEVICE_TYPE_MEDIACODEC);
    /// No hardware device; what C's lookups return when they find none.
    pub const NONE: Self = Self(ffi::AV_HWDEVICE_TYPE_NONE);
    /// OpenHarmony codec device (`ohcodec`).
    pub const OHCODEC: Self = Self(ffi::AV_HWDEVICE_TYPE_OHCODEC);
    /// OpenCL (`opencl`).
    pub const OPENCL: Self = Self(ffi::AV_HWDEVICE_TYPE_OPENCL);
    /// Intel Quick Sync Video (`qsv`).
    pub const QSV: Self = Self(ffi::AV_HWDEVICE_TYPE_QSV);
    /// VA-API (`vaapi`).
    pub const VAAPI: Self = Self(ffi::AV_HWDEVICE_TYPE_VAAPI);
    /// NVIDIA VDPAU (`vdpau`).
    pub const VDPAU: Self = Self(ffi::AV_HWDEVICE_TYPE_VDPAU);
    /// Apple `VideoToolbox` (`videotoolbox`).
    pub const VIDEOTOOLBOX: Self = Self(ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX);
    /// Vulkan (`vulkan`).
    pub const VULKAN: Self = Self(ffi::AV_HWDEVICE_TYPE_VULKAN);

    /// The device type C encodes as `raw`, whatever its value.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVHWDeviceType) -> Self {
        Self(raw)
    }

    /// The C encoding of this device type.
    #[inline]
    #[must_use]
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
            None => f.debug_tuple("DeviceType").field(&self.0).finish(),
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

/// Wraps: `AVHWFrameTransferDirection`
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
/// use ffmpeg_bindings::libavutil::libavutil::{hwcontext::TransferDirection, mem::alloc_array_zeroed};
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
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVHWFrameTransferDirection {
        match self {
            Self::From => ffi::AV_HWFRAME_TRANSFER_DIRECTION_FROM,
            Self::To => ffi::AV_HWFRAME_TRANSFER_DIRECTION_TO,
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
const EINVAL: c_int = -(ffi::EINVAL.cast_signed());

/// A reference to an initialised hardware frames context: one
/// `av_hwframe_ctx_alloc` made and `av_hwframe_ctx_init` set up, whose
/// bytes are its `AVHWFramesContext` and which nothing writes while the
/// reference is borrowed.
// A private view of a frames context reference as the buffer reference a
// frame holds; built only from a frame's `hw_frames_ctx` (which `Frame`'s
// invariant makes such a reference), from a `FramesReferenceRef`, and from
// callers of the unsafe entry points who vouch for one.
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
        // initialised hardware frames context, whose public fields nothing
        // writes.
        frame.hw_frames_context().map(|frames| frames.hw_frames())
    }

    /// The context's allocated surface size and software format.
    fn geometry(self) -> FramesGeometry {
        let data = self.0.parts().0.as_ptr();
        // SAFETY: by the type's invariant the reference's data is a live,
        // initialised `AVHWFramesContext` (C allocates it as the leading
        // member of its private context, aligned by `av_mallocz`), which
        // nothing writes while the reference is borrowed.
        let ctx = unsafe { FramesContextRef::from_ptr(data.cast()) }
            .expect("a frames context reference has data");
        FramesGeometry {
            sw_format: ctx.sw_format(),
            width: ctx.width(),
            height: ctx.height(),
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
                &raw mut list,
                0,
            )
        };
        if ret < 0 {
            // C leaves `list` unspecified on failure: nothing to free.
            return Err(ret);
        }
        let mut len = 0;
        if !list.is_null() {
            loop {
                // SAFETY: a successful call left a terminated array at `list`, and `len` has
                // not passed its terminator.
                let at = unsafe { list.add(len) };
                // SAFETY: as above; every element up to the terminator is initialised.
                if unsafe { at.read() } == ffi::AV_PIX_FMT_NONE {
                    break;
                }
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

impl FrameRef<'_> {
    /// Wraps: `av_hwframe_transfer_get_formats`
    ///
    /// The software pixel formats a transfer with this hardware frame can
    /// use, from its hardware frames context
    /// ([`hw_frames_context`](Self::hw_frames_context)): those its data can
    /// be downloaded into ([`TransferDirection::From`]) or uploaded from
    /// ([`TransferDirection::To`]), in the backend's order of preference. C
    /// promises at least one format on success. The list is the caller's.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::frame::Frame;
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::TransferDirection;
    /// let software = Frame::new().unwrap();
    /// assert!(
    ///     software
    ///         .as_ref()
    ///         .transfer_formats(TransferDirection::From)
    ///         .is_err()
    /// );
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
                linesize: linesize.cast_signed(),
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
    /// Wraps: `av_hwframe_transfer_data`
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
    /// changed, which is checked. `Frame` cannot tell a hardware frame from a
    /// software frame given a context; [`HardwareFrame`]'s transfers
    /// ([`download_to`](HardwareFrame::download_to),
    /// [`upload_from`](HardwareFrame::upload_from),
    /// [`copy_from`](HardwareFrame::copy_from)) are the safe forms for the
    /// hardware frames this crate allocates.
    // Remains for hardware frames C hands over as plain frames (hardware
    // decoders, filters), until their wrappers hand out `HardwareFrame`s.
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
    /// Wraps: `av_hwdevice_get_type_name`
    ///
    /// The name C gives this device type, as FFmpeg's options spell it
    /// (`cuda`, `vaapi`, ...); `None` for [`NONE`](Self::NONE) and any value
    /// C does not name.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceType;
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

    /// Wraps: `av_hwdevice_find_type_by_name`
    ///
    /// The device type C names `name` (see [`name`](Self::name)), or `None`
    /// for any other string. Names are matched exactly, case included:
    /// `hwcontext.h` calls the lookup case-insensitive, but C compares with
    /// `strcmp`.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceType;
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

    /// Wraps: `av_hwdevice_iterate_types`
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
        READ = ffi::AV_HWFRAME_MAP_READ,
        /// The mapping must be writable.
        WRITE = ffi::AV_HWFRAME_MAP_WRITE,
        /// The mapped frame will be overwritten whole, so its current data
        /// need not be loaded; what is not overwritten is unspecified.
        OVERWRITE = ffi::AV_HWFRAME_MAP_OVERWRITE,
        /// The mapping must be direct: no copy in the map or unmap steps.
        DIRECT = ffi::AV_HWFRAME_MAP_DIRECT,
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
    may_add_reference(count.cast_unsigned())
}

/// Whether a buffer counting `count` references is within
/// [`MAX_REFERENCES`]: C adds some references unchecked (to the device or
/// frames context it returns from a chain of derived ones), and a result
/// past the limit is released instead.
fn count_within_limit(count: c_int) -> bool {
    count.cast_unsigned() <= MAX_REFERENCES
}

/// Wraps: `av_buffer_unref`
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
        unsafe { ffi::av_buffer_unref(&raw mut slot) }
    }
}

/// Wraps: `av_buffer_ref`
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
        unsafe { ffi::av_buffer_unref(&raw mut slot) }
    }
}

/// Wraps: `av_buffer_ref`
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
    /// reference handle. The context itself is reached through
    /// [`context`](DeviceReferenceRef::context).
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceReferenceBox;
/// let _ = DeviceReferenceBox::new();
/// ```
///
/// Its handles are borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceReferenceBox;
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::DeviceReferenceBox;
/// fn send<T: Send>(_: T) {}
/// fn move_away(device: DeviceReferenceBox) {
///     send(device);
/// }
/// ```
pub type DeviceReferenceBox = CBox<DeviceReference, ContextUnref>;

impl DeviceReference {
    /// Wraps: `av_hwdevice_ctx_alloc`
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

    /// Wraps: `av_hwdevice_ctx_create`
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
                &raw mut out,
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

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the conventions' getters and setters take `&self` and `&mut self` on the handles"
)]
impl DeviceReferenceRef<'_> {
    /// The address of the device context, which identifies it.
    fn context_addr(&self) -> usize {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.as_ptr()).data) };
        // SAFETY: a read of the `data` field of a live reference.
        unsafe { data_ptr.read() }.addr()
    }

    /// The device type of the context, as C set it at allocation.
    // The public getter is AVHWDeviceContext's.
    #[expect(
        clippy::cast_ptr_alignment,
        reason = "the buffer's data is the `av_mallocz` block of the context, aligned for it"
    )]
    fn kind(&self) -> DeviceType {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.as_ptr()).data) };
        // SAFETY: a read of the `data` field of a live reference.
        let data = unsafe { data_ptr.read() };
        let ctx = data.cast::<ffi::AVHWDeviceContext>();
        // SAFETY: `ctx` is to a live object, as the access below relies on,
        // so the place of its `type_` field is in bounds; nothing is accessed here.
        let type_ptr = unsafe { addr_of!((*ctx).type_) };
        // SAFETY: by the type's invariant, `data` is the context's public
        // struct, whose `type` C never changes after allocation.
        DeviceType(unsafe { type_ptr.read() })
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

    /// Wraps: `av_hwdevice_ctx_create_derived`
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
                &raw mut out,
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

    /// Wraps: `av_hwdevice_ctx_create_derived_opts`
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
                &raw mut out,
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

    /// Wraps: `av_hwframe_ctx_alloc`
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

    /// Wraps: `av_hwdevice_hwconfig_alloc`
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
/// backend needs through [`context_mut`](Self::context_mut): `hwctx`, the
/// backend's device struct from its `hwcontext_<api>.h`
/// ([`DeviceContextMut::into_hwctx`]), and optionally a free callback and
/// user data; then [`init`](Self::init) it.
///
/// Hand-written: a [`DeviceReferenceBox`] whose reference handles are
/// withheld until the context is initialised, since every routine taking a
/// device needs an initialised one; only the context's own handles
/// ([`DeviceContextRef`], [`DeviceContextMut`]) are lent. Dropping it
/// releases the context, as C does with one that failed to initialise.
/// Neither `Send` nor `Sync`, as [`DeviceReference`]:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitDevice;
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
    /// as it does) that has no other reference and was not initialised, and
    /// whose `free` and `user_opaque` are unset, a backend's, or this
    /// crate's.
    #[must_use]
    pub unsafe fn from_c(ptr: *mut ffi::AVBufferRef) -> Option<Self> {
        // SAFETY: by the caller's contract; the box lends no reference
        // handle until `init` initialised the context.
        unsafe { DeviceReferenceBox::from_c(ptr) }.map(Self)
    }

    /// Gives the reference up, as the `AVBufferRef` C knows it by, still
    /// uninitialised.
    #[must_use = "the reference leaks unless C releases it"]
    pub fn into_c(self) -> *mut ffi::AVBufferRef {
        self.0.into_c()
    }

    /// The context, shared.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a device context reference has data.
    #[must_use]
    pub fn context(&self) -> DeviceContextRef<'_> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.0.as_c_ptr()).data) };
        // SAFETY: a read of the `data` field of the live reference the box
        // holds; its data is the context's public struct.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the reference keeps the context live for the borrow, and
        // nothing writes it except through `context_mut`, which the shared
        // borrow excludes.
        unsafe { DeviceContextRef::from_ptr(ctx.cast()) }
            .expect("a device context reference has data")
    }

    /// The context, exclusively, to fill in before [`init`](Self::init).
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a device context reference has data.
    #[must_use]
    pub fn context_mut(&mut self) -> DeviceContextMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.0.as_c_ptr()).data) };
        // SAFETY: as in `context`.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the box holds the context's only reference, so nothing else
        // reaches the context for the borrow.
        unsafe { DeviceContextMut::from_ptr(ctx.cast()) }
            .expect("a device context reference has data")
    }

    /// Wraps: `av_hwdevice_ctx_init`
    ///
    /// Finalizes the context once it is filled in: the backend checks and
    /// completes it (its `device_init`, when it has one). The initialised
    /// context can then be used and shared.
    ///
    /// # Errors
    ///
    /// The backend's error; the context is released then.
    ///
    /// # Safety
    ///
    /// The hardware context must hold what the device type's
    /// `hwcontext_<api>.h` requires before `av_hwdevice_ctx_init`: backends
    /// dereference the caller's device handles there unchecked (D3D11VA
    /// calls through its `device`, Vulkan through its function loader), so
    /// a context left as allocated is not enough.
    pub unsafe fn init(self) -> Result<DeviceReferenceBox, c_int> {
        // SAFETY: the only reference to an allocated device context, never
        // initialised before (`init` consumes it); C runs the backend's
        // `device_init` on it, which reads what the caller filled in, as the
        // caller vouches.
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceBox;
/// let _ = FramesReferenceBox::new();
/// ```
///
/// Its handles are borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceBox;
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesReferenceBox;
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

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "the conventions' getters and setters take `&self` and `&mut self` on the handles"
)]
impl<'a> FramesReferenceRef<'a> {
    /// The address of the frames context, which identifies it.
    fn context_addr(&self) -> usize {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.as_ptr()).data) };
        // SAFETY: a read of the `data` field of a live reference.
        unsafe { data_ptr.read() }.addr()
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

    /// The reference as the transfer checks view a frame's frames context.
    fn hw_frames(&self) -> HwFrames<'a> {
        // SAFETY: a live reference, for `'a`, to an initialised frames
        // context: C's `av_mallocz` block of its private context, wholly
        // initialised, whose public struct is the buffer's window, which
        // nothing writes any more and which C made read-only (so it is never
        // writable, nor an `UninitBuffer`). The view stays in the transfer
        // checks, which read the context's fields and hand it to C.
        let buf = unsafe { BufferReferenceRef::from_ptr(self.as_ptr().cast_mut()) };
        HwFrames(buf.expect("a handle's reference is not NULL"))
    }

    /// Wraps: `av_hwframe_transfer_get_formats`
    ///
    /// The software pixel formats a transfer with the context's hardware
    /// frames can use: those their data can be downloaded into
    /// ([`TransferDirection::From`]) or uploaded from
    /// ([`TransferDirection::To`]), in the backend's order of preference. C
    /// promises at least one format on success. The list is the caller's.
    ///
    /// # Errors
    ///
    /// The backend's error; `AVERROR(ENOSYS)` for a backend that transfers no
    /// data (MediaCodec, OpenHarmony).
    pub fn transfer_formats(&self, dir: TransferDirection) -> Result<HeapVec<PixelFormat>, c_int> {
        self.hw_frames().transfer_formats(dir)
    }

    /// Wraps: `av_hwframe_get_buffer`
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

    /// Wraps: `av_hwframe_ctx_create_derived`
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
                &raw mut out,
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
/// size, pool, `hwctx` (the backend's frames struct), free callback and
/// user data through [`context_mut`](Self::context_mut)
/// ([`FramesContextMut`]), then [`init`](Self::init) it.
///
/// Hand-written, as [`UninitDevice`] is: a [`FramesReferenceBox`] that lends
/// no reference handle until the context is initialised, only the
/// context's own handles. Dropping it releases the context. Neither `Send`
/// nor `Sync`, as [`FramesReference`]:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;
/// fn send<T: Send>(_: T) {}
/// fn move_away(frames: UninitFrames) {
///     send(frames);
/// }
/// ```
pub struct UninitFrames(FramesReferenceBox);

impl UninitFrames {
    /// Adopts `ptr`, the only reference to an uninitialised frames context;
    /// `None` for NULL.
    ///
    /// # Safety
    ///
    /// `ptr` must be NULL or a reference the caller owns and hands over, to
    /// a frames context `av_hwframe_ctx_alloc` allocated that has no other
    /// reference and was not initialised, and whose `free`, `user_opaque`,
    /// `pool` and backend struct are as allocated, this crate's, or C's
    /// under the rules of `hwcontext.h` and the backend's header.
    pub(crate) unsafe fn from_c(ptr: *mut ffi::AVBufferRef) -> Option<Self> {
        // SAFETY: by the caller's contract; the box lends no handle to the
        // context until `init` initialised it.
        unsafe { FramesReferenceBox::from_c(ptr) }.map(Self)
    }

    /// The context, shared.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a frames context reference has data.
    #[must_use]
    pub fn context(&self) -> FramesContextRef<'_> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.0.as_c_ptr()).data) };
        // SAFETY: a read of the `data` field of the live reference the box
        // holds; its data is the context's public struct.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the reference keeps the context live for the borrow, and
        // nothing writes it except through `context_mut`, which the shared
        // borrow excludes.
        unsafe { FramesContextRef::from_ptr(ctx.cast()) }
            .expect("a frames context reference has data")
    }

    /// The context, exclusively, to fill in before [`init`](Self::init).
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a frames context reference has data.
    #[must_use]
    pub fn context_mut(&mut self) -> FramesContextMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.0.as_c_ptr()).data) };
        // SAFETY: as in `context`.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the box holds the context's only reference, so nothing else
        // reaches the context for the borrow.
        unsafe { FramesContextMut::from_ptr(ctx.cast()) }
            .expect("a frames context reference has data")
    }

    /// Wraps: `av_hwframe_ctx_init`
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
    /// is released then. C does not count running out of memory for a
    /// preallocated frame's own `AVFrame` as one: it stops preallocating
    /// and succeeds.
    pub fn init(self) -> Result<FramesReferenceBox, c_int> {
        // C validates the format and size, runs the backend's `frames_init`
        // and preallocates frames through `av_hwframe_get_buffer`.
        // SAFETY: the only reference to an allocated frames context, never
        // initialised before (`init` consumes it), written only through
        // `FramesContextMut`: plain values C validates or takes as they are,
        // Rust hooks, and a pool and backend struct their callers vouch for.
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
/// ([`map`](Self::map)), or by copying it out
/// ([`download_to`](Self::download_to)); [`upload_from`](Self::upload_from)
/// and [`copy_from`](Self::copy_from) copy data in.
///
/// Every value holds a frame that `av_hwframe_get_buffer` or
/// `av_hwframe_map` filled, with a frames context, which nothing else
/// reaches. Hand-written: an owner of an `AVFrame` that keeps it out of the
/// frame API, whose readers would take its surface handles for bytes:
///
/// ```compile_fail,E0599
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame;
/// fn planes(frame: &HardwareFrame) {
///     let _ = frame.as_ref().plane(0);
/// }
/// ```
///
/// Its frames context is borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame;
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame;
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
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `buf` field is in bounds; nothing is accessed here.
        let buf_ptr = unsafe { &raw const (*p).buf };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `extended_buf` field is in bounds; nothing is accessed here.
        let extended_buf_ptr = unsafe { &raw const (*p).extended_buf };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `hw_frames_ctx` field is in bounds; nothing is accessed here.
        let hw_frames_ctx_ptr = unsafe { &raw const (*p).hw_frames_ctx };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `nb_extended_buf` field is in bounds; nothing is accessed here.
        let nb_extended_buf_ptr = unsafe { &raw const (*p).nb_extended_buf };
        // SAFETY: reads of the `buf`, `extended_buf`, `nb_extended_buf` and
        // `hw_frames_ctx` fields of the live frame `self` owns.
        let (bufs, extended, count, frames) = (
            unsafe { buf_ptr.read() },
            unsafe { extended_buf_ptr.read() },
            unsafe { nb_extended_buf_ptr.read() },
            unsafe { hw_frames_ctx_ptr.read() },
        );
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
        let extended_entry = |i: usize| {
            // SAFETY: `extended` holds `count` live entries when not NULL, and `i < count`.
            let at = unsafe { extended.add(i) };
            // SAFETY: as above; the entry is initialised.
            unsafe { at.read() }
        };
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
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a hardware frame holds its frames context.
    #[must_use]
    pub fn frames(&self) -> FramesReferenceRef<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `hw_frames_ctx` field is in bounds; nothing is accessed here.
        let hw_frames_ctx_ptr = unsafe { addr_of!((*self.frame.as_c_ptr()).hw_frames_ctx) };
        // SAFETY: a read of the `hw_frames_ctx` field of the live frame
        // `self` owns.
        let buf = unsafe { hw_frames_ctx_ptr.read() };
        // SAFETY: by the type's invariant, a live reference the frame owns
        // to an initialised frames context, which nothing releases or
        // replaces while `self` is borrowed.
        unsafe { FramesReferenceRef::from_ptr(buf) }
            .expect("a hardware frame holds its frames context")
    }

    /// Wraps: `av_hwframe_map`
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
        dst.as_mut().set_format(format.into_raw());
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

    /// Wraps: `av_hwframe_map`
    ///
    /// Maps the frame into another frames context, `frames`, as a hardware
    /// frame of that context's format, with `flags`: the contexts' devices
    /// must be compatible, as for a context derived from this frame's
    /// ([`FramesReferenceRef::derive`]). When this frame's context was
    /// derived from `frames`, the result is the frame this one was mapped
    /// from, as a new reference (an unmapping), in that frame's own context:
    /// `frames` for the frames the derived context allocates, but the third
    /// context of a frame mapped into the derived one from there. The new
    /// frame holds a reference to this one, or to the frame it was mapped
    /// from.
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
        let (p, format) = (dst.as_c_ptr(), frames.context().format());
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `format` field is in bounds; nothing is accessed here.
        let format_ptr = unsafe { &raw mut (*p).format };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `hw_frames_ctx` field is in bounds; nothing is accessed here.
        let hw_frames_ctx_ptr = unsafe { &raw mut (*p).hw_frames_ctx };
        // For the unsafe operations below: writes of two fields of the fresh frame `dst` owns,
        // which nothing else reaches: the format, and its frames context
        // reference, which the frame then owns and releases.
        {
            // SAFETY: as the comment above this block states.
            unsafe { format_ptr.write(format.into_raw()) };
            // SAFETY: as the comment above this block states.
            unsafe { hw_frames_ctx_ptr.write(target.into_c()) };
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
        // A frame `av_hwframe_map` filled, holding `frames`' context or,
        // unmapping, a reference to the frame this one was mapped from, with
        // that frame's context: the type's invariant.
        Ok(HardwareFrame {
            frame: dst,
            _context: PhantomData,
        })
    }

    /// Wraps: `av_hwframe_transfer_data`
    ///
    /// Downloads the frame's data into the software frame `dst`, as
    /// [`FrameMut::transfer_data_from`] does from a hardware source: `dst`
    /// must use a format the frames context lists for downloads
    /// ([`FramesReferenceRef::transfer_formats`] with
    /// [`TransferDirection::From`]) and hold a writable image of the
    /// context's allocated size, or have no buffers (as [`Frame::new`] makes
    /// it), in which case it is given new ones of that size, in its format if
    /// it has one, else in the first the context lists. Only the planes' data
    /// moves; no property is copied.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a `dst` holding a hardware frames context, which
    /// is no software frame; otherwise as
    /// [`transfer_data_from`](FrameMut::transfer_data_from): its refusals of
    /// `dst` (unlisted format, planes not holding the image, shared or
    /// read-only buffers, display size past the surfaces), the listing's and
    /// the backend's errors. A `dst` receiving new buffers is unchanged on
    /// error.
    pub fn download_to(&self, mut dst: FrameMut<'_>) -> Result<(), c_int> {
        if dst.as_ref().hw_frames_context().is_some() {
            return Err(EINVAL);
        }
        // SAFETY: the source is a hardware frame of its frames context (the
        // type's invariant), and `dst` holds no frames context.
        unsafe { dst.transfer_data_from(self.frame.as_ref()) }
    }

    /// Wraps: `av_hwframe_transfer_data`
    ///
    /// Uploads the data of the software frame `src` into the frame's
    /// surface, as [`FrameMut::transfer_data_from`] does into a hardware
    /// destination: `src` must use a format the frames context lists for
    /// uploads ([`FramesReferenceRef::transfer_formats`] with
    /// [`TransferDirection::To`]) and hold an image of the context's
    /// allocated size, which a frame
    /// [`alloc_buffers`](FrameMut::alloc_buffers) gave buffers at that size
    /// does. Only the planes' data moves.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a `src` holding a hardware frames context, which
    /// is no software frame ([`copy_from`](Self::copy_from) copies between
    /// hardware frames); otherwise as
    /// [`transfer_data_from`](FrameMut::transfer_data_from): its refusals of
    /// `src`, the listing's and the backend's errors.
    pub fn upload_from(&mut self, src: FrameRef<'_>) -> Result<(), c_int> {
        if src.hw_frames_context().is_some() {
            return Err(EINVAL);
        }
        // SAFETY: the destination is a hardware frame of its frames context
        // (the type's invariant), reached through `&mut self`, and `src`
        // holds no frames context.
        unsafe { self.frame.as_mut().transfer_data_from(src) }
    }

    /// Wraps: `av_hwframe_transfer_data`
    ///
    /// Copies the data of the hardware frame `src` into the frame's surface,
    /// where the backend of either frame can: `src`'s first, then this
    /// one's. A frame is not copied onto itself:
    ///
    /// ```compile_fail,E0502
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::HardwareFrame;
    /// fn copy_itself(frame: &mut HardwareFrame) {
    ///     let _ = frame.copy_from(frame);
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// As [`transfer_data_from`](FrameMut::transfer_data_from) between
    /// hardware frames: `AVERROR(EINVAL)` for a display size past either
    /// context's surfaces; `AVERROR(ENOSYS)` when either backend transfers
    /// no data, either context is derived, or neither backend copies
    /// between them; the backends' errors.
    pub fn copy_from(&mut self, src: &HardwareFrame) -> Result<(), c_int> {
        // SAFETY: both frames are hardware frames of their frames contexts
        // (the type's invariant), the destination reached through
        // `&mut self` and the source a distinct frame, read only.
        unsafe { self.frame.as_mut().transfer_data_from(src.frame.as_ref()) }
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
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::MappedFrame;
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

define_ctype!(
    /// Wraps: AVHWDeviceContext
    ///
    /// The state a hardware API keeps for one device, shared by everything
    /// derived from it (frames contexts, codecs, filters). Its getters are on
    /// [`DeviceContextRef`]; its setters, which only a context under
    /// construction or being freed lends, on [`DeviceContextMut`].
    ///
    /// C reaches a context only through the data of an `AVBufferRef`: each
    /// reference to the buffer is one counted share of the context, and the
    /// last one released runs the backend's `device_uninit`, then the context's
    /// [free callback](DeviceContextMut::set_free_callback), and frees the
    /// hardware context and the struct. So nothing here owns a `DeviceContext`
    /// directly: a context under construction is lent by the
    /// [`UninitDevice`] holding its only reference
    /// ([`context`](UninitDevice::context),
    /// [`context_mut`](UninitDevice::context_mut)); an initialised one, shared
    /// read-only, by any [`DeviceReferenceRef`]
    /// ([`context`](DeviceReferenceRef::context)).
    ///
    /// Its fields are:
    ///
    /// - `type`, the [`DeviceType`], set by the allocator and never changed
    ///   ([`device_type`](DeviceContextRef::device_type), no setter);
    /// - `av_class`, the logging class C's allocator sets ([`class`](DeviceContextRef::class), no
    ///   setter);
    /// - `hwctx`, the device type's own hardware context (an `AVVAAPIDeviceContext` for VAAPI, an
    ///   `AVCUDADeviceContext` for CUDA, and so on), allocated and freed with the context, which
    ///   the caller fills before initialisation: typed by the caller
    ///   ([`hwctx`](DeviceContextRef::hwctx), [`into_hwctx`](DeviceContextMut::into_hwctx), both
    ///   `unsafe`);
    /// - `free` and `user_opaque`, a callback C runs at the last release and the data it is meant
    ///   for, which Rust fills with a closure
    ///   ([`set_free_callback`](DeviceContextMut::set_free_callback)) and any `Send + Sync` value
    ///   ([`set_user_data`](DeviceContextMut::set_user_data)).
    ///
    /// Neither `Send` nor `Sync`, as [`DeviceReference`]; the user data and
    /// free callback are `Send` (and the data `Sync`) all the same, since C
    /// may release the last reference on any thread (a codec's frame
    /// threads hold some).
    DeviceContext,
    DeviceContextRef,
    DeviceContextMut,
    ffi::AVHWDeviceContext
);

// SAFETY: `av_hwdevice_ctx_alloc` sets `av_class` to its static
// `hwdevice_ctx_class` (no options, children or offsets; an `item_name`
// that reads the backend's name from the `FFHWDeviceContext` the context
// heads, and does not log), and nothing here writes it. Every handle
// addresses a context that allocator made, inside its `FFHWDeviceContext`:
// handles are lent only by the reference owners, which adopt C's contexts,
// and `DeviceContext` is never a Rust value (no `CZeroable`, and the
// handles' `from_ptr` is the crate's).
impl_class_object!(DeviceContext, DeviceContextRef, DeviceContextMut, av_class);

/// A device context's free callback, as
/// [`take_free_callback`](DeviceContextMut::take_free_callback) hands it back:
/// called once, with the context, when its last reference is released.
pub type FreeCallback = Box<dyn for<'c> FnOnce(DeviceContextMut<'c>) + Send>;

/// The Rust state a device context keeps in its `user_opaque` while its
/// `free` is [`release_hooks`]: the caller's data and free callback.
struct Hooks {
    data: Option<Box<dyn Any + Send + Sync>>,
    on_free: Option<FreeCallback>,
}

/// [`release_hooks`] as the one function pointer value the setters store in
/// `free` and [`is_release_hooks`] compares with. Two pointers to one
/// function taken at different sites may differ (a function can be
/// duplicated across codegen units, and Miri gives them distinct addresses);
/// one value read from a static compares equal to itself.
static RELEASE_HOOKS: unsafe extern "C" fn(*mut ffi::AVHWDeviceContext) = release_hooks;

/// Whether `free` is [`RELEASE_HOOKS`], the mark of a `user_opaque` holding
/// [`Hooks`].
fn is_release_hooks(free: Option<unsafe extern "C" fn(*mut ffi::AVHWDeviceContext)>) -> bool {
    free.is_some_and(|f| ptr::fn_addr_eq(f, RELEASE_HOOKS))
}

/// A device context's `free` while Rust's [`Hooks`] are installed: runs the
/// free callback with the context, then drops the hooks, the user data with
/// them, and clears both fields. A panic in either aborts the process.
unsafe extern "C" fn release_hooks(ctx: *mut ffi::AVHWDeviceContext) {
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `user_opaque` field is in bounds; nothing is accessed here.
    let user_opaque_ptr = unsafe { addr_of!((*ctx).user_opaque) };
    // SAFETY: hwdevice_ctx_free calls `free` once, with the live context whose
    // last reference is gone, so nothing else reaches it; this function is
    // its `free` only while `user_opaque` holds the `Box<Hooks>` the setters
    // leaked, which is read here without a reference to the context.
    let hooks = unsafe { user_opaque_ptr.read() }.cast::<Hooks>();
    if hooks.is_null() {
        return;
    }
    // SAFETY: a live `Hooks` only this call reaches; the borrow ends before
    // the callback runs, which may reach it again through its handle.
    let on_free = unsafe { (*hooks).on_free.take() };
    if let Some(on_free) = on_free {
        // SAFETY: the context is live and exclusively this call's until it
        // returns; the handle cannot outlive the callback (`for<'c>`).
        if let Some(ctx) = unsafe { DeviceContextMut::from_ptr(ctx) } {
            on_free(ctx);
        }
    }
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `free` field is in bounds; nothing is accessed here.
    let free_ptr = unsafe { addr_of_mut!((*ctx).free) };
    // The callback may have taken the hooks out, or installed new ones.
    // SAFETY: as above, the callback's handle is gone.
    let free = unsafe { free_ptr.replace(None) };
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `user_opaque` field is in bounds; nothing is accessed here.
    let user_opaque_ptr = unsafe { addr_of_mut!((*ctx).user_opaque) };
    // SAFETY: as above.
    let hooks = unsafe { user_opaque_ptr.replace(ptr::null_mut()) };
    if is_release_hooks(free) && !hooks.is_null() {
        // SAFETY: `free` marks `user_opaque` as the `Box<Hooks>` a setter
        // leaked, now detached from the context.
        drop(unsafe { Box::from_raw(hooks.cast::<Hooks>()) });
    }
}

impl<'a> DeviceContextRef<'a> {
    /// Field: AVHWDeviceContext.type
    ///
    /// The hardware API the context drives, fixed when it was allocated.
    #[must_use]
    pub fn device_type(&self) -> DeviceType {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `type_` field is in bounds; nothing is accessed here.
        let type_ptr = unsafe { addr_of!((*self.as_ptr()).type_) };
        // SAFETY: a read of the `type` field projected through the shared
        // handle's pointer to a live context; nothing writes it after
        // allocation.
        DeviceType::from_raw(unsafe { type_ptr.read() })
    }

    /// Field: `AVHWDeviceContext.av_class`
    ///
    /// The context's logging class, which C's allocator sets
    /// (`AVHWDeviceContext`, in the hardware device log category); `None`
    /// only for a context C did not allocate and left without one.
    #[must_use]
    pub fn class(&self) -> Option<ClassRef<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `av_class` field is in bounds; nothing is accessed here.
        let av_class_ptr = unsafe { addr_of!((*self.as_ptr()).av_class) };
        // SAFETY: as in `device_type`, for the `av_class` field.
        let class = unsafe { av_class_ptr.read() };
        // SAFETY: NULL, or a well-formed class that outlives the context:
        // `av_hwdevice_ctx_alloc` stores its static `hwdevice_ctx_class`.
        unsafe { ClassRef::from_ptr(class.cast_mut()) }
    }

    /// Field: AVHWDeviceContext.hwctx
    ///
    /// The device type's hardware context, as a `T`; `None` when the device
    /// type has none.
    ///
    /// # Safety
    ///
    /// `T::C` must be the public hardware context type of the context's
    /// [`device_type`](Self::device_type), as its `hwcontext_<type>.h`
    /// declares it (`AVVAAPIDeviceContext` for [`DeviceType::VAAPI`], ...):
    /// C allocates the backend's size, and nothing records the type. Nothing
    /// may write the fields read through the result while it lives, which
    /// the backends do not after initialisation.
    #[must_use]
    pub unsafe fn hwctx<T: CCell + 'a>(&self) -> Option<T::Ref<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { addr_of!((*self.as_ptr()).hwctx) };
        // SAFETY: as in `device_type`, for the `hwctx` pointer, which C never
        // replaces.
        let hwctx = unsafe { hwctx_ptr.read() };
        // SAFETY: NULL, or the context's hardware context, live as long as
        // the context; the caller vouches for its type and for writers.
        unsafe { T::ref_from_void_ptr(hwctx) }
    }

    /// Field: AVHWDeviceContext.free
    ///
    /// Whether a free callback runs when the context's last reference is
    /// released: one [`set_free_callback`](DeviceContextMut::set_free_callback)
    /// installed, or the backend's own, which its device creation set. User
    /// data alone is not one, though it is dropped then.
    #[must_use]
    pub fn has_free_callback(&self) -> bool {
        if let Some(hooks) = self.hooks() {
            hooks.on_free.is_some()
        } else {
            // SAFETY: the handle's pointer is to a live context, so the place of its
            // `free` field is in bounds; nothing is accessed here.
            let free = unsafe { &raw const (*self.as_ptr()).free };
            // SAFETY: as in `device_type`, for the `free` field.
            unsafe { free.read() }.is_some()
        }
    }

    /// Field: `AVHWDeviceContext.user_opaque`
    ///
    /// The value [`set_user_data`](DeviceContextMut::set_user_data) stored,
    /// borrowed from the context; `None` when it stored none, or one of
    /// another type. The private data a backend keeps there for its own free
    /// callback is not readable.
    #[must_use]
    pub fn user_data<T: Any>(&self) -> Option<&'a T> {
        self.hooks()?.data.as_deref()?.downcast_ref()
    }

    /// The Rust hooks the context's `user_opaque` holds, if any.
    fn hooks(&self) -> Option<&'a Hooks> {
        let p = self.as_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw const (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
        // SAFETY: as in `device_type`, for the `free` and `user_opaque`
        // fields.
        let (free, opaque) = (unsafe { free_ptr.read() }, unsafe {
            user_opaque_ptr.read()
        });
        if !is_release_hooks(free) {
            return None;
        }
        // SAFETY: `release_hooks` as `free` marks `user_opaque` as NULL or the
        // `Box<Hooks>` a setter leaked; only a `DeviceContextMut` replaces or
        // frees it, and none exists while this shared borrow lives.
        unsafe { opaque.cast::<Hooks>().as_ref() }
    }
}

impl fmt::Debug for DeviceContextRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceContext")
            .field("device_type", &self.device_type())
            .field("has_free_callback", &self.has_free_callback())
            .finish_non_exhaustive()
    }
}

impl<'a> DeviceContextMut<'a> {
    /// Field: AVHWDeviceContext.hwctx
    ///
    /// The device type's hardware context, as a `T`, to fill before
    /// [`UninitDevice::init`], borrowed for as long as this handle
    /// (reborrow with `as_mut()` to keep it); `None` when the device type
    /// has none.
    ///
    /// # Safety
    ///
    /// `T::C` must be the context's hardware context type, as for
    /// [`DeviceContextRef::hwctx`]. Whatever is written must be what the
    /// device type's header allows: the backend's `device_init` reads it,
    /// and its `device_uninit` releases what it finds there whether the
    /// context was initialised or not.
    #[must_use]
    pub unsafe fn into_hwctx<T: CCell + 'a>(mut self) -> Option<T::Mut<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { addr_of!((*self.as_mut_ptr()).hwctx) };
        // SAFETY: a read of the `hwctx` pointer projected through the
        // exclusive handle's pointer to a live context.
        let hwctx = unsafe { hwctx_ptr.read() };
        // SAFETY: NULL, or the context's hardware context, which this
        // exclusive borrow, consumed here, reaches alone for `'a`; the
        // caller vouches for its type.
        unsafe { T::mut_from_void_ptr(hwctx) }
    }

    /// Field: AVHWDeviceContext.free
    ///
    /// Makes `f` the context's free callback, dropping the one this set
    /// before: C calls it once, on the thread releasing the context's last
    /// reference, after the backend's `device_uninit` and before the
    /// hardware context and the struct are freed. Its handle reaches the
    /// [user data](Self::set_user_data), which is dropped after it returns;
    /// a callback it sets is dropped uncalled. A panic in it aborts the
    /// process.
    ///
    /// # Errors
    ///
    /// Refused, giving `f` back, when the context's `free` or `user_opaque`
    /// is someone else's, either one alone: a backend's device creation sets
    /// `free`, and most backends `user_opaque` with it (DRM, OpenCL, D3D12VA
    /// and Vulkan set `free` only).
    ///
    /// The callback may run on another thread, so it must be `Send`:
    ///
    /// ```compile_fail,E0277
    /// use std::rc::Rc;
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType};
    /// let mut device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    /// let local = Rc::new(1);
    /// let _ = device.context_mut().set_free_callback(move |_| drop(local));
    /// ```
    pub fn set_free_callback<F>(&mut self, f: F) -> Result<(), F>
    where
        F: FnOnce(DeviceContextMut<'_>) + Send + 'static,
    {
        match self.hooks_mut() {
            Some(hooks) => {
                hooks.on_free = Some(Box::new(f));
                Ok(())
            }
            None => Err(f),
        }
    }

    /// Field: AVHWDeviceContext.free
    ///
    /// Takes the free callback [`set_free_callback`](Self::set_free_callback)
    /// installed out, uncalled; `None` when it installed none.
    #[must_use]
    pub fn take_free_callback(&mut self) -> Option<FreeCallback> {
        let on_free = self.hooks_mut_if_installed()?.on_free.take();
        self.release_empty_hooks();
        on_free
    }

    /// Field: `AVHWDeviceContext.user_opaque`
    ///
    /// Makes `data` the context's user data, dropping the value this set
    /// before. It lives as long as the context, readable through
    /// [`DeviceContextRef::user_data`] (the free callback's handle
    /// included), and is dropped on the thread releasing the context's last
    /// reference, hence `Send + Sync`.
    ///
    /// # Errors
    ///
    /// Refused, giving `data` back, when the context's `free` or
    /// `user_opaque` is someone else's, as for
    /// [`set_free_callback`](Self::set_free_callback).
    ///
    /// Every thread holding the context may read it, so it must be `Sync`:
    ///
    /// ```compile_fail,E0277
    /// use std::cell::Cell;
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::{DeviceReference, DeviceType};
    /// let mut device = DeviceReference::alloc(DeviceType::VAAPI).unwrap();
    /// let _ = device.context_mut().set_user_data(Cell::new(1));
    /// ```
    pub fn set_user_data<T>(&mut self, data: T) -> Result<(), T>
    where
        T: Any + Send + Sync,
    {
        match self.hooks_mut() {
            Some(hooks) => {
                hooks.data = Some(Box::new(data));
                Ok(())
            }
            None => Err(data),
        }
    }

    /// Field: `AVHWDeviceContext.user_opaque`
    ///
    /// Takes the user data [`set_user_data`](Self::set_user_data) stored out;
    /// `None` when it stored none.
    #[must_use]
    pub fn take_user_data(&mut self) -> Option<Box<dyn Any + Send + Sync>> {
        let data = self.hooks_mut_if_installed()?.data.take();
        self.release_empty_hooks();
        data
    }

    /// The context's Rust hooks, installed first when `free` and
    /// `user_opaque` are both unset; `None` when either is someone else's.
    fn hooks_mut(&mut self) -> Option<&mut Hooks> {
        if self.as_ref().hooks().is_none() {
            let p = self.as_mut_ptr();
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `free` field is in bounds; nothing is accessed here.
            let free_ptr = unsafe { &raw const (*p).free };
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `user_opaque` field is in bounds; nothing is accessed here.
            let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
            // SAFETY: reads of the `free` and `user_opaque` fields projected
            // through the exclusive handle's pointer to a live context.
            let unset =
                unsafe { free_ptr.read() }.is_none() && unsafe { user_opaque_ptr.read() }.is_null();
            if !unset {
                return None;
            }
            let hooks = Box::into_raw(Box::new(Hooks {
                data: None,
                on_free: None,
            }));
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `free` field is in bounds; nothing is accessed here.
            let free_ptr = unsafe { &raw mut (*p).free };
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `user_opaque` field is in bounds; nothing is accessed here.
            let user_opaque_ptr = unsafe { &raw mut (*p).user_opaque };
            // For the unsafe operations below: writes of both fields through the exclusive handle;
            // neither held anything to release, and C reads them only in
            // hwdevice_ctx_free, which then calls `release_hooks` to free
            // the box.
            {
                // SAFETY: as the comment above this block states.
                unsafe { user_opaque_ptr.write(hooks.cast()) };
                // SAFETY: as the comment above this block states.
                unsafe { free_ptr.write(Some(RELEASE_HOOKS)) };
            }
        }
        self.hooks_mut_if_installed()
    }

    /// The context's Rust hooks, if `user_opaque` holds them.
    fn hooks_mut_if_installed(&mut self) -> Option<&mut Hooks> {
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw const (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
        // SAFETY: as in `hooks_mut`, for both fields.
        let (free, opaque) = (unsafe { free_ptr.read() }, unsafe {
            user_opaque_ptr.read()
        });
        if !is_release_hooks(free) {
            return None;
        }
        // SAFETY: `release_hooks` as `free` marks `user_opaque` as NULL or the
        // `Box<Hooks>` a setter leaked, which the exclusive borrow reaches
        // alone.
        unsafe { opaque.cast::<Hooks>().as_mut() }
    }

    /// Frees the context's Rust hooks and clears `free` and `user_opaque`
    /// once the hooks hold nothing.
    fn release_empty_hooks(&mut self) {
        let empty = self
            .hooks_mut_if_installed()
            .is_some_and(|h| h.data.is_none() && h.on_free.is_none());
        if !empty {
            return;
        }
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw mut (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw mut (*p).user_opaque };
        // For the unsafe operations below: writes of both fields through the exclusive handle; the
        // hooks were installed, so `user_opaque` held their box.
        let hooks = {
            // SAFETY: as the comment above this block states.
            unsafe { free_ptr.write(None) };
            // SAFETY: as the comment above this block states.
            unsafe { user_opaque_ptr.replace(ptr::null_mut()) }
        };
        // SAFETY: the empty `Box<Hooks>` a setter leaked, now detached.
        drop(unsafe { Box::from_raw(hooks.cast::<Hooks>()) });
    }
}

impl fmt::Debug for DeviceContextMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> DeviceReferenceRef<'a> {
    /// The device context the reference keeps alive, shared: its type, class,
    /// hardware context and user data. An initialised context is not written
    /// through any reference, so there is no exclusive form.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a device context reference has data.
    #[must_use]
    pub fn context(&self) -> DeviceContextRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.as_ptr()).data) };
        // SAFETY: a read of the `data` field of the live reference, which by
        // the type's invariant is an initialised context's public struct.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the reference keeps the context live for `'a`, and nothing
        // writes an initialised context's fields while it is shared.
        unsafe { DeviceContextRef::from_ptr(ctx.cast()) }
            .expect("a device context reference has data")
    }

    /// Wraps: `av_hwdevice_get_hwframe_constraints`
    ///
    /// The limits on frames of this device at its maximum capabilities: the
    /// pixel formats and sizes a hardware frames context may use; `None`
    /// when the backend reports none, fails, or C cannot allocate them.
    #[must_use]
    pub fn frames_constraints(&self) -> Option<FramesConstraintsBox> {
        let (device, config) = (self.as_ptr().cast_mut(), ptr::null());
        // SAFETY: a reference to an initialised context, borrowed for the
        // call; a NULL configuration asks every backend for its maximum
        // capabilities. C returns NULL or a fresh structure the caller owns,
        // with NONE-terminated av_malloc lists, released by
        // av_hwframe_constraints_free.
        let constraints = unsafe { ffi::av_hwdevice_get_hwframe_constraints(device, config) };
        // SAFETY: as above.
        unsafe { FramesConstraintsBox::from_c(constraints) }
    }

    /// Wraps: `av_hwdevice_get_hwframe_constraints`
    ///
    /// The limits on frames of this device under the hardware configuration
    /// `config` ([`alloc_hw_config`](Self::alloc_hw_config)), as
    /// [`frames_constraints`](Self::frames_constraints); also `None` for a
    /// configuration of another device type, whose struct the backend would
    /// misread.
    #[must_use]
    pub fn frames_constraints_for_config(&self, config: &HwConfig) -> Option<FramesConstraintsBox> {
        if config.device_type() != self.kind() {
            return None;
        }
        let (device, config) = (self.as_ptr().cast_mut(), config.as_ptr());
        // SAFETY: as in `frames_constraints`, with a configuration the
        // device's backend allocated (`av_hwdevice_hwconfig_alloc`): zeroed,
        // or filled through its raw seam as the backend's header asks, and
        // only read by C.
        let constraints = unsafe { ffi::av_hwdevice_get_hwframe_constraints(device, config) };
        // SAFETY: as in `frames_constraints`.
        unsafe { FramesConstraintsBox::from_c(constraints) }
    }
}

define_ctype!(
    /// Wraps: AVHWFramesConstraints
    ///
    /// The limits a device puts on hardware frames: the pixel formats a
    /// frames context may use as its hardware and software formats, and the
    /// range of frame sizes. Its getters are on [`FramesConstraintsRef`]
    /// (`as_ref()`), its setters on [`FramesConstraintsMut`] (`as_mut()`).
    ///
    /// The caller owns it alone, as a [`FramesConstraintsBox`], from
    /// [`DeviceReferenceRef::frames_constraints`] or
    /// [`FramesConstraints::new`]. Each format list is NULL ("unknown") or an
    /// `av_malloc` block the structure owns, holding formats up to a
    /// terminating [`PixelFormat::NONE`]; the getters lend the formats
    /// before it.
    ///
    /// `Send` and `Sync`: plain values and two arrays freed with `av_free`
    /// on any thread, and only the exclusive handle writes.
    FramesConstraints,
    FramesConstraintsRef,
    FramesConstraintsMut,
    ffi::AVHWFramesConstraints
);

// SAFETY: four integers and two owned av_malloc blocks of plain values, which
// av_hwframe_constraints_free releases on any thread; nothing in it is tied
// to a thread.
unsafe impl Send for FramesConstraints {}
// SAFETY: the shared handle only reads, and only `FramesConstraintsMut`,
// which a shared borrow excludes, writes the struct or its lists.
unsafe impl Sync for FramesConstraints {}
// SAFETY: as for `Sync for FramesConstraints`: copies of a shared handle only
// read.
unsafe impl Send for FramesConstraintsRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for FramesConstraintsRef<'_> {}

/// Wraps: `av_hwframe_constraints_free`
/// Handwritten
///
/// Releases a [`FramesConstraintsBox`]: C frees both format lists, then the
/// structure. Its constructor is C's own first step: a zero-filled structure
/// with sizes unlimited. `impl_cdrop!` would need a raw-pointer adapter only
/// to pass the slot C takes.
///
/// Stateless: C frees with `av_free`, on any thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct FramesConstraintsFree;

// SAFETY: av_hwframe_constraints_free av_freep's both lists (NULL or
// av_malloc blocks the structure owns, by its invariant) and then the
// structure, NULLing only this local slot; it reads nothing else.
unsafe impl CDrop<FramesConstraints> for FramesConstraintsFree {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<FramesConstraints>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVHWFramesConstraints>();
        // SAFETY: the caller hands over the box's sole claim on a live
        // structure; C keeps no pointer to the slot.
        unsafe { ffi::av_hwframe_constraints_free(&raw mut slot) }
    }
}

// SAFETY: a fresh av_mallocz block of the structure's size, which `c_drop`
// releases: NULL lists, which the getters read as unknown and C frees as
// nothing, and the sizes av_hwdevice_get_hwframe_constraints starts from.
unsafe impl CNew<FramesConstraints> for FramesConstraintsFree {
    fn c_new(&self) -> Option<NonNull<FramesConstraints>> {
        let size = size_of::<ffi::AVHWFramesConstraints>();
        // SAFETY: av_mallocz takes any size and returns NULL or a fresh,
        // zero-filled block aligned for any C object.
        let p = unsafe { ffi::av_mallocz(size) }.cast::<ffi::AVHWFramesConstraints>();
        let p = NonNull::new(p)?;
        // SAFETY: `p.as_ptr()` is to a live object, as the block below relies on, so the
        // place of its `max_height` field is in bounds; nothing is accessed here.
        let max_height_ptr = unsafe { &raw mut (*p.as_ptr()).max_height };
        // SAFETY: `p.as_ptr()` is to a live object, as the block below relies on, so the
        // place of its `max_width` field is in bounds; nothing is accessed here.
        let max_width_ptr = unsafe { &raw mut (*p.as_ptr()).max_width };
        // For the unsafe operations below: writes of two fields of the block just allocated, which
        // nothing else reaches.
        {
            // SAFETY: as the comment above this block states.
            unsafe { max_width_ptr.write(c_int::MAX) };
            // SAFETY: as the comment above this block states.
            unsafe { max_height_ptr.write(c_int::MAX) };
        }
        Some(p.cast())
    }
}

/// Frame constraints, owned: both format lists are freed with the structure
/// (`av_hwframe_constraints_free`). No `Clone`: C has no copy.
///
/// Its format lists are borrowed from it:
///
/// ```compile_fail,E0505
/// use ffmpeg_bindings::libavutil::libavutil::hwcontext::FramesConstraints;
/// let constraints = FramesConstraints::new().unwrap();
/// let formats = constraints.as_ref().valid_sw_formats();
/// drop(constraints);
/// let _ = formats;
/// ```
pub type FramesConstraintsBox = CBox<FramesConstraints, FramesConstraintsFree>;

impl FramesConstraints {
    /// Constraints with no format lists and the sizes C starts from: 0 to
    /// `i32::MAX` in both dimensions; `None` when C cannot allocate them.
    #[must_use]
    pub fn new() -> Option<FramesConstraintsBox> {
        FramesConstraintsBox::new()
    }
}

/// One of the constraints' two format lists.
#[derive(Clone, Copy)]
enum FormatList {
    Hardware,
    Software,
}

impl<'a> FramesConstraintsRef<'a> {
    /// The formats of `which` list before its terminating `NONE`, borrowed
    /// from the constraints; `None` for NULL.
    #[expect(
        clippy::similar_names,
        reason = "the names follow the C variables they mirror"
    )]
    fn list(&self, which: FormatList) -> Option<CSlice<'a, PixelFormat>> {
        let p = self.as_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `valid_hw_formats` field is in bounds; nothing is accessed here.
        let valid_hw_formats_ptr = unsafe { &raw const (*p).valid_hw_formats };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `valid_sw_formats` field is in bounds; nothing is accessed here.
        let valid_sw_formats_ptr = unsafe { &raw const (*p).valid_sw_formats };
        // SAFETY: a read of one list pointer projected through the shared
        // handle's pointer to live constraints.
        let list = match which {
            // SAFETY: as the comment above this statement states.
            FormatList::Hardware => unsafe { valid_hw_formats_ptr.read() },
            // SAFETY: as the comment above this statement states.
            FormatList::Software => unsafe { valid_sw_formats_ptr.read() },
        };
        // By the type's invariant, NULL or a NONE-terminated list the
        // constraints own, which nothing frees or writes while they are
        // borrowed shared; `PixelFormat` is transparent over `AVPixelFormat`.
        let list = NonNull::new(list)?.cast::<PixelFormat>();
        let mut len = 0;
        loop {
            // SAFETY: every element up to and including the terminator is live, as above,
            // and `len` has not passed the terminator.
            let at = unsafe { list.add(len) };
            // SAFETY: as above; the element is initialised.
            if unsafe { at.read() } == PixelFormat::NONE {
                break;
            }
            len += 1;
        }
        // SAFETY: `len` initialised elements, live and unwritten for `'a`.
        Some(unsafe { CSlice::from_raw_parts(list, len) })
    }

    /// Field: `AVHWFramesConstraints.valid_hw_formats`
    ///
    /// The formats a frames context of the device may use as its `format`,
    /// borrowed from the constraints; `None` when they list none (a
    /// backend's constraints always do).
    #[must_use]
    pub fn valid_hw_formats(&self) -> Option<CSlice<'a, PixelFormat>> {
        self.list(FormatList::Hardware)
    }

    /// Field: `AVHWFramesConstraints.valid_sw_formats`
    ///
    /// The formats a frames context of the device may use as its
    /// `sw_format`, borrowed from the constraints; `None` when the device
    /// does not know them.
    #[must_use]
    pub fn valid_sw_formats(&self) -> Option<CSlice<'a, PixelFormat>> {
        self.list(FormatList::Software)
    }

    /// Field: `AVHWFramesConstraints.min_width`
    ///
    /// The smallest frame width; 0 when not known.
    #[must_use]
    pub fn min_width(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `min_width` field is in bounds; nothing is accessed here.
        let min_width_ptr = unsafe { addr_of!((*self.as_ptr()).min_width) };
        // SAFETY: a read of an `int` field projected through the shared
        // handle's pointer to live constraints.
        unsafe { min_width_ptr.read() }
    }

    /// Field: `AVHWFramesConstraints.min_height`
    ///
    /// The smallest frame height; 0 when not known.
    #[must_use]
    pub fn min_height(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `min_height` field is in bounds; nothing is accessed here.
        let min_height_ptr = unsafe { addr_of!((*self.as_ptr()).min_height) };
        // SAFETY: as in `min_width`, for the `min_height` field.
        unsafe { min_height_ptr.read() }
    }

    /// Field: `AVHWFramesConstraints.max_width`
    ///
    /// The largest frame width; `i32::MAX` when not known or unlimited.
    #[must_use]
    pub fn max_width(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `max_width` field is in bounds; nothing is accessed here.
        let max_width_ptr = unsafe { addr_of!((*self.as_ptr()).max_width) };
        // SAFETY: as in `min_width`, for the `max_width` field.
        unsafe { max_width_ptr.read() }
    }

    /// Field: `AVHWFramesConstraints.max_height`
    ///
    /// The largest frame height; `i32::MAX` when not known or unlimited.
    #[must_use]
    pub fn max_height(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `max_height` field is in bounds; nothing is accessed here.
        let max_height_ptr = unsafe { addr_of!((*self.as_ptr()).max_height) };
        // SAFETY: as in `min_width`, for the `max_height` field.
        unsafe { max_height_ptr.read() }
    }
}

impl fmt::Debug for FramesConstraintsRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let list = |l: Option<CSlice<'_, PixelFormat>>| l.map(|l| l.elems().collect::<Vec<_>>());
        f.debug_struct("FramesConstraints")
            .field("valid_hw_formats", &list(self.valid_hw_formats()))
            .field("valid_sw_formats", &list(self.valid_sw_formats()))
            .field("min_width", &self.min_width())
            .field("min_height", &self.min_height())
            .field("max_width", &self.max_width())
            .field("max_height", &self.max_height())
            .finish()
    }
}

impl FramesConstraintsMut<'_> {
    /// Makes `with` (a NONE-terminated list, or none) the list in `which`,
    /// and returns the formats the old one held.
    #[expect(
        clippy::similar_names,
        reason = "the names follow the C variables they mirror"
    )]
    fn replace_list(
        &mut self,
        which: FormatList,
        with: Option<HeapVec<PixelFormat>>,
    ) -> Option<HeapVec<PixelFormat>> {
        let len = self.as_ref().list(which).map_or(0, |l| l.len());
        let with = with.map_or(ptr::null_mut(), |v| v.into_raw_parts().0);
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `valid_hw_formats` field is in bounds; nothing is accessed here.
        let valid_hw_formats_ptr = unsafe { &raw mut (*p).valid_hw_formats };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `valid_sw_formats` field is in bounds; nothing is accessed here.
        let valid_sw_formats_ptr = unsafe { &raw mut (*p).valid_sw_formats };
        // SAFETY: a read and a write of one list pointer projected through
        // the exclusive handle's pointer to live constraints; `with` is NULL
        // or an av_malloc block holding a NONE-terminated list, whose claim
        // moves to the constraints.
        let old = match which {
            // SAFETY: as the comment above this statement states.
            FormatList::Hardware => unsafe { valid_hw_formats_ptr.replace(with.cast()) },
            // SAFETY: as the comment above this statement states.
            FormatList::Software => unsafe { valid_sw_formats_ptr.replace(with.cast()) },
        };
        // SAFETY: by the type's invariant, NULL or an av_malloc block the
        // constraints owned, holding `len` initialised formats and their
        // terminator, which no longer reaches them; `av_free` ignores the
        // length.
        unsafe { HeapVec::from_raw_parts(old.cast::<PixelFormat>(), len) }
    }

    /// Makes a terminated copy of `formats` (up to its first `NONE`) the
    /// list in `which`, freeing the old one; none for `None`.
    fn set_list(
        &mut self,
        which: FormatList,
        formats: Option<&[PixelFormat]>,
    ) -> Result<(), c_int> {
        let copy = match formats {
            None => None,
            Some(formats) => {
                let len = formats
                    .iter()
                    .position(|&f| f == PixelFormat::NONE)
                    .unwrap_or(formats.len());
                let terminated = alloc_array_from_fn(len + 1, |i| {
                    if i < len {
                        formats[i]
                    } else {
                        PixelFormat::NONE
                    }
                });
                Some(terminated.ok_or(ENOMEM)?)
            }
        };
        drop(self.replace_list(which, copy));
        Ok(())
    }

    /// Field: `AVHWFramesConstraints.valid_hw_formats`
    ///
    /// Makes a copy of `formats` the hardware formats, up to the first
    /// [`PixelFormat::NONE`] it holds, and frees the old list; `None` lists
    /// none. `Err(AVERROR(ENOMEM))`, with the old list kept, when C cannot
    /// allocate the copy.
    ///
    /// # Errors
    ///
    /// `Err(AVERROR(ENOMEM))`, with the old list kept, when C cannot allocate the copy.
    pub fn set_valid_hw_formats(&mut self, formats: Option<&[PixelFormat]>) -> Result<(), c_int> {
        self.set_list(FormatList::Hardware, formats)
    }

    /// Field: `AVHWFramesConstraints.valid_hw_formats`
    ///
    /// Takes the hardware formats out (without their terminator), leaving
    /// none; `None` when there were none.
    #[must_use]
    pub fn take_valid_hw_formats(&mut self) -> Option<HeapVec<PixelFormat>> {
        self.replace_list(FormatList::Hardware, None)
    }

    /// Field: `AVHWFramesConstraints.valid_sw_formats`
    ///
    /// Makes a copy of `formats` the software formats, as
    /// [`set_valid_hw_formats`](Self::set_valid_hw_formats); `None` marks
    /// them unknown.
    ///
    /// # Errors
    ///
    /// Returns C's negative `AVERROR` code when it fails.
    pub fn set_valid_sw_formats(&mut self, formats: Option<&[PixelFormat]>) -> Result<(), c_int> {
        self.set_list(FormatList::Software, formats)
    }

    /// Field: `AVHWFramesConstraints.valid_sw_formats`
    ///
    /// Takes the software formats out (without their terminator), leaving
    /// them unknown; `None` when they were.
    #[must_use]
    pub fn take_valid_sw_formats(&mut self) -> Option<HeapVec<PixelFormat>> {
        self.replace_list(FormatList::Software, None)
    }

    /// Field: `AVHWFramesConstraints.min_width`
    ///
    /// Sets the smallest frame width.
    pub fn set_min_width(&mut self, width: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `min_width` field is in bounds; nothing is accessed here.
        let min_width_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).min_width) };
        // SAFETY: a write of an `int` field projected through the exclusive
        // handle's pointer to live constraints.
        unsafe { min_width_ptr.write(width) }
    }

    /// Field: `AVHWFramesConstraints.min_height`
    ///
    /// Sets the smallest frame height.
    pub fn set_min_height(&mut self, height: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `min_height` field is in bounds; nothing is accessed here.
        let min_height_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).min_height) };
        // SAFETY: as in `set_min_width`, for the `min_height` field.
        unsafe { min_height_ptr.write(height) }
    }

    /// Field: `AVHWFramesConstraints.max_width`
    ///
    /// Sets the largest frame width.
    pub fn set_max_width(&mut self, width: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `max_width` field is in bounds; nothing is accessed here.
        let max_width_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).max_width) };
        // SAFETY: as in `set_min_width`, for the `max_width` field.
        unsafe { max_width_ptr.write(width) }
    }

    /// Field: `AVHWFramesConstraints.max_height`
    ///
    /// Sets the largest frame height.
    pub fn set_max_height(&mut self, height: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `max_height` field is in bounds; nothing is accessed here.
        let max_height_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).max_height) };
        // SAFETY: as in `set_min_width`, for the `max_height` field.
        unsafe { max_height_ptr.write(height) }
    }
}

impl fmt::Debug for FramesConstraintsMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

define_ctype!(
    /// Wraps: AVHWFramesContext
    ///
    /// The state a hardware API keeps for a pool of interchangeable frames on
    /// one device: their hardware and software pixel formats, their size, and
    /// the pool they come from. Its getters are on [`FramesContextRef`]; its
    /// setters, which only a context under construction or being freed
    /// lends, on [`FramesContextMut`].
    ///
    /// C reaches a context only through the data of an `AVBufferRef`, as a
    /// device context: each reference is one counted share of it, and the
    /// last one released uninitialises the pool the backend made, runs the
    /// backend's `frames_uninit`, then the context's
    /// [free callback](FramesContextMut::set_free_callback), and releases the
    /// context it was derived from, its device reference, the hardware
    /// context and the struct. So nothing here owns a `FramesContext`
    /// directly: a context under construction is lent by the
    /// [`UninitFrames`] holding its only reference
    /// ([`context`](UninitFrames::context),
    /// [`context_mut`](UninitFrames::context_mut)); an initialised one, shared
    /// read-only, by any [`FramesReferenceRef`]
    /// ([`context`](FramesReferenceRef::context)).
    ///
    /// Its fields are:
    ///
    /// - `av_class`, the logging class C's allocator sets ([`class`](FramesContextRef::class), no
    ///   setter);
    /// - `device_ref` and `device_ctx`, the reference to the device the context was allocated on,
    ///   which the context owns, and that device's context, both set by the allocator and never
    ///   changed: the backend and the hardware context were chosen by that device
    ///   ([`device`](FramesContextRef::device),
    ///   [`device_context`](FramesContextRef::device_context), no setters; `device().try_clone()`
    ///   is an owned reference);
    /// - `hwctx`, the device type's own frames context (an `AVVAAPIFramesContext` for VAAPI, and
    ///   so on), allocated and freed with the context, which the caller may fill before
    ///   initialisation: typed by the caller ([`hwctx`](FramesContextRef::hwctx),
    ///   [`into_hwctx`](FramesContextMut::into_hwctx), both `unsafe`);
    /// - `free` and `user_opaque`, a callback C runs at the last release and the data it is meant
    ///   for, which Rust fills with a closure
    ///   ([`set_free_callback`](FramesContextMut::set_free_callback)) and any `Send + Sync` value
    ///   ([`set_user_data`](FramesContextMut::set_user_data));
    /// - `pool`, where `av_hwframe_get_buffer` takes the surfaces from: one the caller sets before
    ///   initialisation, which the context then owns ([`set_pool`](FramesContextMut::set_pool),
    ///   `unsafe`), or else the one the backend makes and C releases
    ///   ([`has_pool`](FramesContextRef::has_pool), [`pool`](FramesContextRef::pool), `unsafe`);
    /// - `initial_pool_size`, `format`, `sw_format`, `width` and `height`, plain values the caller
    ///   sets before initialisation, which C checks (a hardware format the backend lists, a valid
    ///   image size) and the backend uses ([`format`](FramesContextRef::format),
    ///   [`set_format`](FramesContextMut::set_format), and so on).
    ///
    /// Neither `Send` nor `Sync`, as [`FramesReference`]; the user data, free
    /// callback and pool are `Send` (and the data `Sync`) all the same, since
    /// C may release the last reference on any thread (a codec's frame
    /// threads hold some).
    FramesContext,
    FramesContextRef,
    FramesContextMut,
    ffi::AVHWFramesContext
);

// SAFETY: `av_hwframe_ctx_alloc` sets `av_class` to its static
// `hwframe_ctx_class` (no options, children or offsets, and
// `av_default_item_name`), and nothing here writes it. Every handle
// addresses a context that allocator made, inside its `FFHWFramesContext`,
// as for `DeviceContext`.
impl_class_object!(FramesContext, FramesContextRef, FramesContextMut, av_class);

/// A frames context's free callback, as
/// [`take_free_callback`](FramesContextMut::take_free_callback) hands it back:
/// called once, with the context, when its last reference is released.
pub type FramesFreeCallback = Box<dyn for<'c> FnOnce(FramesContextMut<'c>) + Send>;

/// The Rust state a frames context keeps in its `user_opaque` while its
/// `free` is [`release_frames_hooks`]: the caller's data, free callback and
/// pool, whose surfaces `pool` points at while it is set.
struct FramesHooks {
    data: Option<Box<dyn Any + Send + Sync>>,
    on_free: Option<FramesFreeCallback>,
    pool: Option<BufferPoolBox>,
}

impl FramesHooks {
    fn is_empty(&self) -> bool {
        self.data.is_none() && self.on_free.is_none() && self.pool.is_none()
    }
}

/// [`release_frames_hooks`] as the one function pointer value the setters
/// store in `free` and [`is_release_frames_hooks`] compares with, as
/// [`RELEASE_HOOKS`] is for device contexts.
static RELEASE_FRAMES_HOOKS: unsafe extern "C" fn(*mut ffi::AVHWFramesContext) =
    release_frames_hooks;

/// Whether `free` is [`RELEASE_FRAMES_HOOKS`], the mark of a `user_opaque`
/// holding [`FramesHooks`].
fn is_release_frames_hooks(
    free: Option<unsafe extern "C" fn(*mut ffi::AVHWFramesContext)>,
) -> bool {
    free.is_some_and(|f| ptr::fn_addr_eq(f, RELEASE_FRAMES_HOOKS))
}

/// A frames context's `free` while Rust's [`FramesHooks`] are installed:
/// releases the hooks' pool and clears `pool` (which may still point at the
/// backend's pool, which C uninitialised before), so that the callback finds
/// none, as `hwcontext.h` promises it; then runs the free callback with the
/// context, drops the hooks, the user data with them, and clears `free` and
/// `user_opaque`. A panic in any of them aborts the process.
unsafe extern "C" fn release_frames_hooks(ctx: *mut ffi::AVHWFramesContext) {
    // SAFETY: `ctx` is to a live object, as the block below relies on, so the
    // place of its `pool` field is in bounds; nothing is accessed here.
    let pool_ptr = unsafe { &raw mut (*ctx).pool };
    // SAFETY: `ctx` is to a live object, as the block below relies on, so the
    // place of its `user_opaque` field is in bounds; nothing is accessed here.
    let user_opaque_ptr = unsafe { &raw const (*ctx).user_opaque };
    // For the unsafe operations below: hwframe_ctx_free calls `free` once, with the live context
    // whose last reference is gone, so nothing else reaches it; it reads `pool` no
    // more. This function is its `free` only while `user_opaque` holds the
    // `Box<FramesHooks>` the setters leaked, read here without a reference
    // to the context.
    let hooks = {
        // SAFETY: as the comment above this block states.
        unsafe { pool_ptr.write(ptr::null_mut()) };
        // SAFETY: as the comment above this block states.
        unsafe { user_opaque_ptr.read() }
    }
    .cast::<FramesHooks>();
    if hooks.is_null() {
        return;
    }
    // SAFETY: a live `FramesHooks` only this call reaches; each borrow ends
    // before the callback runs, which may reach it again through its handle.
    let pool = unsafe { (*hooks).pool.take() };
    // No frame of the context is left to use a surface: each holds a
    // reference to the context. The pool goes before the callback, as the
    // header promises.
    drop(pool);
    // SAFETY: as above.
    let on_free = unsafe { (*hooks).on_free.take() };
    if let Some(on_free) = on_free {
        // SAFETY: the context is live and exclusively this call's until it
        // returns; the handle cannot outlive the callback (`for<'c>`).
        if let Some(ctx) = unsafe { FramesContextMut::from_ptr(ctx) } {
            on_free(ctx);
        }
    }
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `free` field is in bounds; nothing is accessed here.
    let free_ptr = unsafe { addr_of_mut!((*ctx).free) };
    // The callback may have taken the hooks out, or installed new ones.
    // SAFETY: as above, the callback's handle is gone.
    let free = unsafe { free_ptr.replace(None) };
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `user_opaque` field is in bounds; nothing is accessed here.
    let user_opaque_ptr = unsafe { addr_of_mut!((*ctx).user_opaque) };
    // SAFETY: as above.
    let hooks = unsafe { user_opaque_ptr.replace(ptr::null_mut()) };
    // SAFETY: `ctx` is to a live object, as the access below relies on,
    // so the place of its `pool` field is in bounds; nothing is accessed here.
    let pool_ptr = unsafe { addr_of_mut!((*ctx).pool) };
    // SAFETY: as above; a pool the callback set goes with its hooks.
    unsafe { pool_ptr.write(ptr::null_mut()) };
    if is_release_frames_hooks(free) && !hooks.is_null() {
        // SAFETY: `free` marks `user_opaque` as the `Box<FramesHooks>` a
        // setter leaked, now detached from the context.
        drop(unsafe { Box::from_raw(hooks.cast::<FramesHooks>()) });
    }
}

impl<'a> FramesContextRef<'a> {
    /// Field: `AVHWFramesContext.av_class`
    ///
    /// The context's logging class, which C's allocator sets
    /// (`AVHWFramesContext`); `None` only for a context C did not allocate
    /// and left without one.
    #[must_use]
    pub fn class(&self) -> Option<ClassRef<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `av_class` field is in bounds; nothing is accessed here.
        let av_class_ptr = unsafe { addr_of!((*self.as_ptr()).av_class) };
        // SAFETY: a read of the `av_class` field projected through the shared
        // handle's pointer to a live context; nothing writes it after
        // allocation.
        let class = unsafe { av_class_ptr.read() };
        // SAFETY: NULL, or a well-formed class that outlives the context:
        // `av_hwframe_ctx_alloc` stores its static `hwframe_ctx_class`.
        unsafe { ClassRef::from_ptr(class.cast_mut()) }
    }

    /// Field: `AVHWFramesContext.device_ref`
    ///
    /// The device the context was allocated on, through the reference the
    /// context owns and releases with its last one; the device lives at
    /// least as long. [`try_clone`](DeviceReferenceRef::try_clone) it for a
    /// reference of the caller's own.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a frames context holds its device.
    #[must_use]
    pub fn device(&self) -> DeviceReferenceRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `device_ref` field is in bounds; nothing is accessed here.
        let device_ref_ptr = unsafe { addr_of!((*self.as_ptr()).device_ref) };
        // SAFETY: as in `class`, for the `device_ref` field, which C sets at
        // allocation and releases only after the context's free callback.
        let device = unsafe { device_ref_ptr.read() };
        // SAFETY: a live reference the context owns, to the initialised
        // device context `av_hwframe_ctx_alloc` was given (only initialised
        // devices lend the reference handle that allocates frames contexts,
        // and C's callers allocate them on initialised devices).
        unsafe { DeviceReferenceRef::from_ptr(device) }.expect("a frames context holds its device")
    }

    /// Field: `AVHWFramesContext.device_ctx`
    ///
    /// The context of the device the context was allocated on, shared: the
    /// data of [`device`](Self::device).
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a frames context has a device context.
    #[must_use]
    pub fn device_context(&self) -> DeviceContextRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `device_ctx` field is in bounds; nothing is accessed here.
        let device_ctx_ptr = unsafe { addr_of!((*self.as_ptr()).device_ctx) };
        // SAFETY: as in `class`, for the `device_ctx` field, which C sets at
        // allocation to the data of `device_ref`.
        let ctx = unsafe { device_ctx_ptr.read() };
        // SAFETY: the initialised device context the context's `device_ref`
        // keeps live for `'a`; nothing writes an initialised device context
        // while it is shared.
        unsafe { DeviceContextRef::from_ptr(ctx) }.expect("a frames context has a device context")
    }

    /// Field: AVHWFramesContext.hwctx
    ///
    /// The device type's frames context, as a `T`; `None` when the device
    /// type has none.
    ///
    /// # Safety
    ///
    /// `T::C` must be the public frames context type of the device's
    /// [`device_type`](DeviceContextRef::device_type), as its
    /// `hwcontext_<type>.h` declares it (`AVVAAPIFramesContext` for
    /// [`DeviceType::VAAPI`], ...): C allocates the backend's size, and
    /// nothing records the type. Nothing may write the fields read through
    /// the result while it lives: some backends do so as they allocate
    /// frames (D3D11VA records its textures), which an initialised context
    /// does through any reference.
    #[must_use]
    pub unsafe fn hwctx<T: CCell + 'a>(&self) -> Option<T::Ref<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { addr_of!((*self.as_ptr()).hwctx) };
        // SAFETY: as in `class`, for the `hwctx` pointer, which C never
        // replaces.
        let hwctx = unsafe { hwctx_ptr.read() };
        // SAFETY: NULL, or the context's frames struct, live as long as the
        // context; the caller vouches for its type and for writers.
        unsafe { T::ref_from_void_ptr(hwctx) }
    }

    /// Field: AVHWFramesContext.free
    ///
    /// Whether a free callback runs when the context's last reference is
    /// released: one [`set_free_callback`](FramesContextMut::set_free_callback)
    /// installed, or one C code set (a hardware decoder's). User data and a
    /// pool alone are not one, though they are dropped then.
    #[must_use]
    pub fn has_free_callback(&self) -> bool {
        if let Some(hooks) = self.hooks() {
            hooks.on_free.is_some()
        } else {
            // SAFETY: the handle's pointer is to a live context, so the place of its
            // `free` field is in bounds; nothing is accessed here.
            let free = unsafe { &raw const (*self.as_ptr()).free };
            // SAFETY: as in `class`, for the `free` field.
            unsafe { free.read() }.is_some()
        }
    }

    /// Field: `AVHWFramesContext.user_opaque`
    ///
    /// The value [`set_user_data`](FramesContextMut::set_user_data) stored,
    /// borrowed from the context; `None` when it stored none, or one of
    /// another type. The data C code keeps there for its own free callback
    /// is not readable.
    #[must_use]
    pub fn user_data<T: Any>(&self) -> Option<&'a T> {
        self.hooks()?.data.as_deref()?.downcast_ref()
    }

    /// Field: AVHWFramesContext.pool
    ///
    /// Whether the context has a pool: one [`set_pool`](FramesContextMut::set_pool)
    /// or C code set, or, once initialised, the one its backend made.
    #[must_use]
    pub fn has_pool(&self) -> bool {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `pool` field is in bounds; nothing is accessed here.
        let pool_ptr = unsafe { addr_of!((*self.as_ptr()).pool) };
        // SAFETY: as in `class`, for the `pool` field.
        !unsafe { pool_ptr.read() }.is_null()
    }

    /// Field: AVHWFramesContext.pool
    ///
    /// The pool the context's frames come from, shared; `None` when it has
    /// none ([`has_pool`](Self::has_pool)). It lives as long as the context:
    /// the backend's is released with it, and one
    /// [`set_pool`](FramesContextMut::set_pool) set is the context's.
    ///
    /// # Safety
    ///
    /// The pool's buffers must be bytes, as a [`BufferPoolRef`] hands them
    /// out (a writable [`UninitBuffer`](super::buffer::UninitBuffer) of the
    /// pool's size from [`get`](BufferPoolRef::get)): most backends' pools
    /// hold surface handles in place of data pointers (VA-API's are surface
    /// IDs, D3D11VA's descriptors), which only a pool of bytes given to
    /// [`set_pool`](FramesContextMut::set_pool), or a backend whose surfaces
    /// are bytes, is not. The pool's allocator, which C may then call on any
    /// thread the handle reaches, must allow that, and a pool C code set must
    /// stay live as long as the context.
    #[must_use]
    pub unsafe fn pool(&self) -> Option<BufferPoolRef<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `pool` field is in bounds; nothing is accessed here.
        let pool_ptr = unsafe { addr_of!((*self.as_ptr()).pool) };
        // SAFETY: as in `class`, for the `pool` field.
        let pool = unsafe { pool_ptr.read() };
        // SAFETY: NULL, or a pool live as long as the context: the backend's
        // (released by the context's last release, before which the field
        // is not cleared while shared), the hooks' (dropped there too), or
        // C code's, as the caller vouches; the caller vouches for its
        // buffers being bytes.
        unsafe { BufferPoolRef::from_ptr(pool) }
    }

    /// Field: `AVHWFramesContext.initial_pool_size`
    ///
    /// How many frames initialisation allocates (and releases) to fill the
    /// pool; for backends whose pools cannot grow, also how many frames the
    /// pool holds at most. Zero or less preallocates nothing.
    #[must_use]
    pub fn initial_pool_size(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `initial_pool_size` field is in bounds; nothing is accessed here.
        let initial_pool_size_ptr = unsafe { addr_of!((*self.as_ptr()).initial_pool_size) };
        // SAFETY: as in `class`, for the `initial_pool_size` field.
        unsafe { initial_pool_size_ptr.read() }
    }

    /// Field: AVHWFramesContext.format
    ///
    /// The hardware pixel format of the context's surfaces, the format of its
    /// frames; [`PixelFormat::NONE`] until set.
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `format` field is in bounds; nothing is accessed here.
        let format_ptr = unsafe { addr_of!((*self.as_ptr()).format) };
        // SAFETY: as in `class`, for the `format` field.
        PixelFormat::from_raw(unsafe { format_ptr.read() })
    }

    /// Field: `AVHWFramesContext.sw_format`
    ///
    /// The pixel format of the data the surfaces hold, as the backend lays
    /// it out (or the fully planar format of its depth and subsampling, where
    /// the hardware API does not say); [`PixelFormat::NONE`] until set.
    #[must_use]
    pub fn sw_format(&self) -> PixelFormat {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `sw_format` field is in bounds; nothing is accessed here.
        let sw_format_ptr = unsafe { addr_of!((*self.as_ptr()).sw_format) };
        // SAFETY: as in `class`, for the `sw_format` field.
        PixelFormat::from_raw(unsafe { sw_format_ptr.read() })
    }

    /// Field: AVHWFramesContext.width
    ///
    /// The allocated width of the surfaces, in pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `width` field is in bounds; nothing is accessed here.
        let width_ptr = unsafe { addr_of!((*self.as_ptr()).width) };
        // SAFETY: as in `class`, for the `width` field.
        unsafe { width_ptr.read() }
    }

    /// Field: AVHWFramesContext.height
    ///
    /// The allocated height of the surfaces, in pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `height` field is in bounds; nothing is accessed here.
        let height_ptr = unsafe { addr_of!((*self.as_ptr()).height) };
        // SAFETY: as in `class`, for the `height` field.
        unsafe { height_ptr.read() }
    }

    /// The Rust hooks the context's `user_opaque` holds, if any.
    fn hooks(&self) -> Option<&'a FramesHooks> {
        let p = self.as_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw const (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
        // SAFETY: as in `class`, for the `free` and `user_opaque` fields.
        let (free, opaque) = (unsafe { free_ptr.read() }, unsafe {
            user_opaque_ptr.read()
        });
        if !is_release_frames_hooks(free) {
            return None;
        }
        // SAFETY: `release_frames_hooks` as `free` marks `user_opaque` as NULL
        // or the `Box<FramesHooks>` a setter leaked; only a
        // `FramesContextMut` replaces or frees it, and none exists while this
        // shared borrow lives.
        unsafe { opaque.cast::<FramesHooks>().as_ref() }
    }
}

impl fmt::Debug for FramesContextRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FramesContext")
            .field("format", &self.format())
            .field("sw_format", &self.sw_format())
            .field("width", &self.width())
            .field("height", &self.height())
            .field("initial_pool_size", &self.initial_pool_size())
            .field("has_pool", &self.has_pool())
            .field("has_free_callback", &self.has_free_callback())
            .finish_non_exhaustive()
    }
}

impl<'a> FramesContextMut<'a> {
    /// Field: AVHWFramesContext.hwctx
    ///
    /// The device type's frames context, as a `T`, to fill before
    /// [`UninitFrames::init`], borrowed for as long as this handle (reborrow
    /// with `as_mut()` to keep it); `None` when the device type has none.
    ///
    /// # Safety
    ///
    /// `T::C` must be the context's frames context type, as for
    /// [`FramesContextRef::hwctx`]. Whatever is written must be what the
    /// device type's header allows: the backend's `frames_init` uses the
    /// surfaces or textures a caller provides there, and its
    /// `frames_uninit` releases what it finds whether the context was
    /// initialised or not.
    #[must_use]
    pub unsafe fn into_hwctx<T: CCell + 'a>(mut self) -> Option<T::Mut<'a>> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `hwctx` field is in bounds; nothing is accessed here.
        let hwctx_ptr = unsafe { addr_of!((*self.as_mut_ptr()).hwctx) };
        // SAFETY: a read of the `hwctx` pointer projected through the
        // exclusive handle's pointer to a live context.
        let hwctx = unsafe { hwctx_ptr.read() };
        // SAFETY: NULL, or the context's frames struct, which this exclusive
        // borrow, consumed here, reaches alone for `'a`; the caller vouches
        // for its type.
        unsafe { T::mut_from_void_ptr(hwctx) }
    }

    /// Field: AVHWFramesContext.free
    ///
    /// Makes `f` the context's free callback, dropping the one this set
    /// before: C calls it once, on the thread releasing the context's last
    /// reference, after the backend's `frames_uninit` and the release of the
    /// context's pool, and before its device reference, frames struct and
    /// the struct itself are released. Its handle reaches the
    /// [user data](Self::set_user_data), which is dropped after it returns;
    /// a callback or pool it sets is dropped unused. A panic in it aborts the
    /// process.
    ///
    /// # Errors
    ///
    /// Refused, giving `f` back, when the context's `free` or `user_opaque`
    /// is someone else's, either one alone: hardware decoders set `free`
    /// alone (NVDEC, to uninitialise the pool it set) or with `user_opaque`
    /// (Vulkan).
    ///
    /// The callback may run on another thread, so it must be `Send`:
    ///
    /// ```compile_fail,E0277
    /// use std::rc::Rc;
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;
    /// fn local(mut frames: UninitFrames) {
    ///     let local = Rc::new(1);
    ///     let _ = frames.context_mut().set_free_callback(move |_| drop(local));
    /// }
    /// ```
    pub fn set_free_callback<F>(&mut self, f: F) -> Result<(), F>
    where
        F: FnOnce(FramesContextMut<'_>) + Send + 'static,
    {
        match self.hooks_mut() {
            Some(hooks) => {
                hooks.on_free = Some(Box::new(f));
                Ok(())
            }
            None => Err(f),
        }
    }

    /// Field: AVHWFramesContext.free
    ///
    /// Takes the free callback [`set_free_callback`](Self::set_free_callback)
    /// installed out, uncalled; `None` when it installed none.
    #[must_use]
    pub fn take_free_callback(&mut self) -> Option<FramesFreeCallback> {
        let on_free = self.hooks_mut_if_installed()?.on_free.take();
        self.release_empty_hooks();
        on_free
    }

    /// Field: `AVHWFramesContext.user_opaque`
    ///
    /// Makes `data` the context's user data, dropping the value this set
    /// before. It lives as long as the context, readable through
    /// [`FramesContextRef::user_data`] (the free callback's handle
    /// included), and is dropped on the thread releasing the context's last
    /// reference, hence `Send + Sync`.
    ///
    /// # Errors
    ///
    /// Refused, giving `data` back, when the context's `free` or
    /// `user_opaque` is someone else's, as for
    /// [`set_free_callback`](Self::set_free_callback).
    ///
    /// Every thread holding the context may read it, so it must be `Sync`:
    ///
    /// ```compile_fail,E0277
    /// use std::cell::Cell;
    /// use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;
    /// fn local(mut frames: UninitFrames) {
    ///     let _ = frames.context_mut().set_user_data(Cell::new(1));
    /// }
    /// ```
    pub fn set_user_data<T>(&mut self, data: T) -> Result<(), T>
    where
        T: Any + Send + Sync,
    {
        match self.hooks_mut() {
            Some(hooks) => {
                hooks.data = Some(Box::new(data));
                Ok(())
            }
            None => Err(data),
        }
    }

    /// Field: `AVHWFramesContext.user_opaque`
    ///
    /// Takes the user data [`set_user_data`](Self::set_user_data) stored out;
    /// `None` when it stored none.
    #[must_use]
    pub fn take_user_data(&mut self) -> Option<Box<dyn Any + Send + Sync>> {
        let data = self.hooks_mut_if_installed()?.data.take();
        self.release_empty_hooks();
        data
    }

    /// Field: AVHWFramesContext.pool
    ///
    /// Makes `pool` the one `av_hwframe_get_buffer` takes the context's
    /// surfaces from, in place of the one the backend would make, dropping
    /// the pool this set before. The context owns it from then on: it is
    /// dropped (its free buffers released, and the rest as they come back)
    /// when the context's last reference is released, before the free
    /// callback runs.
    ///
    /// # Errors
    ///
    /// Refused, giving `pool` back, when the context's `free` or
    /// `user_opaque` is someone else's, as for
    /// [`set_free_callback`](Self::set_free_callback), or its `pool` is (C
    /// code's, which C never releases).
    ///
    /// # Safety
    ///
    /// Every buffer `pool` hands out must be a surface of the context's
    /// backend, as its `hwcontext_<api>.h` describes the buffers of
    /// `AVHWFramesContext.pool` (VA-API: a `VASurfaceID` as the data
    /// pointer; D3D11VA: an `AVD3D11FrameDescriptor`; CUDA: device memory
    /// for a frame of the context's size): the backend takes one for each
    /// frame and hands it to the hardware API, and to its transfers and
    /// mappings, unchecked. A pool of plain bytes ([`BufferPool::new`]) is
    /// one only for a backend whose surfaces are bytes.
    ///
    /// [`BufferPool::new`]: super::buffer::BufferPool::new
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "`CBox::as_c_ptr` is ambiguous between the owner and its uninitialised form as a path"
    )]
    pub unsafe fn set_pool(&mut self, pool: BufferPoolBox) -> Result<(), BufferPoolBox> {
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the access below relies on,
        // so the place of its `pool` field is in bounds; nothing is accessed here.
        let pool_ptr = unsafe { addr_of!((*p).pool) };
        // SAFETY: a read of the `pool` field projected through the exclusive
        // handle's pointer to a live context.
        let current = unsafe { pool_ptr.read() };
        let owned = self
            .hooks_mut_if_installed()
            .and_then(|h| h.pool.as_ref())
            .map(|own| own.as_c_ptr());
        if !current.is_null() && owned != Some(current) {
            return Err(pool);
        }
        let new = pool.as_c_ptr();
        match self.hooks_mut() {
            Some(hooks) => {
                let old = hooks.pool.replace(pool);
                // SAFETY: the handle's pointer is to a live object, as the access below relies on,
                // so the place of its `pool` field is in bounds; nothing is accessed here.
                let pool_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).pool) };
                // SAFETY: as above, a write of the field; the hooks own the
                // pool it points at until they drop it, after clearing it.
                unsafe { pool_ptr.write(new) };
                drop(old);
                Ok(())
            }
            None => Err(pool),
        }
    }

    /// Field: AVHWFramesContext.pool
    ///
    /// Takes the pool [`set_pool`](Self::set_pool) set out, leaving the
    /// context without one, as allocated: initialisation then has the backend
    /// make its own. `None` when it set none (the backend's, or C code's,
    /// pool is not the caller's to take).
    #[must_use]
    pub fn take_pool(&mut self) -> Option<BufferPoolBox> {
        let pool = self.hooks_mut_if_installed()?.pool.take()?;
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `pool` field is in bounds; nothing is accessed here.
        let pool_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).pool) };
        // SAFETY: a write of the `pool` field projected through the exclusive
        // handle's pointer to a live context, which pointed at the hooks'
        // pool.
        unsafe { pool_ptr.write(ptr::null_mut()) };
        self.release_empty_hooks();
        Some(pool)
    }

    /// Field: `AVHWFramesContext.initial_pool_size`
    ///
    /// Sets how many frames initialisation preallocates, as for
    /// [`FramesContextRef::initial_pool_size`]. Backends whose pools cannot
    /// grow need it set to at least the frames used at once (a codec's
    /// reference frames and more) when the context has no pool of the
    /// caller's.
    pub fn set_initial_pool_size(&mut self, size: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `initial_pool_size` field is in bounds; nothing is accessed here.
        let initial_pool_size_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).initial_pool_size) };
        // SAFETY: a write of the `initial_pool_size` field projected through
        // the exclusive handle's pointer to a live context; any value is one
        // C and the backends take (zero or less preallocates nothing).
        unsafe { initial_pool_size_ptr.write(size) }
    }

    /// Field: AVHWFramesContext.format
    ///
    /// Sets the hardware pixel format of the context's surfaces: one the
    /// device's backend lists, or [`UninitFrames::init`] fails with
    /// `AVERROR(ENOSYS)`.
    pub fn set_format(&mut self, format: PixelFormat) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `format` field is in bounds; nothing is accessed here.
        let format_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).format) };
        // SAFETY: as in `set_initial_pool_size`, for the `format` field;
        // `av_hwframe_ctx_init` checks it against the backend's list.
        unsafe { format_ptr.write(format.into_raw()) }
    }

    /// Field: `AVHWFramesContext.sw_format`
    ///
    /// Sets the pixel format of the data the surfaces hold, as for
    /// [`FramesContextRef::sw_format`]: one the backend supports, or its
    /// initialisation fails.
    pub fn set_sw_format(&mut self, format: PixelFormat) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `sw_format` field is in bounds; nothing is accessed here.
        let sw_format_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).sw_format) };
        // SAFETY: as in `set_initial_pool_size`, for the `sw_format` field;
        // backends check it in `frames_init`, and the transfer wrappers
        // check software frames against it.
        unsafe { sw_format_ptr.write(format.into_raw()) }
    }

    /// Field: AVHWFramesContext.width
    ///
    /// Sets the allocated width of the surfaces, in pixels: a size
    /// `av_image_check_size` accepts with the height, or
    /// [`UninitFrames::init`] fails with `AVERROR(EINVAL)`.
    pub fn set_width(&mut self, width: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `width` field is in bounds; nothing is accessed here.
        let width_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).width) };
        // SAFETY: as in `set_initial_pool_size`, for the `width` field;
        // `av_hwframe_ctx_init` checks the size.
        unsafe { width_ptr.write(width) }
    }

    /// Field: AVHWFramesContext.height
    ///
    /// Sets the allocated height of the surfaces, in pixels, as for
    /// [`set_width`](Self::set_width).
    pub fn set_height(&mut self, height: i32) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `height` field is in bounds; nothing is accessed here.
        let height_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).height) };
        // SAFETY: as in `set_width`, for the `height` field.
        unsafe { height_ptr.write(height) }
    }

    /// The context's Rust hooks, installed first when `free` and
    /// `user_opaque` are both unset; `None` when either is someone else's.
    fn hooks_mut(&mut self) -> Option<&mut FramesHooks> {
        if self.as_ref().hooks().is_none() {
            let p = self.as_mut_ptr();
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `free` field is in bounds; nothing is accessed here.
            let free_ptr = unsafe { &raw const (*p).free };
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `user_opaque` field is in bounds; nothing is accessed here.
            let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
            // SAFETY: reads of the `free` and `user_opaque` fields projected
            // through the exclusive handle's pointer to a live context.
            let unset =
                unsafe { free_ptr.read() }.is_none() && unsafe { user_opaque_ptr.read() }.is_null();
            if !unset {
                return None;
            }
            let hooks = Box::into_raw(Box::new(FramesHooks {
                data: None,
                on_free: None,
                pool: None,
            }));
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `free` field is in bounds; nothing is accessed here.
            let free_ptr = unsafe { &raw mut (*p).free };
            // SAFETY: `p` is to a live object, as the block below relies on, so the
            // place of its `user_opaque` field is in bounds; nothing is accessed here.
            let user_opaque_ptr = unsafe { &raw mut (*p).user_opaque };
            // For the unsafe operations below: writes of both fields through the exclusive handle;
            // neither held anything to release, and C reads them only in
            // hwframe_ctx_free, which then calls `release_frames_hooks` to
            // free the box.
            {
                // SAFETY: as the comment above this block states.
                unsafe { user_opaque_ptr.write(hooks.cast()) };
                // SAFETY: as the comment above this block states.
                unsafe { free_ptr.write(Some(RELEASE_FRAMES_HOOKS)) };
            }
        }
        self.hooks_mut_if_installed()
    }

    /// The context's Rust hooks, if `user_opaque` holds them.
    fn hooks_mut_if_installed(&mut self) -> Option<&mut FramesHooks> {
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw const (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw const (*p).user_opaque };
        // SAFETY: as in `hooks_mut`, for both fields.
        let (free, opaque) = (unsafe { free_ptr.read() }, unsafe {
            user_opaque_ptr.read()
        });
        if !is_release_frames_hooks(free) {
            return None;
        }
        // SAFETY: `release_frames_hooks` as `free` marks `user_opaque` as NULL
        // or the `Box<FramesHooks>` a setter leaked, which the exclusive
        // borrow reaches alone.
        unsafe { opaque.cast::<FramesHooks>().as_mut() }
    }

    /// Frees the context's Rust hooks and clears `free` and `user_opaque`
    /// once the hooks hold nothing.
    fn release_empty_hooks(&mut self) {
        let empty = self.hooks_mut_if_installed().is_some_and(|h| h.is_empty());
        if !empty {
            return;
        }
        let p = self.as_mut_ptr();
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `free` field is in bounds; nothing is accessed here.
        let free_ptr = unsafe { &raw mut (*p).free };
        // SAFETY: `p` is to a live object, as the block below relies on, so the
        // place of its `user_opaque` field is in bounds; nothing is accessed here.
        let user_opaque_ptr = unsafe { &raw mut (*p).user_opaque };
        // For the unsafe operations below: writes of both fields through the exclusive handle; the
        // hooks were installed, so `user_opaque` held their box.
        let hooks = {
            // SAFETY: as the comment above this block states.
            unsafe { free_ptr.write(None) };
            // SAFETY: as the comment above this block states.
            unsafe { user_opaque_ptr.replace(ptr::null_mut()) }
        };
        // SAFETY: the empty `Box<FramesHooks>` a setter leaked, now detached.
        drop(unsafe { Box::from_raw(hooks.cast::<FramesHooks>()) });
    }
}

impl fmt::Debug for FramesContextMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl DeviceReferenceRef<'_> {
    /// A new, initialised frames context on this device, of hardware format
    /// `format` holding data of `sw_format`, at `width` x `height`, with
    /// `initial_pool_size` frames preallocated in the pool its backend makes:
    /// [`alloc_frames`](Self::alloc_frames), the fields set through
    /// [`FramesContextMut`], then [`init`](UninitFrames::init). Fill an
    /// [`UninitFrames`] by hand for anything more (a pool, the backend's
    /// frames struct, hooks).
    ///
    /// # Errors
    ///
    /// [`UninitFrames::init`]'s; `AVERROR(ENOMEM)` where
    /// [`alloc_frames`](Self::alloc_frames) returns `None`.
    pub fn create_frames(
        &self,
        format: PixelFormat,
        sw_format: PixelFormat,
        width: i32,
        height: i32,
        initial_pool_size: i32,
    ) -> Result<FramesReferenceBox, c_int> {
        let mut frames = self.alloc_frames().ok_or(ENOMEM)?;
        let mut ctx = frames.context_mut();
        ctx.set_format(format);
        ctx.set_sw_format(sw_format);
        ctx.set_width(width);
        ctx.set_height(height);
        ctx.set_initial_pool_size(initial_pool_size);
        frames.init()
    }
}

impl<'a> FramesReferenceRef<'a> {
    /// The frames context the reference keeps alive, shared: its formats,
    /// size, pool, device and user data. An initialised context is not
    /// written through any reference, so there is no exclusive form.
    ///
    /// # Panics
    ///
    /// Only if C breaks the guarantee that a frames context reference has data.
    #[must_use]
    pub fn context(&self) -> FramesContextRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `data` field is in bounds; nothing is accessed here.
        let data_ptr = unsafe { addr_of!((*self.as_ptr()).data) };
        // SAFETY: a read of the `data` field of the live reference, which by
        // the type's invariant is an initialised context's public struct.
        let ctx = unsafe { data_ptr.read() };
        // SAFETY: the reference keeps the context live for `'a`, and nothing
        // writes an initialised context's fields while it is shared.
        unsafe { FramesContextRef::from_ptr(ctx.cast()) }
            .expect("a frames context reference has data")
    }
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../../../tests/common/hwdevice.rs"]
pub(crate) mod hwdevice;

#[cfg(test)]
#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../../../tests/common/hwframes.rs"]
mod fixture;

#[cfg(test)]
#[expect(
    dead_code,
    reason = "a helper shared by several test crates; this one uses only some of it"
)]
#[path = "../../../tests/common/hwbackend.rs"]
pub(crate) mod backend;

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#[expect(
    clippy::default_trait_access,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#[expect(
    clippy::assert_is_empty,
    reason = "assertions keep the form of the C behaviour they check"
)]
mod unit_tests {
    use core::ffi::c_uint;
    use core::mem::{align_of, size_of};
    use std::collections::HashSet;

    use super::*;
    use crate::libavutil::libavutil::buffer::BufferReference;
    use crate::libavutil::libavutil::dict::Dict;
    use crate::libavutil::libavutil::mem::alloc_array_zeroed;

    /// `AVERROR(ENOSYS)`.
    const ENOSYS: c_int = -(ffi::ENOSYS.cast_signed());

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
        i32::MIN.cast_unsigned(),
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
            assert_eq!(t.into_raw(), i as c_uint, "{t:?}");
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
            let t = DeviceType::from_raw(raw);
            assert_eq!(t.constant_name(), None);
            assert_eq!(format!("{t:?}"), format!("DeviceType({raw})"));
        }
    }

    #[test]
    fn device_type_default_is_none_and_zero() {
        assert_eq!(DeviceType::default(), DeviceType::NONE);
        assert_eq!(DeviceType::default().into_raw(), 0);
    }

    #[test]
    fn device_type_raw_conversions_round_trip_every_value() {
        let all = NAMED.iter().map(|&(t, _)| t.into_raw()).chain(UNNAMED);
        for raw in all {
            let c = raw;
            assert_eq!(DeviceType::from_raw(c).into_raw(), c);
            assert_eq!(ffi::AVHWDeviceType::from(DeviceType::from(c)), c);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn device_type_arrays_are_plain_elements() {
        let zeroed = alloc_array_zeroed::<DeviceType>(4).unwrap();
        assert!(zeroed.as_slice().iter().all(|&t| t == DeviceType::NONE));
        let values: Vec<DeviceType> = UNNAMED
            .iter()
            .map(|&raw| DeviceType::from_raw(raw))
            .chain(NAMED.iter().map(|&(t, _)| t))
            .collect();
        let a = HeapVec::from_slice(&values).unwrap();
        assert_eq!(a.as_slice(), &values[..]);
    }

    #[test]
    fn transfer_direction_encodes_as_the_c_enumerators() {
        assert_eq!(
            TransferDirection::From.into_raw(),
            ffi::AV_HWFRAME_TRANSFER_DIRECTION_FROM
        );
        assert_eq!(
            TransferDirection::To.into_raw(),
            ffi::AV_HWFRAME_TRANSFER_DIRECTION_TO
        );
        assert_eq!(TransferDirection::From.into_raw(), 0);
        assert_eq!(TransferDirection::To.into_raw(), 1);
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
        pub(super) use fixture::{
            EMPTY, GRAY_FIRST, INERT, LISTING_FAILS, NO_LIST, TRANSFER_FAILS, YUV, pattern,
        };

        use super::super::fixture;
        use super::*;
        use crate::libavutil::libavutil::buffer::BufferReferenceBox;

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
                ffi::AV_PIX_FMT_MEDIACODEC,
                ffi::AV_PIX_FMT_YUV420P,
                16,
                8,
            ))
        }

        /// A hardware frame of `ctx` whose surface holds `pattern(seed)`.
        pub(super) fn hw_frame(ctx: &BufferReferenceBox, seed: u8) -> FrameBox {
            // SAFETY: as the comment below states, for this part of the operation.
            let new_hw_frame_2 = unsafe { fixture::new_hw_frame(ctx.as_c_ptr(), pattern(seed)) };
            // SAFETY: `ctx` is a live fake context; the fresh frame satisfies
            // `Frame`'s invariant: `hw_frames_ctx` references that context,
            // and `data[3]` is the start of `buf[0]`'s window, the surface.
            unsafe { FrameBox::from_c(new_hw_frame_2) }.unwrap()
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn holds_image_refuses_images_past_the_buffers() {
        let f = software(YUV420P, 16, 8);
        // av_frame_get_buffer pads the height to 32 rows: past that, and
        // past the line size, the rows leave the buffer or the line.
        assert!(!holds_image(f.as_ref(), YUV420P, 16, 64, false));
        assert!(!holds_image(f.as_ref(), YUV420P, 4096, 8, false));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn holds_image_refuses_shared_planes_for_writes_only() {
        let f = software(YUV420P, 16, 8);
        let copy = f.as_ref().try_clone().unwrap();
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, false));
        assert!(!holds_image(f.as_ref(), YUV420P, 16, 8, true));
        drop(copy);
        assert!(holds_image(f.as_ref(), YUV420P, 16, 8, true));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn holds_image_refuses_unknown_and_hardware_formats() {
        let f = software(YUV420P, 16, 8);
        for format in [
            PixelFormat::NONE,
            PixelFormat::VAAPI,
            PixelFormat::from_raw(100_000),
        ] {
            assert!(!holds_image(f.as_ref(), format, 16, 8, false), "{format:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn holds_image_checks_a_palette() {
        let f = software(PixelFormat::PAL8, 16, 8);
        assert!(holds_image(f.as_ref(), PixelFormat::PAL8, 16, 8, true));
        let mut f = software(PixelFormat::PAL8, 16, 8);
        f.as_mut().clear_plane(1);
        assert!(!holds_image(f.as_ref(), PixelFormat::PAL8, 16, 8, false));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn transfer_without_contexts_is_refused_by_c() {
        let mut clean = Frame::new().unwrap();
        let counted = software(GRAY8, 4, 4);
        let mut counted_dst = software(GRAY8, 4, 4);
        // For the unsafe operations below: no frame holds a hardware frames context.
        {
            // SAFETY: as the comment above this block states.
            unsafe {
                assert_eq!(
                    clean.as_mut().transfer_data_from(counted.as_ref()),
                    Err(EINVAL)
                );
            };
            // SAFETY: as the comment above this block states.
            unsafe {
                assert_eq!(
                    counted_dst.as_mut().transfer_data_from(counted.as_ref()),
                    Err(ENOSYS)
                );
            };
            // SAFETY: as the comment above this block states.
            unsafe {
                assert_eq!(
                    clean
                        .as_mut()
                        .transfer_data_from(Frame::new().unwrap().as_ref()),
                    Err(EINVAL)
                );
            };
        }
    }

    /// C's `transfer_data_alloc` leaks what the frame held and leaves the
    /// bytes around the copied lines uninitialised; the Rust composition
    /// releases it (`LeakSanitizer` sees no leak) and zeroes them.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
        unsafe { (*c_dst.as_c_ptr()).format = YUV420P.into_raw() };
        // An `AVFrame` takes more than this; every other allocation is done.
        assert!(size_of::<ffi::AVFrame>() > 256);
        crate::libavutil::libavutil::mem::set_max_alloc(256);
        // SAFETY: `src` is a hardware frame of the fake context.
        let ret = unsafe { dst.as_mut().transfer_data_from(src.as_ref()) };
        // SAFETY: as above, through C's own routine.
        let c_ret = unsafe { ffi::av_hwframe_transfer_data(c_dst.as_c_ptr(), src.as_c_ptr(), 0) };
        crate::libavutil::libavutil::mem::set_max_alloc(c_int::MAX as usize);
        assert_eq!(ret, Err(ENOMEM));
        assert_eq!(c_ret, ENOMEM);
        assert_eq!(dst.as_ref().pts(), 3);
        assert!(dst.as_ref().buffer(0).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn inert_backends_are_refused_before_c_calls_their_null_transfers() {
        let inert = hw::inert_context();
        let src = hw::hw_frame(&inert, 6);
        let mut sw = software(YUV420P, 16, 8);
        let mut clean = Frame::new().unwrap();
        clean.as_mut().set_pixel_format(YUV420P);
        let mut hw_dst = hw::hw_frame(&inert, 7);
        let yuv = hw::context(&hw::YUV, 16, 8);
        let mut other = hw::hw_frame(&yuv, 8);
        // For the unsafe operations below: every hardware frame is one of its fake context.
        {
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(sw.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS)) };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(clean.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS)) };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(hw_dst.as_mut().transfer_data_from(sw.as_ref()), Err(ENOSYS)) };
            // SAFETY: as the comment above this block states.
            unsafe {
                assert_eq!(
                    hw_dst.as_mut().transfer_data_from(other.as_ref()),
                    Err(ENOSYS)
                );
            };
            // SAFETY: as the comment above this block states.
            unsafe { assert_eq!(other.as_mut().transfer_data_from(src.as_ref()), Err(ENOSYS)) };
        }
    }
    /// Wave 1, batch 1: names, the backend list, map flags, and the device
    /// and frames contexts and hardware frames over the test backends of
    /// `tests/common/hwbackend.rs`, adopted through the crate's seams.
    mod contexts {
        use core::ffi::CStr;

        use super::*;
        use crate::libavutil::libavutil::buffer::MAX_REFERENCES;
        use crate::libavutil::libavutil::dict::Dict;
        use crate::libavutil::libavutil::hwcontext::backend::{
            self as fx, Event, FramesHw, FramesHwctx, HWCONFIG_SIZE, Kind, averror,
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
            // SAFETY: `buf` is to a live object, as the block below relies on, so the
            // place of its `buffer` field is in bounds; nothing is accessed here.
            let buffer_ptr = unsafe { &raw const (*buf).buffer };
            // For the unsafe operations below: a live reference, whose private buffer body starts
            // with the mirrored prefix; this test alone references it.
            {
                // SAFETY: as the comment above this block states.
                let body = unsafe { buffer_ptr.read().cast::<BufferCount>() };
                // SAFETY: as the comment above this block states.
                unsafe { (*body).refcount = count };
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

        /// Fills a frames context for `kind`, as `fx::fill_frames` does.
        fn fill(mut ctx: FramesContextMut<'_>, kind: Kind, w: c_int, h: c_int, pool: c_int) {
            ctx.set_format(PixelFormat::from_raw(kind.format()));
            ctx.set_sw_format(PixelFormat::NV12);
            ctx.set_width(w);
            ctx.set_height(h);
            ctx.set_initial_pool_size(pool);
        }

        /// A frames context of `kind` on `device`, filled with [`fill`] and
        /// `hwctx`, initialised through the wrapper.
        fn frames_with(
            device: &DeviceReferenceBox,
            kind: Kind,
            hwctx: FramesHwctx,
            pool: c_int,
        ) -> Result<FramesReferenceBox, c_int> {
            let mut frames = device.as_ref().alloc_frames().unwrap();
            fill(frames.context_mut(), kind, 64, 32, pool);
            // SAFETY: the test backends' frames struct is a `FramesHwctx`,
            // any of whose results they take.
            unsafe { frames.context_mut().into_hwctx::<FramesHw>() }
                .unwrap()
                .set(hwctx);
            frames.init()
        }

        fn frames(device: &DeviceReferenceBox, kind: Kind) -> FramesReferenceBox {
            let frames = frames_with(device, kind, FramesHwctx::default(), 0).unwrap();
            assert_eq!(fx::take_events(), [Event::FramesInit(kind)]);
            frames
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn names_round_trip_and_unknown_names_are_none() {
            for (t, _) in NAMED {
                match t.name() {
                    None => assert_eq!(t, DeviceType::NONE),
                    Some(name) => assert_eq!(DeviceType::from_name(name), Some(t)),
                }
            }
            for raw in UNNAMED {
                assert_eq!(DeviceType::from_raw(raw).name(), None);
            }
            assert_eq!(DeviceType::CUDA.name(), Some(c"cuda"));
            for unknown in [c"", c"none", c"Vaapi", c"vaapi ", c"cuda\t"] {
                assert_eq!(DeviceType::from_name(unknown), None, "{unknown:?}");
            }
        }

        /// The campaign builds compile no backend: the walk is empty, and
        /// stays so.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn uninit_device_inits_once_and_releases_on_drop() {
            assert!(
                // SAFETY: NULL adopts nothing.
                unsafe { UninitDevice::from_c(ptr::null_mut()) }.is_none()
            );
            let mut uninit = uninit_device(Kind::A);
            assert_eq!(format!("{uninit:?}"), "UninitDevice(VAAPI)");
            let ctx = uninit.context_mut().as_mut_ptr();
            // For the unsafe operations below: the public struct of the context `uninit` owns.
            {
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!((*ctx).type_, ffi::AV_HWDEVICE_TYPE_VAAPI) };
                // SAFETY: as the comment above this block states.
                unsafe { assert!(!(*ctx).hwctx.is_null()) };
            }
            assert!(fx::take_events().is_empty());
            // SAFETY: the test backend's `device_init` reads only its knobs.
            let device = unsafe { uninit.init() }.unwrap();
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn failed_device_init_releases_the_context() {
            let mut uninit = uninit_device(Kind::B);
            let ctx = uninit.context_mut().as_mut_ptr();
            // SAFETY: as the comment below states, for this part of the operation.
            let hwctx_2 = unsafe { (*ctx).hwctx };
            // SAFETY: the backend struct of the context `uninit` owns.
            unsafe { (*hwctx_2.cast::<fx::DeviceHwctx>()).init_ret = -77 };
            // SAFETY: as in `uninit_device_inits_once_and_releases_on_drop`.
            assert_eq!(unsafe { uninit.init() }.err(), Some(-77));
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
            assert_eq!((count(a.as_c_ptr())).cast_unsigned(), MAX_REFERENCES);
            set_count(a.as_c_ptr(), 3);
            drop((found, itself, b));
            assert_eq!(count(a.as_c_ptr()), 1);
            drop(a);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
            fill(uninit.context_mut(), Kind::B, 8, 8, 0);
            assert_eq!(uninit.init().err(), Some(ENOSYS));
            // A size C refuses.
            let mut uninit = a.as_ref().alloc_frames().unwrap();
            fill(uninit.context_mut(), Kind::A, 0, 8, 0);
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
            let surface = unsafe { (*frame.frame.as_c_ptr()).buf[0] };
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn maps_into_another_context_and_back() {
            let a = device(Kind::A);
            let b = device(Kind::B);
            let fa = frames(&a, Kind::A);
            let fb = frames(&b, Kind::B);
            let frame = fa.as_ref().get_buffer().unwrap();
            fx::take_events();
            // SAFETY: the frame's first buffer, a live reference it owns.
            let surface = unsafe { (*frame.frame.as_c_ptr()).buf[0] };
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
            assert_eq!(opaque.addr(), 0x5EED);
            FREED.with(|f| f.set(f.get() + 1));
        }

        /// C runs a `free` C code filled in (through the raw seam) once,
        /// after the backend's uninit, when the last reference goes.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn frames_free_callbacks_run_at_the_last_release() {
            let a = device(Kind::A);
            let mut uninit = a.as_ref().alloc_frames().unwrap();
            let p = uninit.context_mut().as_mut_ptr();
            // For the unsafe operations below: the uninitialised context `uninit` owns.
            {
                // SAFETY: as the comment above this block states.
                unsafe { fx::fill_frames(p, Kind::A, 4, 4, 0) };
                // SAFETY: as the comment above this block states.
                unsafe { (*p).free = Some(count_free) };
                // SAFETY: as the comment above this block states.
                unsafe { (*p).user_opaque = ptr::without_provenance_mut(0x5EED) };
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn name_lookups_take_any_c_string() {
            let long = std::ffi::CString::new("x".repeat(4096)).unwrap();
            assert_eq!(DeviceType::from_name(&long), None);
            let names: Vec<&CStr> = NAMED.iter().filter_map(|&(t, _)| t.name()).collect();
            assert_eq!(names.len(), 14);
        }

        // Hwcontext wave-1 symbol review: the safe transfers of hardware
        // frames and the frames context's format listing, over kind A's
        // transfers (kind B has none).

        /// An NV12 software frame of `w` x `h` given zeroed buffers, its
        /// image then holding bytes derived from `seed`.
        fn nv12(w: c_int, h: c_int, seed: u8) -> FrameBox {
            let mut f = software(PixelFormat::NV12, w, h);
            let mut m = f.as_mut();
            let lines = m.as_ref().linesizes();
            for (i, rows) in [(0, h), (1, (h + 1) / 2)] {
                let ls = lines[i] as usize;
                let plane = m.plane_mut(i).unwrap();
                for r in 0..rows as usize {
                    for x in 0..w as usize {
                        plane[r * ls + x] = seed.wrapping_add((r * 31 + x * 7 + i) as u8);
                    }
                }
            }
            f
        }

        /// The bytes of the `w` x `h` image of an NV12 frame, in the order
        /// kind A's transfers run over them.
        fn nv12_image(f: FrameRef<'_>, w: c_int, h: c_int) -> Vec<u8> {
            let lines = f.linesizes();
            let mut out = Vec::new();
            for (i, rows) in [(0, h), (1, (h + 1) / 2)] {
                let ls = lines[i] as usize;
                let plane = f.plane(i).unwrap();
                for r in 0..rows as usize {
                    out.extend_from_slice(&plane[r * ls..r * ls + w as usize]);
                }
            }
            out
        }

        /// What kind A's download gives after uploads of `images` into a
        /// zeroed surface.
        fn downloaded(images: &[&[u8]]) -> Vec<u8> {
            let mut surface = [0u8; fx::SURFACE_SIZE];
            for image in images {
                for (k, b) in image.iter().enumerate() {
                    let s = &mut surface[k % fx::SURFACE_SIZE];
                    *s = s.wrapping_add(*b);
                }
            }
            let len = images.first().map_or(0, |i| i.len());
            (0..len).map(|k| surface[k % fx::SURFACE_SIZE]).collect()
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn frames_list_their_backends_transfer_formats() {
            let (a, b) = (device(Kind::A), device(Kind::B));
            let (fa, fb) = (frames(&a, Kind::A), frames(&b, Kind::B));
            for dir in [TransferDirection::From, TransferDirection::To] {
                let listed = fa.as_ref().transfer_formats(dir).unwrap();
                assert_eq!(listed.as_slice(), [PixelFormat::NV12]);
                assert_eq!(fb.as_ref().transfer_formats(dir).err(), Some(ENOSYS));
            }
            let frame = fa.as_ref().get_buffer().unwrap();
            let listed = frame.frames().transfer_formats(TransferDirection::To);
            drop((frame, fa, fb, a, b));
            assert_eq!(listed.unwrap().as_slice(), [PixelFormat::NV12]);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn hardware_frames_upload_download_and_copy() {
            let a = device(Kind::A);
            let fa = frames(&a, Kind::A);
            let mut hw = fa.as_ref().get_buffer().unwrap();
            let (src, again) = (nv12(64, 32, 3), nv12(64, 32, 200));
            hw.upload_from(src.as_ref()).unwrap();
            let image = nv12_image(src.as_ref(), 64, 32);
            let expected = downloaded(&[&image]);
            // Into a frame with buffers, and into one without, which gets
            // buffers of the context's format and size.
            let mut dst = software(PixelFormat::NV12, 64, 32);
            hw.download_to(dst.as_mut()).unwrap();
            assert_eq!(nv12_image(dst.as_ref(), 64, 32), expected);
            let mut fresh = Frame::new().unwrap();
            hw.download_to(fresh.as_mut()).unwrap();
            let r = fresh.as_ref();
            assert_eq!(
                (r.pixel_format(), r.width(), r.height()),
                (PixelFormat::NV12, 64, 32)
            );
            assert_eq!(nv12_image(r, 64, 32), expected);
            let t = Event::Transfer(Kind::A);
            assert_eq!(fx::take_events(), [Event::GetBuffer(Kind::A), t, t, t]);
            // Uploads add into the surface; copies between hardware frames
            // move it whole.
            hw.upload_from(again.as_ref()).unwrap();
            let both = downloaded(&[&image, &nv12_image(again.as_ref(), 64, 32)]);
            let mut other = fa.as_ref().get_buffer().unwrap();
            other.copy_from(&hw).unwrap();
            drop(hw);
            let mut copied = Frame::new().unwrap();
            other.download_to(copied.as_mut()).unwrap();
            assert_eq!(nv12_image(copied.as_ref(), 64, 32), both);
            // The downloaded frames outlive the contexts.
            drop((other, fa, a));
            assert_eq!(nv12_image(dst.as_ref(), 64, 32), expected);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn hardware_frame_transfers_refuse_before_c() {
            let (a, b) = (device(Kind::A), device(Kind::B));
            let (fa, fb) = (frames(&a, Kind::A), frames(&b, Kind::B));
            let mut hw = fa.as_ref().get_buffer().unwrap();
            let mut hb = fb.as_ref().get_buffer().unwrap();
            fx::take_events();
            // A frame holding a frames context is no software frame.
            let mut holder = nv12(64, 32, 0);
            drop(
                holder
                    .as_mut()
                    .replace_hw_frames_context(fa.as_ref().try_clone()),
            );
            assert_eq!(hw.download_to(holder.as_mut()), Err(EINVAL_));
            assert_eq!(hw.upload_from(holder.as_ref()), Err(EINVAL_));
            // Unlisted formats, display sizes past the surfaces, a shared
            // destination.
            let yuv = software(PixelFormat::YUV420P, 64, 32);
            assert_eq!(hw.upload_from(yuv.as_ref()), Err(EINVAL_));
            let mut wide = nv12(128, 16, 0);
            assert_eq!(hw.upload_from(wide.as_ref()), Err(EINVAL_));
            assert_eq!(hw.download_to(wide.as_mut()), Err(EINVAL_));
            let mut dst = software(PixelFormat::NV12, 64, 32);
            let shared = dst.as_ref().try_clone().unwrap();
            assert_eq!(hw.download_to(dst.as_mut()), Err(EINVAL_));
            drop(shared);
            // Kind B transfers nothing: C would call its NULL transfers.
            assert_eq!(hb.download_to(dst.as_mut()), Err(ENOSYS));
            assert_eq!(hb.upload_from(dst.as_ref()), Err(ENOSYS));
            assert_eq!(hb.copy_from(&hw), Err(ENOSYS));
            assert_eq!(hw.copy_from(&hb), Err(ENOSYS));
            assert!(fx::take_events().is_empty());
            // The backend's error, which leaves a frame without buffers so.
            let failing = FramesHwctx {
                transfer_ret: -12345,
                ..FramesHwctx::default()
            };
            let ff = frames_with(&a, Kind::A, failing, 0).unwrap();
            let mut hf = ff.as_ref().get_buffer().unwrap();
            assert_eq!(hf.upload_from(dst.as_ref()), Err(-12345));
            let mut fresh = Frame::new().unwrap();
            assert_eq!(hf.download_to(fresh.as_mut()), Err(-12345));
            assert!(!fresh.as_ref().is_counted());
            // C refuses copies into or out of a derived context, whose mapped
            // frames transfer with software frames.
            let flags = MapFlags::READ | MapFlags::WRITE;
            let derived = fa
                .as_ref()
                .derive(PixelFormat::VAAPI, a.as_ref(), flags)
                .unwrap();
            let mut hd = derived.as_ref().get_buffer().unwrap();
            assert_eq!(hd.copy_from(&hw), Err(ENOSYS));
            assert_eq!(hw.copy_from(&hd), Err(ENOSYS));
            let src = nv12(64, 32, 9);
            hd.upload_from(src.as_ref()).unwrap();
            hw.download_to(dst.as_mut()).unwrap();
            let image = nv12_image(src.as_ref(), 64, 32);
            assert_ne!(nv12_image(dst.as_ref(), 64, 32), downloaded(&[&image]));
            hd.download_to(dst.as_mut()).unwrap();
            drop((hd, derived, hf, ff, hw, hb, fa, fb, a, b));
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn frames_clones_stop_at_the_limit() {
            let a = device(Kind::A);
            let fa = frames(&a, Kind::A);
            let copy = fa.as_ref().try_clone().unwrap();
            assert!(copy.as_ref().ptr_eq(fa.as_ref()));
            assert_eq!(count(fa.as_c_ptr()), 2);
            set_count(fa.as_c_ptr(), MAX_REFERENCES);
            assert!(fa.as_ref().try_clone().is_none());
            assert_eq!(fa.as_ref().get_buffer().err(), Some(ENOMEM_));
            set_count(fa.as_c_ptr(), 2);
            drop((copy, fa, a));
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn unmapping_returns_the_mapped_frame_in_its_own_context() {
            let (a, b) = (device(Kind::A), device(Kind::B));
            let (fa, other) = (frames(&a, Kind::A), frames(&a, Kind::A));
            let flags = MapFlags::READ;
            let derived = fa
                .as_ref()
                .derive(PixelFormat::DRM_PRIME, b.as_ref(), flags)
                .unwrap();
            // A frame of a third context, mapped into the derived one.
            let x = other.as_ref().get_buffer().unwrap();
            let mapped = x.map_to(derived.as_ref(), flags).unwrap();
            assert!(mapped.frames().ptr_eq(derived.as_ref()));
            // Mapping it to the context `derived` was derived from unmaps it:
            // back to `x`'s frame, in `other`.
            let back = mapped.map_to(fa.as_ref(), flags).unwrap();
            assert!(back.frames().ptr_eq(other.as_ref()));
            assert!(!back.frames().ptr_eq(fa.as_ref()));
            assert_eq!(back.format(), PixelFormat::VAAPI);
            // The derived context's own frames unmap into `fa`.
            let own = derived.as_ref().get_buffer().unwrap();
            let own_back = own.map_to(fa.as_ref(), flags).unwrap();
            assert!(own_back.frames().ptr_eq(fa.as_ref()));
            drop((own_back, own, back, mapped, x, derived, other, fa, a, b));
            fx::take_events();
        }

        /// A frame's hardware frames context, typed since the frames context
        /// was wrapped: set and taken safely, counted as C counts it, and
        /// copied by reference with a reference counted frame. A frame that
        /// is not reference counted is refused before C copies it, which
        /// `av_frame_copy` would do through the context's backend as from a
        /// surface: kind B has no transfer routine (a NULL call, as
        /// MediaCodec's), kind A would read the frame's NULL planes as one.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn frame_hw_frames_contexts_are_typed_and_uncounted_copies_refused() {
            for kind in [Kind::A, Kind::B] {
                let d = device(kind);
                let fr = frames(&d, kind);
                let mut f = Frame::new().unwrap();
                let mut m = f.as_mut();
                m.set_pixel_format(PixelFormat::NV12);
                m.set_width(64);
                m.set_height(32);
                assert!(
                    m.replace_hw_frames_context(fr.as_ref().try_clone())
                        .is_none()
                );
                assert_eq!(fr.as_ref().ref_count(), 2);
                let held = f.as_ref().hw_frames_context().unwrap();
                assert!(held.ptr_eq(fr.as_ref()));
                assert_eq!(held.context().width(), 64);
                // Not reference counted: every copy is refused, before C.
                assert!(f.as_ref().try_clone().is_none());
                let mut dst = Frame::new().unwrap();
                assert_eq!(dst.as_mut().try_clone_from(f.as_ref()), Err(ENOSYS));
                let mut mem = [0u8; 64 * 48];
                // For the unsafe operations below: `mem` holds a 64 x 32 NV12 image at line size 64
                // and outlives every use of the plane pointers below.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { f.as_mut().set_data_ptr(0, mem.as_mut_ptr()) };
                    // SAFETY: as the comment below states, for this part of the operation.
                    let as_mut_ptr_add = unsafe { mem.as_mut_ptr().add(64 * 32) };
                    // SAFETY: as the comment above this block states.
                    unsafe { f.as_mut().set_data_ptr(1, as_mut_ptr_add) };
                }
                f.as_mut().set_linesize(0, 64);
                f.as_mut().set_linesize(1, 64);
                assert_eq!(dst.as_mut().assign(f.as_ref()), Err(ENOSYS));
                assert!(f.as_ref().try_clone().is_none());
                assert_eq!(fx::take_events(), []);
                // Without planes, `assign` copies no data: it takes the
                // properties and a reference to the context, as C does.
                // For the unsafe operations below: NULL planes are always allowed.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { f.as_mut().set_data_ptr(0, ptr::null_mut()) };
                    // SAFETY: as the comment above this block states.
                    unsafe { f.as_mut().set_data_ptr(1, ptr::null_mut()) };
                }
                dst.as_mut().assign(f.as_ref()).unwrap();
                assert!(
                    dst.as_ref()
                        .hw_frames_context()
                        .unwrap()
                        .ptr_eq(fr.as_ref())
                );
                assert_eq!(fr.as_ref().ref_count(), 3);
                // Reference counted: copied by reference, the context too.
                f.as_mut()
                    .set_buffer(0, Some(BufferReference::alloc_zeroed(64).unwrap()));
                let copy = f.as_ref().try_clone().unwrap();
                assert!(
                    copy.as_ref()
                        .hw_frames_context()
                        .unwrap()
                        .ptr_eq(fr.as_ref())
                );
                assert_eq!(fr.as_ref().ref_count(), 4);
                // The routines C would run through the context refuse it.
                let mut sw = nv12(64, 32, 1);
                assert_eq!(sw.as_mut().copy_data_from(f.as_ref()), Err(ENOSYS));
                assert_eq!(f.as_mut().make_writable(), Err(ENOSYS));
                // Taken out, the reference is the caller's; replaced, the
                // old one comes back.
                let taken = f.as_mut().take_hw_frames_context().unwrap();
                assert!(taken.as_ref().ptr_eq(fr.as_ref()));
                assert!(f.as_ref().hw_frames_context().is_none());
                let back = dst.as_mut().replace_hw_frames_context(Some(taken)).unwrap();
                assert_eq!(fr.as_ref().ref_count(), 4);
                drop((back, copy, dst, f));
                assert_eq!(fr.as_ref().ref_count(), 1);
                assert_eq!(fx::take_events(), []);
                drop((fr, d));
                fx::take_events();
            }
        }
    }

    /// Device contexts and frame constraints over the fake backend of
    /// `tests/common/hwdevice.rs` (hwcontext wave 1, batch 0).
    mod device_context {
        use core::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        use super::super::hwdevice::{
            self, BARE_TYPE, Events, FakeConfig, FakeHw, HW_FORMAT, SIZES, SW_FORMATS,
        };
        use super::*;

        /// A fresh fake context under construction, counting into `events`.
        fn builder(events: &Arc<Events>) -> UninitDevice {
            // SAFETY: `fake_device` returns NULL or the only reference to an
            // uninitialised fake context with no hooks.
            let mut b = unsafe { UninitDevice::from_c(hwdevice::fake_device()) }.unwrap();
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            unsafe { b.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_events(Arc::as_ptr(events));
            b
        }

        /// The C `free` and `user_opaque` of the context behind `b`.
        fn raw_hooks(b: &UninitDevice) -> (bool, bool) {
            let p = b.context().as_ptr();
            // SAFETY: reads of the builder's live context.
            (
                unsafe { (*p).free }.is_some(),
                !unsafe { (*p).user_opaque }.is_null(),
            )
        }

        /// The number of references to the context `arc` holds.
        fn count(arc: &DeviceReferenceBox) -> u32 {
            // SAFETY: a live reference.
            unsafe { (ffi::av_buffer_get_ref_count(arc.as_c_ptr())).cast_unsigned() }
        }

        /// A value that counts its drops.
        struct Dropped(Arc<AtomicUsize>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn builder_getters_read_the_allocated_fields() {
            let events = Arc::new(Events::default());
            let b = builder(&events);
            let ctx = b.context();
            assert_eq!(ctx.device_type(), DeviceType::VAAPI);
            let class = ctx.class().unwrap();
            assert_eq!(class.name(), Some(c"AVHWDeviceContext"));
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            let hw = unsafe { ctx.hwctx::<FakeHw>() }.unwrap();
            assert_eq!(hw.value(), 0);
            assert!(!hw.initialised());
            assert!(!ctx.has_free_callback());
            assert!(ctx.user_data::<u32>().is_none());
            assert!(format!("{:?}", b.context()).contains("device_type: VAAPI"));
            let mut b = b;
            assert!(
                format!("{:?}", b.context_mut()).starts_with("DeviceContext { device_type: VAAPI")
            );
            drop(b);
            assert_eq!(events.get(), (0, 1, 0));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn bare_backend_has_no_hardware_context() {
            let raw = hwdevice::fake_device_of(&BARE_TYPE, ffi::AV_HWDEVICE_TYPE_DRM);
            // SAFETY: the only reference to an uninitialised fake context.
            let mut b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert_eq!(b.context().device_type(), DeviceType::DRM);
            // SAFETY: a bare context has no hardware context to mistype.
            assert!(unsafe { b.context().hwctx::<FakeHw>() }.is_none());
            // SAFETY: as above.
            assert!(unsafe { b.context_mut().into_hwctx::<FakeHw>() }.is_none());
            // SAFETY: the bare backend has no `device_init` to feed.
            let arc = unsafe { b.init() }.unwrap();
            assert!(arc.as_ref().frames_constraints().is_none());
        }

        #[test]
        fn null_references_are_refused() {
            // SAFETY: NULL is accepted and refused.
            assert!(unsafe { UninitDevice::from_c(ptr::null_mut()) }.is_none());
            // SAFETY: as above.
            assert!(unsafe { DeviceReferenceBox::from_c(ptr::null_mut()) }.is_none());
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn user_data_is_typed_replaced_and_taken() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            let drops = Arc::new(AtomicUsize::new(0));
            assert!(
                b.context_mut()
                    .set_user_data(Dropped(drops.clone()))
                    .is_ok()
            );
            assert_eq!(raw_hooks(&b), (true, true));
            assert!(b.context().user_data::<Dropped>().is_some());
            assert!(b.context().user_data::<u8>().is_none());
            // Data alone runs no callback of the caller's.
            assert!(!b.context().has_free_callback());
            assert!(b.context_mut().set_user_data(7u64).is_ok());
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(b.context().user_data::<u64>(), Some(&7));
            let taken = b.context_mut().take_user_data().unwrap();
            assert_eq!(taken.downcast_ref::<u64>(), Some(&7));
            assert!(b.context_mut().take_user_data().is_none());
            assert!(b.context_mut().take_free_callback().is_none());
            // Empty hooks are released and both fields cleared.
            assert_eq!(raw_hooks(&b), (false, false));
            assert!(b.context().user_data::<u64>().is_none());
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn free_callback_runs_once_after_uninit_and_sees_the_data() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            let seen = Arc::new(Mutex::new(Vec::new()));
            let drops = Arc::new(AtomicUsize::new(0));
            b.context_mut()
                .set_user_data(Dropped(drops.clone()))
                .ok()
                .unwrap();
            let (log, ev, dr) = (seen.clone(), events.clone(), drops.clone());
            b.context_mut()
                .set_free_callback(move |mut ctx: DeviceContextMut<'_>| {
                    let data = ctx.as_ref().user_data::<Dropped>().is_some();
                    log.lock()
                        .unwrap()
                        .push((ev.get(), data, dr.load(Ordering::SeqCst)));
                    // A callback installed now is dropped uncalled.
                    let late = log.clone();
                    ctx.set_free_callback(move |_| late.lock().unwrap().clear())
                        .ok()
                        .unwrap();
                })
                .ok()
                .unwrap();
            assert!(b.context().has_free_callback());
            // SAFETY: the fake backend's `device_init` accepts any payload.
            let arc = unsafe { b.init() }.unwrap();
            let other = arc.clone();
            assert_eq!(count(&arc), 2);
            drop(arc);
            assert!(seen.lock().unwrap().is_empty());
            drop(other);
            assert_eq!(*seen.lock().unwrap(), [((1, 1, 0), true, 0)]);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn a_callback_may_take_or_replace_everything() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            let drops = Arc::new(AtomicUsize::new(0));
            b.context_mut()
                .set_user_data(Dropped(drops.clone()))
                .ok()
                .unwrap();
            let dr = drops.clone();
            b.context_mut()
                .set_free_callback(move |mut ctx: DeviceContextMut<'_>| {
                    // Takes the data out and drops it itself, then leaves new
                    // data behind, dropped after it returns.
                    drop(ctx.take_user_data());
                    assert!(ctx.take_free_callback().is_none());
                    ctx.set_user_data(Dropped(dr)).ok().unwrap();
                })
                .ok()
                .unwrap();
            drop(b);
            assert_eq!(drops.load(Ordering::SeqCst), 2);

            let mut b = builder(&events);
            b.context_mut()
                .set_free_callback(|mut ctx: DeviceContextMut<'_>| {
                    assert!(ctx.take_free_callback().is_none());
                    assert!(ctx.take_user_data().is_none());
                })
                .ok()
                .unwrap();
            drop(b);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn replaced_and_taken_callbacks_are_not_called() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            let calls = Arc::new(AtomicUsize::new(0));
            let c = calls.clone();
            b.context_mut()
                .set_free_callback(move |_| {
                    c.fetch_add(100, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
            let c = calls.clone();
            b.context_mut()
                .set_free_callback(move |_| {
                    c.fetch_add(1, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
            let taken = b.context_mut().take_free_callback().unwrap();
            assert_eq!(raw_hooks(&b), (false, false));
            assert!(!b.context().has_free_callback());
            drop(b);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            // A taken callback is the caller's to call.
            let mut other = builder(&events);
            taken(other.context_mut());
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn foreign_hooks_are_neither_read_nor_replaced() {
            let freed = AtomicUsize::new(0);
            let raw = hwdevice::fake_device();
            // SAFETY: the only reference to a fresh fake context; `freed`
            // outlives it.
            unsafe { hwdevice::set_foreign_free(raw, &raw const freed) };
            // SAFETY: as in `builder`; the hooks are a backend's.
            let mut b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert!(b.context().has_free_callback());
            assert!(b.context().user_data::<hwdevice::ForeignPriv>().is_none());
            assert_eq!(b.context_mut().set_user_data(3u8), Err(3));
            assert!(b.context_mut().set_free_callback(|_| ()).is_err());
            assert!(b.context_mut().take_user_data().is_none());
            assert!(b.context_mut().take_free_callback().is_none());
            drop(b);
            assert_eq!(freed.load(Ordering::SeqCst), 1);

            // A cookie without a callback is someone else's as well.
            let raw = hwdevice::fake_device();
            let mut cookie = 0u8;
            // SAFETY: as the comment below states, for this part of the operation.
            let context_3 = unsafe { hwdevice::context(raw) };
            // SAFETY: a write of the fresh context's `user_opaque`.
            unsafe { (*context_3).user_opaque = (&raw mut cookie).cast() };
            // SAFETY: as in `builder`.
            let mut b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert!(b.context_mut().set_user_data(1u8).is_err());
            assert!(!b.context().has_free_callback());
        }

        /// How many times [`backend_free_alone`] ran.
        static FREED_ALONE: AtomicUsize = AtomicUsize::new(0);

        /// A backend's `free` that keeps nothing in `user_opaque`, as DRM's,
        /// OpenCL's, D3D12VA's and Vulkan's device creation install.
        unsafe extern "C" fn backend_free_alone(ctx: *mut ffi::AVHWDeviceContext) {
            // SAFETY: C passes the live context it releases.
            assert!(unsafe { (*ctx).user_opaque.is_null() });
            FREED_ALONE.fetch_add(1, Ordering::SeqCst);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn a_backend_free_alone_is_neither_read_nor_replaced() {
            let raw = hwdevice::fake_device();
            // SAFETY: as the comment below states, for this part of the operation.
            let context_2 = unsafe { hwdevice::context(raw) };
            // SAFETY: a write of the fresh context's `free`, as
            // drm_device_create does, leaving `user_opaque` NULL.
            unsafe { (*context_2).free = Some(backend_free_alone) };
            // SAFETY: as in `builder`; the callback is a backend's.
            let mut b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            assert!(b.context().has_free_callback());
            assert_eq!(b.context_mut().set_user_data(5u16), Err(5));
            assert!(b.context_mut().set_free_callback(|_| ()).is_err());
            assert!(b.context_mut().take_user_data().is_none());
            assert!(b.context_mut().take_free_callback().is_none());
            assert_eq!(raw_hooks(&b), (true, false));
            drop(b);
            assert_eq!(FREED_ALONE.load(Ordering::SeqCst), 1);
        }

        /// The hooks over a context in Rust storage, with no C call, so that
        /// Miri checks the leaked box's provenance across the setters'
        /// reborrows and the trampoline.
        #[test]
        fn hooks_on_rust_storage_without_c() {
            // SAFETY: an all-zero context (NULL pointers, no callback, type
            // NONE) is a valid `AVHWDeviceContext` value.
            let zeroed = unsafe { core::mem::zeroed::<ffi::AVHWDeviceContext>() };
            let ctx = Box::into_raw(Box::new(zeroed));
            let drops = Arc::new(AtomicUsize::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            for round in 0..3 {
                // SAFETY: the boxed context, reached only through this handle
                // until the trampoline runs.
                let mut m = unsafe { DeviceContextMut::from_ptr(ctx) }.unwrap();
                m.set_user_data(Dropped(drops.clone())).ok().unwrap();
                assert!(m.as_ref().user_data::<Dropped>().is_some());
                let c = calls.clone();
                m.set_free_callback(move |mut ctx: DeviceContextMut<'_>| {
                    assert!(ctx.as_ref().user_data::<Dropped>().is_some());
                    c.fetch_add(1, Ordering::SeqCst);
                    if round == 1 {
                        drop(ctx.take_user_data());
                    }
                })
                .ok()
                .unwrap();
                assert!(m.as_ref().has_free_callback());
                if round == 2 {
                    drop(m.take_free_callback());
                    drop(m.take_user_data());
                    assert_eq!(m.as_ref().device_type(), DeviceType::NONE);
                    continue;
                }
                // SAFETY: as hwdevice_ctx_free calls it, with the context
                // nothing else reaches.
                unsafe { release_hooks(ctx) };
                // SAFETY: a read of the boxed context.
                assert!(
                    // SAFETY: as the comment above this statement states.
                    unsafe { (*ctx).free }.is_none() && unsafe { (*ctx).user_opaque }.is_null()
                );
            }
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(drops.load(Ordering::SeqCst), 3);
            // SAFETY: the box leaked above, with no hooks left.
            drop(unsafe { Box::from_raw(ctx) });
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn release_hooks_tolerates_a_cleared_cookie() {
            let raw = hwdevice::fake_device();
            // For the unsafe operations below: the only reference to a fresh fake context: a `free`
            // of `release_hooks` with no cookie must do nothing.
            {
                // SAFETY: as the comment above this block states.
                let ctx = unsafe { hwdevice::context(raw) };
                // SAFETY: as the comment above this block states.
                unsafe { (*ctx).free = Some(RELEASE_HOOKS) };
                // SAFETY: as the comment above this block states.
                unsafe { ffi::av_buffer_unref(&mut { raw }) };
            }
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn init_reports_failure_and_releases_the_context() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            unsafe { b.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_init_ret(-22);
            let called = Arc::new(AtomicUsize::new(0));
            let c = called.clone();
            b.context_mut()
                .set_free_callback(move |_| {
                    c.fetch_add(1, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
            // SAFETY: the fake backend's `device_init` accepts any payload.
            assert_eq!(unsafe { b.init() }.unwrap_err(), -22);
            assert_eq!(events.get(), (1, 1, 0));
            assert_eq!(called.load(Ordering::SeqCst), 1);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn arcs_share_one_context_and_hand_it_back_to_c() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            unsafe { b.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_value(41);
            // SAFETY: the fake backend's `device_init` accepts any payload.
            let arc = unsafe { b.init() }.unwrap();
            // SAFETY: as above.
            let hw = unsafe { arc.as_ref().context().hwctx::<FakeHw>() }.unwrap();
            assert!(hw.initialised());
            assert_eq!(hw.value(), 41);
            let copy = arc.as_ref().try_clone().unwrap();
            assert_eq!(count(&arc), 2);
            assert_eq!(
                copy.as_ref().context().as_ptr(),
                arc.as_ref().context().as_ptr()
            );
            assert!(format!("{:?}", copy.as_ref().context()).starts_with("DeviceContext {"));
            let raw = copy.into_c();
            // SAFETY: the reference `into_c` gave up, to an initialised
            // context.
            let back = unsafe { DeviceReferenceBox::from_c(raw) }.unwrap();
            assert_eq!(count(&back), 2);
            drop((arc, back));
            assert_eq!(events.get(), (1, 1, 0));

            // A builder's reference goes back to C the same way.
            let b = builder(&events);
            let raw = b.into_c();
            // SAFETY: the reference `into_c` gave up, still uninitialised.
            let b = unsafe { UninitDevice::from_c(raw) }.unwrap();
            drop(b);
            assert_eq!(events.get(), (1, 2, 0));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn device_constraints_come_from_the_backend() {
            let events = Arc::new(Events::default());
            for nb_sw in [-1, 0, 2, 4] {
                let mut b = builder(&events);
                // SAFETY: the fake context's hardware context is a `FakeHwctx`.
                unsafe { b.context_mut().into_hwctx::<FakeHw>() }
                    .unwrap()
                    .set_nb_sw(nb_sw);
                // SAFETY: the fake backend's `device_init` accepts any payload.
                let arc = unsafe { b.init() }.unwrap();
                let c = arc.as_ref().frames_constraints().unwrap();
                let c = c.as_ref();
                let hw: Vec<_> = c.valid_hw_formats().unwrap().elems().collect();
                assert_eq!(hw, [PixelFormat::from_raw(HW_FORMAT)]);
                match c.valid_sw_formats() {
                    None => assert_eq!(nb_sw, -1),
                    Some(sw) => {
                        let sw: Vec<_> = sw.elems().map(PixelFormat::into_raw).collect();
                        assert_eq!(sw, SW_FORMATS[..nb_sw as usize]);
                    }
                }
                let sizes = [c.min_width(), c.min_height(), c.max_width(), c.max_height()];
                assert_eq!(sizes, SIZES);

                let mut config = arc.as_ref().alloc_hw_config().unwrap();
                // SAFETY: the fake backend's zeroed configuration, which the
                // config owns.
                unsafe {
                    let p = config.as_mut_ptr().cast::<FakeConfig>();
                    (*p).hw_format = ffi::AV_PIX_FMT_CUDA;
                }
                let c = arc.as_ref().frames_constraints_for_config(&config);
                let hw: Vec<_> = c
                    .unwrap()
                    .as_ref()
                    .valid_hw_formats()
                    .unwrap()
                    .elems()
                    .collect();
                assert_eq!(hw, [PixelFormat::CUDA]);
            }
            assert_eq!(events.get(), (4, 4, 8));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn configurations_of_another_device_type_are_refused() {
            let events = Arc::new(Events::default());
            // SAFETY: the fake backend's `device_init` accepts any payload.
            let vaapi = unsafe { builder(&events).init() }.unwrap();
            let config = vaapi.as_ref().alloc_hw_config().unwrap();
            let raw = hwdevice::fake_device_of(&hwdevice::FAKE_TYPE, ffi::AV_HWDEVICE_TYPE_DRM);
            // SAFETY: the only reference to a fresh, uninitialised fake
            // context.
            let mut drm = unsafe { UninitDevice::from_c(raw) }.unwrap();
            // SAFETY: the fake context's hardware context is a `FakeHwctx`.
            unsafe { drm.context_mut().into_hwctx::<FakeHw>() }
                .unwrap()
                .set_events(Arc::as_ptr(&events));
            // SAFETY: as above.
            let drm = unsafe { drm.init() }.unwrap();
            assert!(
                drm.as_ref()
                    .frames_constraints_for_config(&config)
                    .is_none()
            );
            assert_eq!(events.get().2, 0);
            assert!(
                vaapi
                    .as_ref()
                    .frames_constraints_for_config(&config)
                    .is_some()
            );
            assert_eq!(events.get().2, 1);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn failing_backend_constraints_are_none() {
            let events = Arc::new(Events::default());
            let mut b = builder(&events);
            {
                // SAFETY: the fake context's hardware context is a `FakeHwctx`.
                let mut hw = unsafe { b.context_mut().into_hwctx::<FakeHw>() }.unwrap();
                hw.set_nb_sw(3);
                hw.set_constraints_ret(-12);
            }
            // SAFETY: the fake backend's `device_init` accepts any payload.
            let arc = unsafe { b.init() }.unwrap();
            // C frees the lists the backend filled before failing.
            assert!(arc.as_ref().frames_constraints().is_none());
            assert_eq!(events.get().2, 1);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn new_constraints_start_as_c_does() {
            let c = FramesConstraints::new().unwrap();
            let r = c.as_ref();
            assert!(r.valid_hw_formats().is_none());
            assert!(r.valid_sw_formats().is_none());
            assert_eq!([r.min_width(), r.min_height()], [0, 0]);
            assert_eq!([r.max_width(), r.max_height()], [i32::MAX, i32::MAX]);
            assert!(format!("{r:?}").starts_with("FramesConstraints { valid_hw_formats: None"));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn constraint_setters_copy_terminate_and_free() {
            let mut c = FramesConstraints::new().unwrap();
            let mut m = c.as_mut();
            let list = [PixelFormat::NV12, PixelFormat::YUV420P];
            m.set_valid_sw_formats(Some(&list)).unwrap();
            m.set_valid_hw_formats(Some(&[
                PixelFormat::VAAPI,
                PixelFormat::NONE,
                PixelFormat::CUDA,
            ]))
            .unwrap();
            assert_eq!(
                m.as_ref()
                    .valid_sw_formats()
                    .unwrap()
                    .elems()
                    .collect::<Vec<_>>(),
                list
            );
            assert_eq!(
                m.as_ref()
                    .valid_hw_formats()
                    .unwrap()
                    .elems()
                    .collect::<Vec<_>>(),
                [PixelFormat::VAAPI]
            );
            // The copy is terminated in C's way.
            // SAFETY: a read of the live list and its terminator.
            let raw = unsafe { (*m.as_ref().as_ptr()).valid_sw_formats };
            // SAFETY: `2` is within the block `raw` points to, as the operation below
            // relies on, so the offset stays in bounds; nothing is accessed here.
            let raw_at = unsafe { raw.add(2) };
            // SAFETY: three elements, the terminator included.
            let terminator = unsafe { raw_at.read() };
            assert_eq!(terminator, ffi::AV_PIX_FMT_NONE);
            // An empty list is not an unknown one.
            m.set_valid_sw_formats(Some(&[])).unwrap();
            assert_eq!(m.as_ref().valid_sw_formats().map(|l| l.len()), Some(0));
            assert_eq!(m.take_valid_sw_formats().map(|v| v.len()), Some(0));
            assert!(m.as_ref().valid_sw_formats().is_none());
            assert!(m.take_valid_sw_formats().is_none());
            let taken = m.take_valid_hw_formats().unwrap();
            assert_eq!(taken.as_slice(), [PixelFormat::VAAPI]);
            m.set_valid_hw_formats(Some(&list)).unwrap();
            m.set_valid_hw_formats(None).unwrap();
            assert!(m.as_ref().valid_hw_formats().is_none());
            m.set_min_width(1);
            m.set_min_height(2);
            m.set_max_width(3);
            m.set_max_height(4);
            let r = c.as_ref();
            assert_eq!(
                [r.min_width(), r.min_height(), r.max_width(), r.max_height()],
                [1, 2, 3, 4]
            );
            assert!(format!("{:?}", c.as_mut()).contains("min_height: 2"));
        }

        /// Runs the test `name` alone in a child process, where it may lower
        /// the process-wide allocation limit; true in that child.
        fn isolated(name: &str) -> bool {
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

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn constraints_keep_what_they_have_on_enomem() {
            if !isolated(
                "libavutil::hwcontext::unit_tests::device_context::constraints_keep_what_they_have_on_enomem",
            ) {
                return;
            }
            let mut c = FramesConstraints::new().unwrap();
            c.as_mut()
                .set_valid_sw_formats(Some(&[PixelFormat::NV12]))
                .unwrap();
            // 2 KiB of formats against a 1 KiB limit: enough to fail, and
            // small enough for BorrowSanitizer, which retags every element
            // the list's scan for its terminator visits (64 Ki of them ran
            // away past half an hour and 40 GB there).
            let big = vec![PixelFormat::YUV420P; 1 << 9];
            assert!(size_of_val(&big[..]) > 1 << 10);
            crate::libavutil::libavutil::mem::set_max_alloc(1 << 10);
            let ret = c.as_mut().set_valid_sw_formats(Some(&big));
            crate::libavutil::libavutil::mem::set_max_alloc(i32::MAX as usize);
            assert_eq!(ret, Err(ENOMEM));
            let sw: Vec<_> = c.as_ref().valid_sw_formats().unwrap().elems().collect();
            assert_eq!(sw, [PixelFormat::NV12]);
            // The structure itself (32 bytes) fails, for both constructors.
            let events = Arc::new(Events::default());
            // SAFETY: the fake backend's `device_init` accepts any payload.
            let arc = unsafe { builder(&events).init() }.unwrap();
            crate::libavutil::libavutil::mem::set_max_alloc(16);
            let new = FramesConstraints::new();
            let queried = arc.as_ref().frames_constraints();
            crate::libavutil::libavutil::mem::set_max_alloc(i32::MAX as usize);
            assert!(new.is_none() && queried.is_none());
            assert_eq!(events.get().2, 0);
        }
    }

    /// The frames context wrapper (hwcontext wave 2, batch 0) over the test
    /// backends of `tests/common/hwbackend.rs`: its getters and setters, the
    /// Rust hooks in `free`/`user_opaque` and the pool they own, and their
    /// release.
    mod frames_context {
        use core::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        use super::super::backend::{self as fx, Event, FramesHw, FramesHwctx, Kind};
        use super::*;
        use crate::libavutil::libavutil::buffer::{BufferPool, BufferReference};

        /// An initialised test device of `kind`, adopted as a box, with its
        /// events taken.
        fn device(kind: Kind) -> DeviceReferenceBox {
            // SAFETY: the only reference to an initialised test device
            // context, built as `av_hwdevice_ctx_alloc` builds one.
            let device = unsafe { DeviceReferenceBox::from_c(fx::device(kind)) }.unwrap();
            fx::take_events();
            device
        }

        /// Fills `ctx` for an init of backend `kind` that succeeds.
        fn fill(mut ctx: FramesContextMut<'_>, kind: Kind, pool_size: c_int) {
            ctx.set_format(PixelFormat::from_raw(kind.format()));
            ctx.set_sw_format(PixelFormat::NV12);
            ctx.set_width(64);
            ctx.set_height(32);
            ctx.set_initial_pool_size(pool_size);
        }

        /// The C `free`, `user_opaque` and `pool` of the context behind `u`.
        fn raw_fields(u: &UninitFrames) -> (bool, bool, bool) {
            let p = u.context().as_ptr();
            // SAFETY: reads of the live context `u` holds.
            (
                unsafe { (*p).free }.is_some(),
                !unsafe { (*p).user_opaque }.is_null(),
                !unsafe { (*p).pool }.is_null(),
            )
        }

        /// A value that counts its drops.
        struct Dropped(Arc<AtomicUsize>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        /// A pool of surface-sized bytes counting its allocations into
        /// `allocs`, whose allocator holds a [`Dropped`] counting into
        /// `drops`: the pool is gone once that count moves.
        fn counting_pool(allocs: &Arc<AtomicUsize>, drops: &Arc<AtomicUsize>) -> BufferPoolBox {
            let (a, token) = (allocs.clone(), Dropped(drops.clone()));
            // SAFETY: each buffer is a fresh `size`-byte allocation; the
            // allocator never calls into the pool.
            unsafe {
                BufferPool::with_allocator(fx::SURFACE_SIZE, move |n| {
                    let _ = &token;
                    a.fetch_add(1, Ordering::SeqCst);
                    BufferReference::alloc(n)
                })
            }
            .unwrap()
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn allocated_fields_read_as_c_set_them() {
            let a = device(Kind::A);
            let mut u = a.as_ref().alloc_frames().unwrap();
            let ctx = u.context();
            let class = ctx.class().unwrap().name().unwrap();
            assert_eq!(class, c"AVHWFramesContext");
            assert!(ctx.device().ptr_eq(a.as_ref()));
            assert_eq!(ctx.device_context().device_type(), DeviceType::VAAPI);
            assert_eq!(
                (ctx.format(), ctx.sw_format()),
                (PixelFormat::NONE, PixelFormat::NONE)
            );
            assert_eq!(
                (ctx.width(), ctx.height(), ctx.initial_pool_size()),
                (0, 0, 0)
            );
            assert!(!ctx.has_pool() && !ctx.has_free_callback());
            // SAFETY: the context has no pool to read.
            assert!(unsafe { ctx.pool() }.is_none());
            assert!(ctx.user_data::<u8>().is_none());
            // SAFETY: the test backends' frames struct is a `FramesHwctx`.
            let hw = unsafe { ctx.hwctx::<FramesHw>() }.unwrap().get();
            assert_eq!(hw.init_ret, 0);
            assert_eq!(
                format!("{ctx:?}"),
                "FramesContext { format: NONE, sw_format: NONE, width: 0, height: 0, \
                 initial_pool_size: 0, has_pool: false, has_free_callback: false, .. }"
            );
            assert!(format!("{:?}", u.context_mut()).starts_with("FramesContext { format: NONE"));
            assert_eq!(raw_fields(&u), (false, false, false));
            drop(u);
            assert_eq!(fx::take_events(), [Event::FramesUninit(Kind::A)]);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn setters_reach_c_and_the_initialised_context() {
            let a = device(Kind::A);
            let mut u = a.as_ref().alloc_frames().unwrap();
            fill(u.context_mut(), Kind::A, 2);
            let p = u.context().as_ptr();
            // SAFETY: reads of the live context `u` holds.
            let raw = (
                unsafe { (*p).format },
                unsafe { (*p).sw_format },
                unsafe { (*p).width },
                unsafe { (*p).height },
            );
            assert_eq!(raw, (ffi::AV_PIX_FMT_VAAPI, ffi::AV_PIX_FMT_NV12, 64, 32));
            // SAFETY: as above.
            assert_eq!(unsafe { (*p).initial_pool_size }, 2);
            let frames = u.init().unwrap();
            let ctx = frames.as_ref().context();
            assert_eq!(
                (ctx.format(), ctx.sw_format()),
                (PixelFormat::VAAPI, PixelFormat::NV12)
            );
            assert_eq!(
                (ctx.width(), ctx.height(), ctx.initial_pool_size()),
                (64, 32, 2)
            );
            // C took the backend's pool.
            assert!(ctx.has_pool());
            assert!(ctx.device().ptr_eq(a.as_ref()));
            let frame = frames.as_ref().get_buffer().unwrap();
            assert_eq!(frame.frames().context().width(), 64);
            // SAFETY: the test backends' pools hold zeroed bytes.
            let pool = unsafe { ctx.pool() }.unwrap();
            let buf = pool.get().unwrap();
            assert_eq!(buf.len(), fx::SURFACE_SIZE);
            drop((buf, frame, frames));
            drop(a);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn create_frames_builds_initialised_contexts() {
            let a = device(Kind::A);
            let frames = a
                .as_ref()
                .create_frames(PixelFormat::VAAPI, PixelFormat::NV12, 32, 16, 2)
                .unwrap();
            let ctx = frames.as_ref().context();
            assert_eq!(
                (ctx.format(), ctx.sw_format()),
                (PixelFormat::VAAPI, PixelFormat::NV12)
            );
            assert_eq!(
                (ctx.width(), ctx.height(), ctx.initial_pool_size()),
                (32, 16, 2)
            );
            assert!(ctx.has_pool() && !ctx.has_free_callback());
            assert_eq!(
                fx::take_events(),
                [
                    Event::FramesInit(Kind::A),
                    Event::GetBuffer(Kind::A),
                    Event::GetBuffer(Kind::A)
                ]
            );
            let a_ = a.as_ref();
            let refused = a_.create_frames(PixelFormat::DRM_PRIME, PixelFormat::NV12, 32, 16, 0);
            assert_eq!(refused.err(), Some(-(ffi::ENOSYS.cast_signed())));
            let refused = a_.create_frames(PixelFormat::VAAPI, PixelFormat::NV12, 0, 16, 0);
            assert_eq!(refused.err(), Some(-(ffi::EINVAL.cast_signed())));
            assert_eq!(
                fx::take_events(),
                [Event::FramesUninit(Kind::A), Event::FramesUninit(Kind::A)]
            );
            drop((frames, a));
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn user_data_is_typed_replaced_and_taken() {
            let a = device(Kind::B);
            let mut u = a.as_ref().alloc_frames().unwrap();
            let drops = Arc::new(AtomicUsize::new(0));
            let mut m = u.context_mut();
            m.set_user_data(Dropped(drops.clone())).ok().unwrap();
            assert!(m.as_ref().user_data::<Dropped>().is_some());
            assert!(m.as_ref().user_data::<u32>().is_none());
            assert!(!m.as_ref().has_free_callback());
            m.set_user_data(7u32).unwrap();
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            assert_eq!(m.as_ref().user_data::<u32>(), Some(&7));
            assert_eq!(raw_fields(&u), (true, true, false));
            let taken = u.context_mut().take_user_data().unwrap();
            assert_eq!(taken.downcast_ref::<u32>(), Some(&7));
            // The empty hooks are gone, and the fields as allocated.
            assert_eq!(raw_fields(&u), (false, false, false));
            assert!(u.context_mut().take_user_data().is_none());
            assert!(u.context_mut().take_free_callback().is_none());
            assert!(u.context_mut().take_pool().is_none());
            drop(u);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn free_callback_runs_once_after_uninit_and_the_pool() {
            let a = device(Kind::A);
            let (allocs, pool_drops) = (Arc::default(), Arc::new(AtomicUsize::new(0)));
            let data_drops = Arc::new(AtomicUsize::new(0));
            let seen = Arc::new(Mutex::new(None));
            let mut u = a.as_ref().alloc_frames().unwrap();
            fill(u.context_mut(), Kind::A, 2);
            let mut m = u.context_mut();
            // SAFETY: the test backends take any bytes for a surface.
            unsafe { m.set_pool(counting_pool(&allocs, &pool_drops)) }
                .ok()
                .unwrap();
            m.set_user_data(Dropped(data_drops.clone())).ok().unwrap();
            let (s, pd, dd) = (seen.clone(), pool_drops.clone(), data_drops.clone());
            m.set_free_callback(move |ctx: FramesContextMut<'_>| {
                let c = ctx.as_ref();
                *s.lock().unwrap() = Some((
                    fx::take_events(),
                    pd.load(Ordering::SeqCst),
                    dd.load(Ordering::SeqCst),
                    c.has_pool(),
                    c.user_data::<Dropped>().is_some(),
                    c.device().context().device_type(),
                ));
            })
            .ok()
            .unwrap();
            assert!(m.as_ref().has_free_callback());
            let frames = u.init().unwrap();
            // The preallocation drew from the Rust pool.
            assert_eq!(allocs.load(Ordering::SeqCst), 2);
            let frame = frames.as_ref().get_buffer().unwrap();
            assert_eq!(allocs.load(Ordering::SeqCst), 2);
            assert!(frames.as_ref().context().has_free_callback());
            fx::take_events();
            drop(frames);
            assert!(seen.lock().unwrap().is_none());
            drop(frame);
            let seen = seen.lock().unwrap().take().unwrap();
            assert_eq!(
                seen,
                (
                    vec![Event::FramesUninit(Kind::A)],
                    1,
                    0,
                    false,
                    true,
                    DeviceType::VAAPI
                )
            );
            assert_eq!(data_drops.load(Ordering::SeqCst), 1);
            drop(a);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn rust_pools_are_replaced_and_taken() {
            let a = device(Kind::A);
            let allocs = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let mut u = a.as_ref().alloc_frames().unwrap();
            // SAFETY: the test backends take any bytes for a surface.
            unsafe { u.context_mut().set_pool(counting_pool(&allocs, &drops)) }
                .ok()
                .unwrap();
            assert_eq!(raw_fields(&u), (true, true, true));
            // SAFETY: as above.
            unsafe { u.context_mut().set_pool(counting_pool(&allocs, &drops)) }
                .ok()
                .unwrap();
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            let ours = u.context().as_ptr();
            // SAFETY: a read of the live context.
            let field = unsafe { (*ours).pool };
            // SAFETY: the pool holds bytes.
            let lent = unsafe { u.context().pool() }.unwrap();
            assert!(ptr::eq(lent.as_ptr(), field));
            let pool = u.context_mut().take_pool().unwrap();
            assert!(ptr::eq(pool.as_c_ptr(), field));
            assert_eq!(raw_fields(&u), (false, false, false));
            assert!(u.context_mut().take_pool().is_none());
            // A taken pool still works, and the context makes its own.
            drop(pool.as_ref().get().unwrap());
            fill(u.context_mut(), Kind::A, 1);
            let frames = u.init().unwrap();
            assert!(frames.as_ref().context().has_pool());
            assert_eq!(allocs.load(Ordering::SeqCst), 1);
            drop((frames, pool));
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            drop(a);
            fx::take_events();
        }

        /// `free` set alone, `user_opaque` set alone, or a pool C code set:
        /// the setters refuse and give the value back, and nothing is read
        /// as Rust's.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn foreign_hooks_and_pools_are_neither_read_nor_replaced() {
            unsafe extern "C" fn foreign_free(_: *mut ffi::AVHWFramesContext) {}
            let a = device(Kind::A);
            let allocs = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            for case in 0..2 {
                let mut u = a.as_ref().alloc_frames().unwrap();
                let p = u.context_mut().as_mut_ptr();
                if case == 0 {
                    // SAFETY: a write to the context `u` alone holds, as C code makes it
                    // before init.
                    unsafe { (*p).free = Some(foreign_free) };
                } else {
                    // SAFETY: as above.
                    unsafe { (*p).user_opaque = ptr::without_provenance_mut(0x5EED) };
                }
                let mut m = u.context_mut();
                assert_eq!(m.set_user_data(3u8), Err(3));
                assert!(m.set_free_callback(|_| {}).is_err());
                // SAFETY: the test backends take any bytes for a surface.
                let refused = unsafe { m.set_pool(counting_pool(&allocs, &drops)) };
                drop(refused.unwrap_err());
                assert!(m.take_user_data().is_none());
                assert!(m.take_free_callback().is_none());
                assert!(m.take_pool().is_none());
                assert!(m.as_ref().user_data::<u8>().is_none());
                assert_eq!(m.as_ref().has_free_callback(), case == 0);
                // For the unsafe operations below: as above, restored before the release.
                {
                    // SAFETY: as the comment above this block states.
                    unsafe { (*p).free = None };
                    // SAFETY: as the comment above this block states.
                    unsafe { (*p).user_opaque = ptr::null_mut() };
                }
            }
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            // A pool C code set, which C never releases.
            let foreign = BufferPool::new(fx::SURFACE_SIZE).unwrap();
            let mut u = a.as_ref().alloc_frames().unwrap();
            let p = u.context_mut().as_mut_ptr();
            // SAFETY: as above; `foreign` outlives the context.
            unsafe { (*p).pool = foreign.as_c_ptr() };
            assert!(u.context().has_pool());
            // SAFETY: the test backends take any bytes for a surface.
            let refused = unsafe { u.context_mut().set_pool(counting_pool(&allocs, &drops)) };
            drop(refused.unwrap_err());
            assert!(u.context_mut().take_pool().is_none());
            u.context_mut().set_user_data(1u8).unwrap();
            assert!(u.context_mut().take_pool().is_none());
            fill(u.context_mut(), Kind::A, 1);
            let frames = u.init().unwrap();
            // SAFETY: the foreign pool holds bytes.
            let lent = unsafe { frames.as_ref().context().pool() }.unwrap();
            assert!(ptr::eq(lent.as_ptr(), foreign.as_c_ptr()));
            drop(frames);
            drop((foreign, a));
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn a_callback_may_take_or_replace_everything() {
            let a = device(Kind::B);
            let calls = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let allocs = Arc::new(AtomicUsize::new(0));
            let mut u = a.as_ref().alloc_frames().unwrap();
            let (c, d, al) = (calls.clone(), drops.clone(), allocs.clone());
            let mut m = u.context_mut();
            m.set_user_data(Dropped(drops.clone())).ok().unwrap();
            m.set_free_callback(move |mut ctx: FramesContextMut<'_>| {
                c.fetch_add(1, Ordering::SeqCst);
                assert!(ctx.take_user_data().is_some());
                ctx.set_user_data(Dropped(d.clone())).ok().unwrap();
                let again = c.clone();
                ctx.set_free_callback(move |_| {
                    again.fetch_add(100, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
                // SAFETY: the test backends take any bytes for a surface.
                unsafe { ctx.set_pool(counting_pool(&al, &d)) }
                    .ok()
                    .unwrap();
                assert!(ctx.as_ref().has_pool());
            })
            .ok()
            .unwrap();
            drop(u);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            // The first data, the second, and the pool's allocator.
            assert_eq!(drops.load(Ordering::SeqCst), 3);
            drop(a);
            fx::take_events();
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn init_failures_run_the_hooks_once() {
            let a = device(Kind::A);
            let calls = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let allocs = Arc::new(AtomicUsize::new(0));
            for failure in 0..2 {
                let mut u = a.as_ref().alloc_frames().unwrap();
                fill(u.context_mut(), Kind::A, 2);
                let failing = if failure == 0 {
                    FramesHwctx {
                        init_ret: -9,
                        ..FramesHwctx::default()
                    }
                } else {
                    FramesHwctx {
                        get_buffer_ret: -4,
                        ..FramesHwctx::default()
                    }
                };
                // SAFETY: the test backends' frames struct is a `FramesHwctx`.
                unsafe { u.context_mut().into_hwctx::<FramesHw>() }
                    .unwrap()
                    .set(failing);
                let c = calls.clone();
                let mut m = u.context_mut();
                // SAFETY: the test backends take any bytes for a surface.
                unsafe { m.set_pool(counting_pool(&allocs, &drops)) }
                    .ok()
                    .unwrap();
                m.set_free_callback(move |_| {
                    c.fetch_add(1, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
                assert!(u.init().is_err());
            }
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(drops.load(Ordering::SeqCst), 2);
            drop(a);
            fx::take_events();
        }

        /// A preallocation that runs out of memory for C's `av_frame_alloc`
        /// stops early but succeeds, as C's `hwframe_pool_prealloc` returns
        /// its `ret` of 0 there; the context then works, and its release
        /// runs the hooks once and drops their pool.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn preallocation_out_of_memory_is_ignored_as_in_c() {
            if !hwcontext_isolated(
                "libavutil::hwcontext::unit_tests::frames_context::\
                 preallocation_out_of_memory_is_ignored_as_in_c",
            ) {
                return;
            }
            let a = device(Kind::A);
            let (allocs, drops) = (Arc::default(), Arc::new(AtomicUsize::new(0)));
            let calls = Arc::new(AtomicUsize::new(0));
            let mut u = a.as_ref().alloc_frames().unwrap();
            fill(u.context_mut(), Kind::A, 2);
            let c = calls.clone();
            let mut m = u.context_mut();
            // SAFETY: the test backends take any bytes for a surface.
            unsafe { m.set_pool(counting_pool(&allocs, &drops)) }
                .ok()
                .unwrap();
            m.set_free_callback(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            })
            .ok()
            .unwrap();
            // An `AVFrame` takes more than this; the backend's pool and the
            // preallocation's array less.
            assert!(size_of::<ffi::AVFrame>() > 256);
            crate::libavutil::libavutil::mem::set_max_alloc(256);
            let ret = u.init();
            crate::libavutil::libavutil::mem::set_max_alloc(c_int::MAX as usize);
            let frames = ret.unwrap();
            assert_eq!(allocs.load(Ordering::SeqCst), 0);
            assert_eq!(fx::take_events(), [Event::FramesInit(Kind::A)]);
            drop(frames.as_ref().get_buffer().unwrap());
            assert_eq!(allocs.load(Ordering::SeqCst), 1);
            drop(frames);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            drop(a);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn release_frames_hooks_tolerates_a_cleared_cookie() {
            let a = device(Kind::B);
            let mut u = a.as_ref().alloc_frames().unwrap();
            let p = u.context_mut().as_mut_ptr();
            // SAFETY: the context `u` alone holds: a `free` of
            // `release_frames_hooks` with no cookie must do nothing.
            unsafe { (*p).free = Some(RELEASE_FRAMES_HOOKS) };
            assert!(u.context().user_data::<u8>().is_none());
            assert!(u.context_mut().take_user_data().is_none());
            drop(u);
            drop(a);
            fx::take_events();
        }

        /// The hooks without a pool on a context in Rust storage, released
        /// by calling the trampoline as `hwframe_ctx_free` would: no C call
        /// at all, for Miri.
        #[test]
        fn hooks_release_without_c_or_a_pool() {
            let drops = Arc::new(AtomicUsize::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            // SAFETY: an all-zero `AVHWFramesContext` is the plain struct with
            // NULL pointers, which the hooks and getters below accept.
            let zeroed = unsafe { core::mem::zeroed::<ffi::AVHWFramesContext>() };
            let ctx = Box::into_raw(Box::new(zeroed));
            for round in 0..4 {
                // SAFETY: the boxed context, which nothing else reaches.
                let mut m = unsafe { FramesContextMut::from_ptr(ctx) }.unwrap();
                m.set_user_data(Dropped(drops.clone())).ok().unwrap();
                m.set_user_data(Dropped(drops.clone())).ok().unwrap();
                let (c, d) = (calls.clone(), drops.clone());
                m.set_free_callback(move |mut ctx: FramesContextMut<'_>| {
                    assert!(ctx.as_ref().user_data::<Dropped>().is_some());
                    c.fetch_add(1, Ordering::SeqCst);
                    if round == 1 {
                        // A callback the callback installs is dropped
                        // uncalled, with new data.
                        drop(ctx.take_user_data());
                        ctx.set_user_data(Dropped(d)).ok().unwrap();
                        ctx.set_free_callback(|_| unreachable!()).ok().unwrap();
                    } else if round == 2 {
                        // Taking the last of the hooks frees them early.
                        drop(ctx.take_user_data());
                    }
                })
                .ok()
                .unwrap();
                assert!(m.as_ref().has_free_callback());
                assert!(!m.as_ref().has_pool());
                assert_eq!(m.as_ref().format(), PixelFormat::from_raw(0));
                m.set_width(round);
                assert_eq!(m.as_ref().width(), round);
                if round == 3 {
                    drop(m.take_free_callback());
                    drop(m.take_user_data());
                    // SAFETY: a read of the boxed context.
                    assert!(
                        // SAFETY: as the comment above this statement states.
                        unsafe { (*ctx).free }.is_none() && unsafe { (*ctx).user_opaque }.is_null()
                    );
                    continue;
                }
                // SAFETY: as hwframe_ctx_free calls it, with the context
                // nothing else reaches.
                unsafe { release_frames_hooks(ctx) };
                // SAFETY: a read of the boxed context.
                assert!(
                    // SAFETY: as the comment above this statement states.
                    unsafe { (*ctx).free }.is_none() && unsafe { (*ctx).user_opaque }.is_null()
                );
            }
            assert_eq!(calls.load(Ordering::SeqCst), 3);
            assert_eq!(drops.load(Ordering::SeqCst), 9);
            // SAFETY: the box leaked above, with no hooks left.
            drop(unsafe { Box::from_raw(ctx) });
        }

        /// The hooks on a context in Rust storage, released by calling the
        /// trampoline as `hwframe_ctx_free` would, without C.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn hooks_on_rust_storage_without_c() {
            let drops = Arc::new(AtomicUsize::new(0));
            let calls = Arc::new(AtomicUsize::new(0));
            // SAFETY: an all-zero `AVHWFramesContext` is the plain struct with
            // NULL pointers, which the hooks and getters below accept.
            let ctx = Box::into_raw(Box::new(unsafe {
                core::mem::zeroed::<ffi::AVHWFramesContext>()
            }));
            for round in 0..3 {
                // SAFETY: the boxed context, which nothing else reaches.
                let mut m = unsafe { FramesContextMut::from_ptr(ctx) }.unwrap();
                m.set_user_data(Dropped(drops.clone())).ok().unwrap();
                let c = calls.clone();
                m.set_free_callback(move |ctx: FramesContextMut<'_>| {
                    assert!(!ctx.as_ref().has_pool());
                    c.fetch_add(1, Ordering::SeqCst);
                })
                .ok()
                .unwrap();
                let pool = BufferPool::new(8).unwrap();
                // SAFETY: no backend reads this context.
                unsafe { m.set_pool(pool) }.ok().unwrap();
                if round == 2 {
                    drop(m.take_free_callback());
                    drop(m.take_user_data());
                    drop(m.take_pool());
                    continue;
                }
                // SAFETY: as hwframe_ctx_free calls it, with the context
                // nothing else reaches.
                unsafe { release_frames_hooks(ctx) };
                // SAFETY: reads of the boxed context.
                assert!(
                    // SAFETY: as the comment above this statement states.
                    unsafe { (*ctx).free }.is_none()
                        // SAFETY: as the comment above this statement states.
                        && unsafe { (*ctx).user_opaque }.is_null()
                        // SAFETY: as the comment above this statement states.
                        && unsafe { (*ctx).pool }.is_null()
                );
            }
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(drops.load(Ordering::SeqCst), 3);
            // SAFETY: the box leaked above, with no hooks left.
            drop(unsafe { Box::from_raw(ctx) });
        }

        /// Backend B's routines, none of which run here, with a frames
        /// struct of 4 KiB, larger than the frames context itself (as
        /// Vulkan's private one is), and no device struct.
        static BIG_FRAMES_STRUCT: fx::Shared<fx::HwContextType> = fx::Shared(fx::HwContextType {
            device_hwctx_size: 0,
            frames_hwctx_size: 4096,
            device_init: None,
            device_uninit: None,
            frames_init: None,
            frames_uninit: None,
            ..fx::BACKEND_B.0
        });

        /// The release of a [`big_struct_device`].
        unsafe extern "C" fn free_big_struct_device(_opaque: *mut c_void, data: *mut u8) {
            // SAFETY: the private context `big_struct_device` allocated, at
            // its last reference; it holds nothing else.
            unsafe { ffi::av_free(data.cast()) }
        }

        /// An initialised device of [`BIG_FRAMES_STRUCT`], built as
        /// `av_hwdevice_ctx_alloc` builds one; its backend has no routine
        /// for C to call at init or release.
        fn big_struct_device() -> DeviceReferenceBox {
            // For the unsafe operations below: `av_hwdevice_ctx_alloc`'s steps over a zeroed
            // allocation (no device struct: its size is 0), then C's init,
            // which calls no backend routine, and the only reference.
            {
                // SAFETY: as the comment above this block states.
                let ctxi = unsafe {
                    ffi::av_mallocz(size_of::<fx::DeviceInternal>()).cast::<fx::DeviceInternal>()
                };
                assert!(!ctxi.is_null());
                // SAFETY: as the comment above this block states.
                let buf = unsafe {
                    ffi::av_buffer_create(
                        ctxi.cast(),
                        size_of::<ffi::AVHWDeviceContext>(),
                        Some(free_big_struct_device),
                        ptr::null_mut(),
                        ffi::AV_BUFFER_FLAG_READONLY.cast_signed(),
                    )
                };
                assert!(!buf.is_null());
                // SAFETY: as the comment above this block states.
                unsafe { (*ctxi).p.type_ = ffi::AV_HWDEVICE_TYPE_DRM };
                // SAFETY: as the comment above this block states.
                unsafe { (*ctxi).p.av_class = &raw const fx::DEVICE_CLASS.0 };
                // SAFETY: as the comment above this block states.
                unsafe { (*ctxi).hw_type = &raw const BIG_FRAMES_STRUCT.0 };
                // SAFETY: as the comment above this block states.
                unsafe { assert_eq!(ffi::av_hwdevice_ctx_init(buf), 0) };
                // SAFETY: as the comment above this block states.
                unsafe { DeviceReferenceBox::from_c(buf).unwrap() }
            }
        }

        /// `av_hwframe_ctx_alloc` failing on the backend's frames struct,
        /// after the context itself: C frees the context and takes no
        /// reference to the device, and both constructors report it.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn alloc_fails_whole_when_the_backend_struct_does_not_fit() {
            if !hwcontext_isolated(
                "libavutil::hwcontext::unit_tests::frames_context::alloc_fails_whole_when_the_backend_struct_does_not_fit",
            ) {
                return;
            }
            let d = big_struct_device();
            // The context, its reference and the device's fit; the struct not.
            assert!(size_of::<fx::FramesInternal>() < 1024);
            crate::libavutil::libavutil::mem::set_max_alloc(1024);
            let alloc = d.as_ref().alloc_frames();
            let created =
                d.as_ref()
                    .create_frames(PixelFormat::DRM_PRIME, PixelFormat::NV12, 64, 32, 0);
            crate::libavutil::libavutil::mem::set_max_alloc(c_int::MAX as usize);
            assert!(alloc.is_none());
            assert_eq!(created.unwrap_err(), ENOMEM);
            // SAFETY: a live reference; C atomically loads its count.
            assert_eq!(unsafe { ffi::av_buffer_get_ref_count(d.as_c_ptr()) }, 1);
            // Without the limit the struct is there, zeroed.
            let u = d.as_ref().alloc_frames().unwrap();
            // SAFETY: a read of the live context `u` holds.
            let hwctx = unsafe { (*u.context().as_ptr()).hwctx };
            assert!(!hwctx.is_null());
            // SAFETY: C's `av_mallocz` block of the backend's 4096 bytes.
            let bytes = unsafe { core::slice::from_raw_parts(hwctx.cast::<u8>(), 4096) };
            assert_eq!(bytes, [0u8; 4096].as_slice());
            drop(u);
            // SAFETY: as above.
            assert_eq!(unsafe { ffi::av_buffer_get_ref_count(d.as_c_ptr()) }, 1);
        }
    }
}
