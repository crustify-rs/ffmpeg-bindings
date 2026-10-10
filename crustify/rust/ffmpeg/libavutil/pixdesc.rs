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
//! types they produce, such as [`ColorRange::from_name`].

use core::ffi::{CStr, c_int, c_uint};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ptr::{addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;
use super::pixfmt::{
    ChromaLocation, ColorPrimaries, ColorRange, ColorSpace, ColorTransfer, PixelFormat,
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
}
