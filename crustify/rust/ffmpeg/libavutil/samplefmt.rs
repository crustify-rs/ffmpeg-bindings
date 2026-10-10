//! Safe wrappers over `libavutil/samplefmt.h`.
//!
//! Audio samples live in planes: one plane holding every channel interleaved
//! for a packed [`SampleFormat`], or one plane per channel for a planar one,
//! all in one buffer and `linesize` bytes apart. [`samples_buffer_size`]
//! computes that layout (`av_samples_get_buffer_size`); [`SampleBuffer`] owns
//! a buffer C allocates in it (`av_samples_alloc`), and
//! [`SampleBufferWithPointers`] also the C array of plane pointers
//! (`av_samples_alloc_array_and_samples`). [`SamplesRef`] and [`SamplesMut`]
//! view an owner's planes, or lay them out over a caller's bytes
//! (`av_samples_fill_arrays`); the exclusive view sets samples to silence
//! (`av_samples_set_silence`) and copies them (`av_samples_copy`).

use core::ffi::c_int;
use core::fmt;
use core::mem::MaybeUninit;
use core::ptr;

use ffibox::CPlainElem;

use super::ffi;
use super::mem::HeapVec;

/// Wraps: AVSampleFormat
/// Handwritten
///
/// The format of an audio sample: its native-endian C type (`u8`, `i16`,
/// `i32`, `i64`, `f32` or `f64`) and whether the channels are interleaved in
/// one plane (packed) or each held in a plane of its own (planar, the `P`
/// constants).
///
/// A newtype over the C enum rather than a Rust `enum`: C stores the value in
/// public struct fields its users write directly (`AVCodecContext.sample_fmt`,
/// `AVFrame.format`) and in `NONE`-terminated arrays, so it may hold any
/// `int`, and a Rust view of that memory must accept every one. Match on the
/// associated constants, with a `_` arm for the values C does not name:
///
/// ```
/// use ffmpeg::libavutil::samplefmt::SampleFormat;
/// fn is_float(f: SampleFormat) -> bool {
///     matches!(
///         f,
///         SampleFormat::FLT | SampleFormat::FLTP | SampleFormat::DBL | SampleFormat::DBLP
///     )
/// }
/// assert!(is_float(SampleFormat::FLTP));
/// assert!(!is_float(SampleFormat::NONE));
/// ```
///
/// The default is [`NONE`](Self::NONE), the value C gives an unset format:
/// `av_frame_alloc` writes it (as `-1`) into `AVFrame.format`, and it is the
/// default of `AVCodecContext.sample_fmt`.
///
/// No ffibox primitive covers a C enum: it owns nothing, has no handles, and
/// is `Send + Sync` as the integer it is.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleFormat(ffi::AVSampleFormat);

impl SampleFormat {
    /// No sample format: unknown or unset.
    pub const NONE: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_NONE);
    /// Unsigned 8 bits.
    pub const U8: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_U8);
    /// Signed 16 bits.
    pub const S16: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S16);
    /// Signed 32 bits.
    pub const S32: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S32);
    /// `float`.
    pub const FLT: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT);
    /// `double`.
    pub const DBL: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_DBL);
    /// Unsigned 8 bits, planar.
    pub const U8P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_U8P);
    /// Signed 16 bits, planar.
    pub const S16P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S16P);
    /// Signed 32 bits, planar.
    pub const S32P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S32P);
    /// `float`, planar.
    pub const FLTP: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP);
    /// `double`, planar.
    pub const DBLP: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_DBLP);
    /// Signed 64 bits.
    pub const S64: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S64);
    /// Signed 64 bits, planar.
    pub const S64P: Self = Self(ffi::AVSampleFormat::AV_SAMPLE_FMT_S64P);

    /// The number of sample formats from [`U8`](Self::U8) to
    /// [`S64P`](Self::S64P) these bindings were generated with
    /// (`AV_SAMPLE_FMT_NB`): the length of an array indexed by
    /// [`index`](Self::index).
    ///
    /// A newer libavutil of the same major version may append formats, which
    /// [`index`](Self::index) reports as `None`; this is why C warns against
    /// `AV_SAMPLE_FMT_NB` when linking dynamically.
    pub const COUNT: usize = ffi::AVSampleFormat::AV_SAMPLE_FMT_NB.0 as usize;

    /// The sample format C encodes as `raw`, whatever its value.
    #[inline]
    pub const fn from_raw(raw: ffi::AVSampleFormat) -> Self {
        Self(raw)
    }

    /// The C encoding of this sample format.
    #[inline]
    pub const fn into_raw(self) -> ffi::AVSampleFormat {
        self.0
    }

    /// This sample format's position in `0..COUNT`, or `None` for
    /// [`NONE`](Self::NONE) and any value C does not name.
    #[inline]
    pub const fn index(self) -> Option<usize> {
        let raw = self.0.0;
        if raw >= 0 && (raw as usize) < Self::COUNT {
            Some(raw as usize)
        } else {
            None
        }
    }

    /// The name of the constant this value equals, if any.
    const fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::NONE => "NONE",
            Self::U8 => "U8",
            Self::S16 => "S16",
            Self::S32 => "S32",
            Self::FLT => "FLT",
            Self::DBL => "DBL",
            Self::U8P => "U8P",
            Self::S16P => "S16P",
            Self::S32P => "S32P",
            Self::FLTP => "FLTP",
            Self::DBLP => "DBLP",
            Self::S64 => "S64",
            Self::S64P => "S64P",
            _ => return None,
        })
    }
}

impl Default for SampleFormat {
    #[inline]
    fn default() -> Self {
        Self::NONE
    }
}

impl fmt::Debug for SampleFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => f.debug_tuple("SampleFormat").field(&self.0.0).finish(),
        }
    }
}

impl From<ffi::AVSampleFormat> for SampleFormat {
    #[inline]
    fn from(raw: ffi::AVSampleFormat) -> Self {
        Self::from_raw(raw)
    }
}

impl From<SampleFormat> for ffi::AVSampleFormat {
    #[inline]
    fn from(f: SampleFormat) -> Self {
        f.into_raw()
    }
}

// SAFETY: `SampleFormat` is `#[repr(transparent)]` over `ffi::AVSampleFormat`,
// itself `#[repr(transparent)]` over a `c_int`, so every bit pattern is a
// valid value, and it is a value with no C-side identity: a copy read out of C
// memory is the whole of it.
unsafe impl CPlainElem for SampleFormat {}

impl SampleFormat {
    /// Wraps: av_sample_fmt_is_planar
    ///
    /// Whether each channel has a plane of its own (the `P` constants);
    /// `false` for a packed format, and for [`NONE`](Self::NONE) and every
    /// value C does not name.
    pub fn is_planar(self) -> bool {
        // SAFETY: C bounds-checks the value before indexing its format table;
        // any `int` is accepted.
        unsafe { ffi::av_sample_fmt_is_planar(self.into_raw()) != 0 }
    }
}

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: i32 = -(ffi::EINVAL as i32);
/// `AVERROR(ENOMEM)`.
const AVERROR_ENOMEM: i32 = -(ffi::ENOMEM as i32);

/// A count as the C `int` the sample routines take; `AVERROR(EINVAL)` for
/// one beyond `INT_MAX`, which C, seeing it negative, would refuse too.
fn c_count<T: TryInto<c_int>>(n: T) -> Result<c_int, i32> {
    n.try_into().map_err(|_| AVERROR_EINVAL)
}

/// The `align` C is given for `nb_samples` samples: `align` itself, except
/// that the default (0) for `INT_MAX - 31` samples is passed as the
/// equivalent 1. C rounds the default's sample count up to a multiple of 32
/// as `nb_samples + 32 - 1`, which overflows (signed, undefined) for that
/// count, the one its own range check lets through; the count is a multiple
/// of 32 already, so rounding leaves it alone and 1 asks for the same layout.
fn c_align(nb_samples: c_int, align: u32) -> Result<c_int, i32> {
    if align == 0 && nb_samples == c_int::MAX - 31 {
        return Ok(1);
    }
    c_count(align)
}

/// The size of a sample buffer, as [`samples_buffer_size`] computes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferSize {
    /// The bytes the whole buffer needs.
    pub size: usize,
    /// The bytes from one plane to the next: the aligned size of one
    /// channel's samples for a planar format, of all of them for a packed
    /// one.
    pub linesize: usize,
}

/// Wraps: av_samples_get_buffer_size
///
/// The size of a buffer holding `samples` samples of each of `channels`
/// channels in `format`, every plane padded to a multiple of `align` bytes.
/// An `align` of 0 picks the default: `samples` rounded up to a multiple of
/// 32 and no padding beyond that; 1 means no alignment.
///
/// Fails with `AVERROR(EINVAL)` for [`SampleFormat::NONE`] or any format C
/// does not name, for no channels or no samples, and when the buffer would
/// not fit in an `int`.
pub fn samples_buffer_size(
    channels: usize,
    samples: usize,
    format: SampleFormat,
    align: u32,
) -> Result<BufferSize, i32> {
    let (nb_channels, nb_samples) = (c_count(channels)?, c_count(samples)?);
    let align = c_align(nb_samples, align)?;
    let mut linesize = 0;
    // SAFETY: C writes only `linesize`, a live local, and validates every
    // integer before computing with it.
    let ret = unsafe {
        ffi::av_samples_get_buffer_size(
            &mut linesize,
            nb_channels,
            nb_samples,
            format.into_raw(),
            align,
        )
    };
    let size = usize::try_from(ret).map_err(|_| ret)?;
    let linesize = usize::try_from(linesize).map_err(|_| AVERROR_EINVAL)?;
    Ok(BufferSize { size, linesize })
}

/// Where the planes of a sample buffer lie in its bytes.
///
/// Built only by [`Layout::new`], which proves for a buffer of `len` bytes
/// that every plane's samples lie inside it, and that every byte count the C
/// sample routines compute in `int` for them fits one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    format: SampleFormat,
    channels: usize,
    samples: usize,
    linesize: usize,
    /// `channels` for a planar format, 1 for a packed one.
    planes: usize,
    /// The bytes of one sample in a plane: the format's sample size, times
    /// `channels` for a packed format.
    block_align: usize,
}

impl Layout {
    /// The layout of `planes` planes of `samples` samples, `linesize` bytes
    /// apart, in a buffer of `len` bytes, for a format of `bytes` bytes per
    /// sample; `None` unless every plane's samples lie inside the buffer and
    /// all of a plane's bytes count in an `int`.
    fn new(
        format: SampleFormat,
        bytes: usize,
        planar: bool,
        channels: usize,
        samples: usize,
        linesize: usize,
        len: usize,
    ) -> Option<Self> {
        if bytes == 0 || channels == 0 || samples == 0 {
            return None;
        }
        let planes = if planar { channels } else { 1 };
        let block_align = bytes.checked_mul(channels / planes)?;
        let plane_len = samples.checked_mul(block_align)?;
        c_int::try_from(plane_len).ok()?;
        c_int::try_from(channels).ok()?;
        let end = (planes - 1).checked_mul(linesize)?.checked_add(plane_len)?;
        (linesize >= plane_len && end <= len).then_some(Self {
            format,
            channels,
            samples,
            linesize,
            planes,
            block_align,
        })
    }

    /// The layout C reports for `format` with `linesize`, in a buffer of
    /// `len` bytes.
    fn from_c(
        format: SampleFormat,
        channels: usize,
        samples: usize,
        linesize: c_int,
        len: usize,
    ) -> Option<Self> {
        // SAFETY: C bounds-checks the value before indexing its format table.
        let bytes = unsafe { ffi::av_get_bytes_per_sample(format.into_raw()) };
        let (bytes, linesize) = (
            usize::try_from(bytes).ok()?,
            usize::try_from(linesize).ok()?,
        );
        Self::new(
            format,
            bytes,
            format.is_planar(),
            channels,
            samples,
            linesize,
            len,
        )
    }

    /// The bytes of one plane's samples, its alignment padding excluded.
    fn plane_len(&self) -> usize {
        self.samples * self.block_align
    }

    /// Whether samples `offset..offset + count` exist.
    fn check_range(&self, offset: usize, count: usize) -> Result<(), SamplesError> {
        match offset.checked_add(count) {
            Some(end) if end <= self.samples => Ok(()),
            _ => Err(SamplesError::OutOfRange),
        }
    }

    /// Calls `f` with C's array of plane pointers into the buffer at `base`:
    /// `planes` pointers, `linesize` bytes apart.
    fn with_plane_ptrs<R>(&self, base: *mut u8, f: impl FnOnce(*const *mut u8) -> R) -> R {
        const INLINE: usize = 8;
        let at = |i: usize| base.wrapping_add(i * self.linesize);
        if self.planes <= INLINE {
            let mut ptrs = [ptr::null_mut(); INLINE];
            for (i, p) in ptrs[..self.planes].iter_mut().enumerate() {
                *p = at(i);
            }
            f(ptrs.as_ptr())
        } else {
            let ptrs: Vec<*mut u8> = (0..self.planes).map(at).collect();
            f(ptrs.as_ptr())
        }
    }
}

/// `planes` empty plane pointer slots, for C to fill; `AVERROR(ENOMEM)` when
/// they cannot be allocated.
fn plane_slots(planes: usize) -> Result<Vec<*mut u8>, i32> {
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(planes)
        .map_err(|_| AVERROR_ENOMEM)?;
    slots.resize(planes, ptr::null_mut());
    Ok(slots)
}

/// Lays out the planes of `data` for C, as `av_samples_fill_arrays` does;
/// `AVERROR(EINVAL)` when the buffer is shorter than they need.
fn fill_arrays(
    data: &[MaybeUninit<u8>],
    channels: usize,
    samples: usize,
    format: SampleFormat,
    align: u32,
) -> Result<Layout, i32> {
    let (nb_channels, nb_samples) = (c_count(channels)?, c_count(samples)?);
    let align_arg = c_align(nb_samples, align)?;
    // Checked first: C offsets `buf` by the plane positions whatever its
    // length.
    if samples_buffer_size(channels, samples, format, align)?.size > data.len() {
        return Err(AVERROR_EINVAL);
    }
    let mut slots = plane_slots(if format.is_planar() { channels } else { 1 })?;
    let mut linesize = 0;
    let buf = data.as_ptr().cast::<u8>();
    // SAFETY: `slots` holds the `planar ? channels : 1` pointers C writes;
    // `buf` holds the bytes the layout needs, so the plane pointers C
    // computes from it stay inside it, and C never reads or writes through
    // it.
    let ret = unsafe {
        ffi::av_samples_fill_arrays(
            slots.as_mut_ptr(),
            &mut linesize,
            buf,
            nb_channels,
            nb_samples,
            format.into_raw(),
            align_arg,
        )
    };
    if ret < 0 {
        return Err(ret);
    }
    let layout =
        Layout::from_c(format, channels, samples, linesize, data.len()).ok_or(AVERROR_EINVAL)?;
    debug_assert!(
        slots
            .iter()
            .enumerate()
            .all(|(i, p)| p.addr() == buf.addr() + i * layout.linesize)
    );
    Ok(layout)
}

/// Why [`SamplesMut`] refused to set or copy samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SamplesError {
    /// The samples asked for run past the end of a buffer.
    OutOfRange,
    /// The source and destination of a copy have different sample formats.
    FormatMismatch,
    /// The source and destination of a copy have different channel counts.
    ChannelsMismatch,
}

impl fmt::Display for SamplesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfRange => "samples out of range",
            Self::FormatMismatch => "sample formats differ",
            Self::ChannelsMismatch => "channel counts differ",
        })
    }
}

impl std::error::Error for SamplesError {}

/// Handwritten
///
/// A shared view of the planes of a sample buffer, from
/// [`SampleBuffer::as_ref`] or laid out over a caller's bytes by
/// [`from_buffer`](Self::from_buffer). It reads the samples of each plane,
/// without its alignment padding.
#[derive(Clone, Copy)]
pub struct SamplesRef<'a> {
    /// The whole buffer. Every plane `layout` places in it is initialised;
    /// the padding between planes may not be.
    data: &'a [MaybeUninit<u8>],
    layout: Layout,
}

/// Handwritten
///
/// An exclusive view of the planes of a sample buffer, from
/// [`SampleBuffer::as_mut`] or laid out over a caller's bytes by
/// [`from_buffer`](Self::from_buffer). It writes the samples of each plane,
/// sets them to silence, and copies them, and reads them through
/// [`as_ref`](Self::as_ref).
pub struct SamplesMut<'a> {
    /// As in [`SamplesRef`]; only initialised bytes are ever written to it.
    data: &'a mut [MaybeUninit<u8>],
    layout: Layout,
}

impl fmt::Debug for SamplesRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SamplesRef")
            .field("format", &self.layout.format)
            .field("channels", &self.layout.channels)
            .field("samples", &self.layout.samples)
            .field("linesize", &self.layout.linesize)
            .finish()
    }
}

impl fmt::Debug for SamplesMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<'a> SamplesRef<'a> {
    /// Wraps: av_samples_fill_arrays
    ///
    /// Lays the planes of `samples` samples of `channels` channels in
    /// `format` over `buf`, as [`samples_buffer_size`] computes them with
    /// `align`, from the start of `buf`.
    ///
    /// Fails with `AVERROR(EINVAL)` where [`samples_buffer_size`] does, and
    /// when `buf` is shorter than the size it computes.
    pub fn from_buffer(
        buf: &'a [u8],
        channels: usize,
        samples: usize,
        format: SampleFormat,
        align: u32,
    ) -> Result<Self, i32> {
        // SAFETY: `MaybeUninit<u8>` has `u8`'s layout, every byte of `buf`
        // is initialised, and a shared view never writes.
        let data = unsafe { core::slice::from_raw_parts(buf.as_ptr().cast(), buf.len()) };
        let layout = fill_arrays(data, channels, samples, format, align)?;
        Ok(Self { data, layout })
    }

    /// The format of the samples.
    pub fn format(&self) -> SampleFormat {
        self.layout.format
    }

    /// The number of channels.
    pub fn channels(&self) -> usize {
        self.layout.channels
    }

    /// The number of samples in each channel.
    pub fn samples(&self) -> usize {
        self.layout.samples
    }

    /// The bytes from the start of one plane to the next.
    pub fn linesize(&self) -> usize {
        self.layout.linesize
    }

    /// The number of planes: [`channels`](Self::channels) for a planar
    /// format, 1 for a packed one.
    pub fn plane_count(&self) -> usize {
        self.layout.planes
    }

    /// The samples of plane `index`, its padding excluded: one channel's for
    /// a planar format, every channel's interleaved for a packed one; `None`
    /// past the last plane.
    pub fn plane(&self, index: usize) -> Option<&'a [u8]> {
        (index < self.layout.planes).then(|| {
            let data: &'a [MaybeUninit<u8>] = self.data;
            let start = index * self.layout.linesize;
            let plane = &data[start..start + self.layout.plane_len()];
            // SAFETY: `MaybeUninit<u8>` has `u8`'s layout, and the view's
            // planes are initialised.
            unsafe { &*(plane as *const [MaybeUninit<u8>] as *const [u8]) }
        })
    }

    /// The samples of every plane, in order, as [`plane`](Self::plane) reads
    /// them.
    pub fn planes(&self) -> impl ExactSizeIterator<Item = &'a [u8]> + use<'a> {
        let this = *self;
        (0..this.layout.planes).map(move |i| this.plane(i).unwrap_or_default())
    }
}

impl<'a> SamplesMut<'a> {
    /// Wraps: av_samples_fill_arrays
    ///
    /// As [`SamplesRef::from_buffer`], for writing the samples in `buf`.
    pub fn from_buffer(
        buf: &'a mut [u8],
        channels: usize,
        samples: usize,
        format: SampleFormat,
        align: u32,
    ) -> Result<Self, i32> {
        // SAFETY: `MaybeUninit<u8>` has `u8`'s layout, `buf` is borrowed
        // exclusively for `'a`, and the view only ever writes initialised
        // bytes, so `buf` stays initialised for its owner.
        let data = unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), buf.len()) };
        let layout = fill_arrays(data, channels, samples, format, align)?;
        Ok(Self { data, layout })
    }

    /// A shared view of the same planes.
    pub fn as_ref(&self) -> SamplesRef<'_> {
        SamplesRef {
            data: self.data,
            layout: self.layout,
        }
    }

    /// The same view, reborrowed.
    pub fn as_mut(&mut self) -> SamplesMut<'_> {
        SamplesMut {
            data: self.data,
            layout: self.layout,
        }
    }

    /// The samples of plane `index`, for writing, as
    /// [`SamplesRef::plane`] reads them.
    pub fn plane_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        (index < self.layout.planes).then(|| {
            let start = index * self.layout.linesize;
            let plane = &mut self.data[start..start + self.layout.plane_len()];
            // SAFETY: as in `SamplesRef::plane`; a `&mut [u8]` can only
            // write initialised bytes back.
            unsafe { &mut *(plane as *mut [MaybeUninit<u8>] as *mut [u8]) }
        })
    }

    /// The samples of every plane, for writing, in order.
    pub fn planes_mut(&mut self) -> impl ExactSizeIterator<Item = &mut [u8]> + '_ {
        let (planes, len) = (self.layout.planes, self.layout.plane_len());
        // `linesize >= len >= 1` by `Layout::new`: plane `i` starts chunk
        // `i`, and the last plane's chunk still holds `len` bytes.
        self.data
            .chunks_mut(self.layout.linesize)
            .take(planes)
            .map(move |chunk| {
                let plane = &mut chunk[..len];
                // SAFETY: as in `plane_mut`.
                unsafe { &mut *(plane as *mut [MaybeUninit<u8>] as *mut [u8]) }
            })
    }

    /// Wraps: av_samples_set_silence
    ///
    /// Sets samples `offset..offset + count` of every channel to silence:
    /// `0x80` bytes for [`SampleFormat::U8`] and [`SampleFormat::U8P`], zero
    /// bytes otherwise.
    ///
    /// Fails, writing nothing, when the samples run past the end.
    pub fn set_silence(&mut self, offset: usize, count: usize) -> Result<(), SamplesError> {
        let layout = self.layout;
        layout.check_range(offset, count)?;
        let base = self.data.as_mut_ptr().cast::<u8>();
        layout.with_plane_ptrs(base, |planes| {
            // SAFETY: `planes` points to `layout.planes` pointers into the
            // exclusively borrowed buffer, at each plane, where C writes
            // `count` samples from `offset`, inside the plane by
            // `check_range`; every `int` C computes is at most a plane's
            // byte count, an `int` by `Layout::new`.
            unsafe {
                ffi::av_samples_set_silence(
                    planes,
                    offset as c_int,
                    count as c_int,
                    layout.channels as c_int,
                    layout.format.into_raw(),
                )
            }
        });
        Ok(())
    }

    /// Wraps: av_samples_copy
    ///
    /// Copies samples `src_offset..src_offset + count` of every channel of
    /// `src` to samples `dst_offset..` of this view.
    ///
    /// Fails, writing nothing, when `src` has another format or channel
    /// count, or when the samples run past the end of either.
    pub fn copy_from(
        &mut self,
        src: SamplesRef<'_>,
        src_offset: usize,
        dst_offset: usize,
        count: usize,
    ) -> Result<(), SamplesError> {
        let layout = self.layout;
        if src.layout.format != layout.format {
            return Err(SamplesError::FormatMismatch);
        }
        if src.layout.channels != layout.channels {
            return Err(SamplesError::ChannelsMismatch);
        }
        src.layout.check_range(src_offset, count)?;
        layout.check_range(dst_offset, count)?;
        let dst_base = self.data.as_mut_ptr().cast::<u8>();
        let src_base = src.data.as_ptr().cast::<u8>().cast_mut();
        layout.with_plane_ptrs(dst_base, |dst| {
            src.layout.with_plane_ptrs(src_base, |src| {
                // SAFETY: `dst` and `src` point to the planes of two buffers
                // of the same format and channel count, the first borrowed
                // exclusively and so apart from the second; C writes `count`
                // samples from `dst_offset` in each `dst` plane and reads as
                // many from `src_offset` in each `src` plane, all inside
                // their planes by `check_range`, and initialised in `src`;
                // every `int` C computes is at most a plane's byte count.
                unsafe {
                    ffi::av_samples_copy(
                        dst,
                        src,
                        dst_offset as c_int,
                        src_offset as c_int,
                        count as c_int,
                        layout.channels as c_int,
                        layout.format.into_raw(),
                    )
                }
            })
        });
        Ok(())
    }

    /// Wraps: av_samples_copy
    ///
    /// Copies samples `src_offset..src_offset + count` of every channel to
    /// samples `dst_offset..` of the same channel, within this view; the two
    /// ranges may overlap.
    ///
    /// Fails, writing nothing, when either range runs past the end.
    pub fn copy_within(
        &mut self,
        src_offset: usize,
        dst_offset: usize,
        count: usize,
    ) -> Result<(), SamplesError> {
        let layout = self.layout;
        layout.check_range(src_offset, count)?;
        layout.check_range(dst_offset, count)?;
        let base = self.data.as_mut_ptr().cast::<u8>();
        layout.with_plane_ptrs(base, |planes| {
            // SAFETY: as in `set_silence`, for writes from `dst_offset` and
            // reads from `src_offset`; C passes the same plane pointers as
            // source and destination, so their distance (0) is below the
            // copied size and it copies with `memmove`, which overlap allows.
            unsafe {
                ffi::av_samples_copy(
                    planes,
                    planes,
                    dst_offset as c_int,
                    src_offset as c_int,
                    count as c_int,
                    layout.channels as c_int,
                    layout.format.into_raw(),
                )
            }
        });
        Ok(())
    }
}

/// Handwritten
///
/// A sample buffer C allocated, whose planes are initialised to silence, and
/// which is released by `av_free`. Its samples are read through
/// [`as_ref`](Self::as_ref) and written through [`as_mut`](Self::as_mut).
///
/// The alignment padding between planes stays uninitialised, as C leaves it,
/// and no view reaches it.
pub struct SampleBuffer {
    /// The `av_malloc` block, `size` bytes.
    data: HeapVec<MaybeUninit<u8>>,
    /// Places planes initialised by C inside `data`.
    layout: Layout,
}

impl SampleBuffer {
    /// Wraps: av_samples_alloc
    ///
    /// A buffer of `samples` samples of each of `channels` channels in
    /// `format`, laid out as [`samples_buffer_size`] computes it with
    /// `align`, every sample set to silence.
    ///
    /// Fails with `AVERROR(EINVAL)` where [`samples_buffer_size`] does, and
    /// with `AVERROR(ENOMEM)` when the buffer cannot be allocated.
    pub fn new(
        channels: usize,
        samples: usize,
        format: SampleFormat,
        align: u32,
    ) -> Result<Self, i32> {
        let (nb_channels, nb_samples) = (c_count(channels)?, c_count(samples)?);
        let align_arg = c_align(nb_samples, align)?;
        // Checked first, so that the slots are only allocated for a layout
        // C accepts.
        samples_buffer_size(channels, samples, format, align)?;
        let mut slots = plane_slots(if format.is_planar() { channels } else { 1 })?;
        let mut linesize = 0;
        // SAFETY: `slots` holds the `planar ? channels : 1` pointers C
        // writes, and `linesize` is a live local.
        let ret = unsafe {
            ffi::av_samples_alloc(
                slots.as_mut_ptr(),
                &mut linesize,
                nb_channels,
                nb_samples,
                format.into_raw(),
                align_arg,
            )
        };
        let size = usize::try_from(ret).map_err(|_| ret)?;
        // SAFETY: on success `slots[0]` is an `av_malloc` block of `size`
        // bytes, released by `av_free` (`Heap`), which nothing else owns.
        let data =
            unsafe { HeapVec::from_raw_parts(slots[0].cast(), size) }.ok_or(AVERROR_ENOMEM)?;
        let layout =
            Layout::from_c(format, channels, samples, linesize, size).ok_or(AVERROR_EINVAL)?;
        Ok(Self { data, layout })
    }

    /// The bytes of the whole buffer, padding included.
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// A shared view of the planes.
    pub fn as_ref(&self) -> SamplesRef<'_> {
        SamplesRef {
            data: self.data.as_slice(),
            layout: self.layout,
        }
    }

    /// An exclusive view of the planes.
    pub fn as_mut(&mut self) -> SamplesMut<'_> {
        SamplesMut {
            data: self.data.as_mut_slice(),
            layout: self.layout,
        }
    }
}

impl fmt::Debug for SampleBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SampleBuffer")
            .field("format", &self.layout.format)
            .field("channels", &self.layout.channels)
            .field("samples", &self.layout.samples)
            .field("linesize", &self.layout.linesize)
            .field("size", &self.size())
            .finish()
    }
}

/// Handwritten
///
/// A [`SampleBuffer`] together with the array of its plane pointers, both
/// allocated by C and released by `av_free`, as C code taking a
/// `uint8_t **` of planes (and freeing it with `av_freep`) expects them.
pub struct SampleBufferWithPointers {
    buffer: SampleBuffer,
    /// The `av_calloc` array of `buffer`'s `planes` plane pointers, which C
    /// derived from the block's own pointer. Rust never reads through them.
    pointers: HeapVec<*mut u8>,
}

// SAFETY: the plane pointers are addresses into `buffer`, which this owner
// alone holds; Rust code never reads or writes through them, and a C seam
// over them takes `&mut self`. Both blocks are plain bytes and pointers that
// `av_free` releases from any thread, as `SampleBuffer` (`Send`) is.
unsafe impl Send for SampleBufferWithPointers {}
// SAFETY: `&SampleBufferWithPointers` reaches only shared views of the
// samples, as `&SampleBuffer` (`Sync`) does, and never the pointer array.
unsafe impl Sync for SampleBufferWithPointers {}

impl SampleBufferWithPointers {
    /// Wraps: av_samples_alloc_array_and_samples
    ///
    /// As [`SampleBuffer::new`], with C allocating the array of plane
    /// pointers too.
    ///
    /// Fails with `AVERROR(EINVAL)` where [`samples_buffer_size`] does, and
    /// with `AVERROR(ENOMEM)` when either block cannot be allocated.
    pub fn new(
        channels: usize,
        samples: usize,
        format: SampleFormat,
        align: u32,
    ) -> Result<Self, i32> {
        let (nb_channels, nb_samples) = (c_count(channels)?, c_count(samples)?);
        let align_arg = c_align(nb_samples, align)?;
        let mut array = ptr::null_mut();
        let mut linesize = 0;
        // SAFETY: C writes only the `array` slot and `linesize`, live
        // locals, freeing what it allocated (and leaving `array` NULL) on
        // failure.
        let ret = unsafe {
            ffi::av_samples_alloc_array_and_samples(
                &mut array,
                &mut linesize,
                nb_channels,
                nb_samples,
                format.into_raw(),
                align_arg,
            )
        };
        let size = usize::try_from(ret).map_err(|_| ret)?;
        let planes = if format.is_planar() { channels } else { 1 };
        // SAFETY: on success `array` is an `av_calloc` block of the
        // `planar ? channels : 1` plane pointers C filled, released by
        // `av_free` (`Heap`), which nothing else owns.
        let pointers = unsafe { HeapVec::from_raw_parts(array, planes) }.ok_or(AVERROR_ENOMEM)?;
        // SAFETY: its first pointer is the `av_malloc` block of `size`
        // bytes, likewise.
        let data = unsafe { HeapVec::from_raw_parts(pointers.as_slice()[0].cast(), size) }
            .ok_or(AVERROR_ENOMEM)?;
        let layout =
            Layout::from_c(format, channels, samples, linesize, size).ok_or(AVERROR_EINVAL)?;
        Ok(Self {
            buffer: SampleBuffer { data, layout },
            pointers,
        })
    }

    /// The bytes of the sample buffer, padding included.
    pub fn size(&self) -> usize {
        self.buffer.size()
    }

    /// A shared view of the planes.
    pub fn as_ref(&self) -> SamplesRef<'_> {
        self.buffer.as_ref()
    }

    /// An exclusive view of the planes.
    pub fn as_mut(&mut self) -> SamplesMut<'_> {
        self.buffer.as_mut()
    }

    /// The sample buffer alone, releasing the pointer array.
    pub fn into_buffer(self) -> SampleBuffer {
        self.buffer
    }

    /// C's array of the plane pointers, `uint8_t **`, for a C routine that
    /// reads or writes the planes through it while `self` stays borrowed.
    #[allow(dead_code, reason = "the seam for wrappers of `uint8_t **` consumers")]
    pub(crate) fn as_c_planes(&mut self) -> *mut *mut u8 {
        self.pointers.as_ptr()
    }
}

impl fmt::Debug for SampleBufferWithPointers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SampleBufferWithPointers")
            .field("buffer", &self.buffer)
            .finish_non_exhaustive()
    }
}

impl SampleFormat {
    /// Wraps: av_get_alt_sample_fmt
    ///
    /// This sample format in planar form when `planar` is set, or in packed
    /// form otherwise: itself when it already is, its counterpart with the
    /// same sample type when not. `None` for [`NONE`](Self::NONE) and any
    /// value C does not know.
    ///
    /// ```
    /// use ffmpeg::libavutil::samplefmt::SampleFormat;
    /// assert_eq!(SampleFormat::S16.alt(true), Some(SampleFormat::S16P));
    /// assert_eq!(SampleFormat::S16.alt(false), Some(SampleFormat::S16));
    /// assert_eq!(SampleFormat::NONE.alt(true), None);
    /// ```
    #[must_use]
    pub fn alt(self, planar: bool) -> Option<Self> {
        // SAFETY: `av_get_alt_sample_fmt` takes the format and flag by
        // value and reads its static table only for an index inside it.
        let raw = unsafe { ffi::av_get_alt_sample_fmt(self.0, c_int::from(planar)) };
        Some(Self(raw)).filter(|&f| f != Self::NONE)
    }

    /// Wraps: av_get_packed_sample_fmt
    ///
    /// The packed form of this sample format, its channels interleaved in
    /// one plane: itself when it already is packed. `None` for
    /// [`NONE`](Self::NONE) and any value C does not know.
    #[must_use]
    pub fn packed(self) -> Option<Self> {
        // SAFETY: `av_get_packed_sample_fmt` takes the format by value and
        // reads its static table only for an index inside it.
        let raw = unsafe { ffi::av_get_packed_sample_fmt(self.0) };
        Some(Self(raw)).filter(|&f| f != Self::NONE)
    }

    /// Wraps: av_get_bytes_per_sample
    ///
    /// The size in bytes of one sample of one channel in this format: 1, 2,
    /// 4 or 8, the same for the packed and planar forms. 0 for
    /// [`NONE`](Self::NONE) and any value C does not know.
    #[must_use]
    pub fn bytes_per_sample(self) -> usize {
        // SAFETY: `av_get_bytes_per_sample` takes the format by value and
        // reads its static table only for an index inside it.
        let n = unsafe { ffi::av_get_bytes_per_sample(self.0) };
        // C returns a table entry's bits divided by 8, or 0.
        usize::try_from(n).unwrap_or(0)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_int;
    use core::mem::{align_of, size_of};

    const NAMED: [(SampleFormat, &str); 13] = [
        (SampleFormat::NONE, "NONE"),
        (SampleFormat::U8, "U8"),
        (SampleFormat::S16, "S16"),
        (SampleFormat::S32, "S32"),
        (SampleFormat::FLT, "FLT"),
        (SampleFormat::DBL, "DBL"),
        (SampleFormat::U8P, "U8P"),
        (SampleFormat::S16P, "S16P"),
        (SampleFormat::S32P, "S32P"),
        (SampleFormat::FLTP, "FLTP"),
        (SampleFormat::DBLP, "DBLP"),
        (SampleFormat::S64, "S64"),
        (SampleFormat::S64P, "S64P"),
    ];

    #[test]
    fn layout_matches_the_c_enum() {
        assert_eq!(size_of::<SampleFormat>(), size_of::<ffi::AVSampleFormat>());
        assert_eq!(
            align_of::<SampleFormat>(),
            align_of::<ffi::AVSampleFormat>()
        );
        assert_eq!(size_of::<SampleFormat>(), size_of::<c_int>());
    }

    #[test]
    fn debug_names_the_constants() {
        for (f, name) in NAMED {
            assert_eq!(format!("{f:?}"), name);
        }
    }

    #[test]
    fn debug_shows_unnamed_values_raw() {
        let nb = SampleFormat::from_raw(ffi::AVSampleFormat::AV_SAMPLE_FMT_NB);
        assert_eq!(format!("{nb:?}"), "SampleFormat(12)");
        for raw in [c_int::MIN, -2, 13, c_int::MAX] {
            let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
            assert_eq!(format!("{f:?}"), format!("SampleFormat({raw})"));
        }
    }

    #[test]
    fn index_covers_exactly_the_counted_formats() {
        let counted = &NAMED[1..];
        assert_eq!(counted.len(), SampleFormat::COUNT);
        for (i, &(f, _)) in counted.iter().enumerate() {
            assert_eq!(f.index(), Some(i));
        }
        assert_eq!(SampleFormat::NONE.index(), None);
        for raw in [c_int::MIN, -2, 12, 13, c_int::MAX] {
            assert_eq!(
                SampleFormat::from_raw(ffi::AVSampleFormat(raw)).index(),
                None
            );
        }
    }

    #[test]
    fn raw_conversions_round_trip_every_value() {
        for raw in [c_int::MIN, -2, -1, 0, 11, 12, 13, c_int::MAX] {
            let c = ffi::AVSampleFormat(raw);
            assert_eq!(SampleFormat::from(c).into_raw(), c);
            assert_eq!(ffi::AVSampleFormat::from(SampleFormat::from_raw(c)), c);
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
    fn default_is_none() {
        assert_eq!(SampleFormat::default(), SampleFormat::NONE);
        assert_eq!(SampleFormat::default().into_raw().0, -1);
        assert_eq!(SampleFormat::default().index(), None);
        assert_eq!(format!("{:?}", SampleFormat::default()), "NONE");
    }

    /// A layout of `planes` planes (packed: one, with `channels` channels)
    /// over `len` bytes, without C.
    fn layout(
        bytes: usize,
        planar: bool,
        channels: usize,
        samples: usize,
        linesize: usize,
        len: usize,
    ) -> Option<Layout> {
        Layout::new(
            SampleFormat::S16,
            bytes,
            planar,
            channels,
            samples,
            linesize,
            len,
        )
    }

    #[test]
    fn layout_new_accepts_planes_inside_the_buffer() {
        let l = layout(2, true, 3, 5, 16, 42).unwrap();
        assert_eq!((l.planes, l.block_align, l.plane_len()), (3, 2, 10));
        let l = layout(2, false, 3, 5, 30, 30).unwrap();
        assert_eq!((l.planes, l.block_align, l.plane_len()), (1, 6, 30));
        // The last plane needs only its samples, not a whole line.
        assert!(layout(2, true, 3, 5, 16, 2 * 16 + 10).is_some());
    }

    #[test]
    fn layout_new_rejects_what_c_could_not_address() {
        // Planes past the buffer, or overlapping their neighbours.
        assert_eq!(layout(2, true, 3, 5, 16, 41), None);
        assert_eq!(layout(2, true, 3, 5, 9, 100), None);
        assert_eq!(layout(2, false, 3, 5, 29, 100), None);
        // Nothing to lay out.
        assert_eq!(layout(0, true, 3, 5, 16, 100), None);
        assert_eq!(layout(2, true, 0, 5, 16, 100), None);
        assert_eq!(layout(2, true, 3, 0, 16, 100), None);
        // Byte counts past `INT_MAX`, and overflowing products.
        let big = c_int::MAX as usize;
        assert!(layout(1, true, 1, big, big, big).is_some());
        assert_eq!(layout(1, true, 1, big + 1, big + 1, big + 1), None);
        assert_eq!(layout(2, false, big, 1, usize::MAX, usize::MAX), None);
        assert_eq!(layout(1, true, big + 1, 1, 1, usize::MAX), None);
        assert_eq!(layout(8, true, 2, usize::MAX, usize::MAX, usize::MAX), None);
        assert_eq!(layout(1, true, 3, 1, usize::MAX, usize::MAX), None);
    }

    #[test]
    fn layout_check_range_bounds_offset_and_count() {
        let l = layout(2, true, 2, 5, 10, 20).unwrap();
        for (offset, count) in [(0, 5), (5, 0), (2, 3), (0, 0)] {
            assert_eq!(l.check_range(offset, count), Ok(()));
        }
        for (offset, count) in [(0, 6), (6, 0), (3, 3), (usize::MAX, 1), (1, usize::MAX)] {
            assert_eq!(l.check_range(offset, count), Err(SamplesError::OutOfRange));
        }
    }

    #[test]
    fn layout_plane_ptrs_step_by_linesize_inline_and_spilled() {
        let mut buf = [0u8; 64];
        let base = buf.as_mut_ptr();
        for planes in [1, 8, 9, 16] {
            let l = layout(1, true, planes, 2, 4, 64).unwrap();
            l.with_plane_ptrs(base, |p| {
                for i in 0..planes {
                    // SAFETY: `with_plane_ptrs` passes `planes` pointers.
                    assert_eq!(unsafe { *p.add(i) }, base.wrapping_add(4 * i));
                }
            });
        }
    }

    /// A view over Rust bytes, without C.
    fn view(data: &mut [MaybeUninit<u8>], layout: Layout) -> SamplesMut<'_> {
        assert!(
            layout.planes > 0
                && (layout.planes - 1) * layout.linesize + layout.plane_len() <= data.len()
        );
        SamplesMut { data, layout }
    }

    #[test]
    fn view_planes_skip_uninitialised_padding() {
        // Three planes of 5 two-byte samples, 16 bytes apart; only the
        // planes are initialised.
        let mut data = [MaybeUninit::<u8>::uninit(); 42];
        for p in 0..3 {
            for (i, b) in data[16 * p..16 * p + 10].iter_mut().enumerate() {
                b.write((p * 10 + i) as u8);
            }
        }
        let mut v = view(&mut data, layout(2, true, 3, 5, 16, 42).unwrap());
        let r = v.as_ref();
        assert_eq!(
            (r.plane_count(), r.linesize(), r.samples(), r.channels()),
            (3, 16, 5, 3)
        );
        for p in 0..3 {
            let want: Vec<u8> = (0..10).map(|i| (p * 10 + i) as u8).collect();
            assert_eq!(r.plane(p), Some(&want[..]));
        }
        assert_eq!(r.plane(3), None);
        assert_eq!(r.planes().len(), 3);
        assert!(r.planes().zip(0..).all(|(pl, p)| pl[0] == p * 10));
        // Exclusive planes are disjoint and write only the samples.
        for (p, plane) in v.planes_mut().enumerate() {
            plane.fill(0xa0 + p as u8);
        }
        v.plane_mut(1).unwrap()[9] = 1;
        assert!(v.plane_mut(3).is_none());
        let r = v.as_ref();
        assert_eq!(r.plane(0), Some(&[0xa0; 10][..]));
        assert_eq!(r.plane(1).unwrap()[..9], [0xa1; 9]);
        assert_eq!(r.plane(1).unwrap()[9], 1);
        assert_eq!(r.plane(2), Some(&[0xa2; 10][..]));
    }

    #[test]
    fn view_packed_has_one_plane_of_every_channel() {
        let mut data = [MaybeUninit::new(7u8); 30];
        let mut v = view(&mut data, layout(2, false, 3, 5, 30, 30).unwrap());
        assert_eq!(v.as_ref().plane_count(), 1);
        assert_eq!(v.as_ref().plane(0).unwrap().len(), 30);
        assert_eq!(v.planes_mut().len(), 1);
        assert_eq!(v.as_mut().as_ref().channels(), 3);
    }

    #[test]
    fn view_debug_shows_the_layout() {
        let mut data = [MaybeUninit::new(0u8); 4];
        let v = view(&mut data, layout(2, false, 1, 2, 4, 4).unwrap());
        let s = format!("{v:?}");
        assert_eq!(
            s,
            "SamplesRef { format: S16, channels: 1, samples: 2, linesize: 4 }"
        );
        assert_eq!(format!("{:?}", v.as_ref()), s);
    }

    #[test]
    fn c_count_refuses_what_an_int_cannot_hold() {
        assert_eq!(c_count(0usize), Ok(0));
        assert_eq!(c_count(c_int::MAX as usize), Ok(c_int::MAX));
        assert_eq!(c_count(c_int::MAX as usize + 1), Err(AVERROR_EINVAL));
        assert_eq!(c_count(u32::MAX), Err(AVERROR_EINVAL));
    }

    #[test]
    fn c_align_passes_the_default_for_int_max_minus_31_samples_as_1() {
        let edge = c_int::MAX - 31;
        assert_eq!(edge % 32, 0);
        assert_eq!(c_align(edge, 0), Ok(1));
        assert_eq!(c_align(edge - 1, 0), Ok(0));
        assert_eq!(c_align(edge + 1, 0), Ok(0));
        assert_eq!(c_align(edge, 16), Ok(16));
        assert_eq!(c_align(edge, u32::MAX), Err(AVERROR_EINVAL));
    }

    /// C's `FFALIGN(nb_samples, 32)` overflows for the default alignment of
    /// `INT_MAX - 31` samples (UBSan: `samplefmt.c` `2147483616 + 32 cannot
    /// be represented in type 'int'`); the wrapper gets C's intended layout
    /// without it.
    #[test]
    fn samples_buffer_size_defaults_int_max_minus_31_samples_without_overflow() {
        let n = (c_int::MAX - 31) as usize;
        for f in [SampleFormat::U8, SampleFormat::U8P] {
            assert_eq!(
                samples_buffer_size(1, n, f, 0),
                Ok(BufferSize {
                    size: n,
                    linesize: n
                })
            );
            assert_eq!(
                samples_buffer_size(1, n, f, 0),
                samples_buffer_size(1, n, f, 1)
            );
            assert_eq!(samples_buffer_size(2, n, f, 0), Err(AVERROR_EINVAL));
            assert_eq!(samples_buffer_size(1, n + 1, f, 0), Err(AVERROR_EINVAL));
            // Laid over too few bytes; the 2 GiB allocations are in
            // `tests/ub_safe/samplefmt.rs`.
            assert_eq!(
                SamplesRef::from_buffer(&[0; 64], 1, n, f, 0).unwrap_err(),
                AVERROR_EINVAL
            );
        }
        assert_eq!(
            samples_buffer_size(1, n, SampleFormat::S16, 0),
            Err(AVERROR_EINVAL)
        );
    }

    #[test]
    fn plane_slots_report_allocation_failure() {
        assert_eq!(plane_slots(3).unwrap(), vec![ptr::null_mut::<u8>(); 3]);
        assert_eq!(plane_slots(usize::MAX), Err(AVERROR_ENOMEM));
    }

    #[test]
    fn samples_errors_display() {
        for e in [
            SamplesError::OutOfRange,
            SamplesError::FormatMismatch,
            SamplesError::ChannelsMismatch,
        ] {
            assert!(!e.to_string().is_empty());
            let _: &dyn std::error::Error = &e;
        }
    }

    #[test]
    fn is_planar_names_the_p_formats() {
        for (f, name) in NAMED {
            assert_eq!(f.is_planar(), name.ends_with('P'), "{f:?}");
        }
        for raw in [c_int::MIN, -2, 12, 13, c_int::MAX] {
            assert!(!SampleFormat::from_raw(ffi::AVSampleFormat(raw)).is_planar());
        }
    }

    #[test]
    fn samples_buffer_size_reports_c_s_layout() {
        assert_eq!(
            samples_buffer_size(2, 10, SampleFormat::S16P, 1),
            Ok(BufferSize {
                size: 40,
                linesize: 20
            })
        );
        assert_eq!(
            samples_buffer_size(2, 10, SampleFormat::S16, 16),
            Ok(BufferSize {
                size: 48,
                linesize: 48
            })
        );
        // The default alignment rounds the samples up to 32.
        assert_eq!(
            samples_buffer_size(1, 1, SampleFormat::FLT, 0),
            Ok(BufferSize {
                size: 128,
                linesize: 128
            })
        );
        for (c, n, f, a) in [
            (0, 1, SampleFormat::U8, 1),
            (1, 0, SampleFormat::U8, 1),
            (1, 1, SampleFormat::NONE, 1),
            (1, 1, SampleFormat::U8, u32::MAX),
            (usize::MAX, 1, SampleFormat::U8, 1),
            (1, usize::MAX, SampleFormat::U8, 1),
            (1, c_int::MAX as usize, SampleFormat::S16, 1),
        ] {
            assert_eq!(
                samples_buffer_size(c, n, f, a),
                Err(AVERROR_EINVAL),
                "{c} {n} {f:?} {a}"
            );
        }
    }

    #[test]
    fn sample_buffer_new_is_silent_and_laid_out_by_c() {
        for (f, fill) in [
            (SampleFormat::U8P, 0x80),
            (SampleFormat::U8, 0x80),
            (SampleFormat::DBLP, 0),
        ] {
            let mut b = SampleBuffer::new(3, 7, f, 0).unwrap();
            let want = samples_buffer_size(3, 7, f, 0).unwrap();
            assert_eq!(b.size(), want.size);
            let r = b.as_ref();
            assert_eq!(
                (r.format(), r.channels(), r.samples(), r.linesize()),
                (f, 3, 7, want.linesize)
            );
            assert_eq!(r.plane_count(), if f.is_planar() { 3 } else { 1 });
            assert!(r.planes().all(|p| p.iter().all(|&b| b == fill)), "{f:?}");
            b.as_mut().planes_mut().for_each(|p| p.fill(3));
            assert!(b.as_ref().planes().flatten().all(|&b| b == 3));
        }
        assert_eq!(
            SampleBuffer::new(0, 1, SampleFormat::S16, 1).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            SampleBuffer::new(1, 1, SampleFormat::NONE, 1).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            SampleBuffer::new(1, 1, SampleFormat::S16, u32::MAX).unwrap_err(),
            AVERROR_EINVAL
        );
    }

    #[test]
    fn sample_buffer_with_pointers_points_c_s_array_at_the_planes() {
        let mut b = SampleBufferWithPointers::new(4, 9, SampleFormat::S32P, 32).unwrap();
        let starts: Vec<*const u8> = b.as_ref().planes().map(<[u8]>::as_ptr).collect();
        assert_eq!(starts.len(), 4);
        let array = b.as_c_planes();
        for (i, start) in starts.iter().enumerate() {
            // SAFETY: the array holds one pointer per plane.
            assert_eq!(unsafe { *array.add(i) }.cast_const(), *start);
        }
        assert!(b.as_ref().planes().flatten().all(|&x| x == 0));
        b.as_mut().plane_mut(3).unwrap()[0] = 9;
        let size = b.size();
        let buffer = b.into_buffer();
        assert_eq!(
            (buffer.size(), buffer.as_ref().plane(3).unwrap()[0]),
            (size, 9)
        );
        assert!(
            format!(
                "{:?}",
                SampleBufferWithPointers::new(1, 1, SampleFormat::U8, 1).unwrap()
            )
            .starts_with("SampleBufferWithPointers { buffer: SampleBuffer {")
        );
        assert_eq!(
            SampleBufferWithPointers::new(1, 0, SampleFormat::U8, 1).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            SampleBufferWithPointers::new(0, 1, SampleFormat::U8P, 1).unwrap_err(),
            AVERROR_EINVAL
        );
    }

    #[test]
    fn from_buffer_lays_planes_over_the_caller_s_bytes() {
        let mut bytes = vec![5u8; 64];
        let base = bytes.as_ptr();
        let r = SamplesRef::from_buffer(&bytes, 2, 3, SampleFormat::S16P, 16).unwrap();
        assert_eq!(r.linesize(), 16);
        assert_eq!(r.plane(0).unwrap().as_ptr(), base);
        assert_eq!(r.plane(1).unwrap().as_ptr(), base.wrapping_add(16));
        // The size C needs is 32 bytes: one fewer is refused.
        assert_eq!(
            SamplesRef::from_buffer(&bytes[..31], 2, 3, SampleFormat::S16P, 16).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            SamplesRef::from_buffer(&bytes, 2, 3, SampleFormat::NONE, 16).unwrap_err(),
            AVERROR_EINVAL
        );
        let mut m = SamplesMut::from_buffer(&mut bytes, 2, 3, SampleFormat::S16P, 16).unwrap();
        m.set_silence(1, 2).unwrap();
        assert_eq!(m.as_ref().plane(1), Some(&[5, 5, 0, 0, 0, 0][..]));
        assert_eq!(
            SamplesMut::from_buffer(&mut [], 1, 1, SampleFormat::U8, 1).unwrap_err(),
            AVERROR_EINVAL
        );
        // The padding the view never reaches keeps the caller's bytes.
        assert_eq!(bytes[6..16], [5; 10]);
    }

    #[test]
    fn set_silence_and_copies_check_their_ranges() {
        let mut a = SampleBuffer::new(2, 4, SampleFormat::S16P, 1).unwrap();
        let b = SampleBuffer::new(2, 4, SampleFormat::S16P, 8).unwrap();
        let c = SampleBuffer::new(2, 4, SampleFormat::S16, 1).unwrap();
        let d = SampleBuffer::new(3, 4, SampleFormat::S16P, 1).unwrap();
        let mut a = a.as_mut();
        assert_eq!(a.set_silence(3, 2), Err(SamplesError::OutOfRange));
        assert_eq!(a.copy_within(0, 1, 4), Err(SamplesError::OutOfRange));
        assert_eq!(a.copy_within(1, 0, 4), Err(SamplesError::OutOfRange));
        assert_eq!(
            a.copy_from(b.as_ref(), 1, 0, 4),
            Err(SamplesError::OutOfRange)
        );
        assert_eq!(
            a.copy_from(b.as_ref(), 0, 1, 4),
            Err(SamplesError::OutOfRange)
        );
        assert_eq!(
            a.copy_from(c.as_ref(), 0, 0, 1),
            Err(SamplesError::FormatMismatch)
        );
        assert_eq!(
            a.copy_from(d.as_ref(), 0, 0, 1),
            Err(SamplesError::ChannelsMismatch)
        );
        a.plane_mut(0)
            .unwrap()
            .copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        a.copy_within(0, 1, 3).unwrap();
        assert_eq!(a.as_ref().plane(0), Some(&[1, 2, 1, 2, 3, 4, 5, 6][..]));
        let mut b = b;
        b.as_mut().copy_from(a.as_ref(), 2, 0, 2).unwrap();
        assert_eq!(b.as_ref().plane(0), Some(&[3, 4, 5, 6, 0, 0, 0, 0][..]));
        a.set_silence(0, 4).unwrap();
        a.set_silence(4, 0).unwrap();
        assert!(a.as_ref().planes().flatten().all(|&x| x == 0));
    }

    /// Runs the test `name` alone in a child process, where it may lower
    /// the process-wide allocation limit; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_SAMPLEFMT_ISOLATED";
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

    /// C's allocation failures, under an `av_max_alloc` limit of 64 bytes:
    /// the sample block, and the pointer array before it.
    #[test]
    fn allocation_failures_report_enomem() {
        if !isolated("libavutil::samplefmt::unit_tests::allocation_failures_report_enomem") {
            return;
        }
        crate::libavutil::mem::set_max_alloc(64);
        let enomem = Err(AVERROR_ENOMEM);
        assert_eq!(
            SampleBuffer::new(1, 128, SampleFormat::U8, 1).map(|_| ()),
            enomem
        );
        assert_eq!(
            SampleBufferWithPointers::new(1, 128, SampleFormat::U8, 1).map(|_| ()),
            enomem
        );
        assert_eq!(
            SampleBufferWithPointers::new(16, 1, SampleFormat::U8P, 1).map(|_| ()),
            enomem
        );
        assert_eq!(
            SampleBuffer::new(1, 64, SampleFormat::U8, 1)
                .unwrap()
                .size(),
            64
        );
    }

    #[test]
    fn owners_and_views_cross_threads() {
        fn assert_traits<T: Send + Sync>() {}
        assert_traits::<SampleBuffer>();
        assert_traits::<SampleBufferWithPointers>();
        assert_traits::<SamplesRef<'_>>();
        assert_traits::<SamplesMut<'_>>();
    }

    #[test]
    fn const_fns_evaluate_at_compile_time() {
        const RAW: ffi::AVSampleFormat = SampleFormat::S16P.into_raw();
        const BACK: SampleFormat = SampleFormat::from_raw(RAW);
        const INDEX: Option<usize> = BACK.index();
        assert_eq!(RAW, ffi::AVSampleFormat::AV_SAMPLE_FMT_S16P);
        assert_eq!(INDEX, Some(6));
    }

    #[test]
    fn forms_and_sizes_follow_cs_table() {
        let pairs = [
            (SampleFormat::U8, SampleFormat::U8P, 1),
            (SampleFormat::S16, SampleFormat::S16P, 2),
            (SampleFormat::S32, SampleFormat::S32P, 4),
            (SampleFormat::S64, SampleFormat::S64P, 8),
            (SampleFormat::FLT, SampleFormat::FLTP, 4),
            (SampleFormat::DBL, SampleFormat::DBLP, 8),
        ];
        for (packed, planar, bytes) in pairs {
            for f in [packed, planar] {
                assert_eq!(f.alt(false), Some(packed), "{f:?}");
                assert_eq!(f.alt(true), Some(planar), "{f:?}");
                assert_eq!(f.packed(), Some(packed), "{f:?}");
                assert_eq!(f.bytes_per_sample(), bytes, "{f:?}");
            }
        }
        for raw in [c_int::MIN, -2, -1, 12, 13, c_int::MAX] {
            let f = SampleFormat::from_raw(ffi::AVSampleFormat(raw));
            assert_eq!((f.alt(false), f.alt(true), f.packed()), (None, None, None));
            assert_eq!(f.bytes_per_sample(), 0);
        }
    }
}
