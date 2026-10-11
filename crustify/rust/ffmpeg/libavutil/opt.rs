//! Safe wrappers over `libavutil/opt.h`.
//!
//! [`OptionDef`] is one entry of an `AVClass`'s option table (`AVOption`).
//! Every table is a `static const` C array that no C path writes, allocates
//! or frees, so an option is only ever borrowed: an [`OptionDefRef`] carries
//! the getters, and its strings borrow from the table for the handle's
//! lifetime. [`OptionDefMut`] exists only as the layout type's exclusive
//! handle and has no setters.
//!
//! [`OptionType`] is an option's type (`enum AVOptionType`), an open newtype
//! over the C value.
//!
//! [`OptionRanges`] is the list `av_opt_query_ranges` and
//! `av_opt_query_ranges_default` allocate (`AVOptionRanges`). It is owned as
//! an [`OptionRangesBox`], built by [`OptionRanges::query`] or
//! [`OptionRanges::query_default`] and released by `av_opt_freep_ranges`,
//! which also frees every range it lists. C has no copy routine, so the box is
//! not `Clone`, and the list's shape is read-only: its counts bound the
//! release loop.
//!
//! [`OptionRange`] is one element of a list (`AVOptionRange`), owned by it
//! together with its string and only ever borrowed from it: an
//! [`OptionRangeRef`] reads it, and an [`OptionRangeMut`], reached through the
//! list's exclusive handle, rewrites its bounds and replaces its string with
//! a [`HeapString`], which the list then releases.
//!
//! Raw seams remain where the dependency is wrapped by a later batch: the
//! object a range query inspects (an `AVClass`-enabled `void *`) and an array
//! option's `AVOptionArrayDef`.

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::fmt;
use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CBox, CDrop, CSlice, define_ctype};

use super::ffi;
use super::mem::HeapString;

define_ctype!(
    /// Wraps: AVOption
    ///
    /// One option of an `AVClass`: its name, help text, type, default value,
    /// bounds, flags and unit. Reached as an [`OptionDefRef`] borrowed from
    /// the class's static option table.
    OptionDef,
    OptionDefRef,
    OptionDefMut,
    ffi::AVOption
);

// SAFETY: an `AVOption` is plain data (integers, doubles, an `enum`) and
// pointers into immutable statics: its strings are literals and `arr` a
// `static const AVOptionArrayDef`, all of program lifetime and tied to no
// thread. No C path writes an option table once it is published through an
// `AVClass`, and C itself reads one table from any thread (`av_opt_set`,
// `av_opt_find` on a shared context), so a value or an exclusive handle moves
// between threads as a `[u8; N]` with `&'static CStr` fields does.
unsafe impl Send for OptionDef {}
// SAFETY: as for `OptionDef`: `OptionDefMut` has no setters, so it reaches
// only the same immutable bytes.
unsafe impl Send for OptionDefMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `OptionDefRef`
// has getters only, the struct has no interior mutability, and no C path
// writes a table, so concurrent shared access reads unchanging bytes.
unsafe impl Sync for OptionDef {}
// SAFETY: as for `Sync for OptionDef`: copies of a shared handle only read.
unsafe impl Send for OptionDefRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for OptionDefRef<'_> {}
// SAFETY: `&OptionDefMut` reaches the option only through `as_ref`, a shared
// handle, and the exclusive handle has no setters either.
unsafe impl Sync for OptionDefMut<'_> {}

/// The NUL-terminated string at `ptr`, or `None` for NULL.
///
/// # Safety
///
/// `ptr` must be NULL or address a NUL-terminated string that stays live and
/// unwritten for `'a`.
unsafe fn opt_c_str<'a>(ptr: *const c_char) -> Option<&'a CStr> {
    // SAFETY: a non-NULL `ptr` is a live NUL-terminated string for `'a`, per
    // the caller.
    NonNull::new(ptr.cast_mut()).map(|p| unsafe { CStr::from_ptr(p.as_ptr()) })
}

/// The default value of an option, `AVOption.default_val` read through the
/// union member its type selects, as `av_opt_set_defaults` reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OptionDefault<'a> {
    /// `default_val.i64`: the default of a `BOOL`, `FLAGS`, `INT`, `UINT`,
    /// `INT64`, `UINT64`, `DURATION`, `PIXEL_FMT` or `SAMPLE_FMT` option, or a
    /// `CONST` option's value. A `UINT64` default is its bits as `i64`.
    Int(i64),
    /// `default_val.dbl`: the default of a `DOUBLE`, `FLOAT` or `RATIONAL`
    /// option, which C converts with `av_d2q(dbl, INT_MAX)`.
    Double(f64),
    /// `default_val.str`: the default of a `STRING`, `COLOR`, `IMAGE_SIZE`,
    /// `VIDEO_RATE`, `BINARY`, `CHLAYOUT` or `DICT` option, in the syntax C
    /// parses it with; `None` when there is no default.
    Str(Option<&'a CStr>),
    /// `default_val.arr`: an array option's (`AV_OPT_TYPE_FLAG_ARRAY`)
    /// definition, or NULL.
    ///
    /// Raw until `AVOptionArrayDef` is wrapped; when non-NULL it addresses a
    /// static definition that outlives the option.
    Array(*const ffi::AVOptionArrayDef),
    /// An option type libavutil reads no default for.
    Unsupported,
}

// SAFETY: the only non-`Send` member is `Array`'s raw pointer, which is NULL
// or addresses a `static const AVOptionArrayDef` of the option table (see
// [`OptionDefRef::default_value`]): immutable, of program lifetime and read
// by C from any thread. The other variants are plain values and `&'a CStr`.
unsafe impl Send for OptionDefault<'_> {}
// SAFETY: as above: every variant is read-only data with no interior
// mutability.
unsafe impl Sync for OptionDefault<'_> {}

impl<'a> OptionDefRef<'a> {
    /// Field: AVOption.name
    ///
    /// The option's name; `None` only for the terminating entry of a table,
    /// which libavutil's lookups never return.
    pub fn name(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let name = unsafe { addr_of!((*self.as_ptr()).name).read() };
        // SAFETY: `name` is NULL or a string literal of the option table,
        // which outlives every handle to its entries and is never written.
        unsafe { opt_c_str(name) }
    }

    /// Field: AVOption.help
    ///
    /// Short English help text; `None` when the option has none.
    pub fn help(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let help = unsafe { addr_of!((*self.as_ptr()).help).read() };
        // SAFETY: as for `name`.
        unsafe { opt_c_str(help) }
    }

    /// Field: AVOption.offset
    ///
    /// Byte offset of the option's value within the object its class
    /// describes; 0 for a `CONST` option. Native access only: foreign code
    /// reads and writes values through the `av_opt_get` / `av_opt_set` family.
    pub fn offset(&self) -> i32 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).offset).read() }
    }

    /// Field: AVOption.type
    ///
    /// The option's type: a base type, possibly combined with
    /// [`OptionType::ARRAY`].
    pub fn option_type(&self) -> OptionType {
        // SAFETY: a read of a projected field through the shared handle; every
        // `c_uint` is a valid `AVOptionType`.
        OptionType(unsafe { addr_of!((*self.as_ptr()).type_).read() })
    }

    /// Field: AVOption.default_val
    /// Field: AVOption.default_val.i64
    /// Field: AVOption.default_val.dbl
    /// Field: AVOption.default_val.str
    /// Field: AVOption.default_val.arr
    ///
    /// The default value, read through the union member [`option_type`]
    /// selects. `default_val.q` has no variant: libavutil never reads it, and
    /// takes a `RATIONAL` default from `dbl`.
    ///
    /// [`option_type`]: Self::option_type
    pub fn default_value(&self) -> OptionDefault<'a> {
        use OptionType as T;
        let ty = self.option_type();
        let val = self.as_ptr();
        if ty.is_array() {
            // SAFETY: a read of a projected union member through the shared
            // handle; an array option's table entry initialises `arr`, as
            // `av_opt_set_defaults` relies on, and a pointer has no invalid
            // bit patterns.
            return OptionDefault::Array(unsafe { addr_of!((*val).default_val.arr).read() });
        }
        match ty {
            T::BOOL
            | T::FLAGS
            | T::INT
            | T::UINT
            | T::INT64
            | T::UINT64
            | T::DURATION
            | T::PIXEL_FORMAT
            | T::SAMPLE_FORMAT
            | T::CONST => {
                // SAFETY: a read of a projected union member through the
                // shared handle; all 8 bytes of the union are initialised by
                // its table entry, and every bit pattern is a valid `i64`.
                OptionDefault::Int(unsafe { addr_of!((*val).default_val.i64_).read() })
            }
            T::DOUBLE | T::FLOAT | T::RATIONAL => {
                // SAFETY: as above, for an `f64`.
                OptionDefault::Double(unsafe { addr_of!((*val).default_val.dbl).read() })
            }
            T::STRING
            | T::COLOR
            | T::IMAGE_SIZE
            | T::VIDEO_RATE
            | T::BINARY
            | T::CHANNEL_LAYOUT
            | T::DICT => {
                // SAFETY: as above, for a pointer.
                let s = unsafe { addr_of!((*val).default_val.str_).read() };
                // SAFETY: an option of these types initialises `str` to NULL
                // or a string literal of the table, which `av_opt_set_defaults`
                // and `av_opt_is_set_to_default` read as such; it outlives
                // every handle to the entry and is never written.
                OptionDefault::Str(unsafe { opt_c_str(s) })
            }
            _ => OptionDefault::Unsupported,
        }
    }

    /// Field: AVOption.min
    ///
    /// The minimum valid value.
    pub fn min(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).min).read() }
    }

    /// Field: AVOption.max
    ///
    /// The maximum valid value.
    pub fn max(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).max).read() }
    }

    /// Field: AVOption.flags
    ///
    /// A combination of `AV_OPT_FLAG_*`.
    pub fn flags(&self) -> i32 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).flags).read() }
    }

    /// Field: AVOption.unit
    ///
    /// The logical unit shared by an option and its named constants; `None`
    /// when it belongs to none.
    pub fn unit(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let unit = unsafe { addr_of!((*self.as_ptr()).unit).read() };
        // SAFETY: as for `name`.
        unsafe { opt_c_str(unit) }
    }
}

/// Wraps: AVOptionType
/// Handwritten
///
/// The type of an option (`enum AVOptionType`): the C type of the value it
/// names in its object, and which `av_opt_get_*` / `av_opt_set_*` routines and
/// string syntax access that value.
///
/// Open, like the C enum: an `AVClass`'s option table stores whatever its
/// author wrote, so any `c_uint` is a valid value, and the associated
/// constants name the known ones. A type is a base type, possibly combined
/// with the [`ARRAY`](Self::ARRAY) flag that makes it an array option of that
/// base; [`base`](Self::base) and [`array`](Self::array) convert between the
/// two. `ffi::AVOptionType` has no zero enumerator, and libavutil reads 0 only
/// as the type of a table's terminating entry, so there is no `Default`.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionType(ffi::AVOptionType);

impl OptionType {
    /// A set of named flags, stored as an `unsigned int`.
    pub const FLAGS: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_FLAGS);
    /// An `int`.
    pub const INT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_INT);
    /// An `int64_t`.
    pub const INT64: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_INT64);
    /// A `double`.
    pub const DOUBLE: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_DOUBLE);
    /// A `float`.
    pub const FLOAT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_FLOAT);
    /// A `char *`, NULL or a string of the `av_malloc` family.
    pub const STRING: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_STRING);
    /// An `AVRational`.
    pub const RATIONAL: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_RATIONAL);
    /// A `uint8_t *`, NULL or an array of the `av_malloc` family, immediately
    /// followed by an `int` holding its length in bytes.
    pub const BINARY: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_BINARY);
    /// An `AVDictionary *`.
    pub const DICT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_DICT);
    /// A `uint64_t`.
    pub const UINT64: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_UINT64);
    /// A named constant of the options sharing its unit; it names no field
    /// of the object, and its offset is 0.
    pub const CONST: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_CONST);
    /// Two consecutive `int`s, a width and a height.
    pub const IMAGE_SIZE: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_IMAGE_SIZE);
    /// An `enum AVPixelFormat`.
    pub const PIXEL_FORMAT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_PIXEL_FMT);
    /// An `enum AVSampleFormat`.
    pub const SAMPLE_FORMAT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_SAMPLE_FMT);
    /// An `AVRational` frame rate.
    pub const VIDEO_RATE: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_VIDEO_RATE);
    /// An `int64_t` duration in microseconds.
    pub const DURATION: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_DURATION);
    /// A `uint8_t[4]` RGBA colour.
    pub const COLOR: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_COLOR);
    /// An `int` boolean, which may also be -1 for "auto".
    pub const BOOL: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_BOOL);
    /// An `AVChannelLayout`.
    pub const CHANNEL_LAYOUT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_CHLAYOUT);
    /// An `unsigned int`.
    pub const UINT: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_UINT);
    /// The flag that declares an array option when combined with a base type
    /// (`AV_OPT_TYPE_FLAG_ARRAY`): the option's offset then names a pointer to
    /// elements of that type, immediately followed by an `unsigned int`
    /// element count. Not a type on its own.
    pub const ARRAY: Self = Self(ffi::AVOptionType::AV_OPT_TYPE_FLAG_ARRAY);

    /// Every named value with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 21] = [
        (Self::FLAGS, "FLAGS"),
        (Self::INT, "INT"),
        (Self::INT64, "INT64"),
        (Self::DOUBLE, "DOUBLE"),
        (Self::FLOAT, "FLOAT"),
        (Self::STRING, "STRING"),
        (Self::RATIONAL, "RATIONAL"),
        (Self::BINARY, "BINARY"),
        (Self::DICT, "DICT"),
        (Self::UINT64, "UINT64"),
        (Self::CONST, "CONST"),
        (Self::IMAGE_SIZE, "IMAGE_SIZE"),
        (Self::PIXEL_FORMAT, "PIXEL_FORMAT"),
        (Self::SAMPLE_FORMAT, "SAMPLE_FORMAT"),
        (Self::VIDEO_RATE, "VIDEO_RATE"),
        (Self::DURATION, "DURATION"),
        (Self::COLOR, "COLOR"),
        (Self::BOOL, "BOOL"),
        (Self::CHANNEL_LAYOUT, "CHANNEL_LAYOUT"),
        (Self::UINT, "UINT"),
        (Self::ARRAY, "ARRAY"),
    ];

    /// The type with the raw value `value`, named or not.
    #[inline]
    pub const fn from_value(value: c_uint) -> Self {
        Self(ffi::AVOptionType(value))
    }

    /// The raw value C stores for this type.
    #[inline]
    pub const fn value(self) -> c_uint {
        self.0.0
    }

    /// The name of this value's constant, or `None` for an unnamed value,
    /// which includes every base type combined with [`ARRAY`](Self::ARRAY).
    pub fn name(self) -> Option<&'static str> {
        Self::KNOWN
            .iter()
            .find(|(t, _)| *t == self)
            .map(|&(_, name)| name)
    }

    /// Whether this is an array option's type: the [`ARRAY`](Self::ARRAY)
    /// flag is set.
    #[inline]
    pub const fn is_array(self) -> bool {
        self.value() & Self::ARRAY.value() != 0
    }

    /// The type of an array option's elements: this type without the
    /// [`ARRAY`](Self::ARRAY) flag, as libavutil's `TYPE_BASE` computes it.
    /// A scalar type is its own base.
    #[inline]
    pub const fn base(self) -> Self {
        Self::from_value(self.value() & !Self::ARRAY.value())
    }

    /// The type of an array option whose elements have this type: this type
    /// with the [`ARRAY`](Self::ARRAY) flag set.
    #[inline]
    pub const fn array(self) -> Self {
        Self::from_value(self.value() | Self::ARRAY.value())
    }
}

impl fmt::Debug for OptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.name(), self.base().name()) {
            (Some(name), _) => write!(f, "OptionType::{name}"),
            (None, Some(base)) if self.is_array() => {
                write!(f, "OptionType::{base} | OptionType::ARRAY")
            }
            _ => write!(f, "OptionType({})", self.value()),
        }
    }
}

impl From<ffi::AVOptionType> for OptionType {
    #[inline]
    fn from(raw: ffi::AVOptionType) -> Self {
        Self(raw)
    }
}

impl From<OptionType> for ffi::AVOptionType {
    #[inline]
    fn from(ty: OptionType) -> Self {
        ty.0
    }
}

define_ctype!(
    /// Wraps: AVOptionRanges
    ///
    /// The allowed ranges of one option, `range_count()` ranges for each of
    /// `component_count()` components. Owned as an [`OptionRangesBox`].
    OptionRanges,
    OptionRangesRef,
    OptionRangesMut,
    ffi::AVOptionRanges
);

// SAFETY: a list, its `range` array, the ranges and their `str` are
// `av_malloc` blocks that `av_opt_freep_ranges` releases through `av_free`,
// which any thread may call; the query hands the whole tree over to the
// caller and C keeps no pointer to it (`av_opt_query_ranges` writes
// `nb_components` before returning, and `opt_list`, the only in-tree reader,
// frees what it queried). Nothing in it is tied to a thread, so the sole
// owner, or an exclusive handle, moves between threads as a `Box<[Box<T>]>`
// does.
unsafe impl Send for OptionRanges {}
// SAFETY: as for `OptionRanges`: `OptionRangesMut` reaches the same blocks,
// writing only the ranges' bounds and their `av_free`-released strings
// (`get_mut`), none of which is tied to a thread.
unsafe impl Send for OptionRangesMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `OptionRangesRef`
// and the `OptionRangeRef`s it hands out have getters only, the list has no
// interior mutability, and no C path writes a list after its query returns,
// so concurrent shared access reads unchanging bytes.
unsafe impl Sync for OptionRanges {}
// SAFETY: as for `Sync for OptionRanges`: copies of a shared handle only read.
unsafe impl Send for OptionRangesRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for OptionRangesRef<'_> {}
// SAFETY: `&OptionRangesMut` reaches the list only through `as_ref`, a shared
// handle; `get_mut`, its only path to a write, takes `&mut self`.
unsafe impl Sync for OptionRangesMut<'_> {}

/// `av_opt_freep_ranges`, as the policy of an owned [`OptionRanges`].
///
/// Thread-agnostic: it releases through `av_free`, which any thread may call,
/// so an [`OptionRangesBox`] is `Send` and `Sync`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct OptionRangesFree;

/// An owned range list, released with every range it lists by
/// `av_opt_freep_ranges`.
///
/// Not `Clone`: C has no routine that copies a list.
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::opt::OptionRangesBox;
/// fn copy(b: &OptionRangesBox) -> OptionRangesBox {
///     b.clone()
/// }
/// ```
pub type OptionRangesBox = CBox<OptionRanges, OptionRangesFree>;

/// Wraps: av_opt_freep_ranges
/// Handwritten
///
/// `impl_cdrop!` would need a raw-pointer adapter only to pass the list's
/// slot; the trait method holds the slot itself.
// SAFETY: `av_opt_freep_ranges` releases a list `av_opt_query_ranges` or
// `av_opt_query_ranges_default` allocated: each of its `nb_ranges *
// nb_components` ranges (and their `str`), the `range` array, then the list,
// exactly once. No safe path changes the counts or the array it reads.
unsafe impl CDrop<OptionRanges> for OptionRangesFree {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<OptionRanges>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVOptionRanges>();
        // SAFETY: the caller hands over a live list whose claim it owns;
        // `av_opt_freep_ranges` releases it once and NULLs only our local
        // slot.
        unsafe { ffi::av_opt_freep_ranges(&mut slot) }
    }
}

/// `AVERROR_BUG`: `FFERRTAG('B', 'U', 'G', '!')`.
pub(crate) const AVERROR_BUG: i32 = -i32::from_le_bytes(*b"BUG!");

impl OptionRanges {
    /// Wraps: av_opt_query_ranges
    ///
    /// The allowed ranges of option `key` of `obj`, from its class's
    /// `query_ranges` callback, or as [`query_default`](Self::query_default)
    /// builds them when the class has none. `flags` combines
    /// `AV_OPT_SEARCH_*` with `AV_OPT_MULTI_COMPONENT_RANGE`; without the
    /// latter, the list has one component. Fails with the callback's
    /// `AVERROR` code: the default one returns `AVERROR(ENOMEM)` for an
    /// unknown key as for an allocation failure, and `AVERROR(ENOSYS)` for a
    /// type with no ranges.
    ///
    /// # Safety
    ///
    /// `obj` must address a live `AVClass`-enabled object (a struct whose
    /// first member is a non-NULL `const AVClass *`) or, when `flags` has
    /// `AV_OPT_SEARCH_FAKE_OBJ`, a live non-NULL `const AVClass *`. The class
    /// must be well-formed: a NULL or terminated option table, child
    /// iteration callbacks valid for `obj` when `flags` has
    /// `AV_OPT_SEARCH_CHILDREN`, and a `query_ranges` callback, if any, that
    /// honours `av_opt_query_ranges`' contract for `obj`. No other thread may
    /// write `obj` during the call. The result borrows nothing from `obj`.
    pub unsafe fn query(obj: *mut c_void, key: &CStr, flags: i32) -> Result<OptionRangesBox, i32> {
        let mut ranges = core::ptr::null_mut();
        // SAFETY: `ranges` is a writable slot, `key` a live NUL-terminated
        // string, and the caller vouches for `obj` and its class.
        let ret = unsafe { ffi::av_opt_query_ranges(&mut ranges, obj, key.as_ptr(), flags) };
        if ret < 0 {
            // On failure the list is not the caller's: the default query
            // stores NULL, and a class callback keeps what it allocated.
            return Err(ret);
        }
        // SAFETY: on success `ranges` is a fresh list the caller owns,
        // released by `av_opt_freep_ranges`; `av_opt_query_ranges`
        // dereferences it before returning, so NULL is a broken class
        // callback, reported as a bug.
        unsafe { OptionRangesBox::from_c(ranges) }.ok_or(AVERROR_BUG)
    }

    /// Wraps: av_opt_query_ranges_default
    ///
    /// The allowed ranges of option `key` of `obj`, built from the option's
    /// `min` / `max` and type without the class's `query_ranges` callback:
    /// one range of one component. `flags` is as for [`query`](Self::query),
    /// and so are the errors.
    ///
    /// # Safety
    ///
    /// As [`query`](Self::query), except that `obj` itself, or the class
    /// pointer it addresses, may be NULL (the query then fails with
    /// `AVERROR(ENOMEM)` without reading further), and the class's
    /// `query_ranges` callback is not called.
    pub unsafe fn query_default(
        obj: *mut c_void,
        key: &CStr,
        flags: i32,
    ) -> Result<OptionRangesBox, i32> {
        let mut ranges = core::ptr::null_mut();
        // SAFETY: as for `query`.
        let ret =
            unsafe { ffi::av_opt_query_ranges_default(&mut ranges, obj, key.as_ptr(), flags) };
        if ret < 0 {
            // On failure the default query stores NULL.
            return Err(ret);
        }
        // SAFETY: on success `ranges` is a fresh, non-NULL list the caller
        // owns, released by `av_opt_freep_ranges`.
        unsafe { OptionRangesBox::from_c(ranges) }.ok_or(AVERROR_BUG)
    }
}

impl<'a> OptionRangesRef<'a> {
    /// Field: AVOptionRanges.nb_ranges
    ///
    /// The number of ranges per component.
    pub fn range_count(&self) -> usize {
        // SAFETY: a read of a projected field through the shared handle.
        let n = unsafe { addr_of!((*self.as_ptr()).nb_ranges).read() };
        usize::try_from(n).unwrap_or(0)
    }

    /// Field: AVOptionRanges.nb_components
    ///
    /// The number of components: 1 from the default ranges, and whatever a
    /// class's `query_ranges` callback returned for a query with
    /// `AV_OPT_MULTI_COMPONENT_RANGE`, such as 3 for an `IMAGE_SIZE` (pixel
    /// count, width, height); 1 without that flag, whatever the callback
    /// built.
    pub fn component_count(&self) -> usize {
        // SAFETY: a read of a projected field through the shared handle.
        let n = unsafe { addr_of!((*self.as_ptr()).nb_components).read() };
        usize::try_from(n).unwrap_or(0)
    }

    /// Field: AVOptionRanges.range
    ///
    /// Every range, component-major: component `c`'s range `i` is element
    /// `c * range_count() + i`, as [`get`](Self::get) indexes. An element is
    /// `None` when a class callback left it NULL.
    pub fn ranges(&self) -> RangeIter<'a> {
        RangeIter {
            list: *self,
            front: 0,
            back: self.slots().len(),
        }
    }

    /// Field: AVOptionRanges.range
    ///
    /// Range `index` of component `component`; `None` when either is out of
    /// bounds, or when a class callback left the element NULL.
    pub fn get(&self, index: usize, component: usize) -> Option<OptionRangeRef<'a>> {
        if index >= self.range_count() || component >= self.component_count() {
            return None;
        }
        self.slot(component * self.range_count() + index)?
    }

    /// Element `i` of the `range` array: `None` out of bounds, `Some(None)`
    /// for a NULL element.
    fn slot(&self, i: usize) -> Option<Option<OptionRangeRef<'a>>> {
        let range = self.slots().elem(i)?;
        // SAFETY: a non-NULL element addresses a range the list owns, live
        // and initialised for `'a`; nothing writes it meanwhile, since
        // `OptionRangeMut` needs the list's exclusive handle.
        Some(unsafe { OptionRangeRef::from_ptr(range.cast_mut()) })
    }

    /// The `range` array, `nb_ranges * nb_components` elements; empty when
    /// it is NULL or a count is not positive.
    fn slots(&self) -> CSlice<'a, *const ffi::AVOptionRange> {
        let len = self
            .range_count()
            .checked_mul(self.component_count())
            .unwrap_or(0);
        // SAFETY: a read of a projected field through the shared handle.
        let range = unsafe { addr_of!((*self.as_ptr()).range).read() };
        match NonNull::new(range.cast::<*const ffi::AVOptionRange>()) {
            // SAFETY: a list's `range` array holds `nb_ranges *
            // nb_components` initialised pointers, as `av_opt_freep_ranges`
            // relies on, and the list owns it for `'a`; no safe path writes
            // it. `*mut` and `*const` share a layout.
            Some(p) if len > 0 => unsafe { CSlice::from_raw_parts(p, len) },
            _ => {
                // SAFETY: an empty run reads nothing; a dangling pointer is
                // aligned and non-NULL.
                unsafe { CSlice::from_raw_parts(NonNull::dangling(), 0) }
            }
        }
    }
}

impl OptionRangesMut<'_> {
    /// Field: AVOptionRanges.range
    ///
    /// Range `index` of component `component`, to rewrite; `None` as for
    /// [`OptionRangesRef::get`]. The list's shape stays read-only.
    pub fn get_mut(&mut self, index: usize, component: usize) -> Option<OptionRangeMut<'_>> {
        let range = self.as_ref().get(index, component)?.as_ptr();
        // SAFETY: `range` addresses a live range the list owns, which only
        // this exclusive borrow of the list reaches for its lifetime: a
        // fresh list's elements are distinct allocations, as
        // `av_opt_freep_ranges` frees each one.
        unsafe { OptionRangeMut::from_ptr(range.cast_mut()) }
    }
}

define_ctype!(
    /// Wraps: AVOptionRange
    ///
    /// One allowed range of values, or one allowed value, of an option
    /// component: an element of an [`OptionRanges`] list, which owns it and
    /// its string. No C routine allocates, copies or frees a range alone, so
    /// it is only ever borrowed from its list: as an [`OptionRangeRef`] from
    /// [`OptionRangesRef::get`] or [`OptionRangesRef::ranges`], and as an
    /// [`OptionRangeMut`] from [`OptionRangesMut::get_mut`].
    OptionRange,
    OptionRangeRef,
    OptionRangeMut,
    ffi::AVOptionRange
);

// SAFETY: a range is four doubles, an `int` and `str`, NULL or an
// `av_malloc`-family string the range owns, which `av_opt_freep_ranges` (and
// `set_label` / a dropped `take_label` result) releases through `av_free`,
// which any thread may call. Nothing in it is tied to a thread, so an
// exclusive handle moves between threads as a `&mut (f64, Box<CStr>)` does.
unsafe impl Send for OptionRange {}
// SAFETY: as for `OptionRange`.
unsafe impl Send for OptionRangeMut<'_> {}

// SAFETY: the range has no interior mutability and C writes no range after
// its query returns; the only writers are `OptionRangeMut`'s setters, which
// need the exclusive handle, so concurrent shared access reads unchanging
// bytes.
unsafe impl Sync for OptionRange {}
// SAFETY: as for `Sync for OptionRange`: copies of a shared handle only read.
unsafe impl Send for OptionRangeRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for OptionRangeRef<'_> {}
// SAFETY: `&OptionRangeMut` reaches the range only through `as_ref`, a shared
// handle; every setter takes `&mut self`.
unsafe impl Sync for OptionRangeMut<'_> {}

impl<'a> OptionRangeRef<'a> {
    /// Field: AVOptionRange.str
    ///
    /// The range's string form, when its producer gave one; `None` for every
    /// range of the default query, which leaves it NULL.
    pub fn label(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let label = unsafe { addr_of!((*self.as_ptr()).str_).read() };
        // SAFETY: `str` is NULL or a NUL-terminated string the range owns,
        // freed only with the range or through `OptionRangeMut`, which cannot
        // coexist with a shared borrow of the list for `'a`.
        unsafe { opt_c_str(label) }
    }

    /// Field: AVOptionRange.value_min
    ///
    /// The smallest allowed value, or the value when [`is_range`] is
    /// `false`. For a string option it is the minimum length, for an image
    /// size the minimum pixel count, width or height, by component.
    ///
    /// [`is_range`]: Self::is_range
    pub fn value_min(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).value_min).read() }
    }

    /// Field: AVOptionRange.value_max
    ///
    /// The largest allowed value, as for [`value_min`](Self::value_min).
    pub fn value_max(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).value_max).read() }
    }

    /// Field: AVOptionRange.component_min
    ///
    /// The smallest allowed value of one component of the value; for a
    /// string option, the smallest allowed code point.
    pub fn component_min(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).component_min).read() }
    }

    /// Field: AVOptionRange.component_max
    ///
    /// The largest allowed value of one component, as for
    /// [`component_min`](Self::component_min): 127 limits a string to ASCII.
    pub fn component_max(&self) -> f64 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).component_max).read() }
    }

    /// Field: AVOptionRange.is_range
    ///
    /// Whether the range spans [`value_min`](Self::value_min) to
    /// [`value_max`](Self::value_max) rather than naming the single value
    /// `value_min`: C's flag is non-zero.
    pub fn is_range(&self) -> bool {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).is_range).read() != 0 }
    }
}

impl OptionRangeMut<'_> {
    /// Field: AVOptionRange.str
    ///
    /// Replaces the range's string with `label`, or clears it with `None`,
    /// releasing the previous one; the list releases the new one with
    /// itself.
    pub fn set_label(&mut self, label: Option<HeapString>) {
        drop(self.take_label());
        let new = label.map_or(core::ptr::null_mut(), HeapString::into_raw);
        // SAFETY: a write of a projected field through the exclusive handle;
        // `new` is NULL or an owned `av_malloc`-family string, which the list
        // now owns and `av_opt_freep_ranges` releases with `av_freep`.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).str_).write(new) };
    }

    /// Field: AVOptionRange.str
    ///
    /// Takes the range's string out of it, leaving it without one; `None`
    /// when it has none.
    pub fn take_label(&mut self) -> Option<HeapString> {
        let range = self.as_mut_ptr();
        // SAFETY: a read, then a write, of a projected field through the
        // exclusive handle.
        let old = unsafe { addr_of_mut!((*range).str_).replace(core::ptr::null()) };
        // SAFETY: `old` is NULL or the `av_malloc`-family string the range
        // owned, as `av_opt_freep_ranges` requires of every range, whose claim
        // the NULL just stored hands over: neither the list nor C frees it
        // again.
        unsafe { HeapString::from_raw(old.cast_mut()) }
    }

    /// Field: AVOptionRange.value_min
    ///
    /// Sets the smallest allowed value, or the value of a single-value range.
    pub fn set_value_min(&mut self, value: f64) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).value_min).write(value) }
    }

    /// Field: AVOptionRange.value_max
    ///
    /// Sets the largest allowed value.
    pub fn set_value_max(&mut self, value: f64) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).value_max).write(value) }
    }

    /// Field: AVOptionRange.component_min
    ///
    /// Sets the smallest allowed value of one component.
    pub fn set_component_min(&mut self, value: f64) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).component_min).write(value) }
    }

    /// Field: AVOptionRange.component_max
    ///
    /// Sets the largest allowed value of one component.
    pub fn set_component_max(&mut self, value: f64) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).component_max).write(value) }
    }

    /// Field: AVOptionRange.is_range
    ///
    /// Makes the range span `value_min` to `value_max` (`true`, stored as 1)
    /// or name the single value `value_min` (`false`, stored as 0).
    pub fn set_is_range(&mut self, is_range: bool) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).is_range).write(c_int::from(is_range)) }
    }
}

/// The ranges of an [`OptionRanges`] list in order, from
/// [`OptionRangesRef::ranges`]: a handle for each element, `None` for a NULL
/// one.
#[derive(Clone, Copy)]
pub struct RangeIter<'a> {
    list: OptionRangesRef<'a>,
    front: usize,
    back: usize,
}

impl<'a> Iterator for RangeIter<'a> {
    type Item = Option<OptionRangeRef<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.front += 1;
        self.list.slot(self.front - 1)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}

impl DoubleEndedIterator for RangeIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.list.slot(self.back)
    }
}

impl ExactSizeIterator for RangeIter<'_> {}

impl fmt::Debug for RangeIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RangeIter")
            .field("remaining", &(self.back - self.front))
            .finish()
    }
}

impl core::iter::FusedIterator for RangeIter<'_> {}

#[cfg(test)]
#[path = "../tests/common/opt.rs"]
pub(crate) mod fixture;

#[cfg(test)]
mod unit_tests {
    use super::fixture::{self, CLASS, Ctx, FAKE, NAMED, OPTIONS};
    use super::*;
    use core::ffi::{c_int, c_uint};
    use core::mem::size_of;

    use ffi::AVOptionType as T;

    /// A shared handle to entry `i` of the fixture's static table.
    fn entry(i: usize) -> OptionDefRef<'static> {
        // SAFETY: `OPTIONS` is a live, immutable static table.
        unsafe { OptionDefRef::from_ptr((&raw const OPTIONS.0[i]).cast_mut()) }.unwrap()
    }

    /// Asserts the default `opt` reads as `expected`, through a handle
    /// borrowed for the call.
    #[track_caller]
    fn assert_default(mut opt: ffi::AVOption, expected: OptionDefault<'_>) {
        // SAFETY: `opt` is a live local that outlives the handle.
        let o = unsafe { OptionDefRef::from_ptr(&mut opt) }.unwrap();
        assert_eq!(o.default_value(), expected, "{:?}", o.option_type());
    }

    fn option(ty: T, default_val: ffi::AVOption__bindgen_ty_1) -> ffi::AVOption {
        ffi::AVOption {
            name: c"x".as_ptr(),
            help: core::ptr::null(),
            offset: 0,
            type_: ty,
            default_val,
            min: 0.0,
            max: 0.0,
            flags: 0,
            unit: core::ptr::null(),
        }
    }

    #[test]
    fn getters_read_the_table_entry() {
        let o = entry(0);
        assert_eq!(o.name(), Some(c"int"));
        assert_eq!(o.help(), Some(c"an int"));
        assert_eq!(o.unit(), Some(c"mode"));
        assert_eq!(o.offset(), core::mem::offset_of!(Ctx, int) as i32);
        assert_eq!(o.option_type(), OptionType::INT);
        assert_eq!((o.min(), o.max()), (-10.0, 100.0));
        assert_eq!(o.flags(), 0);
        assert_eq!(o.default_value(), OptionDefault::Int(7));

        let bare = named(c"bare");
        assert_eq!((bare.help(), bare.unit()), (None, None));

        // The terminator reads as empty rather than through NULL.
        let end = entry(NAMED);
        assert_eq!((end.name(), end.help(), end.unit()), (None, None, None));
        // Its zero type is no type libavutil reads a default for.
        assert_eq!(end.default_value(), OptionDefault::Unsupported);
    }

    #[test]
    fn strings_outlive_the_handle_they_came_from() {
        let name = {
            let o = entry(1);
            o.name()
        };
        assert_eq!(name, Some(c"int64"));
    }

    #[test]
    fn default_is_read_through_the_member_its_type_selects() {
        let ints = [
            T::AV_OPT_TYPE_BOOL,
            T::AV_OPT_TYPE_FLAGS,
            T::AV_OPT_TYPE_INT,
            T::AV_OPT_TYPE_UINT,
            T::AV_OPT_TYPE_INT64,
            T::AV_OPT_TYPE_UINT64,
            T::AV_OPT_TYPE_DURATION,
            T::AV_OPT_TYPE_PIXEL_FMT,
            T::AV_OPT_TYPE_SAMPLE_FMT,
            T::AV_OPT_TYPE_CONST,
        ];
        for ty in ints {
            let o = option(ty, ffi::AVOption__bindgen_ty_1 { i64_: -3 });
            assert_default(o, OptionDefault::Int(-3));
        }
        for ty in [
            T::AV_OPT_TYPE_DOUBLE,
            T::AV_OPT_TYPE_FLOAT,
            T::AV_OPT_TYPE_RATIONAL,
        ] {
            let o = option(ty, ffi::AVOption__bindgen_ty_1 { dbl: 0.75 });
            assert_default(o, OptionDefault::Double(0.75));
        }
        let strings = [
            T::AV_OPT_TYPE_STRING,
            T::AV_OPT_TYPE_COLOR,
            T::AV_OPT_TYPE_IMAGE_SIZE,
            T::AV_OPT_TYPE_VIDEO_RATE,
            T::AV_OPT_TYPE_BINARY,
            T::AV_OPT_TYPE_CHLAYOUT,
            T::AV_OPT_TYPE_DICT,
        ];
        for ty in strings {
            let o = option(
                ty,
                ffi::AVOption__bindgen_ty_1 {
                    str_: c"s".as_ptr(),
                },
            );
            assert_default(o, OptionDefault::Str(Some(c"s")));
            let o = option(
                ty,
                ffi::AVOption__bindgen_ty_1 {
                    str_: core::ptr::null(),
                },
            );
            assert_default(o, OptionDefault::Str(None));
        }

        // Any type with the array flag reads `arr`, NULL included.
        let def = ffi::AVOptionArrayDef {
            def: c"1".as_ptr(),
            size_min: 0,
            size_max: 0,
            sep: 0,
        };
        for base in [
            T::AV_OPT_TYPE_INT,
            T::AV_OPT_TYPE_STRING,
            T::AV_OPT_TYPE_DOUBLE,
        ] {
            let ty = T(base.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0);
            let o = option(ty, ffi::AVOption__bindgen_ty_1 { arr: &def });
            assert_default(o, OptionDefault::Array(&def));
            let o = option(
                ty,
                ffi::AVOption__bindgen_ty_1 {
                    arr: core::ptr::null(),
                },
            );
            assert_default(o, OptionDefault::Array(core::ptr::null()));
        }

        // A type outside the enum reads no member: a pointer-sized garbage
        // `str` would otherwise be dereferenced.
        for ty in [T(0), T(99), T(T::AV_OPT_TYPE_UINT.0 + 1)] {
            let o = option(ty, ffi::AVOption__bindgen_ty_1 { i64_: 0x10 });
            assert_default(o, OptionDefault::Unsupported);
        }
    }

    #[test]
    fn handles_from_av_opt_next_and_av_opt_find_are_the_table_entries() {
        let mut prev = core::ptr::null();
        let mut seen = 0;
        loop {
            // SAFETY: `FAKE` is a `const AVClass *` to a well-formed class;
            // `prev` is NULL or the previous result.
            let next = unsafe { ffi::av_opt_next(FAKE.as_obj(), prev) };
            // SAFETY: `next` is NULL or an entry of the static table.
            let Some(o) = (unsafe { OptionDefRef::from_ptr(next.cast_mut()) }) else {
                break;
            };
            assert_eq!(o.as_ptr(), &raw const OPTIONS.0[seen]);
            assert_eq!(o.name(), entry(seen).name());
            prev = next;
            seen += 1;
        }
        assert_eq!(seen, NAMED);

        for (name, unit) in [
            (c"dbl", None),
            (c"seven", Some(c"mode")),
            (c"bit", Some(c"fl")),
        ] {
            let flags = ffi::AV_OPT_SEARCH_FAKE_OBJ as i32;
            let unit_ptr = unit.map_or(core::ptr::null(), CStr::as_ptr);
            // SAFETY: as above; `name` and `unit_ptr` are NULL or live strings.
            let found =
                unsafe { ffi::av_opt_find(FAKE.as_obj(), name.as_ptr(), unit_ptr, 0, flags) };
            // SAFETY: `found` is NULL or an entry of the static table.
            let o = unsafe { OptionDefRef::from_ptr(found.cast_mut()) }.unwrap();
            assert_eq!((o.name(), o.unit()), (Some(name), unit.or(o.unit())));
            if unit.is_some() {
                assert_eq!(o.option_type(), OptionType::CONST);
            }
        }
    }

    /// The table entry named `name`.
    fn named(name: &CStr) -> OptionDefRef<'static> {
        (0..NAMED)
            .map(entry)
            .find(|o| o.name() == Some(name))
            .unwrap()
    }

    fn int(name: &CStr) -> i64 {
        match named(name).default_value() {
            OptionDefault::Int(v) => v,
            d => panic!("{name:?}: {d:?}"),
        }
    }

    fn dbl(name: &CStr) -> f64 {
        match named(name).default_value() {
            OptionDefault::Double(v) => v,
            d => panic!("{name:?}: {d:?}"),
        }
    }

    fn text(name: &CStr) -> Option<&'static CStr> {
        match named(name).default_value() {
            OptionDefault::Str(s) => s,
            d => panic!("{name:?}: {d:?}"),
        }
    }

    /// An object of `CLASS` with the defaults of the options whose flags
    /// masked by `fixture::GROWS` equal `grows` set by C, which is C's
    /// reading of `default_val`; freed with `av_opt_free` on drop.
    struct Defaults(Ctx);

    impl Defaults {
        fn new(grows: c_int) -> Self {
            let mut d = Defaults(Ctx::new(&CLASS));
            // SAFETY: `d.0` is a zeroed object of the well-formed `CLASS`.
            unsafe { ffi::av_opt_set_defaults2(d.0.as_obj(), fixture::GROWS, grows) };
            d
        }
    }

    impl Drop for Defaults {
        fn drop(&mut self) {
            // SAFETY: `self.0` is the object `av_opt_set_defaults2` filled;
            // this frees what it allocated.
            unsafe { ffi::av_opt_free(self.0.as_obj()) };
        }
    }

    /// Every value `av_opt_set_defaults` stores follows from the variant
    /// `default_value` returns.
    #[test]
    fn defaults_agree_with_av_opt_set_defaults() {
        let d = Defaults::new(0);
        let ctx = &d.0;
        assert_eq!(i64::from(ctx.int), int(c"int"));
        assert_eq!(ctx.int64, int(c"int64"));
        assert_eq!(ctx.uint64 as i64, int(c"uint64"));
        assert_eq!(ctx.dbl, dbl(c"dbl"));
        assert_eq!(f64::from(ctx.flt), dbl(c"flt"));
        assert_eq!(f64::from(ctx.q.num) / f64::from(ctx.q.den), dbl(c"q"));
        assert_eq!(i64::from(ctx.boolean), int(c"boolean"));
        assert_eq!(i64::from(ctx.flags), int(c"flags"));
        assert_eq!(ctx.duration, int(c"duration"));
        assert_eq!(i64::from(ctx.bare), int(c"bare"));
        // SAFETY: `av_opt_set_defaults2` stored an `av_strdup` copy of the
        // default.
        let string = unsafe { CStr::from_ptr(ctx.string) };
        assert_eq!(Some(string), text(c"string"));
        assert!(ctx.nostr.is_null() && text(c"nostr").is_none());
        assert_eq!(text(c"size"), Some(c"640x480"));
        assert_eq!(ctx.size, [640, 480]);
        assert_eq!(text(c"rate"), Some(c"25"));
        assert_eq!((ctx.rate.num, ctx.rate.den), (25, 1));
        assert_eq!(text(c"color"), Some(c"red"));
        assert_eq!(ctx.color, [255, 0, 0, 255]);
        assert_eq!(text(c"binary"), Some(c"0a0b"));
        assert_eq!(ctx.binary_len, 2);
        // SAFETY: `binary` holds the `binary_len` bytes decoded from the hex
        // default.
        let binary = unsafe { core::slice::from_raw_parts(ctx.binary, 2) };
        assert_eq!(binary, [0x0a, 0x0b]);
        // The options C grows a block for were left alone.
        assert!(ctx.dict.is_null() && ctx.array.is_null());
    }

    /// As above, for the options whose defaults C builds by growing an
    /// `av_realloc` block.
    #[test]
    fn grown_defaults_agree_with_av_opt_set_defaults() {
        let d = Defaults::new(fixture::GROWS);
        let ctx = &d.0;
        assert_eq!(text(c"dict"), Some(c"k=v"));
        assert!(!ctx.dict.is_null());

        let OptionDefault::Array(def) = named(c"array").default_value() else {
            panic!()
        };
        assert_eq!(def, &raw const fixture::ARRAY_DEF.0);
        assert_eq!(ctx.array_len, 2 as c_uint);
        // SAFETY: `array` holds `array_len` elements parsed from `ARRAY_DEF`.
        let array = unsafe { core::slice::from_raw_parts(ctx.array, 2) };
        assert_eq!(array, [1, 2]);
        let OptionDefault::Array(def) = named(c"empty_array").default_value() else {
            panic!()
        };
        assert!(def.is_null());
        assert!(ctx.empty_array.is_null() && ctx.empty_array_len == 0);
        // The other options were left alone.
        assert!(ctx.int == 0 && ctx.string.is_null());
    }

    /// The option types, their handles and a read default cross threads; a
    /// shared handle is read from several at once.
    #[test]
    fn options_and_defaults_are_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<OptionDef>();
        send_sync::<OptionDefRef<'static>>();
        send_sync::<OptionDefMut<'static>>();
        send_sync::<OptionDefault<'static>>();
        send_sync::<OptionRanges>();
        send_sync::<OptionRangesRef<'static>>();
        send_sync::<OptionRangesMut<'static>>();
        send_sync::<OptionRangesBox>();

        let o = entry(0);
        let default = o.default_value();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..64 {
                        assert_eq!(o.name(), Some(c"int"));
                        assert_eq!(o.default_value(), default);
                        assert_eq!(
                            named(c"string").default_value(),
                            OptionDefault::Str(text(c"string"))
                        );
                    }
                });
            }
            // Another thread's copy of the handle keeps borrowing the table.
            let sent = s.spawn(move || named(c"q")).join().unwrap();
            assert_eq!(sent.default_value(), OptionDefault::Double(0.5));
        });
    }

    #[test]
    fn ranges_policy_is_a_zero_sized_null_niche() {
        assert_eq!(size_of::<OptionRangesFree>(), 0);
        assert_eq!(size_of::<OptionRangesBox>(), size_of::<*mut c_void>());
        assert_eq!(
            size_of::<Option<OptionRangesBox>>(),
            size_of::<*mut c_void>()
        );
        assert_eq!(format!("{OptionRangesFree:?}"), "OptionRangesFree");
    }

    #[test]
    fn averror_bug_is_c_s_tag() {
        // FFERRTAG('B', 'U', 'G', '!') in error.h.
        assert_eq!(AVERROR_BUG, -0x2147_5542);
    }

    #[test]
    fn raw_query_result_adopts_and_returns_to_c() {
        let mut raw = core::ptr::null_mut();
        let flags = ffi::AV_OPT_SEARCH_FAKE_OBJ as i32;
        // SAFETY: `raw` is a writable slot, `FAKE` a fake object of a
        // well-formed class.
        let ret =
            unsafe { ffi::av_opt_query_ranges(&mut raw, FAKE.as_obj(), c"int".as_ptr(), flags) };
        assert_eq!(ret, 1);
        // SAFETY: `raw` is a fresh list we own, released by
        // `av_opt_freep_ranges`.
        let b = unsafe { OptionRangesBox::from_c(raw) }.unwrap();
        assert_eq!(b.as_ref().range_count(), 1);
        let mut back = b.into_c();
        assert_eq!(back, raw);
        // C frees what Rust handed back.
        // SAFETY: `back` is the owned list; C releases it once.
        unsafe { ffi::av_opt_freep_ranges(&mut back) };
        assert!(back.is_null());

        // SAFETY: as above.
        let ret = unsafe {
            ffi::av_opt_query_ranges_default(&mut raw, FAKE.as_obj(), c"q".as_ptr(), flags)
        };
        assert_eq!(ret, 1);
        // The policy alone releases it.
        // SAFETY: `raw` is a fresh list we own.
        unsafe { OptionRangesFree.c_drop(NonNull::new(raw.cast()).unwrap()) };
    }

    /// A list laid out in Rust memory, as a class callback would build one,
    /// reached only through the raw pointer its elements were linked from:
    /// a `Box` or `&mut` retag of it would invalidate them.
    struct Local(*mut LocalData);

    struct LocalData {
        ranges: [ffi::AVOptionRange; 6],
        ptrs: [*mut ffi::AVOptionRange; 6],
        list: ffi::AVOptionRanges,
    }

    impl Local {
        /// `nb_ranges * nb_components` of six ranges, element `hole` NULL.
        fn new(nb_ranges: i32, nb_components: i32, hole: Option<usize>) -> Self {
            let range = |i: usize| ffi::AVOptionRange {
                str_: core::ptr::null(),
                value_min: i as f64,
                value_max: i as f64 + 0.5,
                component_min: 0.0,
                component_max: 1.0,
                is_range: 1,
            };
            let p = Box::into_raw(Box::new(LocalData {
                ranges: core::array::from_fn(range),
                ptrs: [core::ptr::null_mut(); 6],
                list: ffi::AVOptionRanges {
                    range: core::ptr::null_mut(),
                    nb_ranges,
                    nb_components,
                },
            }));
            for i in (0..6).filter(|&i| Some(i) != hole) {
                // SAFETY: `p` is a fresh allocation owned here.
                unsafe { (*p).ptrs[i] = &raw mut (*p).ranges[i] };
            }
            // SAFETY: as above.
            unsafe { (*p).list.range = (&raw mut (*p).ptrs).cast() };
            Local(p)
        }

        /// The same list with a NULL `range` array.
        fn without_array(self) -> Self {
            // SAFETY: `self.0` is the live allocation owned by `self`.
            unsafe { (*self.0).list.range = core::ptr::null_mut() };
            self
        }

        fn handle(&self) -> OptionRangesRef<'_> {
            // SAFETY: `list` is live and initialised for the borrow of `self`,
            // and nothing writes it meanwhile.
            unsafe { OptionRangesRef::from_ptr(&raw mut (*self.0).list) }.unwrap()
        }
    }

    impl Drop for Local {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from `Box::into_raw` and is released once.
            drop(unsafe { Box::from_raw(self.0) });
        }
    }

    /// Reads the value bounds of `r`.
    fn bounds(r: OptionRangeRef<'_>) -> (f64, f64) {
        (r.value_min(), r.value_max())
    }

    #[test]
    fn ranges_index_component_major() {
        let l = Local::new(2, 3, None);
        let r = l.handle();
        assert_eq!((r.range_count(), r.component_count()), (2, 3));
        let all = r.ranges();
        assert_eq!(all.len(), 6);
        assert_eq!(
            all.map(|e| bounds(e.unwrap()).0).collect::<Vec<_>>(),
            [0., 1., 2., 3., 4., 5.]
        );
        // Component `c`'s range `i` is element `c * nb_ranges + i`.
        for c in 0..3 {
            for i in 0..2 {
                assert_eq!(bounds(r.get(i, c).unwrap()).0, (c * 2 + i) as f64);
            }
        }
        assert!(r.get(2, 0).is_none() && r.get(0, 3).is_none() && r.get(usize::MAX, 0).is_none());
    }

    #[test]
    fn ranges_reject_inconsistent_counts() {
        // Negative counts read as zero, so the run is empty.
        let l = Local::new(-1, 3, None);
        let r = l.handle();
        assert_eq!((r.range_count(), r.ranges().len()), (0, 0));
        assert!(r.get(0, 0).is_none());
        let l = Local::new(2, -7, None);
        assert_eq!(
            (l.handle().component_count(), l.handle().ranges().len()),
            (0, 0)
        );

        // A NULL array is empty whatever the counts claim.
        let l = Local::new(2, 3, None).without_array();
        let r = l.handle();
        assert_eq!(r.ranges().len(), 0);
        assert!(r.ranges().next().is_none());
        assert!(r.get(0, 0).is_none());

        // An empty list owns no array.
        let l = Local::new(0, 1, None).without_array();
        assert_eq!(l.handle().ranges().len(), 0);
    }

    #[test]
    fn null_elements_pass_through() {
        let l = Local::new(3, 1, Some(1));
        let r = l.handle();
        // `get` reads a NULL element as no range, and so does the iterator.
        assert!(r.get(1, 0).is_none());
        assert!(r.get(0, 0).is_some() && r.get(2, 0).is_some());
        let holes: Vec<bool> = r.ranges().map(|e| e.is_none()).collect();
        assert_eq!(holes, [false, true, false]);
        assert_eq!(r.slot(1).map(|e| e.is_none()), Some(true));
        assert!(r.slot(3).is_none());
    }

    #[test]
    fn range_iter_is_double_ended_exact_and_fused() {
        let l = Local::new(3, 2, Some(4));
        let r = l.handle();
        let first = |e: Option<OptionRangeRef<'_>>| e.map(|e| e.value_min());
        let mut it = r.ranges();
        assert_eq!(it.size_hint(), (6, Some(6)));
        assert_eq!(it.next().map(first), Some(Some(0.0)));
        assert_eq!(it.next_back().map(first), Some(Some(5.0)));
        assert_eq!(it.next_back().map(first), Some(None));
        assert_eq!(it.len(), 3);
        // A copy resumes where the original stood.
        let copy = it;
        assert_eq!(
            copy.map(first).collect::<Vec<_>>(),
            [Some(1.0), Some(2.0), Some(3.0)]
        );
        assert_eq!(format!("{it:?}"), "RangeIter { remaining: 3 }");
        assert_eq!(it.by_ref().rev().filter_map(first).count(), 3);
        // Exhausted from either end, it stays so.
        assert!(it.next().is_none() && it.next_back().is_none() && it.next().is_none());
        assert_eq!(it.len(), 0);
        assert_eq!(
            r.ranges().rev().map(first).collect::<Vec<_>>(),
            [Some(5.0), None, Some(3.0), Some(2.0), Some(1.0), Some(0.0)]
        );
    }

    #[test]
    fn range_getters_read_every_field() {
        let l = Local::new(2, 1, None);
        let r = l.handle();
        let (a, b) = (r.get(0, 0).unwrap(), r.get(1, 0).unwrap());
        assert_eq!((a.value_min(), a.value_max()), (0.0, 0.5));
        assert_eq!((a.component_min(), a.component_max()), (0.0, 1.0));
        assert!(a.is_range() && b.is_range());
        assert_eq!(a.label(), None);
        // A non-zero flag other than 1 still reads as a range, and 0 as a
        // single value.
        for (raw, expected) in [(0, false), (1, true), (2, true), (-1, true)] {
            // SAFETY: `l.0` is the live allocation owned by `l`, and no
            // handle is in use across this write.
            unsafe { (*l.0).ranges[1].is_range = raw };
            assert_eq!(l.handle().get(1, 0).unwrap().is_range(), expected);
        }
        // SAFETY: as above; the literal outlives `l`.
        unsafe { (*l.0).ranges[0].str_ = c"label".as_ptr() };
        assert_eq!(l.handle().get(0, 0).unwrap().label(), Some(c"label"));
        // SAFETY: as above; the range no longer refers to the literal.
        unsafe { (*l.0).ranges[0].str_ = core::ptr::null() };
    }

    impl Local {
        fn handle_mut(&mut self) -> OptionRangesMut<'_> {
            // SAFETY: `list` is live and initialised for the exclusive borrow
            // of `self`.
            unsafe { OptionRangesMut::from_ptr(&raw mut (*self.0).list) }.unwrap()
        }
    }

    #[test]
    fn range_setters_write_every_field() {
        let mut l = Local::new(2, 2, Some(3));
        let mut m = l.handle_mut();
        assert!(m.get_mut(1, 1).is_none(), "a NULL element");
        assert!(m.get_mut(2, 0).is_none() && m.get_mut(0, 2).is_none());
        {
            let mut e = m.get_mut(1, 0).unwrap();
            e.set_value_min(-4.0);
            e.set_value_max(8.5);
            e.set_component_min(-1.0);
            e.set_component_max(0x10FFFF as f64);
            e.set_is_range(false);
            let r = e.as_ref();
            assert_eq!((r.value_min(), r.value_max()), (-4.0, 8.5));
            assert_eq!((r.component_min(), r.component_max()), (-1.0, 1_114_111.0));
            assert!(!r.is_range());
            e.set_is_range(true);
        }
        // The raw flag is C's 0 / 1.
        // SAFETY: `l.0` is live; the handles above are no longer used.
        assert_eq!(unsafe { (*l.0).ranges[1].is_range }, 1);
        let r = l.handle().get(1, 0).unwrap();
        assert_eq!(bounds(r), (-4.0, 8.5));
        // Its neighbours are untouched.
        assert_eq!(bounds(l.handle().get(0, 0).unwrap()), (0.0, 0.5));
        assert_eq!(bounds(l.handle().get(0, 1).unwrap()), (2.0, 2.5));
    }

    #[test]
    fn range_label_is_replaced_taken_and_released() {
        let mut l = Local::new(1, 1, None);
        let mut m = l.handle_mut();
        let mut e = m.get_mut(0, 0).unwrap();
        assert!(e.take_label().is_none());
        e.set_label(crate::libavutil::mem::string_from_c_str(c"first"));
        assert_eq!(e.as_ref().label(), Some(c"first"));
        // Replacing releases the old string (LeakSanitizer reports a miss).
        e.set_label(crate::libavutil::mem::string_from_c_str(c"second"));
        assert_eq!(e.as_ref().label(), Some(c"second"));
        let taken = e.take_label().unwrap();
        assert_eq!(taken.as_c_str(), c"second");
        assert_eq!(e.as_ref().label(), None);
        assert!(e.take_label().is_none());
        // Clearing an empty label is a no-op; clearing a set one frees it.
        e.set_label(None);
        e.set_label(Some(taken));
        e.set_label(None);
        // SAFETY: `l.0` is live; the handles above are no longer used.
        assert!(unsafe { (*l.0).ranges[0].str_ }.is_null());
    }

    #[test]
    fn ranges_and_their_handles_are_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<OptionRange>();
        send_sync::<OptionRangeRef<'static>>();
        send_sync::<OptionRangeMut<'static>>();
        send_sync::<RangeIter<'static>>();
        send_sync::<OptionType>();
    }

    /// Every named type, against its raw C enumerator.
    const TYPES: [(OptionType, T, &str); 21] = [
        (OptionType::FLAGS, T::AV_OPT_TYPE_FLAGS, "FLAGS"),
        (OptionType::INT, T::AV_OPT_TYPE_INT, "INT"),
        (OptionType::INT64, T::AV_OPT_TYPE_INT64, "INT64"),
        (OptionType::DOUBLE, T::AV_OPT_TYPE_DOUBLE, "DOUBLE"),
        (OptionType::FLOAT, T::AV_OPT_TYPE_FLOAT, "FLOAT"),
        (OptionType::STRING, T::AV_OPT_TYPE_STRING, "STRING"),
        (OptionType::RATIONAL, T::AV_OPT_TYPE_RATIONAL, "RATIONAL"),
        (OptionType::BINARY, T::AV_OPT_TYPE_BINARY, "BINARY"),
        (OptionType::DICT, T::AV_OPT_TYPE_DICT, "DICT"),
        (OptionType::UINT64, T::AV_OPT_TYPE_UINT64, "UINT64"),
        (OptionType::CONST, T::AV_OPT_TYPE_CONST, "CONST"),
        (
            OptionType::IMAGE_SIZE,
            T::AV_OPT_TYPE_IMAGE_SIZE,
            "IMAGE_SIZE",
        ),
        (
            OptionType::PIXEL_FORMAT,
            T::AV_OPT_TYPE_PIXEL_FMT,
            "PIXEL_FORMAT",
        ),
        (
            OptionType::SAMPLE_FORMAT,
            T::AV_OPT_TYPE_SAMPLE_FMT,
            "SAMPLE_FORMAT",
        ),
        (
            OptionType::VIDEO_RATE,
            T::AV_OPT_TYPE_VIDEO_RATE,
            "VIDEO_RATE",
        ),
        (OptionType::DURATION, T::AV_OPT_TYPE_DURATION, "DURATION"),
        (OptionType::COLOR, T::AV_OPT_TYPE_COLOR, "COLOR"),
        (OptionType::BOOL, T::AV_OPT_TYPE_BOOL, "BOOL"),
        (
            OptionType::CHANNEL_LAYOUT,
            T::AV_OPT_TYPE_CHLAYOUT,
            "CHANNEL_LAYOUT",
        ),
        (OptionType::UINT, T::AV_OPT_TYPE_UINT, "UINT"),
        (OptionType::ARRAY, T::AV_OPT_TYPE_FLAG_ARRAY, "ARRAY"),
    ];

    #[test]
    fn option_type_constants_are_the_c_enumerators() {
        assert_eq!(OptionType::KNOWN.len(), TYPES.len());
        for ((ty, raw, name), (known, known_name)) in TYPES.into_iter().zip(OptionType::KNOWN) {
            assert_eq!((ty, name), (known, known_name));
            assert_eq!(ty.value(), raw.0);
            assert_eq!(ffi::AVOptionType::from(ty), raw);
            assert_eq!(OptionType::from(raw), ty);
            assert_eq!(OptionType::from_value(raw.0), ty);
            assert_eq!(ty.name(), Some(name));
            assert_eq!(format!("{ty:?}"), format!("OptionType::{name}"));
        }
        // The base types are 1 through 20, the flag bit 16.
        assert_eq!(OptionType::FLAGS.value(), 1);
        assert_eq!(OptionType::UINT.value(), 20);
        assert_eq!(OptionType::ARRAY.value(), 1 << 16);
        assert_eq!(size_of::<OptionType>(), size_of::<ffi::AVOptionType>());
    }

    #[test]
    fn option_type_array_flag_round_trips() {
        for (ty, ..) in &TYPES[..20] {
            let arr = ty.array();
            assert!(!ty.is_array() && arr.is_array());
            assert_eq!((arr.base(), ty.base()), (*ty, *ty));
            assert_eq!(arr.array(), arr);
            assert_eq!(arr.value(), ty.value() | 0x1_0000);
            assert_eq!(arr.name(), None);
            assert_eq!(format!("{arr:?}"), format!("{ty:?} | OptionType::ARRAY"));
        }
        // The flag alone is an array of the unnamed type 0.
        assert!(OptionType::ARRAY.is_array());
        assert_eq!(OptionType::ARRAY.base().value(), 0);
        assert_eq!(OptionType::from_value(0).array(), OptionType::ARRAY);
    }

    #[test]
    fn option_type_keeps_unnamed_values() {
        for v in [0, 21, 99, 0xFFFF, 0x2_0000, u32::MAX] {
            let ty = OptionType::from_value(v);
            assert_eq!(ty.value(), v);
            assert_eq!(ty.name(), None, "{v:#x}");
            assert_eq!(format!("{ty:?}"), format!("OptionType({v})"));
            assert_eq!(ty.base().value(), v & !0x1_0000);
            assert_eq!(ty.is_array(), v & 0x1_0000 != 0);
        }
        // An unnamed base with the flag prints as a value.
        let odd = OptionType::from_value(99).array();
        assert_eq!(format!("{odd:?}"), format!("OptionType({})", 99 | 0x1_0000));
        // Hashing and equality follow the raw value.
        let set: std::collections::HashSet<_> = TYPES.iter().map(|t| t.0).collect();
        assert_eq!(set.len(), 21);
        assert!(set.contains(&OptionType::from_value(2)));
    }

    /// The type each fixture option reads as is the one its table stores.
    #[test]
    fn option_type_reads_the_table_entry() {
        for i in 0..=NAMED {
            let o = entry(i);
            let raw = OPTIONS.0[i].type_;
            assert_eq!(o.option_type().value(), raw.0);
        }
        let array = named(c"array").option_type();
        assert_eq!((array.base(), array.is_array()), (OptionType::INT, true));
        assert_eq!(named(c"bogus").option_type(), OptionType::from_value(99));
    }
}
