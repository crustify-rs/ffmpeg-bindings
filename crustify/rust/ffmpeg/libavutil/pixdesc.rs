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
//! [`PixelFormatDescriptor`] is `AVPixFmtDescriptor`: how a pixel format
//! lays out its pixels, with four [`ComponentDescriptor`]s embedded by value
//! and its [`PixelFormatFlags`]. C keeps its descriptors in one static,
//! never-written table, so a descriptor has no owner either: a Rust-held one
//! is a value, and its `name` and `alias` strings are borrowed for the whole
//! program.
//!
//! The color property lookups by name (`av_color_range_from_name` and its
//! siblings) are associated functions of the [`pixfmt`](super::pixfmt)
//! types they produce, such as [`ColorRange::from_name`], and so are their
//! inverses (`av_color_range_name` and its siblings), such as
//! [`ColorRange::name`]: C returns its names from static tables, so they are
//! `&'static CStr`s that the lookups take back. The pixel format name lookups,
//! descriptor queries ([`PixelFormat::plane_count`]) and conversion loss
//! queries are methods of [`PixelFormat`], the last reporting a [`Loss`].
//!
//! [`PixelFormat::descriptor`] borrows a format's descriptor from C's table
//! for the whole program, and [`descriptors`] walks the table; a descriptor
//! counts its bits per pixel ([`PixelFormatDescriptorRef::bits_per_pixel`]).
//! [`read_image_line`] and [`write_image_line`] move one component of a row
//! of pixels between an image's [`Planes`] and a `u16` or `u32` line, as a
//! descriptor lays them out. C trusts every descriptor it is given, so these
//! check one first, refusing what only a Rust-built descriptor can hold
//! ([`InvalidDescriptor`], [`ImageLineError`]).

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitOr, BitOrAssign, Range};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{CSlice, CSliceMut, CZeroable, define_ctype};

use super::ffi;
use super::imgutils::{Planes, PlanesMut};
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

/// The set of `AV_PIX_FMT_FLAG_*` bits a [`PixelFormatDescriptor`] carries:
/// how the format stores its values (big-endian, palettized, bit-packed,
/// planar, floating-point) and what they are (RGB, XYZ, Bayer, alpha, a
/// hardware surface).
///
/// C keeps them in a `uint64_t`, so any bit pattern is a valid value and the
/// set operations carry unnamed bits along.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PixelFormatFlags(u64);

impl PixelFormatFlags {
    /// Multi-byte values are big-endian.
    pub const BIG_ENDIAN: Self = Self(ffi::AV_PIX_FMT_FLAG_BE as u64);
    /// Plane 1 holds a palette, and plane 0 holds indexes into it.
    pub const PALETTE: Self = Self(ffi::AV_PIX_FMT_FLAG_PAL as u64);
    /// A component's values are bit-wise packed end to end.
    pub const BITSTREAM: Self = Self(ffi::AV_PIX_FMT_FLAG_BITSTREAM as u64);
    /// A hardware-accelerated format, whose planes are not in memory.
    pub const HWACCEL: Self = Self(ffi::AV_PIX_FMT_FLAG_HWACCEL as u64);
    /// At least one component is not in plane 0.
    pub const PLANAR: Self = Self(ffi::AV_PIX_FMT_FLAG_PLANAR as u64);
    /// RGB-like data, rather than YUV or grayscale.
    pub const RGB: Self = Self(ffi::AV_PIX_FMT_FLAG_RGB as u64);
    /// The format has a straight (not premultiplied) alpha channel in some
    /// form, palettized formats included.
    pub const ALPHA: Self = Self(ffi::AV_PIX_FMT_FLAG_ALPHA as u64);
    /// The values follow a Bayer pattern.
    pub const BAYER: Self = Self(ffi::AV_PIX_FMT_FLAG_BAYER as u64);
    /// IEEE-754 floating-point values, whose precision follows the pixel
    /// size.
    pub const FLOAT: Self = Self(ffi::AV_PIX_FMT_FLAG_FLOAT as u64);
    /// XYZ-like data, rather than YUV, RGB or grayscale.
    pub const XYZ: Self = Self(ffi::AV_PIX_FMT_FLAG_XYZ as u64);

    /// Every named flag with its constant's name, in bit order.
    const KNOWN: [(Self, &'static str); 10] = [
        (Self::BIG_ENDIAN, "BIG_ENDIAN"),
        (Self::PALETTE, "PALETTE"),
        (Self::BITSTREAM, "BITSTREAM"),
        (Self::HWACCEL, "HWACCEL"),
        (Self::PLANAR, "PLANAR"),
        (Self::RGB, "RGB"),
        (Self::ALPHA, "ALPHA"),
        (Self::BAYER, "BAYER"),
        (Self::FLOAT, "FLOAT"),
        (Self::XYZ, "XYZ"),
    ];

    /// The set with no flag.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    pub const fn from_bits_retain(bits: u64) -> Self {
        Self(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    pub const fn bits(self) -> u64 {
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

impl fmt::Debug for PixelFormatFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PixelFormatFlags(")?;
        let mut rest = self.0;
        let mut first = true;
        for &(flag, name) in &Self::KNOWN {
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

impl BitOr for PixelFormatFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PixelFormatFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for PixelFormatFlags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

define_ctype!(
    /// Wraps: AVPixFmtDescriptor
    ///
    /// How a pixel format stores its pixels in up to four planes: its name
    /// and aliases, its component count and layouts, its chroma subsampling
    /// and its [`PixelFormatFlags`]. Its getters are on
    /// [`PixelFormatDescriptorRef`] (`as_ref()`), its setters on
    /// [`PixelFormatDescriptorMut`] (`as_mut()`).
    ///
    /// C's descriptors live in libavutil's static, never-written table; no C
    /// routine allocates, frees or copies one, so a Rust-held descriptor is a
    /// value with no owner, and copying one shares its strings.
    ///
    /// Its `name` and `alias` strings are borrowed for the whole program:
    /// each is NULL or a NUL-terminated string that is never freed or
    /// written, as C's table holds string literals, the setters take only
    /// `&'static CStr`, and [`from_raw`](Self::from_raw) requires it. That
    /// is what lets a getter hand them out as `&'static CStr`, which outlive
    /// the descriptor and can be set into another one.
    ///
    /// No other field is range-checked, by C or here. C's readers of a whole
    /// descriptor (`av_get_bits_per_pixel`, `av_read_image_line2` and their
    /// siblings) index its components by the component count and each
    /// component's plane, and shift by the chroma shifts and depths, so a
    /// wrapper passing a Rust-built descriptor to them checks those first.
    #[derive(Clone, Copy)]
    PixelFormatDescriptor,
    PixelFormatDescriptorRef,
    PixelFormatDescriptorMut,
    ffi::AVPixFmtDescriptor
);

// SAFETY: all-zero is a descriptor C itself stores for every pixel format
// its table leaves out: NULL `name` and `alias`, which the getters read as
// `None` and `av_pix_fmt_desc_next` skips; no component, flag or chroma
// shift. Every accessor takes it, and the type has no teardown to reject it.
unsafe impl CZeroable for PixelFormatDescriptor {}

// SAFETY: a descriptor is integers, four `AVComponentDescriptor` values and
// two pointers to strings that, by the type's invariant, live for the whole
// program and are never written; nothing in it is tied to the thread that
// wrote it, so a value, or an exclusive handle to one, moves between threads
// as a `(u64, &'static CStr)` does.
unsafe impl Send for PixelFormatDescriptor {}
// SAFETY: as for `PixelFormatDescriptor`: the handle is the only access to
// the descriptor's fields, and the strings it reaches are never written.
unsafe impl Send for PixelFormatDescriptorMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `PixelFormatDescriptorRef` has getters only, the struct has no interior
// mutability, and the strings are never written by anyone, so concurrent
// shared access reads unchanging bytes, as through a `&(u64, &'static CStr)`.
unsafe impl Sync for PixelFormatDescriptor {}
// SAFETY: as for `Sync for PixelFormatDescriptor`: copies of a shared handle
// only read.
unsafe impl Send for PixelFormatDescriptorRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for PixelFormatDescriptorRef<'_> {}
// SAFETY: `&PixelFormatDescriptorMut` reaches the fields only through
// `as_ref`, a shared handle; the setters need `&mut`.
unsafe impl Sync for PixelFormatDescriptorMut<'_> {}

impl PixelFormatDescriptor {
    /// The all-zero descriptor: no name, alias, component, flag or chroma
    /// subsampling, as C's table stores for a format it leaves out.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::zeroed()
    }

    /// The descriptor C encodes as `raw`.
    ///
    /// # Safety
    ///
    /// `raw.name` and `raw.alias` must each be NULL or point to a
    /// NUL-terminated string that is never freed or written for the rest of
    /// the program, such as a string literal or a field of a descriptor in
    /// C's own table.
    #[inline]
    #[must_use]
    pub const unsafe fn from_raw(raw: ffi::AVPixFmtDescriptor) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this descriptor, for storing it in a C struct by
    /// value or passing it to a C reader.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVPixFmtDescriptor {
        self.0
    }
}

impl Default for PixelFormatDescriptor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl From<PixelFormatDescriptor> for ffi::AVPixFmtDescriptor {
    #[inline]
    fn from(d: PixelFormatDescriptor) -> Self {
        d.into_raw()
    }
}

/// Field by field: two descriptors are equal when their names and aliases
/// hold the same bytes (or are both absent) and every other field is equal.
impl PartialEq for PixelFormatDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().fields() == other.as_ref().fields()
    }
}

impl Eq for PixelFormatDescriptor {}

impl Hash for PixelFormatDescriptor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().fields().hash(state);
    }
}

impl fmt::Debug for PixelFormatDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for PixelFormatDescriptorRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixelFormatDescriptor")
            .field("name", &self.name())
            .field("component_count", &self.component_count())
            .field("log2_chroma_w", &self.log2_chroma_w())
            .field("log2_chroma_h", &self.log2_chroma_h())
            .field("flags", &self.flags())
            .field("components", &self.fields().5)
            .field("alias", &self.alias())
            .finish()
    }
}

/// The fields of a [`PixelFormatDescriptor`] in declaration order, strings
/// by content, for comparison and hashing.
type DescriptorFields = (
    Option<&'static CStr>,
    u8,
    u8,
    u8,
    u64,
    [ComponentDescriptor; 4],
    Option<&'static CStr>,
);

impl<'a> PixelFormatDescriptorRef<'a> {
    /// Field: AVPixFmtDescriptor.name
    ///
    /// The format's name, such as `yuv420p`; `None` for a descriptor without
    /// one, such as the all-zero descriptor C stores for a format its table
    /// leaves out.
    #[must_use]
    pub fn name(&self) -> Option<&'static CStr> {
        // SAFETY: a read of the `name` pointer projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`.
        let p = unsafe { addr_of!((*self.as_ptr()).name).read() };
        // SAFETY: by the type's invariant, a non-null `name` addresses a
        // NUL-terminated string that is never freed or written for the rest
        // of the program.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// Field: AVPixFmtDescriptor.nb_components
    ///
    /// The number of components each pixel has, 1 to 4 in C's table (0 for
    /// a format it leaves out); the first ones of
    /// [`components`](Self::components) describe them.
    #[must_use]
    pub fn component_count(&self) -> u8 {
        // SAFETY: a read of the `nb_components` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { addr_of!((*self.as_ptr()).nb_components).read() }
    }

    /// Field: AVPixFmtDescriptor.log2_chroma_w
    ///
    /// How far to shift the luma width right, rounding up, to get the chroma
    /// width: 1 for `yuv420p`.
    #[must_use]
    pub fn log2_chroma_w(&self) -> u8 {
        // SAFETY: a read of the `log2_chroma_w` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { addr_of!((*self.as_ptr()).log2_chroma_w).read() }
    }

    /// Field: AVPixFmtDescriptor.log2_chroma_h
    ///
    /// How far to shift the luma height right, rounding up, to get the
    /// chroma height: 1 for `yuv420p`.
    #[must_use]
    pub fn log2_chroma_h(&self) -> u8 {
        // SAFETY: a read of the `log2_chroma_h` field projected through the
        // shared handle's pointer to a live, initialised
        // `AVPixFmtDescriptor`; every bit pattern is a valid `uint8_t`.
        unsafe { addr_of!((*self.as_ptr()).log2_chroma_h).read() }
    }

    /// Field: AVPixFmtDescriptor.flags
    ///
    /// The format's [`PixelFormatFlags`], unnamed bits included.
    #[must_use]
    pub fn flags(&self) -> PixelFormatFlags {
        // SAFETY: a read of the `flags` field projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`;
        // every bit pattern is a valid `uint64_t`.
        PixelFormatFlags(unsafe { addr_of!((*self.as_ptr()).flags).read() })
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The layouts of the four component slots, borrowed from the
    /// descriptor: luma (or red), then chroma (or green and blue), with
    /// alpha always last; slots past the component count are all-zero in
    /// C's table.
    #[must_use]
    pub fn components(&self) -> CSlice<'a, ComponentDescriptor> {
        // SAFETY: `comp` projected through the shared handle's pointer to a
        // live descriptor is four contiguous, initialised
        // `AVComponentDescriptor`s, which `ComponentDescriptor` is
        // `#[repr(transparent)]` over, living as long as the descriptor does,
        // i.e. for `'a`; the projection of a non-null handle pointer is
        // non-null, and a shared run only reads through it.
        unsafe {
            let p = addr_of!((*self.as_ptr()).comp).cast::<ComponentDescriptor>();
            CSlice::from_raw_parts(NonNull::new_unchecked(p.cast_mut()), 4)
        }
    }

    /// Field: AVPixFmtDescriptor.alias
    ///
    /// The format's alternative names, comma-separated, such as `gray8,y8`
    /// for `gray`; `None` for most formats.
    #[must_use]
    pub fn alias(&self) -> Option<&'static CStr> {
        // SAFETY: a read of the `alias` pointer projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`.
        let p = unsafe { addr_of!((*self.as_ptr()).alias).read() };
        // SAFETY: by the type's invariant, a non-null `alias` addresses a
        // NUL-terminated string that is never freed or written for the rest
        // of the program.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
    }

    /// The fields in declaration order, for comparison, hashing and
    /// formatting.
    fn fields(&self) -> DescriptorFields {
        // SAFETY: a read of the `comp` array projected through the shared
        // handle's pointer to a live, initialised `AVPixFmtDescriptor`; it is
        // twenty `int`s, so the copy is a complete, independent value.
        let comp = unsafe { addr_of!((*self.as_ptr()).comp).read() };
        (
            self.name(),
            self.component_count(),
            self.log2_chroma_w(),
            self.log2_chroma_h(),
            self.flags().bits(),
            comp.map(ComponentDescriptor::from_raw),
            self.alias(),
        )
    }

    /// A copy of the whole descriptor, independent of the place it was read
    /// from; its strings are shared, as they live for the whole program.
    #[must_use]
    pub fn get(&self) -> PixelFormatDescriptor {
        // SAFETY: a read of the whole live, initialised `AVPixFmtDescriptor`
        // through the shared handle's pointer; it is plain data, and its two
        // string pointers keep the type's invariant in the copy, since the
        // strings live, unwritten, for the whole program.
        PixelFormatDescriptor(unsafe { self.as_ptr().read() }, core::marker::PhantomData)
    }
}

impl<'a> PixelFormatDescriptorMut<'a> {
    /// Field: AVPixFmtDescriptor.name
    ///
    /// Sets the format's name, or clears it with `None`, leaving the other
    /// fields as they are. The string is borrowed for the whole program, as
    /// C's own names are.
    pub fn set_name(&mut self, name: Option<&'static CStr>) {
        let p = name.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: a write of the `name` pointer projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; `p` is
        // NULL or a `'static` string, never written, which keeps the type's
        // invariant, and the old value is borrowed, so nothing leaks.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).name).write(p) }
    }

    /// Field: AVPixFmtDescriptor.nb_components
    ///
    /// Sets the number of components each pixel has, leaving the other
    /// fields as they are. Any value is stored; C's table only holds 0 to 4.
    pub fn set_component_count(&mut self, count: u8) {
        // SAFETY: a write of the `nb_components` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).nb_components).write(count) }
    }

    /// Field: AVPixFmtDescriptor.log2_chroma_w
    ///
    /// Sets the chroma width shift, leaving the other fields as they are.
    pub fn set_log2_chroma_w(&mut self, shift: u8) {
        // SAFETY: a write of the `log2_chroma_w` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).log2_chroma_w).write(shift) }
    }

    /// Field: AVPixFmtDescriptor.log2_chroma_h
    ///
    /// Sets the chroma height shift, leaving the other fields as they are.
    pub fn set_log2_chroma_h(&mut self, shift: u8) {
        // SAFETY: a write of the `log2_chroma_h` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).log2_chroma_h).write(shift) }
    }

    /// Field: AVPixFmtDescriptor.flags
    ///
    /// Sets the format's flags, unnamed bits included, leaving the other
    /// fields as they are.
    pub fn set_flags(&mut self, flags: PixelFormatFlags) {
        // SAFETY: a write of the `flags` field projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; the old
        // value owns nothing to leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).flags).write(flags.0) }
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The four component slots, exclusively borrowed from the descriptor
    /// for writing in place through their handles.
    #[must_use]
    pub fn components_mut(&mut self) -> CSliceMut<'_, ComponentDescriptor> {
        // SAFETY: `comp` projected through the exclusive handle's pointer to a live
        // descriptor is four contiguous, initialised `AVComponentDescriptor`s, which
        // `ComponentDescriptor` is `#[repr(transparent)]` over; the projection of a
        // non-null handle pointer is non-null; the run is bound by `&mut self`, so this
        // handle, the only access to the descriptor, is frozen while it lives.
        unsafe {
            let p = addr_of_mut!((*self.as_mut_ptr()).comp).cast::<ComponentDescriptor>();
            CSliceMut::from_raw_parts(NonNull::new_unchecked(p), 4)
        }
    }

    /// Field: AVPixFmtDescriptor.comp
    ///
    /// The four component slots, exclusively borrowed for the handle's whole
    /// lifetime, giving the handle up: the by-value counterpart of
    /// [`components_mut`](Self::components_mut), for a run that outlives a
    /// temporary handle (`d.as_mut().into_components_mut()`).
    #[must_use]
    pub fn into_components_mut(mut self) -> CSliceMut<'a, ComponentDescriptor> {
        // SAFETY: as in `components_mut`; the handle is consumed, so the run
        // is the only access left to the descriptor for `'a`.
        unsafe {
            let p = addr_of_mut!((*self.as_mut_ptr()).comp).cast::<ComponentDescriptor>();
            CSliceMut::from_raw_parts(NonNull::new_unchecked(p), 4)
        }
    }

    /// Field: AVPixFmtDescriptor.alias
    ///
    /// Sets the format's comma-separated alternative names, or clears them
    /// with `None`, leaving the other fields as they are. The string is
    /// borrowed for the whole program, as C's own aliases are.
    pub fn set_alias(&mut self, alias: Option<&'static CStr>) {
        let p = alias.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: a write of the `alias` pointer projected through the
        // exclusive handle's pointer to a live `AVPixFmtDescriptor`; `p` is
        // NULL or a `'static` string, never written, which keeps the type's
        // invariant, and the old value is borrowed, so nothing leaks.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).alias).write(p) }
    }

    /// Overwrites every field with `d`'s.
    pub fn set(&mut self, d: PixelFormatDescriptor) {
        // SAFETY: a write of the whole `AVPixFmtDescriptor` through the
        // exclusive handle's pointer to a live one; `d` keeps the type's
        // invariant, and the old value borrows its strings, so nothing leaks.
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

impl PixelFormat {
    /// Wraps: av_get_pix_fmt_string
    ///
    /// Writes a line describing this pixel format into `buf` and returns it,
    /// borrowed from `buf`: its name left-aligned in 11 columns, then its
    /// number of components and bits per pixel right-aligned in 7 and 10
    /// (`"yuv420p           3         12"`), or, for [`NONE`](Self::NONE)
    /// and every other negative value, the header
    /// `"name nb_components nb_bits"` of a table of such lines.
    ///
    /// The text is truncated to fit `buf`, NUL included. `None` for an empty
    /// `buf`, into which C writes nothing, and for a value past the last
    /// format these bindings know (`AV_PIX_FMT_NB`), for which C would read
    /// past its descriptor table.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let mut buf = [0u8; 64];
    /// assert_eq!(
    ///     PixelFormat::YUV420P.describe(&mut buf),
    ///     Some(c"yuv420p           3         12")
    /// );
    /// assert_eq!(PixelFormat::NONE.describe(&mut buf), Some(c"name nb_components nb_bits"));
    /// ```
    pub fn describe(self, buf: &mut [u8]) -> Option<&CStr> {
        if buf.is_empty() || self.into_raw().0 >= ffi::AVPixelFormat::AV_PIX_FMT_NB.0 {
            // C's `snprintf` writes nothing into an empty buffer, and C
            // indexes its table without a bound check.
            return None;
        }
        // A smaller bound than `buf` holds is still a sound one.
        let size = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: `buf` is `size` or more writable bytes, the bound C gives
        // `snprintf`; the value is negative (the header) or below
        // `AV_PIX_FMT_NB`, so it indexes C's table, every entry of which
        // names its format. The returned pointer is `buf`, which C does not
        // retain.
        unsafe { ffi::av_get_pix_fmt_string(buf.as_mut_ptr().cast(), size, self.into_raw()) };
        // `snprintf` NUL-terminated its output within `buf`.
        CStr::from_bytes_until_nul(buf).ok()
    }

    /// Wraps: av_pix_fmt_count_planes
    ///
    /// The number of planes this pixel format stores its components in: 1
    /// for packed formats, up to 4 for planar ones, 0 for hardware formats.
    /// `None` for [`NONE`](Self::NONE) and any value without a descriptor.
    ///
    /// A palettized format ([`PAL8`](Self::PAL8)) counts its one plane of
    /// indices, not the palette C keeps beside it.
    #[must_use]
    pub fn plane_count(self) -> Option<usize> {
        // SAFETY: `av_pix_fmt_count_planes` takes the enum by value and
        // looks it up through `av_pix_fmt_desc_get`, which range-checks it.
        let ret = unsafe { ffi::av_pix_fmt_count_planes(self.into_raw()) };
        usize::try_from(ret).ok()
    }

    /// Wraps: av_pix_fmt_get_chroma_sub_sample
    ///
    /// The chroma subsampling of this pixel format, as base-2 logarithms:
    /// `(horizontal, vertical)`, the amounts to right-shift (rounding up) a
    /// luma width and height by to get the chroma ones. `(1, 1)` for
    /// [`YUV420P`](Self::YUV420P), `(0, 0)` for formats without subsampled
    /// chroma. `None` for [`NONE`](Self::NONE) and any value without a
    /// descriptor.
    #[must_use]
    pub fn chroma_subsampling(self) -> Option<(u8, u8)> {
        let (mut h_shift, mut v_shift): (c_int, c_int) = (0, 0);
        // SAFETY: both out-parameters are live, writable `int`s C writes
        // only during the call; the enum is range-checked by C.
        let ret = unsafe {
            ffi::av_pix_fmt_get_chroma_sub_sample(self.into_raw(), &mut h_shift, &mut v_shift)
        };
        if ret < 0 {
            return None;
        }
        // C copies the descriptor's `uint8_t` fields.
        Some((h_shift as u8, v_shift as u8))
    }
}

impl PixelFormat {
    /// Wraps: av_pix_fmt_desc_get
    ///
    /// This pixel format's descriptor in C's table, borrowed for the whole
    /// program: the table is static and never written. `None` for
    /// [`NONE`](Self::NONE) and every other negative value, and for every
    /// value from the last format these bindings know (`AV_PIX_FMT_NB`) on.
    ///
    /// A value inside that range always has an entry, but one C's table
    /// leaves out is all-zero: it has no [`name`](PixelFormatDescriptorRef::name)
    /// and no component.
    ///
    /// The handle is shared, as C's table is never written; a descriptor to
    /// edit is a copy, [`get`](PixelFormatDescriptorRef::get):
    ///
    /// ```compile_fail,E0599
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let mut desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// desc.set_component_count(0);
    /// ```
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// assert_eq!(desc.name(), Some(c"yuv420p"));
    /// assert_eq!(desc.pixel_format(), Some(PixelFormat::YUV420P));
    /// assert!(PixelFormat::NONE.descriptor().is_none());
    /// ```
    #[must_use]
    pub fn descriptor(self) -> Option<PixelFormatDescriptorRef<'static>> {
        // SAFETY: `av_pix_fmt_desc_get` takes the enum by value, checks its
        // range, and returns NULL or the address of an entry of C's static
        // `const` table, which lives, unwritten, for the whole program and
        // whose `name` and `alias` are NULL or string literals, the type's
        // invariant; a shared `'static` handle only reads it.
        unsafe {
            let p = ffi::av_pix_fmt_desc_get(self.into_raw());
            PixelFormatDescriptorRef::from_ptr(p.cast_mut())
        }
    }
}

impl PixelFormatDescriptorRef<'_> {
    /// Wraps: av_pix_fmt_desc_get_id
    ///
    /// The pixel format whose entry in C's table this handle addresses;
    /// `None` for a descriptor anywhere else, such as a copy of an entry
    /// held in Rust. C compares the address only, never reading through it.
    #[must_use]
    pub fn pixel_format(&self) -> Option<PixelFormat> {
        // SAFETY: C compares `desc` against the bounds of its static table
        // and subtracts the table's base from it only when it lies inside,
        // where a handle to an entry points at an entry's start; it never
        // dereferences `desc`.
        let raw = unsafe { ffi::av_pix_fmt_desc_get_id(self.as_ptr()) };
        Some(PixelFormat::from_raw(raw)).filter(|&f| f != PixelFormat::NONE)
    }

    /// Wraps: av_pix_fmt_desc_next
    ///
    /// The next named descriptor in C's table after the entry this handle
    /// addresses, skipping the entries the table leaves out; `None` past the
    /// last one, and for a descriptor that is not an entry of C's table.
    #[must_use]
    pub fn next_in_table(&self) -> Option<PixelFormatDescriptorRef<'static>> {
        next_descriptor(Some(*self))
    }
}

/// The named C table entry after `prev`, or the first entry for `None`, as a
/// `'static` handle; `None` past the last one, and after a descriptor that
/// is not an entry of C's table.
fn next_descriptor(
    prev: Option<PixelFormatDescriptorRef<'_>>,
) -> Option<PixelFormatDescriptorRef<'static>> {
    let prev = match prev {
        // C walks forward from `prev` inside its own table, so only one of
        // its entries may be passed.
        Some(d) => {
            d.pixel_format()?;
            d.as_ptr()
        }
        None => ptr::null(),
    };
    // SAFETY: `prev` is NULL, from which C starts at its first entry, or an
    // entry of C's static table (`pixel_format` found its address there),
    // from which C steps through the following entries. C returns NULL or
    // the address of one of them, which lives, unwritten, for the whole
    // program with `name` and `alias` NULL or literals, the type's
    // invariant; a shared `'static` handle only reads it.
    unsafe { PixelFormatDescriptorRef::from_ptr(ffi::av_pix_fmt_desc_next(prev).cast_mut()) }
}

/// Wraps: av_pix_fmt_desc_next
///
/// Every named pixel format descriptor in C's table, in the table's order:
/// the order of [`PixelFormat`]'s values, skipping the ones the table leaves
/// out. Each is borrowed for the whole program.
///
/// ```
/// use ffmpeg::libavutil::pixdesc::descriptors;
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// let first = descriptors().next().unwrap();
/// assert_eq!(first.pixel_format(), Some(PixelFormat::YUV420P));
/// assert!(descriptors().any(|d| d.name() == Some(c"rgb24")));
/// ```
#[must_use]
pub fn descriptors() -> Descriptors {
    Descriptors {
        prev: None,
        done: false,
    }
}

/// The iterator over C's pixel format descriptors that [`descriptors`]
/// returns.
#[derive(Clone, Copy, Debug)]
pub struct Descriptors {
    /// The descriptor returned last, `None` before the first.
    prev: Option<PixelFormatDescriptorRef<'static>>,
    /// Whether C reported the end of its table.
    done: bool,
}

impl Iterator for Descriptors {
    type Item = PixelFormatDescriptorRef<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let next = next_descriptor(self.prev);
        self.done = next.is_none();
        self.prev = next;
        next
    }
}

impl core::iter::FusedIterator for Descriptors {}

/// Why a whole-descriptor reader refused a descriptor: a field C would read
/// out of bounds with, or whose arithmetic would overflow `int` or shift by
/// more than its width. C's own table never has one; only a descriptor
/// built or edited in Rust can.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidDescriptor;

impl fmt::Display for InvalidDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("pixel format descriptor outside C's arithmetic range")
    }
}

impl std::error::Error for InvalidDescriptor {}

/// `(value << shift)` as C computes it on an `int`, when that is defined: a
/// non-negative value, a shift below 32 and a result in range.
fn c_shl(value: c_int, shift: c_int) -> Option<c_int> {
    if value < 0 || !(0..32).contains(&shift) {
        return None;
    }
    c_int::try_from(i64::from(value) << shift).ok()
}

impl PixelFormatDescriptorRef<'_> {
    /// The number of components C's bit counters read, and the sum of the
    /// chroma shifts they scale by, when C can index its four component
    /// slots with the one and shift an `int` by the other.
    fn counted_components(&self) -> Result<(usize, c_int), InvalidDescriptor> {
        let count = usize::from(self.component_count());
        let log2_pixels = c_int::from(self.log2_chroma_w()) + c_int::from(self.log2_chroma_h());
        if count > 4 || log2_pixels >= 32 {
            return Err(InvalidDescriptor);
        }
        Ok((count, log2_pixels))
    }

    /// `av_get_bits_per_pixel`'s arithmetic, refused where C's is undefined.
    fn checked_bits_per_pixel(&self) -> Result<c_int, InvalidDescriptor> {
        let (count, log2_pixels) = self.counted_components()?;
        let mut bits: c_int = 0;
        for (c, comp) in self.components().iter().take(count).enumerate() {
            let s = if c == 1 || c == 2 { 0 } else { log2_pixels };
            let scaled = c_shl(comp.depth(), s).ok_or(InvalidDescriptor)?;
            bits = bits.checked_add(scaled).ok_or(InvalidDescriptor)?;
        }
        Ok(bits >> log2_pixels)
    }

    /// `av_get_padded_bits_per_pixel`'s arithmetic, refused where C's is
    /// undefined.
    fn checked_padded_bits_per_pixel(&self) -> Result<c_int, InvalidDescriptor> {
        let (count, log2_pixels) = self.counted_components()?;
        let mut steps: [c_int; 4] = [0; 4];
        for (c, comp) in self.components().iter().take(count).enumerate() {
            let s = if c == 1 || c == 2 { 0 } else { log2_pixels };
            // C stores into its own `int steps[4]` by plane.
            let plane = usize::try_from(comp.plane())
                .ok()
                .filter(|&p| p < 4)
                .ok_or(InvalidDescriptor)?;
            steps[plane] = c_shl(comp.step(), s).ok_or(InvalidDescriptor)?;
        }
        let mut bits = steps
            .iter()
            .try_fold(0 as c_int, |sum, &s| sum.checked_add(s))
            .ok_or(InvalidDescriptor)?;
        if !self.flags().contains(PixelFormatFlags::BITSTREAM) {
            bits = bits.checked_mul(8).ok_or(InvalidDescriptor)?;
        }
        Ok(bits >> log2_pixels)
    }

    /// Wraps: av_get_bits_per_pixel
    ///
    /// The number of bits that represent a pixel, averaged over the chroma
    /// subsampling and excluding padding: 12 for
    /// [`YUV420P`](PixelFormat::YUV420P), 24 for
    /// [`RGB24`](PixelFormat::RGB24) and [`RGB0`](PixelFormat::RGB0).
    ///
    /// # Errors
    ///
    /// [`InvalidDescriptor`] for a descriptor with more than four
    /// components, chroma shifts summing to 32 or more, or a component depth
    /// that is negative or overflows C's `int` sum, on which C would read
    /// past its component slots or overflow. C's own table has none.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let desc = PixelFormat::YUV420P.descriptor().unwrap();
    /// assert_eq!(desc.bits_per_pixel(), Ok(12));
    /// ```
    pub fn bits_per_pixel(&self) -> Result<u32, InvalidDescriptor> {
        self.checked_bits_per_pixel()?;
        // SAFETY: the handle addresses a live, initialised descriptor, which
        // C only reads during the call; `checked_bits_per_pixel` found C's
        // component count, shifts and sums in range for it.
        let bits = unsafe { ffi::av_get_bits_per_pixel(self.as_ptr()) };
        // Non-negative: a sum of non-negative terms, shifted right.
        Ok(bits as u32)
    }

    /// Wraps: av_get_padded_bits_per_pixel
    ///
    /// The number of bits a pixel occupies, padding and unused bits
    /// included, averaged over the chroma subsampling: 12 for
    /// [`YUV420P`](PixelFormat::YUV420P), 32 for [`RGB0`](PixelFormat::RGB0).
    /// C counts the largest step of each plane among the used components
    /// (in bytes, or bits for a [bitstream](PixelFormatFlags::BITSTREAM)
    /// format).
    ///
    /// # Errors
    ///
    /// [`InvalidDescriptor`] for a descriptor with more than four
    /// components, chroma shifts summing to 32 or more, a used component in
    /// a plane outside 0 to 3 (C would write past its four-plane array), or a
    /// step that is negative or overflows C's `int` sum. C's own table has
    /// none.
    ///
    /// ```
    /// use ffmpeg::libavutil::pixfmt::PixelFormat;
    /// let desc = PixelFormat::RGB0.descriptor().unwrap();
    /// assert_eq!(desc.padded_bits_per_pixel(), Ok(32));
    /// ```
    pub fn padded_bits_per_pixel(&self) -> Result<u32, InvalidDescriptor> {
        self.checked_padded_bits_per_pixel()?;
        // SAFETY: the handle addresses a live, initialised descriptor, which
        // C only reads during the call; `checked_padded_bits_per_pixel` found
        // the used components' planes inside C's `steps[4]` and its shifts
        // and sums in range.
        let bits = unsafe { ffi::av_get_padded_bits_per_pixel(self.as_ptr()) };
        // Non-negative: a sum of non-negative terms, shifted right.
        Ok(bits as u32)
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
}

/// An element of the lines [`read_image_line`] fills and
/// [`write_image_line`] reads: `u16` or `u32`, the two sizes C takes.
///
/// Sealed: C knows no other element size, so a line of any other element
/// does not compile:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::imgutils::Planes;
/// use ffmpeg::libavutil::pixdesc::read_image_line;
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// let desc = PixelFormat::GRAY8.descriptor().unwrap();
/// let mut bytes = [0u8; 4];
/// let _ = read_image_line(&mut bytes, Planes::default(), desc, 0, 0, 0, false);
/// ```
pub trait LineElement: sealed::Sealed + Copy {
    /// The element size C is told, in bytes.
    const SIZE: c_int;
}

impl LineElement for u16 {
    const SIZE: c_int = 2;
}

impl LineElement for u32 {
    const SIZE: c_int = 4;
}

/// Why [`read_image_line`] or [`write_image_line`] refused a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageLineError {
    /// The component is not one of the descriptor's four slots.
    Component,
    /// A position, linesize or line length beyond `i32`, or C's `int`
    /// arithmetic on them overflowing.
    OutOfRange,
    /// The component's layout makes C shift by more than a value's width or
    /// overflow `int`. C's own table has no such component.
    InvalidDescriptor,
    /// Part of the line lies outside its plane's slice.
    OutsidePlane,
    /// Plane 1 is too short for every palette entry the read may index.
    OutsidePalette,
}

impl fmt::Display for ImageLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Component => "component index past the descriptor's four slots",
            Self::OutOfRange => "line position, linesize or length out of range",
            Self::InvalidDescriptor => "component layout outside C's arithmetic range",
            Self::OutsidePlane => "line lies outside its plane",
            Self::OutsidePalette => "palette shorter than the entries the line may index",
        })
    }
}

impl std::error::Error for ImageLineError {}

/// Whether a line is read, with or without palette lookups, or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineAccess {
    Read { palette: bool },
    Write,
}

/// The bytes C touches for one line: the byte offsets `span` from row 0 of
/// the component's plane, and, for palette lookups, the bytes of plane 1
/// from its row 0 the lookups may index.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinePlan {
    plane: usize,
    span: Range<i64>,
    palette: Option<u64>,
}

/// What `av_read_image_line2` or `av_write_image_line2` touches when it
/// moves `w > 0` values of component `c`, laid out as `comp` (`[plane,
/// step, offset, shift, depth]`), from pixel `x` of row `y`, mirroring C's
/// `int` arithmetic and refusing where it is undefined; with the plane's
/// linesize as C takes it, or `None` when C returns before touching the
/// image.
fn line_plan(
    [plane, step, offset, shift, depth]: [c_int; 5],
    flags: u64,
    (x, y, c, w): (c_int, c_int, c_int, c_int),
    linesizes: &[isize; 4],
    access: LineAccess,
) -> Result<Option<(LinePlan, c_int)>, ImageLineError> {
    use ImageLineError::{InvalidDescriptor as Invalid, OutOfRange};
    let reading = access != LineAccess::Write;
    // The reader computes `(1ULL << depth) - 1` before anything else.
    if reading && !(0..64).contains(&depth) {
        return Err(Invalid);
    }
    if depth == 0 {
        return Ok(None);
    }
    // C indexes `data[4]` and `linesize[4]` by the plane.
    let plane = usize::try_from(plane)
        .ok()
        .filter(|&p| p < 4)
        .ok_or(Invalid)?;
    let linesize = c_int::try_from(linesizes[plane]).map_err(|_| OutOfRange)?;
    let row = i64::from(y.checked_mul(linesize).ok_or(OutOfRange)?);
    // The mask the reader applies, as C's `unsigned`.
    let mask = if reading {
        ((1u64 << depth) - 1) as u32
    } else {
        0
    };
    let bitstream = flags & u64::from(ffi::AV_PIX_FMT_FLAG_BITSTREAM) != 0;
    let big_endian = flags & u64::from(ffi::AV_PIX_FMT_FLAG_BE) != 0;
    let shift_ok = |s: c_int| (0..32).contains(&s);
    // The span, and the largest value a palette lookup may index with.
    let (span, max_val) = if bitstream && step > 8 {
        // Whole 32-bit words from the row's start: `val >> offset`, or
        // `((1ULL << depth) - 1) << offset` and `val << offset`.
        if !shift_ok(offset) || !(0..64).contains(&depth) {
            return Err(Invalid);
        }
        (row..row + 4 * i64::from(w), u64::from(mask))
    } else if bitstream {
        let skip = x
            .checked_mul(step)
            .and_then(|s| s.checked_add(offset))
            .ok_or(OutOfRange)?;
        let mut shift = 8_i32
            .checked_sub(depth)
            .and_then(|s| s.checked_sub(skip & 7))
            .ok_or(Invalid)?;
        let mut p = row + i64::from(skip >> 3);
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for _ in 0..w {
            // `*p >> shift` or `val << shift`.
            if !shift_ok(shift) {
                return Err(Invalid);
            }
            (lo, hi) = (lo.min(p), hi.max(p + 1));
            shift = shift.checked_sub(step).ok_or(Invalid)?;
            p -= i64::from(shift >> 3);
            shift &= 7;
        }
        // A byte, shifted and masked.
        (lo..hi, u64::from(mask.min(0xff)))
    } else {
        let base = row + i64::from(x.checked_mul(step).ok_or(OutOfRange)?) + i64::from(offset);
        let width = shift.checked_add(depth).ok_or(Invalid)?;
        // `val >> shift` or `val << shift` on an `unsigned`.
        if !shift_ok(shift) {
            return Err(Invalid);
        }
        let (size, base): (i64, i64) = if width <= 8 {
            (1, base + i64::from(big_endian))
        } else if width <= 16 {
            (2, base)
        } else {
            (4, base)
        };
        let last = i64::from(w - 1) * i64::from(step);
        let read_max = (u64::MAX >> (64 - 8 * size as u32)) >> shift;
        (
            base + last.min(0)..base + last.max(0) + size,
            u64::from(mask).min(read_max),
        )
    };
    let palette = match access {
        LineAccess::Read { palette: true } => {
            // `data[1][4 * val + c]`, with `val` an `int` in the bitstream
            // readers.
            let top = 4 * max_val + c as u64;
            if top > c_int::MAX as u64 {
                return Err(Invalid);
            }
            Some(top + 1)
        }
        _ => None,
    };
    Ok(Some((
        LinePlan {
            plane,
            span,
            palette,
        },
        linesize,
    )))
}

/// The offset of row 0 in a plane slice of `len` bytes, rows `linesize`
/// apart: its start for a non-negative linesize, and the start of the last
/// whole row for a negative one, as the [`imgutils`](super::imgutils)
/// module docs lay a plane out.
fn row0(len: usize, linesize: isize) -> usize {
    if linesize >= 0 || len == 0 {
        return 0;
    }
    let l = linesize.unsigned_abs();
    (len - 1) / l * l
}

/// Whether `range`, in bytes from offset `at` of a slice of `len` bytes,
/// lies inside it.
fn inside(len: usize, at: usize, range: &Range<i64>) -> bool {
    let at = at as i64;
    at.checked_add(range.start).is_some_and(|s| s >= 0)
        && at.checked_add(range.end).is_some_and(|e| e <= len as i64)
}

/// The checked arguments of one line read or write: C's `int`s, and the
/// offsets of row 0 in the planes C reaches (`None` for the others).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LineArgs {
    at: [Option<usize>; 4],
    linesizes: [c_int; 4],
    x: c_int,
    y: c_int,
    c: c_int,
    w: c_int,
}

impl LineArgs {
    /// The arguments for moving `len` values of `component` of `desc` at
    /// pixel `x` of row `y` of planes of `lens` bytes, rows `linesizes`
    /// apart; `None` when C touches nothing.
    fn new(
        desc: PixelFormatDescriptorRef<'_>,
        (lens, linesizes): ([usize; 4], &[isize; 4]),
        (x, y, component, len): (usize, usize, usize, usize),
        access: LineAccess,
    ) -> Result<Option<Self>, ImageLineError> {
        let comp = desc
            .components()
            .get(component)
            .ok_or(ImageLineError::Component)?;
        let int = |v: usize| c_int::try_from(v).map_err(|_| ImageLineError::OutOfRange);
        let (x, y, c, w) = (int(x)?, int(y)?, component as c_int, int(len)?);
        if w == 0 {
            return Ok(None);
        }
        let fields = [
            comp.plane(),
            comp.step(),
            comp.offset(),
            comp.shift(),
            comp.depth(),
        ];
        let Some((plan, linesize)) =
            line_plan(fields, desc.flags().bits(), (x, y, c, w), linesizes, access)?
        else {
            return Ok(None);
        };
        let mut args = Self {
            at: [None; 4],
            linesizes: [0; 4],
            x,
            y,
            c,
            w,
        };
        let p = plan.plane;
        let at = row0(lens[p], linesizes[p]);
        if !inside(lens[p], at, &plan.span) {
            return Err(ImageLineError::OutsidePlane);
        }
        (args.at[p], args.linesizes[p]) = (Some(at), linesize);
        if let Some(need) = plan.palette {
            let at = row0(lens[1], linesizes[1]);
            if ((lens[1] - at) as u64) < need {
                return Err(ImageLineError::OutsidePalette);
            }
            args.at[1] = Some(at);
        }
        Ok(Some(args))
    }
}

/// Reads a line as [`read_image_line`] describes, calling `read` with the
/// checked arguments and C's plane pointers.
fn read_line_with(
    len: usize,
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    (x, y, component): (usize, usize, usize),
    palette: bool,
    read: impl FnOnce(&LineArgs, &mut [*const u8; 4]),
) -> Result<(), ImageLineError> {
    let lens = planes.data.map(<[u8]>::len);
    let Some(args) = LineArgs::new(
        desc,
        (lens, &planes.linesizes),
        (x, y, component, len),
        LineAccess::Read { palette },
    )?
    else {
        return Ok(());
    };
    let mut data = [ptr::null(); 4];
    for ((p, plane), at) in data.iter_mut().zip(planes.data).zip(args.at) {
        if let Some(at) = at {
            // `row0` is at most the slice's length; the pointer derives from
            // the whole slice, so C may reach rows on either side of row 0.
            *p = plane.as_ptr().wrapping_add(at);
        }
    }
    read(&args, &mut data);
    Ok(())
}

/// Wraps: av_read_image_line2
///
/// Reads `dst.len()` values of component `component` (0 to 3, in the
/// order of [`components`](PixelFormatDescriptorRef::components)) of the
/// pixels from `x` on of row `y` of an image in `planes`, laid out as
/// `desc` describes, into `dst`, each as a `u16` or `u32`, truncated to the
/// element. With `palette`, each value read is an index, and the byte C
/// stores is the component's entry of plane 1's palette (four bytes per
/// entry, at `4 * index + component`), as for [`PAL8`](PixelFormat::PAL8).
///
/// Row `y` lies `y * linesize` bytes from row 0 of the component's plane,
/// laid out as the [`imgutils`](super::imgutils) module docs describe: row
/// 0 starts the slice for a non-negative linesize, and is its last whole
/// row (the last offset that is a multiple of the linesize) for a negative
/// one. A component with a depth of 0 (absent) reads nothing and leaves
/// `dst` as it is, as does an empty `dst`.
///
/// For a [bitstream](PixelFormatFlags::BITSTREAM) format whose component
/// step exceeds 8 bits ([`XV30BE`](PixelFormat::XV30BE) and
/// [`V30XBE`](PixelFormat::V30XBE)), C takes every component of a pixel
/// from one big-endian 32-bit word per pixel and ignores `x`: the line
/// starts at the row's first word.
///
/// # Errors
///
/// Without touching `dst`: [`ImageLineError::Component`] for a component
/// past 3, [`ImageLineError::OutOfRange`] for a position, linesize or
/// length beyond `i32` or C's `int` arithmetic on them overflowing,
/// [`ImageLineError::InvalidDescriptor`] for a component whose layout makes
/// C's arithmetic undefined, and [`ImageLineError::OutsidePlane`] or
/// [`ImageLineError::OutsidePalette`] when the bytes C would read are not
/// all inside `planes`. With `palette`, plane 1 must hold an entry for every
/// value the component's depth can hold (1024 bytes for an 8-bit index).
///
/// ```
/// use ffmpeg::libavutil::imgutils::Planes;
/// use ffmpeg::libavutil::pixdesc::read_image_line;
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// let desc = PixelFormat::RGB24.descriptor().unwrap();
/// let row = [1, 2, 3, 4, 5, 6];
/// let planes = Planes { data: [&row[..], &[], &[], &[]], linesizes: [6, 0, 0, 0] };
/// let mut green = [0u16; 2];
/// read_image_line(&mut green, planes, desc, 0, 0, 1, false).unwrap();
/// assert_eq!(green, [2, 5]);
/// ```
pub fn read_image_line<T: LineElement>(
    dst: &mut [T],
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
    palette: bool,
) -> Result<(), ImageLineError> {
    let len = dst.len();
    read_line_with(
        len,
        planes,
        desc,
        (x, y, component),
        palette,
        |args, data| {
            // SAFETY: `LineArgs::new` mirrored C's arithmetic for this component
            // and found it defined, and every byte C reads (row `y` from pixel
            // `x` for `w` values, and the palette entries) inside the planes
            // whose row-0 pointers `data` holds; C reads no other plane. `dst`
            // holds `w` elements of `T::SIZE` bytes, the size C writes, and is
            // borrowed apart from the planes; the descriptor is live and only
            // read. C keeps no pointer.
            unsafe {
                ffi::av_read_image_line2(
                    dst.as_mut_ptr().cast(),
                    data.as_mut_ptr(),
                    args.linesizes.as_ptr(),
                    desc.as_ptr(),
                    args.x,
                    args.y,
                    args.c,
                    args.w,
                    c_int::from(palette),
                    T::SIZE,
                )
            }
        },
    )
}

/// Wraps: av_read_image_line
///
/// [`read_image_line`] into `u16`s, through C's original entry point.
///
/// # Errors
///
/// As for [`read_image_line`].
pub fn read_image_line_u16(
    dst: &mut [u16],
    planes: Planes<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
    palette: bool,
) -> Result<(), ImageLineError> {
    let len = dst.len();
    read_line_with(
        len,
        planes,
        desc,
        (x, y, component),
        palette,
        |args, data| {
            // SAFETY: as in `read_image_line`, for `w` 2-byte elements, the size
            // `av_read_image_line` passes on.
            unsafe {
                ffi::av_read_image_line(
                    dst.as_mut_ptr(),
                    data.as_mut_ptr(),
                    args.linesizes.as_ptr(),
                    desc.as_ptr(),
                    args.x,
                    args.y,
                    args.c,
                    args.w,
                    c_int::from(palette),
                )
            }
        },
    )
}

/// Writes a line as [`write_image_line`] describes, calling `write` with
/// the checked arguments and C's plane pointers.
fn write_line_with(
    len: usize,
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    (x, y, component): (usize, usize, usize),
    write: impl FnOnce(&LineArgs, &mut [*mut u8; 4]),
) -> Result<(), ImageLineError> {
    let lens = planes.data.each_ref().map(|d| d.len());
    let Some(args) = LineArgs::new(
        desc,
        (lens, &planes.linesizes),
        (x, y, component, len),
        LineAccess::Write,
    )?
    else {
        return Ok(());
    };
    let mut data = [ptr::null_mut(); 4];
    for ((p, plane), at) in data.iter_mut().zip(&mut planes.data).zip(args.at) {
        if let Some(at) = at {
            // As in `read_line_with`.
            *p = plane.as_mut_ptr().wrapping_add(at);
        }
    }
    write(&args, &mut data);
    Ok(())
}

/// Wraps: av_write_image_line2
///
/// Writes the values of `src` to component `component` (0 to 3) of the
/// pixels from `x` on of row `y` of an image in `planes`, laid out as
/// `desc` describes and as [`read_image_line`] reads them (including the
/// 32-bit-word bitstream formats, for which C ignores `x`).
///
/// C ORs each value, shifted into place, into the bytes already there, as
/// it expects a zeroed image: the bits of earlier values stay set, and
/// value bits beyond the component's depth spill into its neighbours.
/// Nothing is written for a component with a depth of 0 or an empty `src`.
///
/// # Errors
///
/// As for [`read_image_line`], without touching `planes`, except that no
/// palette is involved.
///
/// ```
/// use ffmpeg::libavutil::imgutils::{Planes, PlanesMut};
/// use ffmpeg::libavutil::pixdesc::{read_image_line, write_image_line};
/// use ffmpeg::libavutil::pixfmt::PixelFormat;
/// let desc = PixelFormat::RGB565LE.descriptor().unwrap();
/// let mut row = [0u8; 4];
/// let mut planes = PlanesMut { data: [&mut row[..], &mut [], &mut [], &mut []], linesizes: [4, 0, 0, 0] };
/// write_image_line(&[31u32, 1], &mut planes, desc, 0, 0, 0).unwrap();
/// assert_eq!(row, [0x00, 0xf8, 0x00, 0x08]);
/// ```
pub fn write_image_line<T: LineElement>(
    src: &[T],
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
) -> Result<(), ImageLineError> {
    write_line_with(src.len(), planes, desc, (x, y, component), |args, data| {
        // SAFETY: `LineArgs::new` mirrored C's arithmetic for this component
        // and found it defined, and every byte C reads and writes (row `y`
        // from pixel `x` for `w` values) inside the plane whose row-0
        // pointer `data` holds, borrowed exclusively; C touches no other
        // plane. `src` holds `w` elements of `T::SIZE` bytes, the size C
        // reads; the descriptor is live and only read. C keeps no pointer.
        unsafe {
            ffi::av_write_image_line2(
                src.as_ptr().cast(),
                data.as_mut_ptr(),
                args.linesizes.as_ptr(),
                desc.as_ptr(),
                args.x,
                args.y,
                args.c,
                args.w,
                T::SIZE,
            )
        }
    })
}

/// Wraps: av_write_image_line
///
/// [`write_image_line`] from `u16`s, through C's original entry point.
///
/// # Errors
///
/// As for [`write_image_line`].
pub fn write_image_line_u16(
    src: &[u16],
    planes: &mut PlanesMut<'_>,
    desc: PixelFormatDescriptorRef<'_>,
    x: usize,
    y: usize,
    component: usize,
) -> Result<(), ImageLineError> {
    write_line_with(src.len(), planes, desc, (x, y, component), |args, data| {
        // SAFETY: as in `write_image_line`, for `w` 2-byte elements, the
        // size `av_write_image_line` passes on.
        unsafe {
            ffi::av_write_image_line(
                src.as_ptr(),
                data.as_mut_ptr(),
                args.linesizes.as_ptr(),
                desc.as_ptr(),
                args.x,
                args.y,
                args.c,
                args.w,
            )
        }
    })
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    /// A descriptor with every field set to a distinct value, built without
    /// a C call.
    fn yuv420p_like() -> PixelFormatDescriptor {
        let mut d = PixelFormatDescriptor::new();
        let mut m = d.as_mut();
        m.set_name(Some(c"yuv420p"));
        m.set_component_count(3);
        m.set_log2_chroma_w(1);
        m.set_log2_chroma_h(2);
        m.set_flags(PixelFormatFlags::PLANAR);
        {
            let mut comps = m.components_mut();
            for i in 0..3 {
                comps
                    .get_mut(i)
                    .unwrap()
                    .set(ComponentDescriptor::from_raw(raw(i as i32, 1, 0, 0, 8)));
            }
        }
        m.set_alias(Some(c"i420,yv12x"));
        d
    }

    fn pf_hash_of(d: &PixelFormatDescriptor) -> u64 {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    }

    #[test]
    fn pixel_format_flags_name_c_s_bits() {
        let named = [
            (PixelFormatFlags::BIG_ENDIAN, 1 << 0),
            (PixelFormatFlags::PALETTE, 1 << 1),
            (PixelFormatFlags::BITSTREAM, 1 << 2),
            (PixelFormatFlags::HWACCEL, 1 << 3),
            (PixelFormatFlags::PLANAR, 1 << 4),
            (PixelFormatFlags::RGB, 1 << 5),
            (PixelFormatFlags::ALPHA, 1 << 7),
            (PixelFormatFlags::BAYER, 1 << 8),
            (PixelFormatFlags::FLOAT, 1 << 9),
            (PixelFormatFlags::XYZ, 1 << 10),
        ];
        for (flag, bits) in named {
            assert_eq!(flag.bits(), bits);
            assert_eq!(PixelFormatFlags::from_bits_retain(bits), flag);
        }
        assert_eq!(
            PixelFormatFlags::KNOWN.map(|(f, _)| f),
            named.map(|(f, _)| f)
        );
    }

    #[test]
    fn pixel_format_flags_set_operations_keep_unnamed_bits() {
        let empty = PixelFormatFlags::empty();
        assert!(empty.is_empty());
        assert_eq!(PixelFormatFlags::default(), empty);
        let mut f = PixelFormatFlags::RGB | PixelFormatFlags::ALPHA;
        assert!(f.contains(PixelFormatFlags::RGB));
        assert!(f.contains(PixelFormatFlags::RGB | PixelFormatFlags::ALPHA));
        assert!(!f.contains(PixelFormatFlags::PLANAR));
        assert!(f.contains(empty));
        f |= PixelFormatFlags::from_bits_retain(1 << 6 | 1 << 63);
        assert_eq!(f.bits(), 1 << 5 | 1 << 7 | 1 << 6 | 1 << 63);
        assert_eq!(
            (f & PixelFormatFlags::from_bits_retain(1 << 63 | 1 << 5)).bits(),
            1 << 63 | 1 << 5
        );
        assert!(!f.is_empty());
    }

    #[test]
    fn pixel_format_flags_debug_names_known_bits_and_shows_the_rest() {
        assert_eq!(
            format!("{:?}", PixelFormatFlags::empty()),
            "PixelFormatFlags(0x0)"
        );
        assert_eq!(
            format!("{:?}", PixelFormatFlags::BIG_ENDIAN | PixelFormatFlags::XYZ),
            "PixelFormatFlags(BIG_ENDIAN | XYZ)"
        );
        assert_eq!(
            format!(
                "{:?}",
                PixelFormatFlags::PALETTE | PixelFormatFlags::from_bits_retain(1 << 6)
            ),
            "PixelFormatFlags(PALETTE | 0x40)"
        );
        assert_eq!(
            format!("{:?}", PixelFormatFlags::from_bits_retain(1 << 40)),
            "PixelFormatFlags(0x10000000000)"
        );
    }

    #[test]
    fn pixel_format_flags_debug_passes_on_writer_errors() {
        use core::fmt::Write;

        /// A writer that fails once `budget` bytes are spent.
        struct Budget(usize);
        impl Write for Budget {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.checked_sub(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }

        let mixed = PixelFormatFlags::RGB
            | PixelFormatFlags::ALPHA
            | PixelFormatFlags::from_bits_retain(1 << 6);
        for f in [mixed, PixelFormatFlags::empty()] {
            let full = format!("{f:?}").len();
            for budget in 0..full {
                assert!(write!(Budget(budget), "{f:?}").is_err(), "{f:?} {budget}");
            }
            assert!(write!(Budget(full), "{f:?}").is_ok());
        }
    }

    #[test]
    fn pixel_format_descriptor_new_and_default_are_all_zero() {
        let d = PixelFormatDescriptor::new();
        let r = d.as_ref();
        assert_eq!(r.name(), None);
        assert_eq!(r.alias(), None);
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (0, 0, 0)
        );
        assert!(r.flags().is_empty());
        assert_eq!(r.components().len(), 4);
        assert!(
            r.components()
                .iter()
                .all(|c| c.get() == ComponentDescriptor::new())
        );
        assert_eq!(PixelFormatDescriptor::default(), d);
        let back: ffi::AVPixFmtDescriptor = d.into();
        assert!(back.name.is_null() && back.alias.is_null());
        assert_eq!((back.nb_components, back.flags), (0, 0));
    }

    #[test]
    fn pixel_format_descriptor_setters_write_one_field_each() {
        let mut d = yuv420p_like();
        let r = d.as_ref();
        assert_eq!(r.name(), Some(c"yuv420p"));
        assert_eq!(r.alias(), Some(c"i420,yv12x"));
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (3, 1, 2)
        );
        assert_eq!(r.flags(), PixelFormatFlags::PLANAR);
        let planes: Vec<i32> = r.components().iter().map(|c| c.plane()).collect();
        assert_eq!(planes, [0, 1, 2, 0]);
        assert_eq!(r.components().get(3).unwrap().depth(), 0);
        assert!(r.components().get(4).is_none());

        let mut m = d.as_mut();
        m.set_name(None);
        assert_eq!(m.as_ref().name(), None);
        assert_eq!(m.as_ref().alias(), Some(c"i420,yv12x"));
        m.set_alias(None);
        m.set_component_count(u8::MAX);
        m.set_log2_chroma_w(u8::MAX);
        m.set_log2_chroma_h(0);
        m.set_flags(PixelFormatFlags::from_bits_retain(u64::MAX));
        m.components_mut().get_mut(3).unwrap().set_depth(-1);
        let r = m.as_ref();
        assert_eq!(r.alias(), None);
        assert_eq!(
            (r.component_count(), r.log2_chroma_w(), r.log2_chroma_h()),
            (u8::MAX, u8::MAX, 0)
        );
        assert_eq!(r.flags().bits(), u64::MAX);
        assert_eq!(r.components().get(3).unwrap().depth(), -1);
        assert_eq!(r.components().get(0).unwrap().depth(), 8);
        assert!(m.components_mut().get_mut(4).is_none());
    }

    #[test]
    fn pixel_format_descriptor_raw_round_trip_keeps_every_field() {
        let d = yuv420p_like();
        // The pointers the setters stored, not another instance of the
        // literals: two occurrences of one literal need not share an
        // address.
        let (name, alias) = (d.as_ref().name().unwrap(), d.as_ref().alias().unwrap());
        let back = d.into_raw();
        assert_eq!(back.name, name.as_ptr());
        assert_eq!(back.alias, alias.as_ptr());
        // SAFETY: `back`'s strings are the literals `yuv420p_like` set.
        let back_name = unsafe { CStr::from_ptr(back.name) };
        // SAFETY: as above.
        let back_alias = unsafe { CStr::from_ptr(back.alias) };
        assert_eq!((back_name, back_alias), (c"yuv420p", c"i420,yv12x"));
        assert_eq!(
            (
                back.nb_components,
                back.log2_chroma_w,
                back.log2_chroma_h,
                back.flags
            ),
            (3, 1, 2, 1 << 4)
        );
        assert_eq!(back.comp[2].plane, 2);
        // SAFETY: `back`'s strings are the literals `yuv420p_like` set.
        let again = unsafe { PixelFormatDescriptor::from_raw(back) };
        assert_eq!(again, d);
        assert_eq!(again.as_ref().name().map(CStr::as_ptr), Some(back.name));
    }

    #[test]
    fn pixel_format_descriptor_equality_and_hash_are_field_wise_by_content() {
        let a = yuv420p_like();
        let b = yuv420p_like();
        assert_eq!(a, b);
        assert_eq!(pf_hash_of(&a), pf_hash_of(&b));

        // Same bytes at another address compare equal.
        static OTHER: [u8; 8] = *b"yuv420p\0";
        let other = CStr::from_bytes_with_nul(&OTHER).unwrap();
        let mut c = a;
        c.as_mut().set_name(Some(other));
        assert_ne!(a.as_ref().name().unwrap().as_ptr(), other.as_ptr());
        assert_eq!(a, c);
        assert_eq!(pf_hash_of(&a), pf_hash_of(&c));

        let changes: [fn(&mut PixelFormatDescriptorMut<'_>); 8] = [
            |m| m.set_name(Some(c"yuv420P")),
            |m| m.set_name(None),
            |m| m.set_component_count(4),
            |m| m.set_log2_chroma_w(0),
            |m| m.set_log2_chroma_h(1),
            |m| m.set_flags(PixelFormatFlags::PLANAR | PixelFormatFlags::BIG_ENDIAN),
            |m| m.components_mut().get_mut(3).unwrap().set_shift(1),
            |m| m.set_alias(None),
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut d = a;
            change(&mut d.as_mut());
            assert_ne!(a, d, "change {i}");
        }
    }

    #[test]
    fn pixel_format_descriptor_debug_names_every_field() {
        let mut d = PixelFormatDescriptor::new();
        d.as_mut().set_name(Some(c"gray"));
        d.as_mut().set_component_count(1);
        d.as_mut()
            .components_mut()
            .get_mut(0)
            .unwrap()
            .set(ComponentDescriptor::from_raw(raw(0, 1, 0, 0, 8)));
        d.as_mut().set_alias(Some(c"gray8,y8"));
        let zero = "ComponentDescriptor { plane: 0, step: 0, offset: 0, shift: 0, depth: 0 }";
        let s = format!("{d:?}");
        assert_eq!(
            s,
            format!(
                "PixelFormatDescriptor {{ name: Some(\"gray\"), component_count: 1, \
                 log2_chroma_w: 0, log2_chroma_h: 0, flags: PixelFormatFlags(0x0), \
                 components: [ComponentDescriptor {{ plane: 0, step: 1, offset: 0, \
                 shift: 0, depth: 8 }}, {zero}, {zero}, {zero}], alias: Some(\"gray8,y8\") }}"
            )
        );
        assert_eq!(format!("{:?}", d.as_ref()), s);
    }

    #[test]
    fn pixel_format_descriptor_get_and_set_copy_whole_values() {
        let mut d = yuv420p_like();
        let copy = d.as_ref().get();
        d.as_mut().set_log2_chroma_h(0);
        assert_eq!(copy, yuv420p_like());
        assert_eq!(d.as_ref().log2_chroma_h(), 0);
        // The copy shares the source's strings.
        assert_eq!(
            copy.as_ref().name().unwrap().as_ptr(),
            d.as_ref().name().unwrap().as_ptr()
        );
        d.as_mut().set(PixelFormatDescriptor::new());
        assert_eq!(d, PixelFormatDescriptor::new());
        d.as_mut().set(copy);
        assert_eq!(d, copy);
    }

    #[test]
    fn pixel_format_descriptor_component_runs_cover_the_embedded_array() {
        let mut d = yuv420p_like();
        let base = d.as_ref().as_ptr() as usize;
        let first = d.as_ref().components().as_ptr() as usize;
        assert_eq!(
            first - base,
            core::mem::offset_of!(ffi::AVPixFmtDescriptor, comp)
        );
        {
            let mut comps = d.as_mut().into_components_mut();
            let (mut lo, mut hi) = comps.split_at_mut(2).unwrap();
            lo.get_mut(1).unwrap().set_offset(-1);
            hi.get_mut(1).unwrap().set_offset(7);
        }
        let offsets: Vec<i32> = d.as_ref().components().iter().map(|c| c.offset()).collect();
        assert_eq!(offsets, [0, -1, 0, 7]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_refuses_values_past_the_table_untouched() {
        let mut buf = [b'x'; 40];
        let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
        for raw in [nb, nb + 1, c_int::MAX] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(f.describe(&mut buf), None);
        }
        assert_eq!(PixelFormat::YUV420P.describe(&mut []), None);
        assert_eq!(buf, [b'x'; 40]);
        let last = PixelFormat::from_raw(ffi::AVPixelFormat(nb - 1));
        assert!(last.describe(&mut buf).is_some());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn describe_prints_headers_for_negatives_and_truncates() {
        let mut buf = [b'x'; 40];
        for raw in [c_int::MIN, -2, -1] {
            let f = PixelFormat::from_raw(ffi::AVPixelFormat(raw));
            assert_eq!(f.describe(&mut buf), Some(c"name nb_components nb_bits"));
        }
        assert_eq!(PixelFormat::RGB24.describe(&mut buf[..4]), Some(c"rgb"));
        assert_eq!(&buf[..5], b"rgb\0 ");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn plane_count_and_chroma_subsampling_of_known_formats() {
        assert_eq!(PixelFormat::YUV420P.plane_count(), Some(3));
        assert_eq!(PixelFormat::NV12.plane_count(), Some(2));
        assert_eq!(PixelFormat::RGB24.plane_count(), Some(1));
        assert_eq!(PixelFormat::PAL8.plane_count(), Some(1));
        assert_eq!(PixelFormat::YUVA420P.plane_count(), Some(4));
        assert_eq!(PixelFormat::VAAPI.plane_count(), Some(0));
        assert_eq!(PixelFormat::NONE.plane_count(), None);
        assert_eq!(PixelFormat::YUV420P.chroma_subsampling(), Some((1, 1)));
        assert_eq!(PixelFormat::YUV422P.chroma_subsampling(), Some((1, 0)));
        assert_eq!(PixelFormat::YUV410P.chroma_subsampling(), Some((2, 2)));
        assert_eq!(PixelFormat::RGB24.chroma_subsampling(), Some((0, 0)));
        assert_eq!(PixelFormat::NONE.chroma_subsampling(), None);
        let nb = PixelFormat::from_raw(ffi::AVPixelFormat::AV_PIX_FMT_NB);
        assert_eq!(nb.plane_count(), None);
        assert_eq!(nb.chroma_subsampling(), None);
    }

    const READ: LineAccess = LineAccess::Read { palette: false };
    const PAL: LineAccess = LineAccess::Read { palette: true };
    const WRITE: LineAccess = LineAccess::Write;
    const BE: u64 = ffi::AV_PIX_FMT_FLAG_BE as u64;
    const BITSTREAM: u64 = ffi::AV_PIX_FMT_FLAG_BITSTREAM as u64;

    /// `line_plan` for the span alone.
    fn span(
        comp: [c_int; 5],
        flags: u64,
        at: (c_int, c_int, c_int, c_int),
        linesize: isize,
        access: LineAccess,
    ) -> Result<Option<Range<i64>>, ImageLineError> {
        line_plan(comp, flags, at, &[linesize; 4], access).map(|p| p.map(|(p, _)| p.span))
    }

    #[test]
    fn line_plan_spans_bytes_words_and_steps() {
        // rgb24's green: byte 1 of 3-byte pixels, row 1 of 10-byte rows.
        let plan = line_plan([0, 3, 1, 0, 8], 0, (2, 1, 1, 4), &[10; 4], READ)
            .unwrap()
            .unwrap();
        assert_eq!(
            plan,
            (
                LinePlan {
                    plane: 0,
                    span: 17..27,
                    palette: None
                },
                10
            )
        );
        // gray16: one 2-byte value a pixel; a 32-bit value past 16 bits.
        assert_eq!(
            span([0, 2, 0, 0, 16], 0, (1, 0, 0, 3), 8, READ),
            Ok(Some(2..8))
        );
        assert_eq!(
            span([0, 4, 0, 4, 20], 0, (0, 0, 0, 2), 8, WRITE),
            Ok(Some(0..8))
        );
        // A negative step walks backwards from pixel `x`.
        assert_eq!(
            span([0, -2, 0, 0, 8], 0, (3, 0, 0, 3), 8, READ),
            Ok(Some(-10..-5))
        );
        // rgb565be's red is at offset -1, read from the byte after it.
        assert_eq!(
            span([0, 2, -1, 3, 5], BE, (0, 0, 0, 1), 2, READ),
            Ok(Some(0..1))
        );
        assert_eq!(
            span([0, 2, -1, 3, 5], 0, (0, 0, 0, 1), 2, READ),
            Ok(Some(-1..0))
        );
        // Bit-packed: monob's pixels 3 to 12 are bits 3 to 12.
        assert_eq!(
            span([0, 1, 0, 0, 1], BITSTREAM, (3, 0, 0, 10), 2, READ),
            Ok(Some(0..2))
        );
        assert_eq!(
            span([0, 4, 0, 0, 4], BITSTREAM, (1, 0, 0, 2), 2, WRITE),
            Ok(Some(0..2))
        );
        // Packed into 32-bit words from the row's start, whatever `x` is.
        assert_eq!(
            span([0, 32, 4, 0, 10], BITSTREAM, (9, 2, 0, 3), 16, READ),
            Ok(Some(32..44))
        );
    }

    #[test]
    fn line_plan_returns_nothing_for_absent_components() {
        for access in [READ, PAL, WRITE] {
            assert_eq!(span([7, 0, 0, 99, 0], 0, (0, 0, 0, 1), 8, access), Ok(None));
        }
        // Only the reader shifts by the depth before returning.
        assert_eq!(
            span([0, 1, 0, 0, 64], 0, (0, 0, 0, 1), 8, WRITE),
            Ok(Some(0..4))
        );
        assert_eq!(
            span([0, 1, 0, 0, 64], 0, (0, 0, 0, 1), 8, READ),
            Err(ImageLineError::InvalidDescriptor)
        );
        assert_eq!(
            span([0, 1, 0, 0, -1], 0, (0, 0, 0, 1), 8, READ),
            Err(ImageLineError::InvalidDescriptor)
        );
    }

    #[test]
    fn line_plan_refuses_what_c_cannot_compute() {
        use ImageLineError::{InvalidDescriptor as Invalid, OutOfRange};
        let one = (0, 0, 0, 1);
        // Planes C cannot index its arrays with.
        assert_eq!(span([4, 1, 0, 0, 8], 0, one, 8, READ), Err(Invalid));
        assert_eq!(span([-1, 1, 0, 0, 8], 0, one, 8, WRITE), Err(Invalid));
        // Shifts by a value's width or more, or by a negative amount.
        assert_eq!(span([0, 4, 0, 32, 1], 0, one, 8, READ), Err(Invalid));
        assert_eq!(span([0, 4, 0, -1, 1], 0, one, 8, WRITE), Err(Invalid));
        assert_eq!(span([0, 1, 0, 0, 9], BITSTREAM, one, 8, READ), Err(Invalid));
        assert_eq!(
            span([0, 32, 32, 0, 8], BITSTREAM, one, 8, READ),
            Err(Invalid)
        );
        assert_eq!(
            span([0, 32, 0, 0, -1], BITSTREAM, one, 8, WRITE),
            Err(Invalid)
        );
        // `shift + depth` and `shift - step` overflowing `int`.
        assert_eq!(
            span([0, 1, 0, 8, c_int::MAX], 0, one, 8, WRITE),
            Err(Invalid)
        );
        assert_eq!(
            span([0, c_int::MIN, 0, 0, 1], BITSTREAM, (0, 0, 0, 2), 8, READ),
            Err(Invalid)
        );
        assert_eq!(
            span([0, 1, 0, 0, c_int::MIN], BITSTREAM, one, 8, WRITE),
            Err(Invalid)
        );
        // `y * linesize`, `x * step` and `skip` overflowing `int`.
        assert_eq!(
            span([0, 1, 0, 0, 8], 0, (0, 2, 0, 1), 1 << 30, READ),
            Err(OutOfRange)
        );
        assert_eq!(
            span([0, 1 << 30, 0, 0, 8], 0, (2, 0, 0, 1), 8, READ),
            Err(OutOfRange)
        );
        assert_eq!(
            span([0, 1, c_int::MAX, 0, 1], BITSTREAM, (1, 0, 0, 1), 8, READ),
            Err(OutOfRange)
        );
        // A linesize beyond `int`.
        assert_eq!(
            span([0, 1, 0, 0, 8], 0, one, 1 << 31, READ),
            Err(OutOfRange)
        );
    }

    #[test]
    fn line_plan_sizes_palettes_by_the_largest_index() {
        let pal = |comp, flags, c| {
            line_plan(comp, flags, (0, 0, c, 1), &[8; 4], PAL).map(|p| p.unwrap().0.palette)
        };
        // pal8: 256 entries of 4 bytes, the last read at `4 * 255 + c`.
        assert_eq!(pal([0, 1, 0, 0, 8], 0, 0), Ok(Some(1021)));
        assert_eq!(pal([0, 1, 0, 0, 8], 0, 3), Ok(Some(1024)));
        // A shift leaves fewer bits of the byte read.
        assert_eq!(pal([0, 1, 0, 4, 4], 0, 0), Ok(Some(4 * 15 + 1)));
        // A bit-packed byte holds at most 255.
        assert_eq!(pal([0, 1, 0, 0, 1], BITSTREAM, 2), Ok(Some(4 + 3)));
        // `4 * val + c` must stay in `int` where `val` is one.
        assert_eq!(
            pal([0, 32, 0, 0, 29], BITSTREAM, 3),
            Ok(Some(4 * ((1 << 29) - 1) + 4))
        );
        assert_eq!(
            pal([0, 32, 0, 0, 30], BITSTREAM, 0),
            Err(ImageLineError::InvalidDescriptor)
        );
        assert_eq!(
            line_plan([0, 1, 0, 0, 8], 0, (0, 0, 0, 1), &[8; 4], READ)
                .map(|p| p.unwrap().0.palette),
            Ok(None)
        );
    }

    #[test]
    fn row0_is_the_first_or_last_whole_row() {
        assert_eq!(row0(0, -4), 0);
        assert_eq!(row0(10, 4), 0);
        assert_eq!(row0(10, 0), 0);
        assert_eq!(row0(10, -4), 8);
        assert_eq!(row0(12, -4), 8);
        assert_eq!(row0(13, -4), 12);
        assert_eq!(row0(3, -4), 0);
        assert_eq!(row0(5, isize::MIN), 0);
    }

    #[test]
    fn inside_checks_both_ends() {
        assert!(inside(10, 0, &(0..10)));
        assert!(inside(10, 4, &(-4..6)));
        assert!(!inside(10, 4, &(-5..6)));
        assert!(!inside(10, 4, &(0..7)));
        assert!(inside(0, 0, &(0..0)));
        assert!(!inside(10, 0, &(i64::MIN..0)));
        assert!(!inside(10, 9, &(0..i64::MAX)));
    }

    /// A descriptor with one component, `comp`, in slot 0.
    fn one_component(comp: [c_int; 5], flags: PixelFormatFlags) -> PixelFormatDescriptor {
        let mut d = PixelFormatDescriptor::new();
        let mut m = d.as_mut();
        m.set_component_count(1);
        m.set_flags(flags);
        let [plane, step, offset, shift, depth] = comp;
        m.components_mut()
            .get_mut(0)
            .unwrap()
            .set(ComponentDescriptor::from_raw(raw(
                plane, step, offset, shift, depth,
            )));
        d
    }

    #[test]
    fn line_args_check_positions_and_planes() {
        use ImageLineError::{Component, OutOfRange, OutsidePalette, OutsidePlane};
        let d = one_component([1, 1, 0, 0, 8], PixelFormatFlags::empty());
        let desc = d.as_ref();
        let geometry = |lens| (lens, &[4isize, -4, 4, 4]);
        let args = |lens, at, access| LineArgs::new(desc, geometry(lens), at, access);
        // Plane 1 holds three rows of four bytes, bottom-up: row 0 is last.
        let lens = [0, 12, 0, 0];
        let ok = args(lens, (1, 2, 0, 3), READ).unwrap().unwrap();
        assert_eq!(ok.at, [None, Some(8), None, None]);
        assert_eq!(ok.linesizes, [0, -4, 0, 0]);
        assert_eq!((ok.x, ok.y, ok.c, ok.w), (1, 2, 0, 3));
        // Row 2 is the slice's first; the line may run on into row 1.
        assert!(args(lens, (6, 2, 0, 3), READ).is_ok());
        assert_eq!(args(lens, (10, 2, 0, 3), READ), Err(OutsidePlane));
        assert_eq!(args(lens, (0, 3, 0, 1), WRITE), Err(OutsidePlane));
        assert_eq!(args(lens, (0, 0, 4, 1), READ), Err(Component));
        assert_eq!(args(lens, (1 << 31, 0, 0, 1), READ), Err(OutOfRange));
        assert_eq!(args(lens, (0, 1 << 31, 0, 1), READ), Err(OutOfRange));
        assert_eq!(args(lens, (0, 0, 0, 1 << 31), READ), Err(OutOfRange));
        // Nothing to move, or an absent component: no plane is reached.
        assert_eq!(args([0; 4], (99, 99, 0, 0), READ), Ok(None));
        assert_eq!(args([0; 4], (0, 0, 2, 1), WRITE), Ok(None));
        // The palette is plane 1 from its row 0: the last row of a bottom-up
        // plane, from which the entries run on.
        assert_eq!(args(lens, (0, 0, 0, 1), PAL), Err(OutsidePalette));
        let pal = args([0, 8 + 1021, 0, 0], (0, 0, 0, 1), PAL);
        assert_eq!(pal, Err(OutsidePalette));
        // pal8's indices in plane 0, and its palette in plane 1.
        let d = one_component([0, 1, 0, 0, 8], PixelFormatFlags::PALETTE);
        let pal8 = |lens| LineArgs::new(d.as_ref(), (lens, &[4, 0, 0, 0]), (0, 0, 0, 1), PAL);
        assert_eq!(pal8([4, 1020, 0, 0]), Err(OutsidePalette));
        let ok = pal8([4, 1021, 0, 0]).unwrap().unwrap();
        assert_eq!(ok.at, [Some(0), Some(0), None, None]);
        assert_eq!(ok.linesizes, [4, 0, 0, 0]);
    }

    #[test]
    fn whole_descriptor_checks_mirror_c_arithmetic() {
        let d = yuv420p_like();
        // 8 + 8/8 + 8/8 bits a pixel: chroma 1 x 2 subsampled.
        assert_eq!(d.as_ref().checked_bits_per_pixel(), Ok(10));
        assert_eq!(d.as_ref().checked_padded_bits_per_pixel(), Ok(10));
        let refused = |edit: &dyn Fn(&mut PixelFormatDescriptorMut<'_>)| {
            let mut d = yuv420p_like();
            edit(&mut d.as_mut());
            (
                d.as_ref().checked_bits_per_pixel(),
                d.as_ref().checked_padded_bits_per_pixel(),
            )
        };
        let both = (Err(InvalidDescriptor), Err(InvalidDescriptor));
        assert_eq!(refused(&|m| m.set_component_count(5)), both);
        assert_eq!(refused(&|m| m.set_log2_chroma_w(31)), both);
        let comp = |i: usize, f: [i32; 5]| {
            move |m: &mut PixelFormatDescriptorMut<'_>| {
                let [p, s, o, sh, d] = f;
                m.components_mut()
                    .get_mut(i)
                    .unwrap()
                    .set(ComponentDescriptor::from_raw(raw(p, s, o, sh, d)));
            }
        };
        // A negative depth, or one whose scaling overflows `int`.
        assert_eq!(
            refused(&comp(0, [0, 1, 0, 0, -1])).0,
            Err(InvalidDescriptor)
        );
        assert_eq!(
            refused(&comp(0, [0, 1, 0, 0, 1 << 28])).0,
            Err(InvalidDescriptor)
        );
        // Chroma depths are not scaled, but their sum may overflow.
        assert_eq!(
            refused(&comp(1, [1, 1, 0, 0, 1 << 28])).0,
            Ok((64 + (1 << 28) + 8) >> 3)
        );
        assert_eq!(
            refused(&comp(1, [1, 1, 0, 0, c_int::MAX])).0,
            Err(InvalidDescriptor)
        );
        // A used component's plane indexes C's `steps[4]`.
        assert_eq!(refused(&comp(2, [4, 1, 0, 0, 8])).1, Err(InvalidDescriptor));
        assert_eq!(
            refused(&comp(2, [-1, 1, 0, 0, 8])).1,
            Err(InvalidDescriptor)
        );
        assert_eq!(
            refused(&comp(0, [0, -1, 0, 0, 8])).1,
            Err(InvalidDescriptor)
        );
        // `bits * 8` overflowing, unless the format is bit-packed.
        let wide = comp(1, [1, 1 << 28, 0, 0, 8]);
        assert_eq!(refused(&wide).1, Err(InvalidDescriptor));
        let packed = |m: &mut PixelFormatDescriptorMut<'_>| {
            wide(m);
            m.set_flags(PixelFormatFlags::BITSTREAM);
        };
        assert_eq!(refused(&packed).1, Ok((8 + (1 << 28) + 1) >> 3));
        // Unused slots past the count are never read.
        assert_eq!(refused(&comp(3, [9, -1, 0, 0, -1])), (Ok(10), Ok(10)));
    }

    #[test]
    fn c_shl_is_defined_shifts_only() {
        assert_eq!(c_shl(1, 0), Some(1));
        assert_eq!(c_shl(1, 30), Some(1 << 30));
        assert_eq!(c_shl(1, 31), None);
        assert_eq!(c_shl(0, 31), Some(0));
        assert_eq!(c_shl(3, 32), None);
        assert_eq!(c_shl(3, -1), None);
        assert_eq!(c_shl(-1, 0), None);
    }

    #[test]
    fn line_errors_display_their_cause() {
        let all = [
            ImageLineError::Component,
            ImageLineError::OutOfRange,
            ImageLineError::InvalidDescriptor,
            ImageLineError::OutsidePlane,
            ImageLineError::OutsidePalette,
        ];
        let texts: Vec<String> = all.iter().map(ToString::to_string).collect();
        for (i, t) in texts.iter().enumerate() {
            assert!(!t.is_empty());
            assert!(!texts[..i].contains(t), "{t}");
        }
        assert!(!InvalidDescriptor.to_string().is_empty());
        let _: &dyn std::error::Error = &InvalidDescriptor;
        let _: &dyn std::error::Error = &ImageLineError::Component;
    }

    #[test]
    fn line_wrappers_return_early_without_reaching_c() {
        // No element to move: neither the planes nor `dst` are looked at.
        let d = one_component([0, 1, 0, 0, 8], PixelFormatFlags::empty());
        let mut dst: [u32; 0] = [];
        assert_eq!(
            read_image_line(&mut dst, Planes::default(), d.as_ref(), 7, 7, 0, true),
            Ok(())
        );
        let mut planes = PlanesMut::default();
        assert_eq!(
            write_image_line::<u16>(&[], &mut planes, d.as_ref(), 0, 0, 0),
            Ok(())
        );
        // An absent component reads nothing, and leaves `dst` as it is.
        let mut dst = [9u16; 3];
        assert_eq!(
            read_image_line_u16(&mut dst, Planes::default(), d.as_ref(), 0, 0, 1, false),
            Ok(())
        );
        assert_eq!(dst, [9; 3]);
        assert_eq!(
            write_image_line_u16(&[1], &mut planes, d.as_ref(), 0, 0, 4),
            Err(ImageLineError::Component)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptors_iterator_is_fused_and_walks_the_table() {
        let mut it = descriptors();
        let first = it.next().unwrap();
        assert_eq!(first.pixel_format(), Some(PixelFormat::YUV420P));
        let rest = it.by_ref().count();
        assert!(rest > 100, "{rest}");
        assert!(it.next().is_none());
        assert!(it.next().is_none());
        // Every one has a name, and the walk from any entry matches.
        assert!(descriptors().all(|d| d.name().is_some()));
        let mut from = first;
        for d in descriptors().skip(1) {
            let next = from.next_in_table().unwrap();
            assert_eq!(next.as_ptr(), d.as_ptr());
            from = next;
        }
        assert!(from.next_in_table().is_none());
        // A copy is not in the table.
        let copy = first.get();
        assert_eq!(copy.as_ref().pixel_format(), None);
        assert!(copy.as_ref().next_in_table().is_none());
    }
}
