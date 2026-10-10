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

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitAnd, BitOr, BitOrAssign};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};

use ffibox::{CSlice, CSliceMut, CZeroable, define_ctype};

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
        // SAFETY: `comp` projected through the exclusive handle's pointer to
        // a live descriptor is four contiguous, initialised
        // `AVComponentDescriptor`s, which `ComponentDescriptor` is
        // `#[repr(transparent)]` over; the projection of a non-null handle
        // pointer is non-null; the run is bound by `&mut self`, so this
        // handle, the only access to the descriptor, is frozen while it
        // lives.
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
        let back = d.into_raw();
        assert_eq!(back.name, c"yuv420p".as_ptr());
        assert_eq!(back.alias, c"i420,yv12x".as_ptr());
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
        assert_eq!(copy.as_ref().name().unwrap().as_ptr(), c"yuv420p".as_ptr());
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
}
