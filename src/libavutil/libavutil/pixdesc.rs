//! Safe wrappers over `libavutil/pixdesc.h`.
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
//! The color property lookups by name (`av_color_range_from_name` and its
//! siblings) are associated functions of the [`pixfmt`](super::pixfmt)
//! types they produce, such as [`ColorRange::from_name`], and so are their
//! inverses (`av_color_range_name` and its siblings), such as
//! [`ColorRange::name`]: C returns its names from static tables, so they are
//! `&'static CStr`s that the lookups take back. The pixel format name lookups
//! and conversion loss queries are methods of [`PixelFormat`], reporting a
//! [`Loss`].

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitOr, BitOrAssign};
use core::ptr::{self, addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;
use super::pixfmt::{
    AlphaMode, ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer, PixelFormat,
};

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

impl ComponentDescriptorRef<'_> {
    /// Field: AVComponentDescriptor.plane
    ///
    /// Which of the four planes holds the component.
    #[must_use]
    pub fn plane(&self) -> i32 {
        // SAFETY: a read of the `plane` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).plane).read() }
    }

    /// Field: AVComponentDescriptor.step
    ///
    /// The number of elements between two horizontally consecutive pixels:
    /// bits for bitstream formats, bytes otherwise.
    #[must_use]
    pub fn step(&self) -> i32 {
        // SAFETY: a read of the `step` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).step).read() }
    }

    /// Field: AVComponentDescriptor.offset
    ///
    /// The number of elements before the component of the first pixel: bits
    /// for bitstream formats, bytes otherwise. It may be negative.
    #[must_use]
    pub fn offset(&self) -> i32 {
        // SAFETY: a read of the `offset` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).offset).read() }
    }

    /// Field: AVComponentDescriptor.shift
    ///
    /// The number of least significant bits to shift away to get the value.
    #[must_use]
    pub fn shift(&self) -> i32 {
        // SAFETY: a read of the `shift` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).shift).read() }
    }

    /// Field: AVComponentDescriptor.depth
    ///
    /// The number of bits in the component; 0 for a component the format
    /// does not have.
    #[must_use]
    pub fn depth(&self) -> i32 {
        // SAFETY: a read of the `depth` field projected through the shared
        // handle's pointer to a live, initialised `AVComponentDescriptor`;
        // every bit pattern is a valid `int`.
        unsafe { addr_of!((*self.as_ptr()).depth).read() }
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
        // SAFETY: a write of the `plane` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).plane).write(plane) }
    }

    /// Field: AVComponentDescriptor.step
    ///
    /// Sets the distance between consecutive pixels, leaving the other
    /// fields as they are.
    pub fn set_step(&mut self, step: i32) {
        // SAFETY: a write of the `step` field projected through the exclusive
        // handle's pointer to a live `AVComponentDescriptor`; the old value
        // owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).step).write(step) }
    }

    /// Field: AVComponentDescriptor.offset
    ///
    /// Sets the offset of the first pixel's component, leaving the other
    /// fields as they are; a negative offset is stored as given.
    pub fn set_offset(&mut self, offset: i32) {
        // SAFETY: a write of the `offset` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).offset).write(offset) }
    }

    /// Field: AVComponentDescriptor.shift
    ///
    /// Sets the number of low bits to shift away, leaving the other fields
    /// as they are.
    pub fn set_shift(&mut self, shift: i32) {
        // SAFETY: a write of the `shift` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).shift).write(shift) }
    }

    /// Field: AVComponentDescriptor.depth
    ///
    /// Sets the component's width in bits, leaving the other fields as they
    /// are.
    pub fn set_depth(&mut self, depth: i32) {
        // SAFETY: a write of the `depth` field projected through the
        // exclusive handle's pointer to a live `AVComponentDescriptor`; the
        // old value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).depth).write(depth) }
    }

    /// Overwrites all five fields with `d`'s.
    pub fn set(&mut self, d: ComponentDescriptor) {
        // SAFETY: a write of the whole `AVComponentDescriptor` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(d.into_raw()) }
    }
}

/// The value a `*_from_name` lookup returned, or `None` for the
/// `AVERROR(EINVAL)` it returns for an unknown name.
fn found(ret: c_int) -> Option<c_uint> {
    c_uint::try_from(ret).ok()
}

impl ColorRange {
    /// Wraps: av_color_range_from_name
    ///
    /// The color range `name` names: `unknown`, `tv` or `pc`; `None` for any
    /// other name. Names are matched exactly, case included.
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only
        // compares during the call.
        let ret = unsafe { ffi::av_color_range_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorRange(raw)))
    }
}

impl ColorPrimaries {
    /// Wraps: av_color_primaries_from_name
    ///
    /// The color primaries `name` names, such as `bt709` or `vgamut`; `None`
    /// for any other name. Names are matched exactly, case included; the
    /// `reserved` name shared by two values yields the first,
    /// [`RESERVED0`](Self::RESERVED0).
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_primaries_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorPrimaries(raw)))
    }
}

impl ColorTransfer {
    /// Wraps: av_color_transfer_from_name
    ///
    /// The transfer characteristic `name` names, such as `bt709`,
    /// `smpte2084` or `vlog`; `None` for any other name. Names are matched
    /// exactly, case included; the `reserved` name shared by two values
    /// yields the first, [`RESERVED0`](Self::RESERVED0).
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_transfer_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorTransferCharacteristic(raw)))
    }
}

impl ColorSpace {
    /// Wraps: av_color_space_from_name
    ///
    /// The color space `name` names, such as `gbr` or `bt709`; `None` for any
    /// other name. Names are matched exactly, case included.
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_color_space_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVColorSpace(raw)))
    }
}

impl ChromaLocation {
    /// Wraps: av_chroma_location_from_name
    ///
    /// The chroma location `name` names, such as `left` or `center`; `None`
    /// for any other name. Names are matched exactly, case included.
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_chroma_location_from_name(name.as_ptr()) };
        found(ret).map(|raw| Self::from_raw(ffi::AVChromaLocation(raw)))
    }
}

impl PixelFormat {
    /// Wraps: av_pix_fmt_swap_endianness
    ///
    /// The same format in the other byte order: [`RGB48LE`](Self::RGB48LE)
    /// for [`RGB48BE`](Self::RGB48BE) and back. `None` for a format with no
    /// byte order (one whose C name does not end in `be` or `le`, such as
    /// [`YUV420P`](Self::YUV420P)), for one whose other-endian twin C does
    /// not list, and for [`NONE`](Self::NONE) and every value C does not
    /// name.
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
    /// Wraps: av_color_range_name
    ///
    /// The name C gives this range, `unknown`, `tv` or `pc`, which
    /// [`from_name`](Self::from_name) maps back; `None` for any value C does
    /// not name.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::ColorRange;
    /// assert_eq!(ColorRange::JPEG.name(), Some(c"pc"));
    /// assert_eq!(ColorRange::from_name(c"tv"), Some(ColorRange::MPEG));
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: `av_color_range_name` takes the enum by value, accepts any
        // value, and returns NULL or a string literal from its static table.
        unsafe { static_name(ffi::av_color_range_name(self.into_raw())) }
    }
}

impl ColorPrimaries {
    /// Wraps: av_color_primaries_name
    ///
    /// The name C gives these primaries, such as `bt709` or `vgamut`; `None`
    /// for the values C leaves unnamed (13 to 21, and any other it does not
    /// know). [`RESERVED0`](Self::RESERVED0) and [`RESERVED`](Self::RESERVED)
    /// are both `reserved`, which [`from_name`](Self::from_name) maps back to
    /// the first.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(ffi::av_color_primaries_name(self.into_raw())) }
    }
}

impl ColorTransfer {
    /// Wraps: av_color_transfer_name
    ///
    /// The name C gives this transfer characteristic, such as `bt709`,
    /// `smpte2084` or `vlog`; `None` for any value C does not name.
    /// [`RESERVED0`](Self::RESERVED0) and [`RESERVED`](Self::RESERVED) are
    /// both `reserved`, which [`from_name`](Self::from_name) maps back to the
    /// first.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(ffi::av_color_transfer_name(self.into_raw())) }
    }
}

impl ColorSpace {
    /// Wraps: av_color_space_name
    ///
    /// The name C gives this color space, such as `gbr` or `bt709`; `None`
    /// for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(ffi::av_color_space_name(self.into_raw())) }
    }
}

impl ChromaLocation {
    /// Wraps: av_chroma_location_name
    ///
    /// The name C gives this chroma location, such as `unspecified`, `left`
    /// or `topleft`; `None` for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(ffi::av_chroma_location_name(self.into_raw())) }
    }

    /// Wraps: av_chroma_location_enum_to_pos
    ///
    /// Where this location puts the first chroma sample, as the `(x, y)`
    /// position swscale takes: luma sample `(0, 0)` is the origin and luma
    /// sample `(1, 1)` is `(256, 256)`, so each coordinate is 0, 128 or 256.
    /// `None` for [`UNSPECIFIED`](Self::UNSPECIFIED) and any value C does not
    /// name.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::ChromaLocation;
    /// assert_eq!(ChromaLocation::LEFT.position(), Some((0, 128)));
    /// assert_eq!(ChromaLocation::from_position(0, 128), Some(ChromaLocation::LEFT));
    /// assert_eq!(ChromaLocation::UNSPECIFIED.position(), None);
    /// ```
    #[must_use]
    pub fn position(self) -> Option<(i32, i32)> {
        let (mut x, mut y) = (0, 0);
        // SAFETY: `x` and `y` are live, writable `int`s, which C writes only
        // when it returns 0 and does not keep.
        let ret = unsafe { ffi::av_chroma_location_enum_to_pos(&mut x, &mut y, self.into_raw()) };
        (ret == 0).then_some((x, y))
    }

    /// Wraps: av_chroma_location_pos_to_enum
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
    /// Wraps: av_alpha_mode_name
    ///
    /// The name C gives this alpha mode, `unspecified`, `premultiplied` or
    /// `straight`; `None` for any value C does not name.
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: as in `ColorRange::name`.
        unsafe { static_name(ffi::av_alpha_mode_name(self.into_raw())) }
    }

    /// Wraps: av_alpha_mode_from_name
    ///
    /// The alpha mode `name` names: `unspecified`, `premultiplied` or
    /// `straight`; `None` for any other name. Names are matched exactly,
    /// case included.
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: as in `ColorRange::from_name`.
        let ret = unsafe { ffi::av_alpha_mode_from_name(name.as_ptr()) };
        // C returns its `AVERROR(EINVAL)` through the unsigned enum: read
        // the bits back as the `int` they were.
        found(ret.0 as c_int).map(|raw| Self::from_raw(ffi::AVAlphaMode(raw)))
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
    /// Loss of chroma resolution: the destination subsamples more.
    pub const RESOLUTION: Self = Self(ffi::FF_LOSS_RESOLUTION as c_int);
    /// Loss of bits per component.
    pub const DEPTH: Self = Self(ffi::FF_LOSS_DEPTH as c_int);
    /// A conversion between color spaces (RGB, YUV, gray, ...).
    pub const COLORSPACE: Self = Self(ffi::FF_LOSS_COLORSPACE as c_int);
    /// Loss of the alpha channel.
    pub const ALPHA: Self = Self(ffi::FF_LOSS_ALPHA as c_int);
    /// Quantization to a palette.
    pub const COLORQUANT: Self = Self(ffi::FF_LOSS_COLORQUANT as c_int);
    /// Loss of chroma altogether, such as RGB to gray.
    pub const CHROMA: Self = Self(ffi::FF_LOSS_CHROMA as c_int);
    /// Chroma resolution the destination has to spare: it subsamples less.
    pub const EXCESS_RESOLUTION: Self = Self(ffi::FF_LOSS_EXCESS_RESOLUTION as c_int);
    /// Bits per component the destination has to spare.
    pub const EXCESS_DEPTH: Self = Self(ffi::FF_LOSS_EXCESS_DEPTH as c_int);

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

    /// The set with no kind: a lossless conversion.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    pub const fn from_bits_retain(bits: c_int) -> Self {
        Self(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    pub const fn bits(self) -> c_int {
        self.0
    }

    /// Whether no bit is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
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
    /// Wraps: av_get_pix_fmt_name
    ///
    /// The short name C gives this pixel format, such as `yuv420p` or
    /// `rgb48le`, which [`from_name`](Self::from_name) maps back; `None` for
    /// [`NONE`](Self::NONE) and any value C does not know.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// assert_eq!(PixelFormat::YUV420P.name(), Some(c"yuv420p"));
    /// assert_eq!(PixelFormat::NONE.name(), None);
    /// ```
    #[must_use]
    pub fn name(self) -> Option<&'static CStr> {
        // SAFETY: `av_get_pix_fmt_name` takes the enum by value, accepts any
        // value, and returns NULL or the `name` of a descriptor in C's static
        // table, a string literal.
        unsafe { static_name(ffi::av_get_pix_fmt_name(self.into_raw())) }
    }

    /// Wraps: av_get_pix_fmt
    ///
    /// The pixel format `name` names: a descriptor's name or one of its
    /// aliases, `rgb32` and `bgr32` for the native-endian packed 32-bit RGB
    /// formats, or, failing those, `name` with the target's endianness
    /// suffix (`gray16` is [`GRAY16LE`](Self::GRAY16LE) on a little-endian
    /// target). `None` for any other name; names are matched exactly, case
    /// included.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// assert_eq!(PixelFormat::from_name(c"yuv420p"), Some(PixelFormat::YUV420P));
    /// assert_eq!(PixelFormat::from_name(c"gray16"), Some(PixelFormat::GRAY16));
    /// assert_eq!(PixelFormat::from_name(c"no such format"), None);
    /// ```
    pub fn from_name(name: &CStr) -> Option<Self> {
        // SAFETY: `name` is a live NUL-terminated string, which C only
        // compares, and copies into a bounded local with `snprintf`, during
        // the call.
        let raw = unsafe { ffi::av_get_pix_fmt(name.as_ptr()) };
        Some(Self::from_raw(raw)).filter(|&f| f != Self::NONE)
    }

    /// Wraps: av_get_pix_fmt_loss
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
    /// use ffmpeg::libavutil::pixdesc::Loss;
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let loss = PixelFormat::RGB24.conversion_loss(PixelFormat::GRAY8, false).unwrap();
    /// assert!(loss.contains(Loss::CHROMA | Loss::COLORSPACE));
    /// assert_eq!(PixelFormat::NONE.conversion_loss(PixelFormat::GRAY8, false), Err(-4));
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

    /// Wraps: av_find_best_pix_fmt_of_2
    ///
    /// Which of `dst1` and `dst2` this pixel format converts to with the
    /// least loss, counting the loss of alpha only when `has_alpha` is set.
    /// Ties go to the format with fewer bits per pixel, then to the one with
    /// fewer components, then to `dst1`. A candidate C does not know loses
    /// to the other without scoring; when `dst1` is unknown, `dst2` is the
    /// answer even if it is unknown too.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
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

    /// Wraps: av_find_best_pix_fmt_of_2
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
                &mut loss,
            )
        };
        (Self::from_raw(raw), loss_or_error(loss))
    }
}

#[cfg(test)]
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
        assert_eq!(found(-(ffi::EINVAL as c_int)), None);
        assert_eq!(found(c_int::MIN), None);
    }

    #[test]
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
        // SAFETY: NULL, and a string literal, which lives for the process.
        unsafe {
            assert_eq!(static_name(ptr::null()), None);
            assert_eq!(static_name(c"tv".as_ptr()), Some(c"tv"));
        }
    }

    #[test]
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
        assert_eq!(all.bits(), 0xff);
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
}
