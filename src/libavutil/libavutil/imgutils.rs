//! Safe wrappers over `libavutil/imgutils.h`.
//!
//! [`check_size`] validates image dimensions (`av_image_check_size`);
//! [`copy_plane`] and [`copy_plane_from_uncacheable`] copy one image plane
//! between byte slices (`av_image_copy_plane`, `av_image_copy_plane_uc_from`).
//!
//! A plane is `height` rows, `linesize` bytes apart, held in one slice that
//! spans exactly its rows: row 0 starts the slice for a non-negative linesize
//! and is the last row in memory for a negative one (a bottom-up image), so a
//! plane of `height` rows of `bytewidth` bytes needs
//! `(height - 1) * |linesize| + bytewidth` bytes.
//!
//! A whole image is up to four such planes, borrowed as [`Planes`] or
//! [`PlanesMut`] (C's `data[4]` and `linesize[4]`), or owned by an
//! [`Image`] that `av_image_alloc` lays out in one `av_malloc` block. The
//! layout functions give a pixel format's row widths ([`linesize`],
//! [`fill_linesizes`], and per pixel, [`max_pixel_steps`]), plane sizes ([`fill_plane_sizes`],
//! [`buffer_size_for_linesizes`], [`buffer_size`]) and planes laid out back
//! to back in a caller's buffer ([`fill_pointers`], [`fill_arrays`]);
//! [`copy`], [`copy_from_mut`], [`copy_from_uncacheable`] and
//! [`copy_to_buffer`] copy images, and [`fill_color`] and [`fill_black`]
//! fill them. Every function taking planes checks them against the pixel
//! format's layout before C touches them, and refuses the arguments on which
//! C's own arithmetic overflows.

use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::{fmt, ptr};

use super::ffi;
use super::mem::HeapVec;
use super::opt::ObjectRef;
use super::pixdesc::{InvalidDescriptor, PixelFormatDescriptorRef};
use super::pixfmt::{ColorRange, PixelFormat};
use super::rational::Rational;

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL.cast_signed());

/// Wraps: `av_image_check_size`
///
/// Checks that a `width` x `height` image is valid, as FFmpeg requires of
/// every image it allocates: both dimensions non-zero and at most `INT_MAX`,
/// and every row (with 1 KiB of padding) and the whole image (with 128 rows
/// of padding) addressable in an `int`, assuming 8 bytes per pixel.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` otherwise, after logging the size at `AV_LOG_ERROR` in C's
/// `IMGUTILS` class context.
pub fn check_size(width: u32, height: u32) -> Result<(), i32> {
    // SAFETY: a NULL log context is "no parent context", and a zero offset
    // leaves the error level unchanged; C reads nothing else through
    // pointers.
    let ret = unsafe { ffi::av_image_check_size(width, height, 0, ptr::null_mut()) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: `av_image_check_size`
///
/// As [`check_size`], logging a failure at `AV_LOG_ERROR + log_offset` with
/// `log_ctx` as the parent log context.
///
/// `log_ctx` is the parent log context, `None` for none.
///
/// # Errors
///
/// A `log_offset` above `INT_MAX - AV_LOG_ERROR`, which would overflow the level C logs at, is
/// rejected with `AVERROR(EINVAL)` before C runs.
pub fn check_size_with_context(
    width: u32,
    height: u32,
    log_offset: i32,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<(), i32> {
    if log_offset > c_int::MAX - ffi::AV_LOG_ERROR.cast_signed() {
        return Err(AVERROR_EINVAL);
    }
    let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
    // SAFETY: `log_ctx` is NULL or a logging context, as its handle
    // guarantees, which C only reads while logging; `log_offset` cannot
    // overflow the error level C adds it to.
    let ret = unsafe { ffi::av_image_check_size(width, height, log_offset, log_ctx) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Why [`copy_plane`] or [`copy_plane_from_uncacheable`] refused a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneCopyError {
    /// A dimension or linesize exceeds what C's parameters take.
    TooLarge,
    /// `bytewidth` exceeds the magnitude of a linesize, so rows would
    /// overlap; C asserts against it and aborts.
    RowTooWide,
    /// The destination slice is shorter than the plane's rows reach.
    DestinationTooShort,
    /// The source slice is shorter than the plane's rows reach.
    SourceTooShort,
}

impl fmt::Display for PlaneCopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "plane dimension or linesize out of range",
            Self::RowTooWide => "plane row width exceeds a linesize",
            Self::DestinationTooShort => "destination plane ends past its buffer",
            Self::SourceTooShort => "source plane ends past its buffer",
        })
    }
}

impl std::error::Error for PlaneCopyError {}

/// The offset of row 0 in a plane slice of `len` bytes when `rows` rows of
/// `row_len` bytes, `linesize` apart, all lie inside it, as the
/// [module docs](self) lay a plane out; `None` when a row would leave it.
pub(crate) fn plane_origin(
    len: usize,
    linesize: isize,
    row_len: usize,
    rows: usize,
) -> Option<usize> {
    let Some(last) = rows.checked_sub(1) else {
        return Some(0);
    };
    let span = last.checked_mul(linesize.unsigned_abs())?;
    if span.checked_add(row_len)? > len {
        return None;
    }
    Some(if linesize < 0 { span } else { 0 })
}

/// The offsets of row 0 in `dst` and `src` for a copy of `height` rows of
/// `bytewidth` bytes, after the checks C asserts.
fn plane_copy_origins(
    dst_len: usize,
    dst_linesize: isize,
    src_len: usize,
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> Result<(usize, usize), PlaneCopyError> {
    if bytewidth > dst_linesize.unsigned_abs() || bytewidth > src_linesize.unsigned_abs() {
        return Err(PlaneCopyError::RowTooWide);
    }
    let dst_at = plane_origin(dst_len, dst_linesize, bytewidth, height)
        .ok_or(PlaneCopyError::DestinationTooShort)?;
    let src_at = plane_origin(src_len, src_linesize, bytewidth, height)
        .ok_or(PlaneCopyError::SourceTooShort)?;
    Ok((dst_at, src_at))
}

/// Wraps: `av_image_copy_plane`
///
/// Copies `height` rows of `bytewidth` bytes from the plane in `src`, rows
/// `src_linesize` apart, to the plane in `dst`, rows `dst_linesize` apart,
/// laid out as the [module docs](self) describe: row `r` of `src` becomes
/// row `r` of `dst`, so linesizes of opposite signs flip the plane
/// vertically. Bytes between rows are left alone.
///
/// # Errors
///
/// Fails without touching `dst` when `bytewidth` exceeds the magnitude of either linesize (on which
/// C aborts), when either plane ends past its slice, or when `bytewidth` or `height` exceeds
/// `INT_MAX`.
pub fn copy_plane(
    dst: &mut [u8],
    dst_linesize: i32,
    src: &[u8],
    src_linesize: i32,
    bytewidth: usize,
    height: usize,
) -> Result<(), PlaneCopyError> {
    let (Ok(c_bytewidth), Ok(c_height)) = (c_int::try_from(bytewidth), c_int::try_from(height))
    else {
        return Err(PlaneCopyError::TooLarge);
    };
    let (dst_at, src_at) = plane_copy_origins(
        dst.len(),
        dst_linesize as isize,
        src.len(),
        src_linesize as isize,
        bytewidth,
        height,
    )?;
    if bytewidth == 0 || height == 0 {
        // C copies nothing; the slices' pointers may be dangling.
        return Ok(());
    }
    // SAFETY: `plane_origin` placed both row-0 offsets inside their slices,
    // and the pointers derive from the whole slices.
    let (dst_row0, src_row0) = (unsafe { dst.as_mut_ptr().add(dst_at) }, unsafe {
        src.as_ptr().add(src_at)
    });
    // SAFETY: C writes `bytewidth` bytes at `dst_row0 + r * dst_linesize`
    // and reads as many at `src_row0 + r * src_linesize` for `r` in
    // `0..height`, all inside `dst` and `src` by `plane_origin`; the borrows
    // are disjoint, so no row copy overlaps; both linesizes cover
    // `bytewidth`, so C's asserts hold. C keeps no pointer.
    unsafe {
        ffi::av_image_copy_plane(
            dst_row0,
            dst_linesize,
            src_row0,
            src_linesize,
            c_bytewidth,
            c_height,
        );
    };
    Ok(())
}

/// The row width `av_image_copy_plane_uc_from`'s x86 SSE4 path copies:
/// `FFALIGN(bytewidth, 64)`.
const UC_ROW_ALIGN: usize = 64;

/// The alignment the SSE4 path's `movntdqa` loads and `movdqa` stores
/// require of every row it copies.
const UC_ACCESS_ALIGN: usize = 16;

/// Wraps: `av_image_copy_plane_uc_from`
///
/// As [`copy_plane`], for a source in uncacheable memory (GPU-mapped, say):
/// on x86 CPUs with SSE4.1, C reads it with streaming loads, which can be
/// much faster there. Linesizes are `isize`, as in C.
///
/// That fast path copies whole 64-byte blocks: when both linesizes are at
/// least `bytewidth` rounded up to a multiple of 64, every row's bytes past
/// `bytewidth` up to that multiple may be copied too, which both linesizes
/// cover. Aligning the rows to 64 bytes gets the best performance.
///
/// # Divergence from C
///
/// C takes the fast path whenever both linesizes cover the rounded-up width,
/// and it then faults unless both planes and linesizes are 16-byte aligned,
/// and writes and reads past the plane when the last row of either slice
/// does not hold the rounded-up width. For such planes this wrapper copies
/// the `bytewidth` bytes of each row itself instead, as C's plain path (on
/// other CPUs) does.
///
/// # Errors
///
/// Fails as [`copy_plane`] does, and when `height` exceeds `INT_MAX`, a linesize is `isize::MIN`,
/// or `bytewidth` is within 63 of `isize::MAX`.
pub fn copy_plane_from_uncacheable(
    dst: &mut [u8],
    dst_linesize: isize,
    src: &[u8],
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> Result<(), PlaneCopyError> {
    let Ok(c_height) = c_int::try_from(height) else {
        return Err(PlaneCopyError::TooLarge);
    };
    // `FFABS` of `PTRDIFF_MIN` and `FFALIGN` near `PTRDIFF_MAX` overflow in C.
    if dst_linesize == isize::MIN
        || src_linesize == isize::MIN
        || bytewidth > isize::MAX as usize - (UC_ROW_ALIGN - 1)
    {
        return Err(PlaneCopyError::TooLarge);
    }
    let (dst_at, src_at) = plane_copy_origins(
        dst.len(),
        dst_linesize,
        src.len(),
        src_linesize,
        bytewidth,
        height,
    )?;
    if bytewidth == 0 || height == 0 {
        // C's fast path would loop away on either; it copies nothing.
        return Ok(());
    }
    if !uc_call_is_sound(dst, dst_linesize, src, src_linesize, bytewidth, height) {
        copy_rows(
            dst,
            dst_at,
            dst_linesize,
            src,
            src_at,
            src_linesize,
            bytewidth,
            height,
        );
        return Ok(());
    }
    // SAFETY: as in `copy_plane`.
    let (dst_row0, src_row0) = (unsafe { dst.as_mut_ptr().add(dst_at) }, unsafe {
        src.as_ptr().add(src_at)
    });
    // SAFETY: as in `copy_plane`, with `bytewidth` and `height` positive. On
    // the SSE4 path C copies `FFALIGN(bytewidth, 64)` bytes of each row
    // instead, which `uc_call_is_sound` checked inside both slices (and
    // inside each linesize, so no two rows overlap), from and to 16-byte
    // aligned rows; otherwise it takes the plain path.
    unsafe {
        ffi::av_image_copy_plane_uc_from(
            dst_row0,
            dst_linesize,
            src_row0,
            src_linesize,
            bytewidth.cast_signed(),
            c_height,
        );
    };
    Ok(())
}

/// Whether C may call `av_image_copy_plane_uc_from` for these validated,
/// non-empty planes: its SSE4 path, taken whatever the CPU when both
/// linesizes cover the 64-byte-rounded width, needs that width inside both
/// slices' last rows and 16-byte aligned rows, both of which the plain path
/// does without.
fn uc_call_is_sound(
    dst: &[u8],
    dst_linesize: isize,
    src: &[u8],
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) -> bool {
    // No overflow: `bytewidth` is at most `isize::MAX - 63`.
    let wide = bytewidth.next_multiple_of(UC_ROW_ALIGN);
    let fast = |linesize: isize| linesize >= 0 && wide <= linesize.cast_unsigned();
    if !(fast(dst_linesize) && fast(src_linesize)) {
        return true;
    }
    // Both linesizes are positive: row 0 starts each slice.
    let aligned = |at: *const u8, linesize: isize| {
        at.addr().is_multiple_of(UC_ACCESS_ALIGN)
            && linesize.unsigned_abs().is_multiple_of(UC_ACCESS_ALIGN)
    };
    aligned(dst.as_ptr(), dst_linesize)
        && aligned(src.as_ptr(), src_linesize)
        && plane_origin(dst.len(), dst_linesize, wide, height).is_some()
        && plane_origin(src.len(), src_linesize, wide, height).is_some()
}

/// `image_copy_plane`'s plain row loop, on planes `plane_copy_origins`
/// validated.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the arguments of the C routine it stands for"
)]
fn copy_rows(
    dst: &mut [u8],
    dst_at: usize,
    dst_linesize: isize,
    src: &[u8],
    src_at: usize,
    src_linesize: isize,
    bytewidth: usize,
    height: usize,
) {
    // No overflow: every row start lies inside its slice, at most
    // `isize::MAX` bytes from row 0.
    let row =
        |at: usize, linesize: isize, r: usize| at.wrapping_add_signed(r.cast_signed() * linesize);
    for r in 0..height {
        let d = row(dst_at, dst_linesize, r);
        let s = row(src_at, src_linesize, r);
        dst[d..d + bytewidth].copy_from_slice(&src[s..s + bytewidth]);
    }
}

/// `AVERROR(ENOMEM)`.
const AVERROR_ENOMEM: c_int = -(ffi::ENOMEM.cast_signed());

/// The bytes of the palette a palettized image keeps in plane 1: 256
/// native-endian `u32` entries (`AVPALETTE_SIZE`).
const PALETTE_SIZE: usize = ffi::AVPALETTE_SIZE as usize;

/// The largest buffer alignment the layout functions take: the largest power
/// of two in an `int`.
const MAX_ALIGN: usize = 1 << 30;

/// `Ok` with a non-negative C return value, `Err` with a negative one, an
/// `AVERROR` code.
fn status(ret: c_int) -> Result<c_int, i32> {
    if ret < 0 { Err(ret) } else { Ok(ret) }
}

/// `align` as C takes it, when it is a power of two that fits an `int`.
///
/// C rounds linesizes up with `FFALIGN`, which only aligns to powers of two,
/// and overflows `int` for alignments near `INT_MAX`; this bound keeps its
/// sums in range for every image `av_image_check_size` accepts, whose rows
/// are under 64 MiB.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "`align <= MAX_ALIGN`, checked just above"
)]
fn c_align(align: usize) -> Result<c_int, i32> {
    if align.is_power_of_two() && align <= MAX_ALIGN {
        Ok(align as c_int)
    } else {
        Err(AVERROR_EINVAL)
    }
}

/// `AV_CEIL_RSHIFT(a, b)` for a non-negative `a`: `a / 2^b`, rounded up.
fn ceil_rshift(a: usize, b: u32) -> usize {
    a.div_ceil(1usize << b.min(usize::BITS - 1))
}

/// The parts of a pixel format's descriptor its plane layout depends on,
/// read from C's descriptor table.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    flags: u64,
    nb_components: usize,
    log2_chroma_w: u32,
    log2_chroma_h: u32,
    /// The plane and step of each of the four component slots, unused ones
    /// included (all-zero, as C stores them).
    comps: [(usize, c_int); 4],
    /// The largest step of each plane, and its first component.
    max_steps: MaxPixelSteps,
}

impl Geometry {
    /// `format`'s descriptor, or `None` when it has none (or one whose plane
    /// numbers C could not index its four-plane arrays with).
    fn of(format: PixelFormat) -> Option<Self> {
        let desc = format.descriptor()?;
        let mut comps = [(0, 0); 4];
        for (slot, comp) in comps.iter_mut().zip(desc.components().iter()) {
            let plane = usize::try_from(comp.plane()).ok().filter(|&p| p < 4)?;
            *slot = (plane, comp.step());
        }
        Some(Self {
            flags: desc.flags().bits(),
            nb_components: usize::from(desc.component_count()).min(4),
            log2_chroma_w: desc.log2_chroma_w().into(),
            log2_chroma_h: desc.log2_chroma_h().into(),
            comps,
            max_steps: max_pixel_steps(desc).ok()?,
        })
    }

    fn has(&self, flag: u32) -> bool {
        self.flags & u64::from(flag) != 0
    }

    /// The number of planes the format's components occupy, counting any
    /// plane below the highest one used: `image_copy`'s `planes_nb`.
    fn planes(&self) -> usize {
        self.comps[..self.nb_components]
            .iter()
            .map(|&(plane, _)| plane + 1)
            .max()
            .unwrap_or(0)
    }

    /// The rows plane `plane` of a `height` rows high image holds: chroma
    /// planes 1 and 2 are subsampled vertically.
    fn rows(&self, plane: usize, height: usize) -> usize {
        if plane == 1 || plane == 2 {
            ceil_rshift(height, self.log2_chroma_h)
        } else {
            height
        }
    }

    /// Whether C's `image_get_linesize` overflows `int` for plane `plane` of
    /// a `width` pixels wide image: for a bitstream format, a row of bits C
    /// accepts (at most `INT_MAX`) that its round-up to bytes (`+ 7`)
    /// carries past `INT_MAX`.
    #[expect(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "`width` is a non-negative `int`, so the shifted value is below 2^31"
    )]
    fn linesize_overflows(&self, width: c_int, plane: usize) -> bool {
        if !self.has(ffi::AV_PIX_FMT_FLAG_BITSTREAM) || width < 0 {
            return false;
        }
        // The plane's largest step, and the first component with it.
        let (step, comp) = (
            self.max_steps.steps[plane],
            self.max_steps.components[plane],
        );
        let shift = if comp == 1 || comp == 2 {
            self.log2_chroma_w
        } else {
            0
        };
        // Below `2^31`: `width` is a non-negative `int`.
        let shifted = ceil_rshift(width as usize, shift) as i64;
        let step = i64::from(step);
        if shifted != 0 && step > i64::from(c_int::MAX) / shifted {
            // C rejects the width before rounding.
            return false;
        }
        step * shifted > i64::from(c_int::MAX - 7)
    }
}

/// The largest pixel step of each plane of an image, and the first
/// component with it, as [`max_pixel_steps`] gives them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaxPixelSteps {
    /// The largest step among the components in each plane: the distance
    /// between a component of one pixel and the same component of the next,
    /// in bytes (bits for a
    /// [bitstream](super::pixdesc::PixelFormatFlags::BITSTREAM) format); 0
    /// for a plane no component uses.
    pub steps: [i32; 4],
    /// The first component, in descriptor order, with each plane's step; 0
    /// for a plane no component uses.
    pub components: [usize; 4],
}

/// Wraps: `av_image_fill_max_pixsteps`
///
/// The largest pixel step of each of the four planes of an image laid out
/// as `desc` describes, and the first component with it: what a row of a
/// plane advances by per pixel. C considers all four component slots,
/// unused ones included (all-zero in C's table, so they count for nothing),
/// and only steps above 0.
///
/// # Errors
///
/// [`InvalidDescriptor`] for a descriptor with a component slot in a plane
/// outside 0 to 3, which C would index its four-plane arrays with. C's own
/// table has none.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::max_pixel_steps;
/// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
/// let nv12 = max_pixel_steps(PixelFormat::NV12.descriptor().unwrap()).unwrap();
/// assert_eq!(nv12.steps, [1, 2, 0, 0]);
/// assert_eq!(nv12.components, [0, 1, 0, 0]);
/// ```
pub fn max_pixel_steps(
    desc: PixelFormatDescriptorRef<'_>,
) -> Result<MaxPixelSteps, InvalidDescriptor> {
    if desc
        .components()
        .iter()
        .any(|c| !(0..4).contains(&c.plane()))
    {
        return Err(InvalidDescriptor);
    }
    let (mut steps, mut components) = ([0; 4], [0; 4]);
    // SAFETY: `steps` and `components` are the `int[4]` arrays C fills,
    // indexed by the planes of the descriptor's four component slots, all
    // checked to be 0 to 3; the descriptor is live and only read. C keeps no
    // pointer.
    unsafe {
        ffi::av_image_fill_max_pixsteps(steps.as_mut_ptr(), components.as_mut_ptr(), desc.as_ptr());
    };
    Ok(MaxPixelSteps {
        steps,
        // A component index, 0 to 3.
        components: components.map(|c: c_int| c.unsigned_abs() as usize),
    })
}

/// Wraps: `av_image_check_size2`
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::check_size_max_pixels;
/// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
/// assert_eq!(
///     check_size_max_pixels(1920, 1080, i64::MAX, PixelFormat::YUV420P),
///     Ok(())
/// );
/// assert!(check_size_max_pixels(1920, 1080, 1 << 20, PixelFormat::YUV420P).is_err());
/// ```
///
/// # Errors
///
/// As [`check_size`], for an image of pixel format `format` of at most `max_pixels` pixels
/// (`i64::MAX` for no limit): the row bound uses the format's own row width where it has one (8
/// bytes a pixel otherwise), and `width * height` above `max_pixels` fails too, after logging both
/// at `AV_LOG_ERROR` in C's `IMGUTILS` class context.
pub fn check_size_max_pixels(
    width: u32,
    height: u32,
    max_pixels: i64,
    format: PixelFormat,
) -> Result<(), i32> {
    check_size_max_pixels_with_context(width, height, max_pixels, format, 0, None)
}

/// Wraps: `av_image_check_size2`
///
/// As [`check_size_max_pixels`], logging a failure at
/// `AV_LOG_ERROR + log_offset` with `log_ctx` as the parent log context.
///
/// `log_ctx` is the parent log context, `None` for none.
///
/// # Errors
///
/// A `log_offset` above `INT_MAX - AV_LOG_ERROR`, which would overflow the level C logs at, is
/// rejected with `AVERROR(EINVAL)` before C runs.
pub fn check_size_max_pixels_with_context(
    width: u32,
    height: u32,
    max_pixels: i64,
    format: PixelFormat,
    log_offset: i32,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<(), i32> {
    if log_offset > c_int::MAX - ffi::AV_LOG_ERROR.cast_signed() {
        return Err(AVERROR_EINVAL);
    }
    // C takes plane 0's linesize of a `width as int` wide image. Where its
    // arithmetic would overflow, the row is over 256 MiB and the size
    // invalid for every format: C's fallback for a format without a
    // linesize, 8 bytes a pixel, reaches the same verdict and message.
    let format = match Geometry::of(format) {
        Some(g) if g.linesize_overflows(width.cast_signed(), 0) => PixelFormat::NONE,
        _ => format,
    };
    let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
    // SAFETY: `log_ctx` is NULL or a logging context, as its handle
    // guarantees, which C only reads while logging; `log_offset` cannot
    // overflow the error level C adds it to, and no width overflows the
    // linesize C computes for `format`.
    let ret = unsafe {
        ffi::av_image_check_size2(
            width,
            height,
            max_pixels,
            format.into_raw(),
            log_offset,
            log_ctx,
        )
    };
    status(ret).map(drop)
}

/// Wraps: `av_image_check_sar`
///
/// Checks that `sar`, the sample (pixel) aspect ratio of a `width` x
/// `height` image, is usable: `0/1` (unknown) or positive, and not so
/// extreme that scaling the image's smaller side by it rounds to zero.
/// Fails with `AVERROR(EINVAL)` otherwise.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` otherwise.
pub fn check_sar(width: u32, height: u32, sar: Rational) -> Result<(), i32> {
    // SAFETY: every argument is passed by value, and C rejects
    // non-positive denominators before dividing by them.
    status(unsafe { ffi::av_image_check_sar(width, height, sar.into_raw()) }).map(drop)
}

/// Wraps: `av_image_get_linesize`
///
/// The bytes one row of plane `plane` of a `width` pixels wide image of
/// pixel format `format` takes, without padding: 0 for a plane the format
/// does not use.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::linesize;
/// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
/// assert_eq!(linesize(PixelFormat::YUV420P, 101, 0), Ok(101));
/// assert_eq!(linesize(PixelFormat::YUV420P, 101, 1), Ok(51));
/// assert_eq!(linesize(PixelFormat::RGB24, 101, 1), Ok(0));
/// ```
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` for a format without a descriptor or a hardware format, a negative
/// width, a row wider than `INT_MAX` bytes, and a plane past 3 (which C would read past its arrays
/// for).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "`plane < 4`, checked above"
)]
pub fn linesize(format: PixelFormat, width: i32, plane: usize) -> Result<i32, i32> {
    if plane >= 4 || Geometry::of(format).is_some_and(|g| g.linesize_overflows(width, plane)) {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: by value; `plane` indexes C's four-plane arrays in bounds, and
    // no width overflows C's rounding.
    status(unsafe { ffi::av_image_get_linesize(format.into_raw(), width, plane as c_int) })
}

/// Whether C's `av_image_get_linesize` overflows `int` (undefined) for
/// plane `plane` of a `width` pixels wide image of `format`, which
/// [`linesize`] refuses: C routines that compute row widths themselves
/// (`av_image_copy2` for a frame's copy) must not be handed such a width.
pub(crate) fn linesize_overflows(format: PixelFormat, width: i32, plane: usize) -> bool {
    plane < 4 && Geometry::of(format).is_some_and(|g| g.linesize_overflows(width, plane))
}

/// Wraps: `av_image_fill_linesizes`
///
/// The unpadded row width of each of the four planes of a `width` pixels
/// wide image of pixel format `format`, as [`linesize`] gives them: 0 for
/// the planes it does not use. Fails as [`linesize`] does.
///
/// # Errors
///
/// Fails as [`linesize`] does.
pub fn fill_linesizes(format: PixelFormat, width: i32) -> Result<[i32; 4], i32> {
    if Geometry::of(format).is_some_and(|g| (0..4).any(|p| g.linesize_overflows(width, p))) {
        return Err(AVERROR_EINVAL);
    }
    let mut linesizes = [0; 4];
    // SAFETY: `linesizes` is the `int[4]` C writes; no width overflows C's
    // rounding.
    status(unsafe {
        ffi::av_image_fill_linesizes(linesizes.as_mut_ptr(), format.into_raw(), width)
    })?;
    Ok(linesizes)
}

/// Wraps: `av_image_fill_plane_sizes`
///
/// The bytes each of the four planes of a `height` rows high image of pixel
/// format `format` takes with rows `linesizes` apart: `linesize * rows`, for
/// chroma rows subsampled as the format says, 0 for the planes it does not
/// use, and the 1024 bytes of the palette for plane 1 of a palettized format.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` for a format without a descriptor or a hardware format, a size past
/// `usize::MAX`, and a height that is not positive (for which C divides by zero). A negative
/// linesize is taken as C takes it, converted to `size_t`.
pub fn fill_plane_sizes(
    format: PixelFormat,
    height: i32,
    linesizes: [isize; 4],
) -> Result<[usize; 4], i32> {
    if height <= 0 {
        return Err(AVERROR_EINVAL);
    }
    let mut sizes = [0; 4];
    // SAFETY: `sizes` is the `size_t[4]` C writes and `linesizes` the
    // `ptrdiff_t[4]` it reads; a positive height keeps C's divisions by it,
    // and by its subsampled chroma rows (at least 1), defined.
    status(unsafe {
        ffi::av_image_fill_plane_sizes(
            sizes.as_mut_ptr(),
            format.into_raw(),
            height,
            linesizes.as_ptr(),
        )
    })?;
    Ok(sizes)
}

/// The byte ranges of an image's four planes in its buffer; empty for the
/// planes it does not have.
type PlaneRanges = [Range<usize>; 4];

/// Calls `av_image_fill_pointers` for `buf`, or for no buffer: the byte
/// ranges of the planes C lays out in `buf` (empty without one), and the
/// size of the image. Fails with `AVERROR(EINVAL)` when `buf` is shorter
/// than that size.
fn c_fill_pointers(
    buf: Option<&[u8]>,
    format: PixelFormat,
    height: i32,
    linesizes: [i32; 4],
) -> Result<(PlaneRanges, usize), i32> {
    if height <= 0 {
        return Err(AVERROR_EINVAL);
    }
    let mut data = [ptr::null_mut(); 4];
    // SAFETY: `data` is the `uint8_t *[4]` C writes, `linesizes` the
    // `int[4]` it reads; a NULL buffer only asks for the size; a positive
    // height keeps C's divisions defined, as in `fill_plane_sizes`.
    let size = status(unsafe {
        ffi::av_image_fill_pointers(
            data.as_mut_ptr(),
            format.into_raw(),
            height,
            ptr::null_mut(),
            linesizes.as_ptr(),
        )
    })?
    .unsigned_abs() as usize;
    let Some(buf) = buf else {
        return Ok((Default::default(), size));
    };
    if buf.len() < size {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: as above, for a buffer holding the `size` bytes C returned
    // for these arguments, so every pointer C derives from it stays inside;
    // C never dereferences it.
    status(unsafe {
        ffi::av_image_fill_pointers(
            data.as_mut_ptr(),
            format.into_raw(),
            height,
            buf.as_ptr().cast_mut(),
            linesizes.as_ptr(),
        )
    })?;
    let ranges = plane_ranges(buf.as_ptr().addr(), data.map(<*mut u8>::addr), size);
    Ok((ranges, size))
}

/// The byte ranges of the planes C laid out from address `base`, given the
/// addresses of the planes it returned (0 for NULL) and the image's total
/// size: each plane ends where the next one starts, and a NULL plane is
/// empty.
fn plane_ranges(base: usize, data: [usize; 4], size: usize) -> PlaneRanges {
    let starts = data.map(|p| (p != 0).then(|| p.wrapping_sub(base)));
    core::array::from_fn(|i| match starts[i] {
        Some(start) => {
            start
                ..starts[i + 1..]
                    .iter()
                    .flatten()
                    .next()
                    .copied()
                    .unwrap_or(size)
        }
        None => 0..0,
    })
}

/// `buf` split into the disjoint, ascending plane ranges C laid out in it.
fn split_planes<'a>(mut buf: &'a mut [u8], ranges: &PlaneRanges) -> [&'a mut [u8]; 4] {
    let mut planes: [&mut [u8]; 4] = Default::default();
    let mut at = 0;
    for (plane, range) in planes.iter_mut().zip(ranges) {
        if range.is_empty() {
            continue;
        }
        let (_, rest) = core::mem::take(&mut buf).split_at_mut(range.start - at);
        let (this, rest) = rest.split_at_mut(range.len());
        (*plane, buf, at) = (this, rest, range.end);
    }
    planes
}

/// Wraps: `av_image_fill_pointers`
///
/// The bytes a `height` rows high image of pixel format `format` with rows
/// `linesizes` apart takes in one buffer, its planes back to back as
/// [`fill_pointers`] lays them out. Fails as [`fill_plane_sizes`] does, and
/// with `AVERROR(EINVAL)` for a size past `INT_MAX`.
///
/// # Errors
///
/// Fails as [`fill_plane_sizes`] does, and with `AVERROR(EINVAL)` for a size past `INT_MAX`.
pub fn buffer_size_for_linesizes(
    format: PixelFormat,
    height: i32,
    linesizes: [i32; 4],
) -> Result<usize, i32> {
    c_fill_pointers(None, format, height, linesizes).map(|(_, size)| size)
}

/// Wraps: `av_image_fill_pointers`
///
/// The planes of a `height` rows high image of pixel format `format` with
/// rows `linesizes` apart, laid out back to back from the start of `buf`
/// (palette included), borrowed from `buf` for reading. Each plane spans
/// all its rows at full linesize; a plane the format does not use is empty.
///
/// # Errors
///
/// Fails as [`buffer_size_for_linesizes`] does, and with `AVERROR(EINVAL)` when `buf` is shorter
/// than that size.
pub fn fill_pointers(
    buf: &[u8],
    format: PixelFormat,
    height: i32,
    linesizes: [i32; 4],
) -> Result<Planes<'_>, i32> {
    let (ranges, _) = c_fill_pointers(Some(buf), format, height, linesizes)?;
    Ok(Planes {
        data: core::array::from_fn(|i| &buf[ranges[i].clone()]),
        linesizes: linesizes.map(|l| l as isize),
    })
}

/// Wraps: `av_image_fill_pointers`
///
/// As [`fill_pointers`], with the planes borrowed from `buf` for writing.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_image_fill_pointers` returns when it fails.
pub fn fill_pointers_mut(
    buf: &mut [u8],
    format: PixelFormat,
    height: i32,
    linesizes: [i32; 4],
) -> Result<PlanesMut<'_>, i32> {
    let (ranges, _) = c_fill_pointers(Some(buf), format, height, linesizes)?;
    Ok(PlanesMut {
        data: split_planes(buf, &ranges),
        linesizes: linesizes.map(|l| l as isize),
    })
}

/// Calls `av_image_fill_arrays` for `buf`: the byte ranges of the planes C
/// lays out in it, the linesizes and the size of the image. Fails with
/// `AVERROR(EINVAL)` when `buf` is shorter than that size.
fn c_fill_arrays(
    buf: &[u8],
    format: PixelFormat,
    width: i32,
    height: i32,
    align: usize,
) -> Result<(PlaneRanges, [i32; 4], usize), i32> {
    let align = c_align(align)?;
    let mut data = [ptr::null_mut(); 4];
    let mut linesizes = [0; 4];
    // SAFETY: `data` and `linesizes` are the arrays C writes; a NULL buffer
    // only asks for the size. C checks the size first, which keeps its
    // linesizes under 64 MiB, so neither their rounding nor their alignment
    // to at most 2^30 overflows `int`, and its height positive.
    let size = status(unsafe {
        ffi::av_image_fill_arrays(
            data.as_mut_ptr(),
            linesizes.as_mut_ptr(),
            ptr::null(),
            format.into_raw(),
            width,
            height,
            align,
        )
    })?
    .unsigned_abs() as usize;
    if buf.len() < size {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: as above, for a buffer holding the `size` bytes C returned
    // for these arguments, so every pointer C derives from it stays inside;
    // C never dereferences it.
    status(unsafe {
        ffi::av_image_fill_arrays(
            data.as_mut_ptr(),
            linesizes.as_mut_ptr(),
            buf.as_ptr(),
            format.into_raw(),
            width,
            height,
            align,
        )
    })?;
    let ranges = plane_ranges(buf.as_ptr().addr(), data.map(<*mut u8>::addr), size);
    Ok((ranges, linesizes, size))
}

/// Wraps: `av_image_fill_arrays`
///
/// The planes of a `width` x `height` image of pixel format `format` laid
/// out back to back from the start of `buf`, each row padded to a multiple
/// of `align` bytes, borrowed from `buf` for reading: the layout
/// [`buffer_size`] sizes and [`copy_to_buffer`] writes.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::fill_arrays;
/// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
/// let buf = [0u8; 64 * 48 * 3 / 2];
/// let planes = fill_arrays(&buf, PixelFormat::YUV420P, 64, 48, 1).unwrap();
/// assert_eq!(planes.linesizes, [64, 32, 32, 0]);
/// assert_eq!(planes.data.map(<[u8]>::len), [64 * 48, 32 * 24, 32 * 24, 0]);
/// ```
///
/// # Errors
///
/// Fails as [`buffer_size`] does, and with `AVERROR(EINVAL)` when `buf` is shorter than that size.
pub fn fill_arrays(
    buf: &[u8],
    format: PixelFormat,
    width: i32,
    height: i32,
    align: usize,
) -> Result<Planes<'_>, i32> {
    let (ranges, linesizes, _) = c_fill_arrays(buf, format, width, height, align)?;
    Ok(Planes {
        data: core::array::from_fn(|i| &buf[ranges[i].clone()]),
        linesizes: linesizes.map(|l| l as isize),
    })
}

/// Wraps: `av_image_fill_arrays`
///
/// As [`fill_arrays`], with the planes borrowed from `buf` for writing.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_image_fill_arrays` returns when it fails.
pub fn fill_arrays_mut(
    buf: &mut [u8],
    format: PixelFormat,
    width: i32,
    height: i32,
    align: usize,
) -> Result<PlanesMut<'_>, i32> {
    let (ranges, linesizes, _) = c_fill_arrays(buf, format, width, height, align)?;
    Ok(PlanesMut {
        data: split_planes(buf, &ranges),
        linesizes: linesizes.map(|l| l as isize),
    })
}

/// Wraps: `av_image_get_buffer_size`
///
/// The bytes a `width` x `height` image of pixel format `format` takes in
/// one buffer, its rows padded to a multiple of `align` bytes and its
/// planes (palette included) back to back: the size [`fill_arrays`] lays
/// out and [`copy_to_buffer`] writes.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` for a format without a descriptor or a hardware format, a size
/// [`check_size`] rejects or past `INT_MAX`, and an `align` that is not a power of two up to 2^30
/// (C takes any `int`, which its rounding up is meaningless or overflows for).
pub fn buffer_size(
    format: PixelFormat,
    width: i32,
    height: i32,
    align: usize,
) -> Result<usize, i32> {
    let align = c_align(align)?;
    // SAFETY: by value; C checks the size first, which keeps its linesizes
    // under 64 MiB, so their alignment to at most 2^30 stays in `int`.
    let ret = unsafe { ffi::av_image_get_buffer_size(format.into_raw(), width, height, align) };
    Ok(status(ret)?.unsigned_abs() as usize)
}

/// The planes of an image, borrowed for reading: C's `uint8_t *data[4]` and
/// `linesize[4]` pairs.
///
/// Each plane is laid out as the [module docs](self) describe: rows
/// `linesizes[i]` bytes apart, row 0 first for a non-negative linesize and
/// last for a negative one. A plane the pixel format does not use may be
/// empty. Every function taking planes checks the ones it reads against the
/// pixel format's layout before C runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Planes<'a> {
    /// The bytes of each plane.
    pub data: [&'a [u8]; 4],
    /// The distance from each row to the next, in bytes.
    pub linesizes: [isize; 4],
}

/// The planes of an image, borrowed for writing: as [`Planes`], over
/// disjoint slices.
#[derive(Debug, Default)]
pub struct PlanesMut<'a> {
    /// The bytes of each plane.
    pub data: [&'a mut [u8]; 4],
    /// The distance from each row to the next, in bytes.
    pub linesizes: [isize; 4],
}

impl PlanesMut<'_> {
    /// The same planes, borrowed for reading.
    #[must_use]
    pub fn as_planes(&self) -> Planes<'_> {
        Planes {
            data: self.data.each_ref().map(|d| &**d),
            linesizes: self.linesizes,
        }
    }

    /// The same planes, reborrowed for writing.
    pub fn reborrow(&mut self) -> PlanesMut<'_> {
        PlanesMut {
            data: self.data.each_mut().map(|d| &mut **d),
            linesizes: self.linesizes,
        }
    }
}

impl<'a> From<PlanesMut<'a>> for Planes<'a> {
    fn from(planes: PlanesMut<'a>) -> Self {
        Planes {
            data: planes.data.map(|d| &*d),
            linesizes: planes.linesizes,
        }
    }
}

/// An image in one `av_malloc` block, as `av_image_alloc` lays it out:
/// planes back to back, each row padded to the alignment it was built
/// with. The block is released by `av_free` when the image drops.
#[derive(Debug)]
pub struct Image {
    buf: HeapVec<u8>,
    planes: PlaneRanges,
    linesizes: [i32; 4],
    format: PixelFormat,
    width: i32,
    height: i32,
}

impl Image {
    /// Wraps: `av_image_alloc`
    ///
    /// Allocates a `width` x `height` image of pixel format `format` with
    /// every linesize a multiple of `align` (a power of two up to 2^30), and
    /// with rows padded to a multiple of 8 pixels for an `align` above 7.
    /// The planes are zero-filled; C leaves them uninitialised.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::imgutils::Image;
    /// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
    /// let mut image = Image::alloc(100, 50, PixelFormat::YUV420P, 32).unwrap();
    /// assert_eq!(image.linesizes(), [128, 64, 64, 0]);
    /// image.planes_mut().data[0].fill(16);
    /// assert_eq!(image.planes().data[1].len(), 64 * 25);
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with `AVERROR(EINVAL)` as [`buffer_size`] does, for an `align` below 4 with a
    /// palettized format (after logging it at `AV_LOG_ERROR`), and with `AVERROR(ENOMEM)` when the
    /// allocation fails.
    pub fn alloc(width: i32, height: i32, format: PixelFormat, align: usize) -> Result<Self, i32> {
        let c_align = c_align(align)?;
        let mut pointers = [ptr::null_mut(); 4];
        let mut linesizes = [0; 4];
        // SAFETY: `pointers` and `linesizes` are the arrays C writes. C
        // checks the size first, which keeps the width (rounded up to 8)
        // and the linesizes (aligned to at most 2^30) inside `int`.
        let size = status(unsafe {
            ffi::av_image_alloc(
                pointers.as_mut_ptr(),
                linesizes.as_mut_ptr(),
                width,
                height,
                format.into_raw(),
                c_align,
            )
        })?
        .unsigned_abs() as usize;
        // C allocated the plane sizes it returns the sum of, plus `align`.
        let len = size + align;
        // SAFETY: on success `pointers[0]` is a fresh `av_malloc` block of
        // `len` bytes that the caller owns and releases with `av_free`, the
        // `Heap` policy; every byte is a valid `MaybeUninit<u8>`.
        let buf = unsafe { HeapVec::<MaybeUninit<u8>>::from_raw_parts(pointers[0].cast(), len) };
        let buf = buf.ok_or(AVERROR_ENOMEM)?;
        // C wrote nothing into the block: it has no systematic palette for
        // the one palettized format, and no padding before it. One write
        // for the whole block, not a `fill` of a `&mut` per byte.
        // SAFETY: `buf` owns the `len` bytes at its pointer, all writable.
        unsafe { ptr::write_bytes(buf.as_ptr(), 0, len) };
        // SAFETY: the write initialised every byte.
        let buf = unsafe { buf.assume_init() };
        let planes = plane_ranges(pointers[0].addr(), pointers.map(<*mut u8>::addr), size);
        Ok(Self {
            buf,
            planes,
            linesizes,
            format,
            width,
            height,
        })
    }

    /// The image's pixel format.
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// The image's width, in pixels.
    #[must_use]
    pub fn width(&self) -> i32 {
        self.width
    }

    /// The image's height, in pixels.
    #[must_use]
    pub fn height(&self) -> i32 {
        self.height
    }

    /// The distance between rows of each plane, in bytes; 0 for the planes
    /// the format does not use.
    #[must_use]
    pub fn linesizes(&self) -> [i32; 4] {
        self.linesizes
    }

    /// The image's planes, for reading.
    #[must_use]
    pub fn planes(&self) -> Planes<'_> {
        let buf = self.buf.as_slice();
        Planes {
            data: core::array::from_fn(|i| &buf[self.planes[i].clone()]),
            linesizes: self.linesizes.map(|l| l as isize),
        }
    }

    /// The image's planes, for writing.
    pub fn planes_mut(&mut self) -> PlanesMut<'_> {
        PlanesMut {
            data: split_planes(self.buf.as_mut_slice(), &self.planes),
            linesizes: self.linesizes.map(|l| l as isize),
        }
    }
}

/// Why [`copy`], [`copy_from_mut`] or [`copy_from_uncacheable`] refused to
/// copy an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageCopyError {
    /// The pixel format has no descriptor, or is a hardware format: C
    /// copies nothing.
    UnsupportedFormat,
    /// The width or height is negative, or the width makes a row wider than
    /// `INT_MAX` bytes.
    InvalidSize,
    /// Plane `index` cannot be copied as given; for the palette of a
    /// palettized format (plane 1), a slice shorter than its 1024 bytes.
    Plane {
        /// The plane, 0 to 3.
        index: usize,
        /// What is wrong with it.
        error: PlaneCopyError,
    },
}

impl fmt::Display for ImageCopyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("pixel format without copyable planes"),
            Self::InvalidSize => f.write_str("image dimension out of range"),
            Self::Plane { index, error } => write!(f, "plane {index}: {error}"),
        }
    }
}

impl std::error::Error for ImageCopyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Plane { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Where row 0 of each plane an operation touches starts in its slice;
/// `None` for the planes it leaves alone.
type Origins = [Option<usize>; 4];

/// What C's `image_copy` copies of a `width` x `height` image: the row
/// width and rows of each plane, and whether plane 1 is a palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CopyPlan {
    extents: [Option<(usize, usize)>; 4],
    palette: bool,
}

impl CopyPlan {
    #[expect(
        clippy::cast_sign_loss,
        reason = "non-negative sizes the checks above validated"
    )]
    fn new(format: PixelFormat, width: i32, height: i32) -> Result<Self, ImageCopyError> {
        let g = Geometry::of(format)
            .filter(|g| !g.has(ffi::AV_PIX_FMT_FLAG_HWACCEL))
            .ok_or(ImageCopyError::UnsupportedFormat)?;
        let (Ok(w), Ok(h)) = (usize::try_from(width), usize::try_from(height)) else {
            return Err(ImageCopyError::InvalidSize);
        };
        let mut plan = Self::default();
        if g.has(ffi::AV_PIX_FMT_FLAG_PAL) {
            // C copies `width` bytes of plane 0 a row, then the palette.
            plan.extents[0] = Some((w, h));
            plan.palette = true;
            return Ok(plan);
        }
        for i in 0..g.planes() {
            let bytewidth = linesize(format, width, i).map_err(|_| ImageCopyError::InvalidSize)?;
            plan.extents[i] = Some((bytewidth as usize, g.rows(i, h)));
        }
        Ok(plan)
    }

    /// The offsets of row 0 in each plane of `dst` and `src` C copies,
    /// after the checks C asserts, and that each copied row lies inside its
    /// plane; `None` for the planes C leaves alone or copies nothing of.
    fn origins(
        &self,
        dst: &PlanesMut<'_>,
        src: &Planes<'_>,
    ) -> Result<(Origins, Origins), ImageCopyError> {
        let (mut dst_at, mut src_at) = ([None; 4], [None; 4]);
        for (i, extent) in self.extents.iter().enumerate() {
            let Some((bytewidth, rows)) = *extent else {
                continue;
            };
            let (d, s) = plane_copy_origins(
                dst.data[i].len(),
                dst.linesizes[i],
                src.data[i].len(),
                src.linesizes[i],
                bytewidth,
                rows,
            )
            .map_err(|error| ImageCopyError::Plane { index: i, error })?;
            if bytewidth > 0 && rows > 0 {
                (dst_at[i], src_at[i]) = (Some(d), Some(s));
            }
        }
        if self.palette {
            let short = |error| ImageCopyError::Plane { index: 1, error };
            if dst.data[1].len() < PALETTE_SIZE {
                return Err(short(PlaneCopyError::DestinationTooShort));
            }
            if src.data[1].len() < PALETTE_SIZE {
                return Err(short(PlaneCopyError::SourceTooShort));
            }
            (dst_at[1], src_at[1]) = (Some(0), Some(0));
        }
        Ok((dst_at, src_at))
    }
}

/// The C plane pointers for planes whose row 0 is at `at`, NULL for the
/// planes C must skip.
fn row0_ptrs_mut(planes: &mut PlanesMut<'_>, at: Origins) -> [*mut u8; 4] {
    let mut ptrs = [ptr::null_mut(); 4];
    for ((p, data), at) in ptrs.iter_mut().zip(&mut planes.data).zip(at) {
        if let Some(at) = at {
            // SAFETY: `at` is inside `data`, which the pointer derives from
            // whole, so C may reach every row of it.
            *p = unsafe { data.as_mut_ptr().add(at) };
        }
    }
    ptrs
}

/// As [`row0_ptrs_mut`], for planes C reads.
fn row0_ptrs(planes: &Planes<'_>, at: Origins) -> [*const u8; 4] {
    let mut ptrs = [ptr::null(); 4];
    for ((p, data), at) in ptrs.iter_mut().zip(&planes.data).zip(at) {
        if let Some(at) = at {
            // SAFETY: as in `row0_ptrs_mut`.
            *p = unsafe { data.as_ptr().add(at) };
        }
    }
    ptrs
}

/// `linesizes` as the `int`s `av_image_copy` takes, for the planes it
/// copies.
fn int_linesizes(
    linesizes: [isize; 4],
    at: Origins,
    error: PlaneCopyError,
) -> Result<[c_int; 4], ImageCopyError> {
    let mut out = [0; 4];
    for (i, (l, at)) in linesizes.into_iter().zip(at).enumerate() {
        if at.is_some() {
            out[i] = c_int::try_from(l).map_err(|_| ImageCopyError::Plane { index: i, error })?;
        }
    }
    Ok(out)
}

/// The checked arguments `av_image_copy` and `av_image_copy2` take.
type CopyArgs = ([*mut u8; 4], [c_int; 4], [*const u8; 4], [c_int; 4]);

fn copy_args(
    dst: &mut PlanesMut<'_>,
    src: &Planes<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
) -> Result<CopyArgs, ImageCopyError> {
    let plan = CopyPlan::new(format, width, height)?;
    let (dst_at, src_at) = plan.origins(dst, src)?;
    let dst_ls = int_linesizes(dst.linesizes, dst_at, PlaneCopyError::TooLarge)?;
    let src_ls = int_linesizes(src.linesizes, src_at, PlaneCopyError::TooLarge)?;
    Ok((
        row0_ptrs_mut(dst, dst_at),
        dst_ls,
        row0_ptrs(src, src_at),
        src_ls,
    ))
}

/// Wraps: `av_image_copy`
///
/// Copies a `width` x `height` image of pixel format `format` from `src` to
/// `dst`: in each plane the format uses, as many rows as it has (chroma
/// rows subsampled) of the bytes [`linesize`] gives, row `r` to row `r` as
/// [`copy_plane`] does; for a palettized format, `width` bytes of each row
/// of plane 0 and the 1024-byte palette at the start of plane 1. Bytes
/// between rows, and other planes, are left alone.
///
/// # Errors
///
/// Fails without touching `dst` for a format C copies nothing of, a negative dimension, and a plane
/// whose slices or linesizes cannot hold the copy ([`ImageCopyError`]), including linesizes beyond
/// `i32` and rows wider than a linesize, on which C aborts.
pub fn copy(
    dst: &mut PlanesMut<'_>,
    src: Planes<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
) -> Result<(), ImageCopyError> {
    let (dst_ptrs, dst_ls, src_ptrs, src_ls) = copy_args(dst, &src, format, width, height)?;
    // SAFETY: every non-NULL pointer is row 0 of a plane whose copied rows
    // `CopyPlan::origins` checked inside its slice (the palette's 1024
    // bytes included), with linesizes covering the row width as C asserts;
    // C skips NULL planes and only reads the source. `dst` is borrowed
    // exclusively and apart from `src`, so no copy overlaps; the format has
    // a descriptor and no width overflows C's linesizes. C keeps no pointer.
    unsafe {
        ffi::av_image_copy(
            dst_ptrs.as_ptr(),
            dst_ls.as_ptr(),
            src_ptrs.as_ptr(),
            src_ls.as_ptr(),
            format.into_raw(),
            width,
            height,
        );
    };
    Ok(())
}

/// Wraps: `av_image_copy2`
///
/// As [`copy`], from the planes of another writable image, borrowed for
/// reading: C's `av_image_copy` for sources typed `uint8_t *`, without
/// first reborrowing them with [`PlanesMut::as_planes`].
///
/// # Errors
///
/// As [`copy`].
pub fn copy_from_mut(
    dst: &mut PlanesMut<'_>,
    src: &PlanesMut<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
) -> Result<(), ImageCopyError> {
    let (dst_ptrs, dst_ls, src_ptrs, src_ls) =
        copy_args(dst, &src.as_planes(), format, width, height)?;
    let src_ptrs = src_ptrs.map(<*const u8>::cast_mut);
    // SAFETY: as in `copy`: `av_image_copy2` forwards to `av_image_copy`,
    // which only reads through the source pointers despite their type.
    unsafe {
        ffi::av_image_copy2(
            dst_ptrs.as_ptr(),
            dst_ls.as_ptr(),
            src_ptrs.as_ptr(),
            src_ls.as_ptr(),
            format.into_raw(),
            width,
            height,
        );
    };
    Ok(())
}

/// Wraps: `av_image_copy_uc_from`
///
/// As [`copy`], for a source in uncacheable memory (GPU-mapped, say), each
/// plane copied as [`copy_plane_from_uncacheable`] copies one: rows padded
/// to 64 bytes may be copied whole where both linesizes cover them.
///
/// # Divergence from C
///
/// When a plane is one C's SSE4 path faults on or overruns (see
/// [`copy_plane_from_uncacheable`]), or the image is empty (on which that
/// path loops away), the planes are copied one by one through
/// [`copy_plane_from_uncacheable`] and the palette by a plain copy, rather
/// than all through `av_image_copy_uc_from`.
///
/// # Errors
///
/// Fails as [`copy`] does, and for linesizes or row widths [`copy_plane_from_uncacheable`] refuses.
pub fn copy_from_uncacheable(
    dst: &mut PlanesMut<'_>,
    src: Planes<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
) -> Result<(), ImageCopyError> {
    let plan = CopyPlan::new(format, width, height)?;
    let (dst_at, src_at) = plan.origins(dst, &src)?;
    let mut whole = width > 0 && height > 0;
    for (i, extent) in plan.extents.iter().enumerate() {
        let Some((bytewidth, rows)) = *extent else {
            continue;
        };
        let (dl, sl) = (dst.linesizes[i], src.linesizes[i]);
        // `copy_plane_from_uncacheable`'s range checks.
        if dl == isize::MIN
            || sl == isize::MIN
            || bytewidth > isize::MAX as usize - (UC_ROW_ALIGN - 1)
        {
            let error = PlaneCopyError::TooLarge;
            return Err(ImageCopyError::Plane { index: i, error });
        }
        whole &= bytewidth > 0
            && rows > 0
            && uc_call_is_sound(dst.data[i], dl, src.data[i], sl, bytewidth, rows);
    }
    if !whole {
        for (i, extent) in plan.extents.iter().enumerate() {
            if let Some((bytewidth, rows)) = *extent {
                let (dl, sl) = (dst.linesizes[i], src.linesizes[i]);
                copy_plane_from_uncacheable(dst.data[i], dl, src.data[i], sl, bytewidth, rows)
                    .map_err(|error| ImageCopyError::Plane { index: i, error })?;
            }
        }
        if plan.palette {
            dst.data[1][..PALETTE_SIZE].copy_from_slice(&src.data[1][..PALETTE_SIZE]);
        }
        return Ok(());
    }
    let (dst_ls, src_ls) = (dst.linesizes, src.linesizes);
    let dst_ptrs = row0_ptrs_mut(dst, dst_at);
    let src_ptrs = row0_ptrs(&src, src_at);
    // SAFETY: as in `copy`, with linesizes in `ptrdiff_t`. Every copied
    // plane is non-empty (C's SSE4 path loops away on an empty one) and
    // passed `uc_call_is_sound`, so where C takes that path its padded
    // rows lie inside both slices and are 16-byte aligned.
    unsafe {
        ffi::av_image_copy_uc_from(
            dst_ptrs.as_ptr(),
            dst_ls.as_ptr(),
            src_ptrs.as_ptr(),
            src_ls.as_ptr(),
            format.into_raw(),
            width,
            height,
        );
    };
    Ok(())
}

/// Wraps: `av_image_copy_to_buffer`
///
/// Copies a `width` x `height` image of pixel format `format` from `src`
/// into `dst`, its planes (palette included) back to back and each row
/// padded to a multiple of `align` bytes, as [`fill_arrays`] lays them out;
/// returns the bytes written, [`buffer_size`]'s size. Padding is left
/// alone.
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)` as [`buffer_size`] does, when `dst` is shorter than that size, and
/// when a plane of `src` does not hold the rows C reads (1024 bytes of plane 1 for a palette) or
/// has a linesize beyond `i32`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "non-negative sizes the checks above validated; `align` is bounded by `c_align`'s check"
)]
pub fn copy_to_buffer(
    dst: &mut [u8],
    src: Planes<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
    align: usize,
) -> Result<usize, i32> {
    let size = buffer_size(format, width, height, align)?;
    if size > dst.len() {
        return Err(AVERROR_EINVAL);
    }
    let g = Geometry::of(format).ok_or(AVERROR_EINVAL)?;
    let row_widths = fill_linesizes(format, width)?;
    // `buffer_size` accepted the image, so it has rows.
    let rows = height as usize;
    let mut at = [None; 4];
    let mut linesizes = [0; 4];
    for i in 0..g.planes().max(1) {
        let row_width = row_widths[i] as usize;
        at[i] = Some(
            plane_origin(
                src.data[i].len(),
                src.linesizes[i],
                row_width,
                g.rows(i, rows),
            )
            .ok_or(AVERROR_EINVAL)?,
        );
        linesizes[i] = c_int::try_from(src.linesizes[i]).map_err(|_| AVERROR_EINVAL)?;
    }
    if g.has(ffi::AV_PIX_FMT_FLAG_PAL) {
        if src.data[1].len() < PALETTE_SIZE {
            return Err(AVERROR_EINVAL);
        }
        at[1] = Some(0);
    }
    let src_ptrs = row0_ptrs(&src, at);
    let dst_size = c_int::try_from(dst.len()).unwrap_or(c_int::MAX);
    // SAFETY: C writes the `size` bytes it checks against `dst_size`, which
    // `dst` holds; it reads each plane's rows from the row-0 pointers, all
    // inside their slices as checked above with C's own row widths and
    // counts, and the palette's 1024 bytes. The source planes are shared
    // borrows apart from the exclusive `dst`. The size checks C repeats
    // keep its linesize arithmetic in `int`. C keeps no pointer.
    let ret = unsafe {
        ffi::av_image_copy_to_buffer(
            dst.as_mut_ptr(),
            dst_size,
            src_ptrs.as_ptr(),
            linesizes.as_ptr(),
            format.into_raw(),
            width,
            height,
            align as c_int,
        )
    };
    status(ret).map(|n| n.unsigned_abs() as usize)
}

/// The row-0 pointers and linesizes of the planes `av_image_fill_color`
/// fills in `dst` for a `width` x `height` image, after checking each plane
/// holds them.
#[expect(
    clippy::cast_sign_loss,
    reason = "non-negative sizes the checks above validated"
)]
fn fill_args(
    dst: &mut PlanesMut<'_>,
    format: PixelFormat,
    width: i32,
    height: i32,
) -> Result<([*mut u8; 4], [isize; 4]), i32> {
    let g = Geometry::of(format).ok_or(AVERROR_EINVAL)?;
    let nb_planes = format.plane_count().ok_or(AVERROR_EINVAL)?.min(4);
    let mut at = [None; 4];
    for (i, slot) in at.iter_mut().enumerate().take(nb_planes) {
        let row_width = linesize(format, width, i)? as usize;
        let rows = g.rows(i, height as usize);
        *slot = Some(
            plane_origin(dst.data[i].len(), dst.linesizes[i], row_width, rows)
                .ok_or(AVERROR_EINVAL)?,
        );
    }
    let linesizes = dst.linesizes;
    Ok((row0_ptrs_mut(dst, at), linesizes))
}

/// Wraps: `av_image_fill_color`
///
/// Fills a `width` x `height` image of pixel format `format` in `dst` with
/// one color: `color` holds a value per component in the format's
/// component order (red, green, blue or luma, chroma U, chroma V, then
/// alpha), integers or, for float formats, the bits of the float. Each row
/// of each plane is written for the bytes [`linesize`] gives; bytes between
/// rows are left alone. A negative height fills nothing.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::imgutils::{Image, fill_color};
/// use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;
/// let mut image = Image::alloc(4, 2, PixelFormat::RGB24, 1).unwrap();
/// fill_color(
///     &mut image.planes_mut(),
///     PixelFormat::RGB24,
///     [1, 2, 3, 0],
///     4,
///     2,
/// )
/// .unwrap();
/// assert_eq!(&image.planes().data[0][..6], &[1, 2, 3, 1, 2, 3]);
/// ```
///
/// # Errors
///
/// Fails with `AVERROR(EINVAL)`, before writing, for a format without a descriptor, a hardware
/// format, a format C cannot fill (components wider than 32 bits, or pixel groups wider than 32
/// bytes), a width [`linesize`] rejects, and a plane of `dst` that does not hold its rows.
pub fn fill_color(
    dst: &mut PlanesMut<'_>,
    format: PixelFormat,
    color: [u32; 4],
    width: i32,
    height: i32,
) -> Result<(), i32> {
    // C fills no rows for any negative height; `INT_MIN` would overflow its
    // rounding of chroma rows.
    let height = height.max(0);
    let (ptrs, linesizes) = fill_args(dst, format, width, height)?;
    // SAFETY: every row C writes, `linesize(format, width, i)` bytes of each
    // of the plane's rows, lies inside its plane of the exclusively
    // borrowed `dst`, from the row-0 pointers checked above; `color` is the
    // `uint32_t[4]` C reads; no width overflows C's linesizes. C keeps no
    // pointer.
    status(unsafe {
        ffi::av_image_fill_color(
            ptrs.as_ptr(),
            linesizes.as_ptr(),
            format.into_raw(),
            color.as_ptr(),
            width,
            height,
            0,
        )
    })
    .map(drop)
}

/// Wraps: `av_image_fill_color`
///
/// # Errors
///
/// Checks that [`fill_color`] can fill a `width` pixels wide image of pixel format `format` with
/// `color`, without an image: fails as it does for the format, the color and the width.
pub fn check_fill_color(format: PixelFormat, color: [u32; 4], width: i32) -> Result<(), i32> {
    if Geometry::of(format).is_some_and(|g| (0..4).any(|p| g.linesize_overflows(width, p))) {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: NULL planes ask C only to check its arguments; `color` is the
    // `uint32_t[4]` it reads; no width overflows C's linesizes.
    status(unsafe {
        ffi::av_image_fill_color(
            ptr::null(),
            ptr::null(),
            format.into_raw(),
            color.as_ptr(),
            width,
            0,
            0,
        )
    })
    .map(drop)
}

/// Wraps: `av_image_fill_black`
///
/// Fills a `width` x `height` image of pixel format `format` in `dst` with
/// black, as [`fill_color`] fills with a color: zero for RGB, XYZ and
/// palette indices, the floor of `range` for luma (16 at 8 bits for
/// limited range, 0 for [`ColorRange::JPEG`]), mid-scale for chroma, and
/// opaque alpha; monochrome white formats get their set bit.
///
/// # Errors
///
/// Fails as [`fill_color`] does, and for a limited-range format C has no black for (luma below 8
/// bits).
pub fn fill_black(
    dst: &mut PlanesMut<'_>,
    format: PixelFormat,
    range: ColorRange,
    width: i32,
    height: i32,
) -> Result<(), i32> {
    // As in `fill_color`.
    let height = height.max(0);
    let (ptrs, linesizes) = fill_args(dst, format, width, height)?;
    // SAFETY: as in `fill_color`; the range is passed by value.
    status(unsafe {
        ffi::av_image_fill_black(
            ptrs.as_ptr(),
            linesizes.as_ptr(),
            format.into_raw(),
            range.into_raw(),
            width,
            height,
        )
    })
    .map(drop)
}

/// Wraps: `av_image_fill_black`
///
/// Checks that [`fill_black`] can fill a `width` pixels wide image of pixel
/// format `format` and color range `range`, without an image.
///
/// # Errors
///
/// Returns the negative `AVERROR` code `av_image_fill_black` returns when it fails.
pub fn check_fill_black(format: PixelFormat, range: ColorRange, width: i32) -> Result<(), i32> {
    if Geometry::of(format).is_some_and(|g| (0..4).any(|p| g.linesize_overflows(width, p))) {
        return Err(AVERROR_EINVAL);
    }
    // SAFETY: NULL planes ask C only to check its arguments, which it
    // passes on to `av_image_fill_color` with NULL planes; no width
    // overflows C's linesizes.
    status(unsafe {
        ffi::av_image_fill_black(
            ptr::null(),
            ptr::null(),
            format.into_raw(),
            range.into_raw(),
            width,
            0,
        )
    })
    .map(drop)
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
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
    use super::*;

    /// A 64-byte aligned buffer, so tests choose every row's alignment.
    #[repr(C, align(64))]
    struct Aligned<const N: usize>([u8; N]);

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    /// `av_image_copy_plane` of the same planes, the C reference of the plain
    /// path.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the arguments of the C routine it stands for"
    )]
    fn c_copy_plane(
        dst: &mut [u8],
        dst_at: usize,
        dl: i32,
        src: &[u8],
        src_at: usize,
        sl: i32,
        w: usize,
        h: usize,
    ) {
        // For the unsafe operations below: the callers pass planes `plane_copy_origins` accepted,
        // with positive `w` and `h`, at the origins it computed.
        {
            // SAFETY: as the comment below states, for this part of the operation.
            let as_mut_ptr_add = unsafe { dst.as_mut_ptr().add(dst_at) };
            // SAFETY: as the comment below states, for this part of the operation.
            let as_ptr_add = unsafe { src.as_ptr().add(src_at) };
            // SAFETY: as the comment above this block states.
            unsafe {
                ffi::av_image_copy_plane(
                    as_mut_ptr_add,
                    dl,
                    as_ptr_add,
                    sl,
                    w as c_int,
                    h as c_int,
                );
            };
        }
    }

    #[test]
    fn plane_origin_spans_exactly_the_rows() {
        assert_eq!(plane_origin(0, 5, 3, 0), Some(0));
        assert_eq!(plane_origin(0, -5, 3, 0), Some(0));
        assert_eq!(plane_origin(3, 5, 3, 1), Some(0));
        assert_eq!(plane_origin(2, 5, 3, 1), None);
        assert_eq!(plane_origin(13, 5, 3, 3), Some(0));
        assert_eq!(plane_origin(12, 5, 3, 3), None);
        assert_eq!(plane_origin(13, -5, 3, 3), Some(10));
        assert_eq!(plane_origin(12, -5, 3, 3), None);
        // Overlapping and repeated rows.
        assert_eq!(plane_origin(4, 1, 3, 2), Some(0));
        assert_eq!(plane_origin(3, 0, 3, 100), Some(0));
        // Spans beyond `usize` never fit.
        assert_eq!(plane_origin(usize::MAX - 1, isize::MAX, 1, 3), None);
        assert_eq!(plane_origin(usize::MAX, isize::MAX, 1, 3), Some(0));
        assert_eq!(plane_origin(usize::MAX, 1, usize::MAX, 2), None);
    }

    #[test]
    fn plane_copy_origins_rejects_as_c_asserts() {
        use PlaneCopyError::*;
        assert_eq!(plane_copy_origins(100, 4, 100, 8, 5, 1), Err(RowTooWide));
        assert_eq!(plane_copy_origins(100, 8, 100, -4, 5, 0), Err(RowTooWide));
        assert_eq!(
            plane_copy_origins(100, isize::MIN, 100, 8, 5, 1),
            Ok((0, 0))
        );
        assert_eq!(
            plane_copy_origins(100, isize::MIN, 100, 8, 5, 2),
            Err(DestinationTooShort)
        );
        assert_eq!(
            plane_copy_origins(12, 8, 13, 8, 5, 2),
            Err(DestinationTooShort)
        );
        assert_eq!(plane_copy_origins(13, 8, 12, -8, 5, 2), Err(SourceTooShort));
        assert_eq!(plane_copy_origins(13, -8, 13, 8, 5, 2), Ok((8, 0)));
    }

    #[test]
    fn plane_copy_errors_display() {
        for e in [
            PlaneCopyError::TooLarge,
            PlaneCopyError::RowTooWide,
            PlaneCopyError::DestinationTooShort,
            PlaneCopyError::SourceTooShort,
        ] {
            assert!(!e.to_string().is_empty());
            let _: &dyn std::error::Error = &e;
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copy_rows_matches_c_plain_path() {
        for (dl, sl, w, h) in [
            (7isize, 9isize, 5usize, 4usize),
            (-7, 9, 5, 4),
            (7, -9, 7, 3),
            (-8, -8, 8, 1),
        ] {
            let src = pattern(256, 3);
            let (mut rust, mut c) = (pattern(256, 100), pattern(256, 100));
            let (dst_at, src_at) = plane_copy_origins(rust.len(), dl, src.len(), sl, w, h).unwrap();
            copy_rows(&mut rust, dst_at, dl, &src, src_at, sl, w, h);
            c_copy_plane(&mut c, dst_at, dl as i32, &src, src_at, sl as i32, w, h);
            assert_eq!(rust, c, "{dl} {sl} {w} {h}");
        }
    }

    #[test]
    fn uc_call_is_sound_only_for_aligned_covered_fast_planes() {
        let buf = Aligned([0u8; 512]);
        let b = &buf.0[..];
        // Not eligible for the fast path: C's plain path takes anything.
        assert!(uc_call_is_sound(&b[1..], 63, b, 64, 10, 3));
        assert!(uc_call_is_sound(&b[1..], -64, &b[3..], 64, 10, 3));
        assert!(uc_call_is_sound(&b[1..200], 64, b, 64, 65, 2));
        // Eligible, aligned and covered.
        assert!(uc_call_is_sound(b, 64, b, 128, 10, 3));
        assert!(uc_call_is_sound(&b[..192], 64, &b[16..16 + 192], 64, 64, 3));
        // Eligible but misaligned rows or pointers, or a short last row.
        assert!(!uc_call_is_sound(b, 72, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, b, 80 + 8, 10, 3));
        assert!(!uc_call_is_sound(&b[8..], 64, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, &b[1..], 64, 10, 3));
        assert!(!uc_call_is_sound(&b[..64 * 2 + 10], 64, b, 64, 10, 3));
        assert!(!uc_call_is_sound(b, 64, &b[..64 * 2 + 63], 64, 10, 3));
    }

    /// Regression for a C defect: `av_image_copy_plane_uc_from` faults on
    /// such planes on SSE4.1 CPUs; the wrapper copies them as C's plain path
    /// does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn misaligned_fast_planes_copy_as_the_plain_path() {
        let src = Aligned::<1024>(core::array::from_fn(|i| (i * 7 % 251) as u8));
        for (dst_off, dl, src_off, sl, w, h) in [
            (0usize, 72isize, 0usize, 64isize, 10usize, 4usize),
            (8, 64, 0, 64, 10, 4),
            (0, 64, 1, 64, 64, 4),
            (0, 80, 0, 96, 17, 3),
        ] {
            let mut rust = Aligned([0xEEu8; 1024]);
            let mut c = Aligned([0xEEu8; 1024]);
            let len = (h - 1) * dl as usize + w;
            copy_plane_from_uncacheable(
                &mut rust.0[dst_off..dst_off + len],
                dl,
                &src.0[src_off..],
                sl,
                w,
                h,
            )
            .unwrap();
            c_copy_plane(
                &mut c.0, dst_off, dl as i32, &src.0, src_off, sl as i32, w, h,
            );
            assert_eq!(rust.0, c.0, "{dst_off} {dl} {src_off} {sl} {w} {h}");
        }
    }

    /// A fast-path plane whose last row lacks the rounded-up width copies
    /// only `bytewidth` bytes there, never past the slice.
    #[test]
    fn short_last_rows_copy_as_the_plain_path() {
        let src = Aligned::<256>(core::array::from_fn(|i| i as u8));
        let mut dst = Aligned([0u8; 256]);
        let len = 64 * 2 + 10;
        copy_plane_from_uncacheable(&mut dst.0[..len], 64, &src.0, 64, 10, 3).unwrap();
        for r in 0..3 {
            assert_eq!(dst.0[r * 64..r * 64 + 10], src.0[r * 64..r * 64 + 10]);
        }
        assert!(dst.0[len..].iter().all(|&b| b == 0));
    }

    #[test]
    fn oversized_arguments_are_rejected_before_c() {
        let mut dst = [0u8; 8];
        let src = [0u8; 8];
        assert_eq!(
            copy_plane(&mut dst, 8, &src, 8, c_int::MAX as usize + 1, 0),
            Err(PlaneCopyError::TooLarge)
        );
        assert_eq!(
            copy_plane(&mut dst, 8, &src, 8, 1, c_int::MAX as usize + 1),
            Err(PlaneCopyError::TooLarge)
        );
        let uc = |dl, sl, w, h| copy_plane_from_uncacheable(&mut [], dl, &[], sl, w, h);
        assert_eq!(uc(isize::MIN, 8, 1, 0), Err(PlaneCopyError::TooLarge));
        assert_eq!(uc(8, isize::MIN, 1, 0), Err(PlaneCopyError::TooLarge));
        assert_eq!(
            uc(isize::MAX, isize::MAX, isize::MAX as usize - 62, 0),
            Err(PlaneCopyError::TooLarge)
        );
        assert_eq!(
            uc(isize::MAX, isize::MAX, isize::MAX as usize - 63, 0),
            Ok(())
        );
        assert_eq!(
            uc(8, 8, 1, c_int::MAX as usize + 1),
            Err(PlaneCopyError::TooLarge)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn overflowing_log_offsets_are_rejected_before_c() {
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        let r = check_size_with_context(0, 0, max + 1, None);
        assert_eq!(r, Err(AVERROR_EINVAL));
        // The size is valid, so nothing is logged.
        let r = check_size_with_context(16, 16, c_int::MAX, None);
        assert_eq!(r, Err(AVERROR_EINVAL));
        let r = check_size_with_context(16, 16, max, None);
        assert_eq!(r, Ok(()));
        assert_eq!(AVERROR_EINVAL, -22);
    }

    /// Runs the test `name` alone in a child process, where it may force the
    /// process-wide CPU flags; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_IMGUTILS_ISOLATED";
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

    /// Without SSE4.1, C copies even fast-path planes on its plain path:
    /// `bytewidth` bytes a row, as `av_image_copy_plane` does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uncacheable_copy_without_sse4_is_the_plain_copy() {
        if !isolated(
            "libavutil::imgutils::unit_tests::uncacheable_copy_without_sse4_is_the_plain_copy",
        ) {
            return;
        }
        // SAFETY: no flags at all, in a process running only this test.
        unsafe { ffi::av_force_cpu_flags(0) };
        let src = Aligned::<1024>(core::array::from_fn(|i| (i * 5 % 253) as u8));
        for (dl, sl, w, h) in [
            (64isize, 64isize, 10usize, 4usize),
            (128, 192, 64, 3),
            (64, 128, 33, 5),
        ] {
            let (mut rust, mut c) = (Aligned([0x11u8; 1024]), Aligned([0x11u8; 1024]));
            copy_plane_from_uncacheable(&mut rust.0, dl, &src.0, sl, w, h).unwrap();
            c_copy_plane(&mut c.0, 0, dl as i32, &src.0, 0, sl as i32, w, h);
            assert_eq!(rust.0, c.0, "{dl} {sl} {w} {h}");
        }
        // SAFETY: as above; detection again.
        unsafe { ffi::av_force_cpu_flags(-1) };
    }

    #[test]
    fn c_align_takes_powers_of_two_up_to_2_30() {
        for a in [1, 2, 4, 64, MAX_ALIGN] {
            assert_eq!(c_align(a), Ok(a as c_int));
        }
        for a in [0, 3, 24, MAX_ALIGN + 1, MAX_ALIGN << 1, usize::MAX] {
            assert_eq!(c_align(a), Err(AVERROR_EINVAL), "{a}");
        }
    }

    #[test]
    fn ceil_rshift_rounds_up() {
        assert_eq!(ceil_rshift(0, 1), 0);
        assert_eq!(ceil_rshift(1, 1), 1);
        assert_eq!(ceil_rshift(5, 1), 3);
        assert_eq!(ceil_rshift(5, 2), 2);
        assert_eq!(ceil_rshift(8, 2), 2);
        assert_eq!(ceil_rshift(7, 0), 7);
        assert_eq!(ceil_rshift(usize::MAX, 1), usize::MAX / 2 + 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn geometry_reads_the_descriptor() {
        let g = Geometry::of(PixelFormat::YUV420P).unwrap();
        assert_eq!(g.planes(), 3);
        assert_eq!(
            (g.rows(0, 5), g.rows(1, 5), g.rows(2, 5), g.rows(3, 5)),
            (5, 3, 3, 5)
        );
        assert!(!g.has(ffi::AV_PIX_FMT_FLAG_PAL));
        let pal = Geometry::of(PixelFormat::PAL8).unwrap();
        assert!(pal.has(ffi::AV_PIX_FMT_FLAG_PAL));
        assert_eq!(pal.planes(), 1);
        let hw = Geometry::of(PixelFormat::VAAPI).unwrap();
        assert!(hw.has(ffi::AV_PIX_FMT_FLAG_HWACCEL));
        assert_eq!(hw.planes(), 0);
        assert!(Geometry::of(PixelFormat::NONE).is_none());
        let nb = PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_NB);
        assert!(Geometry::of(nb).is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn linesize_overflow_is_found_exactly_where_c_overflows() {
        let mono = Geometry::of(PixelFormat::MONOBLACK).unwrap();
        assert!(!mono.linesize_overflows(c_int::MAX - 7, 0));
        assert!(mono.linesize_overflows(c_int::MAX - 6, 0));
        assert!(mono.linesize_overflows(c_int::MAX, 0));
        assert!(!mono.linesize_overflows(c_int::MAX, 1));
        assert!(!mono.linesize_overflows(-1, 0));
        let bgr4 = Geometry::of(PixelFormat::BGR4).unwrap();
        assert!(!bgr4.linesize_overflows(536_870_910, 0));
        assert!(bgr4.linesize_overflows(536_870_911, 0));
        // C rejects the width before rounding.
        assert!(!bgr4.linesize_overflows(536_870_912, 0));
        let xv30 = Geometry::of(PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_XV30BE));
        assert!(
            (0..c_int::MAX)
                .step_by(1 << 16)
                .chain([c_int::MAX])
                .all(|w| { !xv30.unwrap().linesize_overflows(w, 0) })
        );
        let yuv = Geometry::of(PixelFormat::YUV420P).unwrap();
        assert!(!yuv.linesize_overflows(c_int::MAX, 0));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn overflowing_or_out_of_range_linesizes_are_refused_before_c() {
        assert_eq!(linesize(PixelFormat::YUV420P, 16, 4), Err(AVERROR_EINVAL));
        assert_eq!(
            linesize(PixelFormat::YUV420P, 16, usize::MAX),
            Err(AVERROR_EINVAL)
        );
        assert_eq!(
            linesize(PixelFormat::MONOBLACK, c_int::MAX, 0),
            Err(AVERROR_EINVAL)
        );
        assert_eq!(
            linesize(PixelFormat::MONOBLACK, c_int::MAX - 7, 0),
            Ok(c_int::MAX / 8)
        );
        assert_eq!(
            fill_linesizes(PixelFormat::BGR4, 536_870_911),
            Err(AVERROR_EINVAL)
        );
        assert_eq!(
            fill_linesizes(PixelFormat::BGR4, 536_870_909),
            Ok([268_435_455, 0, 0, 0])
        );
        assert_eq!(
            check_fill_color(PixelFormat::MONOWHITE, [0; 4], c_int::MAX),
            Err(AVERROR_EINVAL)
        );
        assert_eq!(
            check_fill_black(PixelFormat::MONOWHITE, ColorRange::JPEG, c_int::MAX),
            Err(AVERROR_EINVAL)
        );
        for h in [0, -1, c_int::MIN] {
            assert_eq!(
                fill_plane_sizes(PixelFormat::YUV420P, h, [1; 4]),
                Err(AVERROR_EINVAL)
            );
            assert_eq!(
                buffer_size_for_linesizes(PixelFormat::YUV420P, h, [1; 4]),
                Err(AVERROR_EINVAL)
            );
        }
    }

    #[test]
    fn plane_ranges_end_where_the_next_plane_starts() {
        let r = plane_ranges(0x1000, [0x1000, 0x1064, 0x107D, 0], 150);
        assert_eq!(r, [0..100, 100..125, 125..150, 0..0]);
        let r = plane_ranges(0x1000, [0x1000, 0, 0, 0], 7);
        assert_eq!(r, [0..7, 0..0, 0..0, 0..0]);
        let r = plane_ranges(0x1000, [0; 4], 0);
        assert_eq!(r, [0..0, 0..0, 0..0, 0..0]);
    }

    #[test]
    fn split_planes_hands_out_disjoint_ranges() {
        let mut buf: Vec<u8> = (0..20).collect();
        let planes = split_planes(&mut buf, &[0..8, 8..12, 12..20, 0..0]);
        assert_eq!(planes.each_ref().map(|p| p.len()), [8, 4, 8, 0]);
        assert_eq!(planes[1], &[8, 9, 10, 11]);
        // A gap before a plane is skipped.
        let planes = split_planes(&mut buf, &[2..4, 0..0, 10..11, 0..0]);
        assert_eq!((&*planes[0], &*planes[2]), (&[2, 3][..], &[10][..]));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn image_alloc_zero_fills_and_matches_its_layout() {
        let mut img = Image::alloc(33, 17, PixelFormat::YUV420P, 16).unwrap();
        assert_eq!(img.linesizes(), [48, 32, 32, 0]);
        assert_eq!(
            (img.format(), img.width(), img.height()),
            (PixelFormat::YUV420P, 33, 17)
        );
        let lens = img.planes().data.map(<[u8]>::len);
        assert_eq!(lens, [48 * 17, 32 * 9, 32 * 9, 0]);
        assert!(img.planes().data.iter().all(|p| p.iter().all(|&b| b == 0)));
        assert_eq!(img.buf.len(), lens.iter().sum::<usize>() + 16);
        img.planes_mut().data[2].fill(7);
        assert!(img.planes().data[1].iter().all(|&b| b == 0));
        let pal = Image::alloc(5, 3, PixelFormat::PAL8, 4).unwrap();
        assert_eq!(
            pal.planes().data.map(<[u8]>::len),
            [8 * 3, PALETTE_SIZE, 0, 0]
        );
        assert_eq!(
            Image::alloc(5, 3, PixelFormat::PAL8, 2).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            Image::alloc(5, 3, PixelFormat::YUV420P, 3).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            Image::alloc(0, 3, PixelFormat::YUV420P, 1).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            Image::alloc(5, 3, PixelFormat::VAAPI, 1).unwrap_err(),
            AVERROR_EINVAL
        );
    }

    #[test]
    fn planes_reborrow_and_convert() {
        let mut buf = [1u8; 8];
        let mut planes = PlanesMut {
            data: [&mut buf[..], &mut [], &mut [], &mut []],
            linesizes: [8, 0, 0, 0],
        };
        planes.reborrow().data[0][0] = 5;
        assert_eq!(planes.as_planes().data[0][0], 5);
        let shared: Planes<'_> = planes.into();
        assert_eq!(shared.linesizes, [8, 0, 0, 0]);
        assert_eq!(shared.data[0], &[5, 1, 1, 1, 1, 1, 1, 1]);
        let empty = Planes::default();
        assert!(empty.data.iter().all(|d| d.is_empty()));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copy_plans_follow_the_format() {
        let p = CopyPlan::new(PixelFormat::YUV420P, 5, 3).unwrap();
        assert_eq!(p.extents, [Some((5, 3)), Some((3, 2)), Some((3, 2)), None]);
        assert!(!p.palette);
        let p = CopyPlan::new(PixelFormat::PAL8, 5, 3).unwrap();
        assert_eq!(p.extents, [Some((5, 3)), None, None, None]);
        assert!(p.palette);
        let p = CopyPlan::new(PixelFormat::NV12, 5, 3).unwrap();
        assert_eq!(p.extents, [Some((5, 3)), Some((6, 2)), None, None]);
        use ImageCopyError::*;
        assert_eq!(
            CopyPlan::new(PixelFormat::NONE, 5, 3),
            Err(UnsupportedFormat)
        );
        assert_eq!(
            CopyPlan::new(PixelFormat::VAAPI, 5, 3),
            Err(UnsupportedFormat)
        );
        assert_eq!(CopyPlan::new(PixelFormat::YUV420P, -1, 3), Err(InvalidSize));
        assert_eq!(CopyPlan::new(PixelFormat::YUV420P, 5, -1), Err(InvalidSize));
        assert_eq!(
            CopyPlan::new(PixelFormat::MONOBLACK, c_int::MAX, 1),
            Err(InvalidSize)
        );
    }

    #[test]
    fn image_copy_errors_display_and_chain() {
        use std::error::Error;
        let plane = ImageCopyError::Plane {
            index: 2,
            error: PlaneCopyError::SourceTooShort,
        };
        assert_eq!(
            plane.to_string(),
            "plane 2: source plane ends past its buffer"
        );
        assert!(plane.source().is_some());
        for e in [
            ImageCopyError::UnsupportedFormat,
            ImageCopyError::InvalidSize,
        ] {
            assert!(!e.to_string().is_empty());
            assert!(e.source().is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn short_palettes_and_wide_linesizes_are_refused() {
        let src_buf = [9u8; 2048];
        let mut dst_buf = [0u8; 2048];
        let (pal_src, rest) = src_buf.split_at(PALETTE_SIZE);
        let (pal_dst, rest_dst) = dst_buf.split_at_mut(PALETTE_SIZE);
        let src = Planes {
            data: [rest, &pal_src[..PALETTE_SIZE - 1], &[], &[]],
            linesizes: [4, 0, 0, 0],
        };
        let mut dst = PlanesMut {
            data: [rest_dst, pal_dst, &mut [], &mut []],
            linesizes: [4, 0, 0, 0],
        };
        let short = ImageCopyError::Plane {
            index: 1,
            error: PlaneCopyError::SourceTooShort,
        };
        assert_eq!(copy(&mut dst, src, PixelFormat::PAL8, 4, 4), Err(short));
        assert_eq!(
            copy_from_uncacheable(&mut dst, src, PixelFormat::PAL8, 4, 4),
            Err(short)
        );
        let wide = Planes {
            linesizes: [1 << 40, 0, 0, 0],
            data: [&src_buf[..4], &src_buf[..PALETTE_SIZE], &[], &[]],
        };
        let too_large = ImageCopyError::Plane {
            index: 0,
            error: PlaneCopyError::TooLarge,
        };
        assert_eq!(
            copy(&mut dst, wide, PixelFormat::PAL8, 4, 1),
            Err(too_large)
        );
        assert!(dst.data[1].iter().all(|&b| b == 0));
    }

    /// Planes of a 70 x 9 `YUV420P` image in `buf`, each starting one byte
    /// past a 64-byte boundary.
    fn misaligned_planes(buf: &mut [u8]) -> PlanesMut<'_> {
        let ranges = [
            1..1 + 128 * 9,
            1217..1217 + 64 * 5,
            1601..1601 + 64 * 5,
            0..0,
        ];
        PlanesMut {
            data: split_planes(buf, &ranges),
            linesizes: [128, 64, 64, 0],
        }
    }

    /// Regression for a C defect: `av_image_copy_uc_from` faults on
    /// misaligned planes on SSE4.1 CPUs; such images are copied plane by
    /// plane as C's plain path does, matching `av_image_copy`.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn misaligned_uncacheable_images_copy_as_the_plain_copy() {
        let mut src = Image::alloc(70, 9, PixelFormat::YUV420P, 64).unwrap();
        for (i, p) in src.planes_mut().data.iter_mut().enumerate() {
            for (j, b) in p.iter_mut().enumerate() {
                *b = (i * 31 + j * 7) as u8;
            }
        }
        let mut rust = Aligned([0x55u8; 4096]);
        let mut plain = Aligned([0x55u8; 4096]);
        let fmt = PixelFormat::YUV420P;
        copy_from_uncacheable(
            &mut misaligned_planes(&mut rust.0),
            src.planes(),
            fmt,
            70,
            9,
        )
        .unwrap();
        copy(
            &mut misaligned_planes(&mut plain.0),
            src.planes(),
            fmt,
            70,
            9,
        )
        .unwrap();
        assert_eq!(rust.0, plain.0);
        assert_ne!(rust.0, [0x55u8; 4096]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uncacheable_linesizes_c_cannot_negate_are_refused() {
        let src = [7u8; 8];
        let mut dst = [0u8; 8];
        let src = Planes {
            data: [&src, &[], &[], &[]],
            linesizes: [8, 0, 0, 0],
        };
        let mut planes = PlanesMut {
            data: [&mut dst, &mut [], &mut [], &mut []],
            linesizes: [isize::MIN, 0, 0, 0],
        };
        let too_large = ImageCopyError::Plane {
            index: 0,
            error: PlaneCopyError::TooLarge,
        };
        let gray = PixelFormat::GRAY8;
        assert_eq!(
            copy_from_uncacheable(&mut planes, src, gray, 4, 1),
            Err(too_large)
        );
        assert_eq!(planes.data[0], &[0; 8]);
    }

    /// Planes of a 70 x 3 `PAL8` image in `buf`, plane 0 at `offset` and
    /// the palette at 1024.
    fn pal8_planes(buf: &mut [u8], offset: usize) -> PlanesMut<'_> {
        let ranges = [offset..offset + 128 * 3, 1024..2048, 0..0, 0..0];
        PlanesMut {
            data: split_planes(buf, &ranges),
            linesizes: [128, 0, 0, 0],
        }
    }

    /// A palettized image C's SSE4 path cannot take (a misaligned plane, or
    /// no pixels) still gets its palette, as `av_image_copy` copies it.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uncacheable_palettes_copy_on_the_plain_path() {
        let pal = PixelFormat::PAL8;
        let mut src = Image::alloc(70, 3, pal, 64).unwrap();
        for (i, p) in src.planes_mut().data.iter_mut().enumerate() {
            for (j, b) in p.iter_mut().enumerate() {
                *b = (i * 17 + j * 3) as u8;
            }
        }
        for (w, h, offset) in [(70, 3, 1), (0, 3, 0), (70, 0, 0)] {
            let mut rust = Aligned([0u8; 4096]);
            let mut plain = Aligned([0u8; 4096]);
            let mut fast = pal8_planes(&mut rust.0, offset);
            copy_from_uncacheable(&mut fast, src.planes(), pal, w, h).unwrap();
            copy(
                &mut pal8_planes(&mut plain.0, offset),
                src.planes(),
                pal,
                w,
                h,
            )
            .unwrap();
            assert_eq!(rust.0, plain.0, "{w}x{h} {offset}");
            assert_eq!(rust.0[1024..2048], *src.planes().data[1]);
        }
    }

    #[test]
    fn max_pixel_steps_refuses_planes_c_cannot_index() {
        use super::super::pixdesc::{ComponentDescriptor, PixelFormatDescriptor};
        for plane in [4, -1, i32::MAX, i32::MIN] {
            let mut d = PixelFormatDescriptor::new();
            // An unused slot (past the count, depth 0) still counts: C reads
            // all four.
            d.as_mut()
                .components_mut()
                .get_mut(3)
                .unwrap()
                .set(ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
                    plane,
                    step: 0,
                    offset: 0,
                    shift: 0,
                    depth: 0,
                }));
            assert_eq!(
                max_pixel_steps(d.as_ref()),
                Err(InvalidDescriptor),
                "{plane}"
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn geometry_reads_steps_through_the_safe_descriptor() {
        // nv12: Y in plane 0, interleaved U and V in plane 1.
        let g = Geometry::of(PixelFormat::NV12).unwrap();
        assert_eq!(g.max_steps.steps, [1, 2, 0, 0]);
        assert_eq!(g.max_steps.components, [0, 1, 0, 0]);
        assert_eq!(g.comps, [(0, 1), (1, 2), (1, 2), (0, 0)]);
        assert_eq!(g.planes(), 2);
    }
}
