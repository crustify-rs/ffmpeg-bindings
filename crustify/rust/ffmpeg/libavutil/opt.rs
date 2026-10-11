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
//! [`OptionArrayDef`] is an array option's definition (`AVOptionArrayDef`):
//! its default, element count bounds and separator. C tables hold them as
//! `static const` values, so [`OptionDefault::Array`] borrows one as an
//! [`OptionArrayDefRef`]; a Rust-owned value, built by
//! [`OptionArrayDef::new`] and edited through an [`OptionArrayDefMut`], can
//! only point its default at a `'static` string.
//!
//! An `AVOptions`-enabled object, the `void *obj` the `av_opt_*` routines
//! read and write, is borrowed as an [`ObjectRef`] (getters: values by name,
//! formatted or numeric, serialization, children) or an [`ObjectMut`]
//! (setters: parsed or typed values, defaults, key-value strings, copies,
//! freeing). The objects themselves are wrapped by later layers; until then
//! a handle is adopted from a raw pointer under [`ObjectRef`]'s contract.
//! Several C setters and getters reach undefined behaviour for some values
//! (unchecked `double` to integer conversions, a scalar store over an array,
//! an out-of-bounds type name lookup); the wrappers refuse those values
//! before C runs, as each documents. [`parse_key_value`] reads one
//! `key=value` pair of an options string.
//!
//! Raw seams remain where the dependency is wrapped by a later batch: the
//! object a range query inspects (an `AVClass`-enabled `void *`).

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::fmt;
use core::marker::PhantomData;
use core::mem::size_of;
use core::num::NonZeroU32;
use core::ops::BitOr;
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use std::ffi::CString;

use ffibox::{CBox, CDrop, CSlice, CZeroable, define_ctype};

use super::ffi;
use super::log::ClassRef;
use super::mem::HeapString;
use super::{channel_layout, eval, parseutils};

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
///
/// Two defaults are equal when they are equal values, and two `Array`
/// defaults when they are the same definition (or both absent).
#[derive(Clone, Copy, Debug)]
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
    /// definition, borrowed from the table like the option; `None` when the
    /// option has none, which C reads as no default, no bounds and a comma
    /// separator.
    Array(Option<OptionArrayDefRef<'a>>),
    /// An option type libavutil reads no default for.
    Unsupported,
}

impl PartialEq for OptionDefault<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Double(a), Self::Double(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => match (a, b) {
                (Some(a), Some(b)) => a.ptr_eq(*b),
                (a, b) => a.is_none() && b.is_none(),
            },
            (Self::Unsupported, Self::Unsupported) => true,
            _ => false,
        }
    }
}

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
            let arr = unsafe { addr_of!((*val).default_val.arr).read() };
            // SAFETY: `arr` is NULL or addresses a definition of the table
            // (in C, a `static const AVOptionArrayDef`), which outlives every
            // handle to its entries and is never written; C reads it as such
            // in `av_opt_set_defaults` and `av_opt_set`.
            return OptionDefault::Array(unsafe { OptionArrayDefRef::from_ptr(arr.cast_mut()) });
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
    /// Wraps: AVOptionArrayDef
    ///
    /// The definition of an array option (`AV_OPT_TYPE_FLAG_ARRAY`), which
    /// its `AVOption.default_val.arr` points to: the default value as a
    /// string, the element count bounds and the separator `av_opt_set` and
    /// `av_opt_get` split and join elements with.
    ///
    /// Published definitions are `static const` and only borrowed, as an
    /// [`OptionArrayDefRef`] from [`OptionDefault::Array`]. A Rust-owned
    /// value starts from [`OptionArrayDef::new`]; its default can only be a
    /// `'static` string, so every handle's `def` is NULL or a string that
    /// outlives it. No C routine allocates, copies or frees a definition.
    OptionArrayDef,
    OptionArrayDefRef,
    OptionArrayDefMut,
    ffi::AVOptionArrayDef
);

// SAFETY: all-zero is a definition every accessor and C reader accepts: no
// default (`def` NULL, which `av_opt_set_defaults` skips), no minimum, no
// maximum, and the comma separator (`sep` 0).
unsafe impl CZeroable for OptionArrayDef {}

// SAFETY: a definition is integers, a `char` and `def`, which is NULL or a
// pointer to an immutable string of program lifetime (a C literal, or the
// `&'static CStr` a setter stored); nothing in it is tied to a thread, so a
// value or an exclusive handle moves between threads as a struct of
// `&'static CStr` and integers does.
unsafe impl Send for OptionArrayDef {}
// SAFETY: as for `OptionArrayDef`: the exclusive handle is the only access to
// the definition while it lives.
unsafe impl Send for OptionArrayDefMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: the setters need
// an `OptionArrayDefMut`, the struct has no interior mutability, and no C path
// writes a definition, so concurrent shared access reads unchanging bytes.
unsafe impl Sync for OptionArrayDef {}
// SAFETY: as for `Sync for OptionArrayDef`: copies of a shared handle only
// read.
unsafe impl Send for OptionArrayDefRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for OptionArrayDefRef<'_> {}
// SAFETY: `&OptionArrayDefMut` reaches the definition only through `as_ref`,
// a shared handle; the setters need `&mut`.
unsafe impl Sync for OptionArrayDefMut<'_> {}

/// Why [`OptionArrayDefMut::set_separator`] refused a separator: one that
/// `libavutil/opt.h` does not allow, and that `av_opt_set_defaults` aborts on
/// when alphanumeric or a backslash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidSeparator(pub u8);

impl fmt::Display for InvalidSeparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid array option separator '{}'",
            self.0.escape_ascii()
        )
    }
}

impl core::error::Error for InvalidSeparator {}

impl OptionArrayDef {
    /// A definition with no default, no minimum or maximum element count and
    /// the comma separator: the all-zero definition, which C reads as it
    /// reads a NULL `default_val.arr`.
    pub const fn new() -> Self {
        Self(
            ffi::AVOptionArrayDef {
                def: core::ptr::null(),
                size_min: 0,
                size_max: 0,
                sep: 0,
            },
            core::marker::PhantomData,
        )
    }
}

impl Default for OptionArrayDef {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> OptionArrayDefRef<'a> {
    /// Field: AVOptionArrayDef.def
    ///
    /// The default value, elements joined by [`separator`](Self::separator)
    /// as `av_opt_get` would serialize them (`"1,2"`), which
    /// `av_opt_set_defaults` parses; `None` when the option has no default.
    pub fn def(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let def = unsafe { addr_of!((*self.as_ptr()).def).read() };
        // SAFETY: `def` is NULL or a string literal of the table, or the
        // `&'static CStr` a setter stored; either outlives the handle and is
        // never written.
        unsafe { opt_c_str(def) }
    }

    /// Field: AVOptionArrayDef.size_min
    ///
    /// The minimum number of elements; when non-zero, the default holds at
    /// least that many (C fails to set fewer).
    pub fn size_min(&self) -> u32 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).size_min).read() }
    }

    /// Field: AVOptionArrayDef.size_max
    ///
    /// The maximum number of elements; `None` when unlimited (C's 0).
    pub fn size_max(&self) -> Option<NonZeroU32> {
        // SAFETY: a read of a projected field through the shared handle.
        NonZeroU32::new(unsafe { addr_of!((*self.as_ptr()).size_max).read() })
    }

    /// Field: AVOptionArrayDef.sep
    ///
    /// The separator between elements in an option's string form, as C uses
    /// it: the stored byte, or `b','` when none is stored (C's 0).
    pub fn separator(&self) -> u8 {
        // SAFETY: a read of a projected field through the shared handle.
        let sep = unsafe { addr_of!((*self.as_ptr()).sep).read() } as u8;
        if sep == 0 { b',' } else { sep }
    }

    /// Whether `self` and `other` borrow the same definition.
    pub fn ptr_eq(self, other: OptionArrayDefRef<'_>) -> bool {
        core::ptr::eq(self.as_ptr(), other.as_ptr())
    }
}

impl fmt::Debug for OptionArrayDefRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OptionArrayDef")
            .field("def", &self.def())
            .field("size_min", &self.size_min())
            .field("size_max", &self.size_max())
            .field("separator", &char::from(self.separator()))
            .finish()
    }
}

impl OptionArrayDefMut<'_> {
    /// Field: AVOptionArrayDef.def
    ///
    /// Sets the default value, in the syntax of [`OptionArrayDefRef::def`],
    /// or none. `'static`, because a definition holds the pointer for as long
    /// as an option table may publish it.
    pub fn set_def(&mut self, def: Option<&'static CStr>) {
        let def = def.map_or(core::ptr::null(), CStr::as_ptr);
        // SAFETY: a write of a projected field through the exclusive handle;
        // `def` is NULL or a `'static` immutable string, as every reader of
        // the field requires.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).def).write(def) }
    }

    /// Field: AVOptionArrayDef.size_min
    ///
    /// Sets the minimum number of elements.
    pub fn set_size_min(&mut self, size_min: u32) {
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).size_min).write(size_min) }
    }

    /// Field: AVOptionArrayDef.size_max
    ///
    /// Sets the maximum number of elements, or none.
    pub fn set_size_max(&mut self, size_max: Option<NonZeroU32>) {
        let size_max = size_max.map_or(0, NonZeroU32::get);
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).size_max).write(size_max) }
    }

    /// Field: AVOptionArrayDef.sep
    ///
    /// Sets the separator: a printable ASCII character other than a letter,
    /// a digit or the backslash, as `libavutil/opt.h` requires, or 0 for the
    /// comma. Refuses any other byte, leaving the separator unchanged.
    pub fn set_separator(&mut self, sep: u8) -> Result<(), InvalidSeparator> {
        let valid = sep == 0
            || (sep.is_ascii_graphic() || sep == b' ')
                && !sep.is_ascii_alphanumeric()
                && sep != b'\\';
        if !valid {
            return Err(InvalidSeparator(sep));
        }
        // SAFETY: a write of a projected field through the exclusive handle.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).sep).write(sep as c_char) }
        Ok(())
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

/// Wraps: av_set_options_string
///
/// Sets options of `ctx` from the list `opts`: `key` and `value` pairs, each
/// key followed by one byte of `key_val_sep` and its value, the pairs
/// separated by one byte of `pairs_sep`. Keys and values are tokens as
/// `av_get_token` reads them: surrounding whitespace dropped, `\` escaping
/// one byte and `'...'` quoting a run. Each pair is set as
/// `av_opt_set(ctx, key, value, AV_OPT_SEARCH_CHILDREN)` does, in order.
///
/// Returns how many pairs were set. Fails at the first pair that does not
/// parse (an empty key, or a key without a separator after it) with
/// `AVERROR(EINVAL)`, or that `av_opt_set` rejects, with its error
/// (`AVERROR_OPTION_NOT_FOUND` for an unknown key, after logging it); the
/// pairs before it stay set. A failing image-size value is not undone
/// either: C copies its uninitialised result into the field, which then
/// holds unspecified sizes. An empty `opts` sets nothing.
///
/// `ctx` stays a raw seam while `AVClass`-enabled objects have no safe
/// handle.
///
/// # Safety
///
/// - `ctx` must be NULL (every key is then not found), or point to a live
///   `AVClass`-enabled object: a struct whose first member is a
///   `const AVClass *`, NULL (likewise) or a valid class whose option table
///   is terminated and describes fields of `ctx` (each offset and type
///   naming a field of that type), and whose `child_next`, if any, yields
///   objects that meet this contract too.
/// - Every field an option of `ctx` or its children names must hold a value
///   C may release when it replaces it: NULL or an `av_malloc`-family block
///   for strings, binaries (with their length field) and arrays (with their
///   count), and NULL or a dictionary C owns for dictionaries. The values C
///   stores are `ctx`'s to release (`av_opt_free`).
/// - Each value must be one `av_opt_set` stores without undefined
///   behaviour. C converts a numeric value to the option's type after
///   checking it against the option's `min` and `max` only, so those must
///   lie within the range of the type (an `int` for `int`, `bool`, `flags`
///   and rational options, an `f32` for `float` ones; C itself maps a value
///   of exactly 2^63 or 2^64 to `INT64_MAX` or `UINT64_MAX`); and C converts
///   a `color` option's opacity to a byte without rejecting NaN, so no
///   `color` value may end in an `@` specifier `strtod` reads as NaN
///   (`red@nan`).
/// - Nothing else may access `ctx`, its children or their option fields
///   during the call, and `ctx`'s class callbacks must be valid for `av_log`
///   to call on `ctx`.
pub unsafe fn set_options_string(
    ctx: *mut c_void,
    opts: &CStr,
    key_val_sep: &CStr,
    pairs_sep: &CStr,
) -> Result<usize, i32> {
    // SAFETY: `opts`, `key_val_sep` and `pairs_sep` are live NUL-terminated
    // strings C only reads, and the caller vouches for `ctx`, its fields and
    // the values `opts` sets.
    let ret = unsafe {
        ffi::av_set_options_string(ctx, opts.as_ptr(), key_val_sep.as_ptr(), pairs_sep.as_ptr())
    };
    if ret < 0 { Err(ret) } else { Ok(ret as usize) }
}

/// `AVERROR(EINVAL)`.
const EINVAL: i32 = -(ffi::EINVAL as i32);

/// `AVERROR(ERANGE)`.
const ERANGE: i32 = -(ffi::ERANGE as i32);

/// `AV_LOG_MAX_OFFSET`: the level offset of C's quiet parses
/// (`av_parse_ratio_quiet`), which lifts every message above `AV_LOG_TRACE`.
const LOG_MAX_OFFSET: c_int = (ffi::AV_LOG_TRACE as c_int) - ffi::AV_LOG_QUIET;

/// The `AV_OPT_SEARCH_*` flags a lookup on an object takes.
///
/// Only [`CHILDREN`](Self::CHILDREN): `AV_OPT_SEARCH_FAKE_OBJ` describes
/// another kind of object, a bare `const AVClass *`, on which the getters
/// find no value and the setters dereference NULL; `AV_OPT_ALLOW_NULL` is
/// [`ObjectRef::get_nullable`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SearchFlags(c_int);

impl SearchFlags {
    /// Search the object itself only.
    pub const NONE: Self = Self(0);
    /// `AV_OPT_SEARCH_CHILDREN`: search the object's children (see
    /// [`ObjectRef::children`]) first, recursively, then the object.
    pub const CHILDREN: Self = Self(ffi::AV_OPT_SEARCH_CHILDREN as c_int);

    /// The C flag bits.
    pub const fn bits(self) -> c_int {
        self.0
    }
}

/// The `AV_OPT_SERIALIZE_*` flags of [`ObjectRef::serialize`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SerializeFlags(c_int);

impl SerializeFlags {
    /// Serialize every option that matches the option flags.
    pub const NONE: Self = Self(0);
    /// `AV_OPT_SERIALIZE_SKIP_DEFAULTS`: skip options set to their defaults.
    pub const SKIP_DEFAULTS: Self = Self(ffi::AV_OPT_SERIALIZE_SKIP_DEFAULTS as c_int);
    /// `AV_OPT_SERIALIZE_OPT_FLAGS_EXACT`: serialize only options whose flags
    /// equal the option flags, rather than include them.
    pub const OPT_FLAGS_EXACT: Self = Self(ffi::AV_OPT_SERIALIZE_OPT_FLAGS_EXACT as c_int);
    /// `AV_OPT_SERIALIZE_SEARCH_CHILDREN`: serialize the children's options
    /// first, recursively.
    pub const SEARCH_CHILDREN: Self = Self(ffi::AV_OPT_SERIALIZE_SEARCH_CHILDREN as c_int);

    /// The C flag bits.
    pub const fn bits(self) -> c_int {
        self.0
    }
}

impl BitOr for SerializeFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Handwritten
///
/// A shared borrow of an `AVOptions`-enabled object, the `void *obj` of
/// `opt.h`: a C struct whose first member is a non-NULL `const AVClass *`
/// whose option table describes the struct's own fields. Its getters read
/// option values by name; [`ObjectMut`] adds the setters.
///
/// The concrete objects (codec, format and filter contexts, ...) are wrapped
/// by later layers, which hand out these handles over them. Until then a
/// handle is adopted from a raw pointer with [`from_ptr`](Self::from_ptr),
/// the raw seam whose contract every handle upholds for its lifetime:
///
/// - the object is live, and its first member a non-NULL pointer to a class
///   valid for [`ClassRef`] that outlives the borrow and has a name;
/// - every option of the class has a type `opt.h` defines (possibly
///   `AV_OPT_TYPE_FLAG_ARRAY` of one), non-NaN bounds (within the `int`
///   range for pixel and sample formats, which C converts), a unit on every
///   `CONST` option, and a non-`CONST` option's `offset` addresses a field of
///   the object of the C type its type stores, holding a valid value:
///   strings, binary buffers, arrays and dictionaries NULL or owned by the
///   object (`av_malloc` family, `av_dict_*`), a binary length matching its
///   buffer, an array count matching its array, a valid channel layout;
/// - the class's defaults are values C parses without undefined behaviour,
///   and an array option's separator is valid for `av_opt_set_defaults`;
/// - the class's callbacks honour their [`ClassRef`] contracts for the
///   object; `child_next` returns NULL or children that satisfy this contract
///   for the borrow too and that the object owns, and a non-zero
///   `state_flags_offset` addresses an `unsigned` of the object;
/// - for a shared handle, nothing writes the object or its children during
///   the borrow, the class callbacks a getter runs included; for an
///   exclusive one, nothing else accesses them.
///
/// Not `Send` or `Sync`: whether an object may move between threads is its
/// concrete type's to say.
#[derive(Clone, Copy)]
pub struct ObjectRef<'a> {
    obj: NonNull<c_void>,
    _borrow: PhantomData<&'a c_void>,
}

/// Handwritten
///
/// An exclusive borrow of an `AVOptions`-enabled object: the getters of
/// [`ObjectRef`] through [`as_ref`](Self::as_ref), and the setters, which
/// parse, check and store option values, free them, and copy them from
/// another object of the class. Adopted with [`from_ptr`](Self::from_ptr),
/// under [`ObjectRef`]'s contract.
pub struct ObjectMut<'a> {
    obj: NonNull<c_void>,
    _borrow: PhantomData<&'a mut c_void>,
}

impl fmt::Debug for ObjectRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ObjectRef")
            .field(&self.class().name())
            .finish()
    }
}

impl fmt::Debug for ObjectMut<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ObjectMut")
            .field(&self.as_ref().class().name())
            .finish()
    }
}

impl<'a> ObjectRef<'a> {
    /// The handle of the object at `obj`; `None` when `obj` or its class
    /// pointer is NULL.
    ///
    /// # Safety
    ///
    /// `obj` must be NULL or address an object that satisfies [`ObjectRef`]'s
    /// contract for a shared borrow of `'a`.
    pub unsafe fn from_ptr(obj: *mut c_void) -> Option<Self> {
        let obj = NonNull::new(obj)?;
        // SAFETY: a non-NULL `obj` is a live object starting with its class
        // pointer, per the caller.
        let class = unsafe { obj.cast::<*const ffi::AVClass>().read() };
        (!class.is_null()).then_some(Self {
            obj,
            _borrow: PhantomData,
        })
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.obj.as_ptr()
    }

    /// The object's class.
    pub fn class(&self) -> ClassRef<'a> {
        // SAFETY: the handle's contract makes the first member a live class
        // pointer, which nothing writes while the object is borrowed.
        let class = unsafe { self.obj.cast::<*mut ffi::AVClass>().read() };
        // SAFETY: `from_ptr` refused a NULL class, and the contract makes it
        // a well-formed class that outlives the borrow.
        let class = unsafe { ClassRef::from_ptr(class) };
        class.expect("an object's class is not NULL")
    }

    /// Wraps: av_opt_child_next
    ///
    /// The object's `AVOptions`-enabled children, as its class's
    /// `child_next` callback lists them (a format context's private data and
    /// I/O context, say); none when the class has no callback.
    pub fn children(&self) -> Children<'a> {
        Children {
            parent: *self,
            prev: None,
            done: false,
        }
    }

    /// Wraps: av_opt_get
    ///
    /// The value of option `name`, formatted as a string in the syntax
    /// [`ObjectMut::set`] parses: a number, `true` / `false` / `auto`, a
    /// hexadecimal dump of a binary option, `key=value` pairs separated by
    /// `:` for a dictionary, elements separated by the array's separator; an
    /// unset string, binary or dictionary option reads as `""`. A named
    /// constant reads as its value.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option (or one without a value
    /// in the object), `AVERROR(EINVAL)` for a value that does not format,
    /// `AVERROR(ENOMEM)`.
    pub fn get(&self, name: &CStr, flags: SearchFlags) -> Result<HeapString, i32> {
        self.get_raw(name, flags.0)?.ok_or(AVERROR_BUG)
    }

    /// Wraps: av_opt_get
    ///
    /// As [`get`](Self::get), but `None` for a string, binary or dictionary
    /// option, or an array, that is unset (NULL), rather than `""`
    /// (`AV_OPT_ALLOW_NULL`).
    pub fn get_nullable(&self, name: &CStr, flags: SearchFlags) -> Result<Option<HeapString>, i32> {
        self.get_raw(name, flags.0 | ffi::AV_OPT_ALLOW_NULL as c_int)
    }

    /// `av_opt_get` with `search_flags`.
    fn get_raw(&self, name: &CStr, search_flags: c_int) -> Result<Option<HeapString>, i32> {
        let mut out = ptr::null_mut();
        // SAFETY: the handle's contract covers C's reads of the object, its
        // children and its class; `name` is a live C string and `out` a
        // writable slot. No FAKE_OBJ flag reaches C.
        let ret = unsafe { ffi::av_opt_get(self.as_ptr(), name.as_ptr(), search_flags, &mut out) };
        if ret < 0 {
            return Err(ret);
        }
        // SAFETY: on success `out` is NULL or a fresh `av_malloc`-family
        // string the caller owns and releases with `av_free`.
        Ok(unsafe { HeapString::from_raw(out.cast::<c_char>()) })
    }

    /// Wraps: av_opt_get_int
    ///
    /// The value of numeric option `name` as an integer: an integer, flags,
    /// boolean, format or duration as stored; a float, double or rational
    /// truncated toward zero; a named constant's value.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for an
    /// array or a non-numeric option. Also `AVERROR(ERANGE)`, before C reads
    /// it, for a float, double or rational whose value is NaN or outside the
    /// `i64` range (`1/0`): C converts it unchecked, undefined behaviour.
    pub fn get_int(&self, name: &CStr, flags: SearchFlags) -> Result<i64, i32> {
        if let Some((o, _)) = self.find(name, None, flags.0)
            && !o.option_type().is_array()
            && matches!(
                o.option_type(),
                OptionType::FLOAT | OptionType::DOUBLE | OptionType::RATIONAL
            )
            && !fits_i64(self.get_double(name, flags)?)
        {
            return Err(ERANGE);
        }
        let mut out = 0;
        // SAFETY: as for `get`; `out` is a writable `int64_t`, and the check
        // above leaves C only conversions it defines.
        let ret = unsafe { ffi::av_opt_get_int(self.as_ptr(), name.as_ptr(), flags.0, &mut out) };
        if ret < 0 { Err(ret) } else { Ok(out) }
    }

    /// Wraps: av_opt_get_double
    ///
    /// The value of numeric option `name` as a double; a rational's
    /// quotient, which is infinite or NaN for a zero denominator.
    ///
    /// # Errors
    ///
    /// As [`get_int`](Self::get_int), without the range check.
    pub fn get_double(&self, name: &CStr, flags: SearchFlags) -> Result<f64, i32> {
        let (obj, mut out) = (self.as_ptr(), 0.0);
        // SAFETY: as for `get`; `out` is a writable double.
        let ret = unsafe { ffi::av_opt_get_double(obj, name.as_ptr(), flags.0, &mut out) };
        if ret < 0 { Err(ret) } else { Ok(out) }
    }

    /// Wraps: av_opt_get_image_size
    ///
    /// The width and height of image size option `name`.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)`
    /// (logged) for an option of another type.
    pub fn get_image_size(&self, name: &CStr, flags: SearchFlags) -> Result<(i32, i32), i32> {
        let (mut w, mut h) = (0, 0);
        // SAFETY: as for `get`; `w` and `h` are writable `int`s.
        let ret = unsafe {
            ffi::av_opt_get_image_size(self.as_ptr(), name.as_ptr(), flags.0, &mut w, &mut h)
        };
        if ret < 0 { Err(ret) } else { Ok((w, h)) }
    }

    /// Wraps: av_opt_get_array_size
    ///
    /// The number of elements of array option `name`.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for an
    /// option that is not an array.
    pub fn get_array_size(&self, name: &CStr, flags: SearchFlags) -> Result<usize, i32> {
        let (obj, mut out) = (self.as_ptr(), 0);
        // SAFETY: as for `get`; `out` is a writable `unsigned`.
        let ret = unsafe { ffi::av_opt_get_array_size(obj, name.as_ptr(), flags.0, &mut out) };
        if ret < 0 { Err(ret) } else { Ok(out as usize) }
    }

    /// Wraps: av_opt_is_set_to_default_by_name
    ///
    /// Whether option `name` holds its default value. A named constant
    /// always does.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, the code of C's parse
    /// of a default string it cannot parse, `AVERROR_PATCHWELCOME` (logged)
    /// for a type C cannot compare.
    pub fn is_set_to_default(&self, name: &CStr, flags: SearchFlags) -> Result<bool, i32> {
        let obj = self.as_ptr();
        // SAFETY: as for `get`.
        let ret = unsafe { ffi::av_opt_is_set_to_default_by_name(obj, name.as_ptr(), flags.0) };
        if ret < 0 { Err(ret) } else { Ok(ret > 0) }
    }

    /// Wraps: av_opt_flag_is_set
    ///
    /// Whether flags option `field` of the object (not of a child) has the
    /// bits of `flag`, a named constant of its unit. `false` as well when
    /// either does not exist or `flag` is not a constant.
    ///
    /// Beyond C: C returns the bitwise and of the field and the flag
    /// converted to an `int`, which drops the bits of a 64-bit field above
    /// bit 31; a flag set only there reads as set here. A `field` whose value
    /// C cannot convert to an integer without undefined behaviour (as
    /// [`get_int`](Self::get_int) refuses it) reads as not set, as C's failed
    /// read does.
    pub fn flag_is_set(&self, field: &CStr, flag: &CStr) -> bool {
        let Some((field_opt, _)) = self.find(field, None, 0) else {
            return false;
        };
        // C looks the flag up as a constant of the field's unit, and as an
        // option that is no constant (never a match) when it has none.
        let Some(unit) = field_opt.unit() else {
            return false;
        };
        // A lookup with a unit finds only named constants, so C's own
        // `flag->type != AV_OPT_TYPE_CONST` test always passes here.
        let Some((flag_opt, _)) = self.find(flag, Some(unit), 0) else {
            return false;
        };
        let Ok(value) = self.get_int(field, SearchFlags::NONE) else {
            return false;
        };
        // SAFETY: as for `get`; the lookups above found both options, and
        // `get_int` succeeded, so C's own read of the field is defined.
        let ret = unsafe { ffi::av_opt_flag_is_set(self.as_ptr(), field.as_ptr(), flag.as_ptr()) };
        ret != 0
            || matches!(flag_opt.default_value(), OptionDefault::Int(bits) if value & bits != 0)
    }

    /// Wraps: av_opt_serialize
    ///
    /// The object's options as `key=value` pairs in one string, which
    /// [`ObjectMut::set_from_string`] parses back: each option whose flags
    /// include `opt_flags` (or equal them, with
    /// [`SerializeFlags::OPT_FLAGS_EXACT`]), named constants excluded, as
    /// [`get`](Self::get) formats it; names and values escaped with `\` where
    /// they hold a separator.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` (logged) for a separator that is NUL or `\`, or two
    /// equal separators; the error of the first option that does not format;
    /// `AVERROR(ENOMEM)`.
    pub fn serialize(
        &self,
        opt_flags: i32,
        flags: SerializeFlags,
        key_val_sep: u8,
        pairs_sep: u8,
    ) -> Result<HeapString, i32> {
        let mut out = ptr::null_mut();
        // SAFETY: as for `get`; `out` is a writable slot.
        let ret = unsafe {
            ffi::av_opt_serialize(
                self.as_ptr(),
                opt_flags,
                flags.0,
                &mut out,
                key_val_sep as c_char,
                pairs_sep as c_char,
            )
        };
        if ret < 0 {
            return Err(ret);
        }
        // SAFETY: on success `out` is the fresh `av_malloc` string
        // `av_bprint_finalize` handed over, which the caller frees.
        unsafe { HeapString::from_raw(out) }.ok_or(AVERROR_BUG)
    }

    /// Wraps: av_opt_show2
    ///
    /// Logs the object's options at `AV_LOG_INFO`, in `log_ctx`'s context:
    /// each option whose flags intersect `req_flags` and miss `rej_flags`,
    /// with its type, flags, help, ranges and default, and the named
    /// constants of its unit.
    pub fn show(&self, log_ctx: Option<ObjectRef<'_>>, req_flags: i32, rej_flags: i32) {
        let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
        // SAFETY: as for `get`; `log_ctx` is NULL or a live object as a
        // handle guarantees. C returns -1 only for a NULL object.
        unsafe { ffi::av_opt_show2(self.as_ptr(), log_ctx, req_flags, rej_flags) };
    }

    /// The option `name` of `unit` (a named constant), or not a constant when
    /// `unit` is `None`, in the object or, with `AV_OPT_SEARCH_CHILDREN` in
    /// `search_flags`, a child, and the object that holds it: what
    /// `av_opt_find2` finds for the getters and setters.
    fn find(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        search_flags: c_int,
    ) -> Option<(OptionDefRef<'a>, NonNull<c_void>)> {
        let mut target = ptr::null_mut();
        let unit = unit.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: as for `get`; `target` is a writable slot.
        let o = unsafe {
            ffi::av_opt_find2(
                self.as_ptr(),
                name.as_ptr(),
                unit,
                0,
                search_flags,
                &mut target,
            )
        };
        // SAFETY: a found option is an entry of a class's option table,
        // which outlives the borrow and is never written.
        let o = unsafe { OptionDefRef::from_ptr(o.cast_mut()) }?;
        Some((o, NonNull::new(target)?))
    }
}

/// Whether C converts `d` to an `int64_t` without undefined behaviour: it is
/// not NaN and truncates into the range.
fn fits_i64(d: f64) -> bool {
    (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&d)
}

/// Whether C converts `d` to an `int` without undefined behaviour.
fn fits_int(d: f64) -> bool {
    d > -2_147_483_649.0 && d < 2_147_483_648.0
}

/// An iterator over an object's `AVOptions`-enabled children, from
/// [`ObjectRef::children`].
#[derive(Clone, Debug)]
pub struct Children<'a> {
    parent: ObjectRef<'a>,
    prev: Option<ObjectRef<'a>>,
    done: bool,
}

impl<'a> Iterator for Children<'a> {
    type Item = ObjectRef<'a>;

    fn next(&mut self) -> Option<ObjectRef<'a>> {
        if self.done {
            return None;
        }
        let prev = self.prev.map_or(ptr::null_mut(), |p| p.as_ptr());
        // SAFETY: the parent is live and well-formed for `'a`, and `prev` is
        // NULL or the child `child_next` returned last, as C expects.
        let child = unsafe { ffi::av_opt_child_next(self.parent.as_ptr(), prev) };
        // SAFETY: the parent's contract makes each child an object that
        // satisfies it for `'a`.
        match unsafe { ObjectRef::from_ptr(child) } {
            Some(c) => {
                self.prev = Some(c);
                Some(c)
            }
            None => {
                self.done = true;
                None
            }
        }
    }
}

impl core::iter::FusedIterator for Children<'_> {}

/// A cursor over an object's `AVOptions`-enabled children with exclusive
/// access to each in turn, from [`ObjectMut::children_mut`].
#[derive(Debug)]
pub struct ChildrenMut<'a> {
    parent: NonNull<c_void>,
    prev: Option<NonNull<c_void>>,
    done: bool,
    _borrow: PhantomData<&'a mut c_void>,
}

impl ChildrenMut<'_> {
    /// Wraps: av_opt_child_next
    ///
    /// The next child, borrowed exclusively until the cursor moves on.
    pub fn next_child(&mut self) -> Option<ObjectMut<'_>> {
        if self.done {
            return None;
        }
        let prev = self.prev.map_or(ptr::null_mut(), NonNull::as_ptr);
        // SAFETY: as for `Children::next`; the cursor holds the parent's
        // exclusive borrow, and no child handle is live while it moves.
        let child = unsafe { ffi::av_opt_child_next(self.parent.as_ptr(), prev) };
        // SAFETY: the parent's contract makes each child an object it owns
        // that satisfies the contract; the returned handle borrows the
        // cursor, so it is the only one in use until it drops.
        match unsafe { ObjectMut::from_ptr(child) } {
            Some(c) => {
                self.prev = NonNull::new(child);
                Some(c)
            }
            None => {
                self.done = true;
                None
            }
        }
    }
}

impl<'a> ObjectMut<'a> {
    /// The exclusive handle of the object at `obj`; `None` when `obj` or its
    /// class pointer is NULL.
    ///
    /// # Safety
    ///
    /// `obj` must be NULL or address an object that satisfies [`ObjectRef`]'s
    /// contract for an exclusive borrow of `'a`.
    pub unsafe fn from_ptr(obj: *mut c_void) -> Option<Self> {
        // SAFETY: as for `ObjectRef::from_ptr`, which the caller's stronger
        // contract implies.
        let shared = unsafe { ObjectRef::from_ptr(obj) }?;
        Some(Self {
            obj: shared.obj,
            _borrow: PhantomData,
        })
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut c_void {
        self.obj.as_ptr()
    }

    /// A shared handle, for the getters.
    pub fn as_ref(&self) -> ObjectRef<'_> {
        ObjectRef {
            obj: self.obj,
            _borrow: PhantomData,
        }
    }

    /// A shorter exclusive reborrow.
    pub fn as_mut(&mut self) -> ObjectMut<'_> {
        ObjectMut {
            obj: self.obj,
            _borrow: PhantomData,
        }
    }

    /// The children, each borrowed exclusively in turn; see
    /// [`ObjectRef::children`].
    pub fn children_mut(&mut self) -> ChildrenMut<'_> {
        ChildrenMut {
            parent: self.obj,
            prev: None,
            done: false,
            _borrow: PhantomData,
        }
    }

    /// Wraps: av_opt_set
    ///
    /// Parses `val` as option `name`'s type and stores it: an arithmetic
    /// expression, possibly with SI suffixes and the unit's named constants,
    /// for a number (`"2*max"`, `"1k"`); `+` / `-` separated flag names for
    /// flags (`"+a-b"`); `key=value` pairs separated by `:` for a
    /// dictionary; hexadecimal for binary; elements separated by the array's
    /// separator for an array; a name or description for the other types.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for a
    /// read-only option, a value that does not parse, or a non-runtime option
    /// of an initialized object, `AVERROR(ERANGE)` for a value out of the
    /// option's range, `AVERROR(ENOMEM)`; most are logged in the object's
    /// context.
    ///
    /// Also, before C runs, for a value whose parse could reach undefined
    /// behaviour in C: `AVERROR(EINVAL)` for an expression that
    /// [`eval::parse_and_eval`] refuses, a color whose alpha reads as NaN
    /// (`"red@nan"`), or a channel layout of an ambisonic order of exactly
    /// `INT_MAX`; `AVERROR(ERANGE)` for a `+` / `-` flag operand whose value
    /// is NaN or outside the `i64` range (`"+1e30"`), or a rational value in
    /// the option's bounds but outside the `int` range, both of which C
    /// converts unchecked. A refused array element refuses the whole value.
    ///
    /// Beyond C: a color or image size that does not parse leaves the option
    /// as it was; C stores the bytes of its uninitialised parse buffer.
    ///
    /// [`eval::parse_and_eval`]: super::eval::parse_and_eval
    pub fn set(&mut self, name: &CStr, val: &CStr, flags: SearchFlags) -> Result<(), i32> {
        if let Some(err) = self.as_ref().value_refusal(name, val.to_bytes(), flags.0)? {
            return Err(err);
        }
        let kept = self.as_ref().clobbered_field(name, flags.0);
        let obj = self.as_mut_ptr();
        // SAFETY: the handle's exclusive borrow covers C's writes to the
        // object and its children, which free and replace values the object
        // owns; `name` and `val` are live C strings; the check above refused
        // every value whose parse could reach undefined behaviour.
        let ret = unsafe { ffi::av_opt_set(obj, name.as_ptr(), val.as_ptr(), flags.0) };
        if ret < 0 {
            if let Some((field, bytes, len)) = kept {
                // SAFETY: `field` addresses the option's `len`-byte value in
                // the object or a child, which the exclusive borrow lets us
                // write; `bytes` holds what it held before the call.
                unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), field.as_ptr(), len) };
            }
            return Err(ret);
        }
        Ok(())
    }

    /// Wraps: av_opt_set_int
    ///
    /// Stores `val` in numeric option `name`, converted to its type.
    ///
    /// # Errors
    ///
    /// As [`set`](Self::set), without parse errors. Also, before C runs,
    /// `AVERROR(EINVAL)` for an array option, which C would overwrite as a
    /// scalar, and `AVERROR(ERANGE)` for a value outside the `int` range in a
    /// rational's bounds, which C converts unchecked.
    pub fn set_int(&mut self, name: &CStr, val: i64, flags: SearchFlags) -> Result<(), i32> {
        if let Some(err) = self.as_ref().number_refusal(name, 1.0, val, flags.0) {
            return Err(err);
        }
        // SAFETY: as for `set`; the check above refused the stores C makes
        // with undefined behaviour.
        let ret = unsafe { ffi::av_opt_set_int(self.as_mut_ptr(), name.as_ptr(), val, flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_double
    ///
    /// Stores `val` in numeric option `name`, converted to its type (rounded
    /// to nearest for an integer).
    ///
    /// # Errors
    ///
    /// As [`set_int`](Self::set_int); NaN or a value outside the `int` range
    /// in a rational's bounds is refused with `AVERROR(ERANGE)`.
    pub fn set_double(&mut self, name: &CStr, val: f64, flags: SearchFlags) -> Result<(), i32> {
        if let Some(err) = self.as_ref().number_refusal(name, val, 1, flags.0) {
            return Err(err);
        }
        // SAFETY: as for `set_int`.
        let ret = unsafe { ffi::av_opt_set_double(self.as_mut_ptr(), name.as_ptr(), val, flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_bin
    ///
    /// Stores a copy of `val` in binary option `name`, replacing and freeing
    /// its buffer; an empty `val` leaves it NULL.
    ///
    /// # Errors
    ///
    /// As [`set`](Self::set), and `AVERROR(EINVAL)` (logged) for an option of
    /// another type. Also, before C runs, `AVERROR(ERANGE)` for a `val` of
    /// more than `INT_MAX` bytes, and `AVERROR(EINVAL)` for an array option,
    /// whose type C's error message would look up out of bounds.
    pub fn set_bin(&mut self, name: &CStr, val: &[u8], flags: SearchFlags) -> Result<(), i32> {
        let len = c_int::try_from(val.len()).map_err(|_| ERANGE)?;
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::BINARY, flags.0)
        {
            return Err(err);
        }
        // SAFETY: as for `set`; `val` holds `len` readable bytes, and C reads
        // none when `len` is 0.
        let ret = unsafe {
            ffi::av_opt_set_bin(self.as_mut_ptr(), name.as_ptr(), val.as_ptr(), len, flags.0)
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_image_size
    ///
    /// Stores `width` by `height` in image size option `name`.
    ///
    /// # Errors
    ///
    /// As [`set_bin`](Self::set_bin), and `AVERROR(EINVAL)` (logged) for a
    /// negative dimension.
    pub fn set_image_size(
        &mut self,
        name: &CStr,
        width: i32,
        height: i32,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::IMAGE_SIZE, flags.0)
        {
            return Err(err);
        }
        // SAFETY: as for `set_bin`.
        let ret = unsafe {
            ffi::av_opt_set_image_size(self.as_mut_ptr(), name.as_ptr(), width, height, flags.0)
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_defaults
    ///
    /// Sets every option of the object, except read-only ones, to its
    /// default, freeing the values it replaces.
    ///
    /// When memory runs out, an option whose default C fails to parse is left
    /// unset or as C's parse left it: a video rate then holds the numerator
    /// `sscanf` stored and an uninitialised denominator, as in C.
    pub fn set_defaults(&mut self) {
        // SAFETY: the handle's exclusive borrow covers C's writes; the
        // contract makes every default parse without undefined behaviour and
        // every replaced value one the object owns.
        unsafe { ffi::av_opt_set_defaults(self.as_mut_ptr()) }
    }

    /// Wraps: av_opt_set_defaults2
    ///
    /// As [`set_defaults`](Self::set_defaults), for the options whose flags
    /// and `mask` equal `flags` (`AV_OPT_FLAG_*` bits).
    pub fn set_defaults_masked(&mut self, mask: i32, flags: i32) {
        // SAFETY: as for `set_defaults`.
        unsafe { ffi::av_opt_set_defaults2(self.as_mut_ptr(), mask, flags) }
    }

    /// Wraps: av_opt_set_from_string
    ///
    /// Parses `opts` as `key=value` pairs and sets each as [`set`](Self::set)
    /// does, on the object itself: pairs are separated by any byte of
    /// `pairs_sep` and keys from values by any byte of `key_val_sep`. The
    /// first pairs may omit their keys, which are then taken in order from
    /// `shorthand` until a pair names its key. Returns the number of options
    /// set.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` (logged) for a pair without a key where none is
    /// implied, or the error of the first pair that does not set, after the
    /// ones before it have been set. Also, before C runs, the error of
    /// [`set`](Self::set)'s refusal of any pair's value, with no pair set.
    ///
    /// A color or image size pair that does not parse leaves C's
    /// uninitialised parse buffer in its option, as C does: the pair C
    /// stopped at is not known here, so [`set`](Self::set)'s restore does
    /// not apply.
    pub fn set_from_string(
        &mut self,
        opts: &CStr,
        shorthand: &[&CStr],
        key_val_sep: &CStr,
        pairs_sep: &CStr,
    ) -> Result<usize, i32> {
        if let Some(err) = self
            .as_ref()
            .pairs_refusal(opts, shorthand, key_val_sep, pairs_sep)?
        {
            return Err(err);
        }
        let shorthand: Vec<*const c_char> = shorthand
            .iter()
            .map(|s| s.as_ptr())
            .chain([ptr::null()])
            .collect();
        // SAFETY: as for `set`, for each pair; `shorthand` is NULL-terminated
        // and every string is live during the call.
        let ret = unsafe {
            ffi::av_opt_set_from_string(
                self.as_mut_ptr(),
                opts.as_ptr(),
                shorthand.as_ptr(),
                key_val_sep.as_ptr(),
                pairs_sep.as_ptr(),
            )
        };
        if ret < 0 { Err(ret) } else { Ok(ret as usize) }
    }

    /// Wraps: av_opt_free
    ///
    /// Frees every value the object's options own (strings, binary buffers,
    /// dictionaries, arrays, channel layout maps) and leaves them unset; the
    /// object stays valid.
    ///
    /// Beyond C: C keeps a binary option's length when it frees the buffer,
    /// after which C's own readers dereference the NULL buffer for that many
    /// bytes; the length is reset to 0 here.
    pub fn free_options(&mut self) {
        let obj = self.as_mut_ptr();
        // SAFETY: the handle's exclusive borrow covers C's writes; the
        // contract makes every value the options hold one the object owns,
        // which C frees once and leaves NULL or zeroed.
        unsafe { ffi::av_opt_free(obj) };
        for o in self.as_ref().class().options().iter() {
            if o.option_type() == OptionType::BINARY {
                let offset = o.offset() as usize + size_of::<*mut u8>();
                // SAFETY: the contract makes a binary option's offset address
                // a `{ uint8_t *, int }` pair of the object, which the
                // exclusive borrow lets us write.
                unsafe {
                    obj.cast::<u8>()
                        .add(offset)
                        .cast::<c_int>()
                        .write_unaligned(0)
                };
            }
        }
    }

    /// Wraps: av_opt_copy
    ///
    /// Copies every option value of `src` into the object, which must be of
    /// the same class: values `src` owns are duplicated, and the ones they
    /// replace freed. Children are not copied.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` when the classes differ; otherwise the last error of
    /// an option that did not copy (`AVERROR(ENOMEM)`), the others copied.
    pub fn copy_from(&mut self, src: ObjectRef<'_>) -> Result<(), i32> {
        // SAFETY: the exclusive borrow of `self` and the shared one of `src`
        // cover C's writes and reads, and keep the two objects distinct; C
        // compares the classes before reading either object's options.
        let ret = unsafe { ffi::av_opt_copy(self.as_mut_ptr(), src.as_ptr()) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

/// Handwritten
///
/// A Rust object that is only its class pointer: an `AVOptions`-enabled
/// object of a class without options, children or offsets into the object,
/// such as one [`Class::new`] builds. It is a log context, for
/// [`ObjectRef::show`] or [`eval::parse_and_eval`], and an object on which
/// the option API finds no option.
///
/// ```
/// use ffmpeg::libavutil::log::{Class, ClassCategory};
/// use ffmpeg::libavutil::opt::{BareObject, SearchFlags};
/// static CLASS: Class = Class::new(c"MyContext", ClassCategory::NA);
/// let mut obj = BareObject::new(CLASS.as_ref()).unwrap();
/// assert!(obj.as_object().get(c"anything", SearchFlags::NONE).is_err());
/// obj.as_object_mut().set_defaults();
/// ```
///
/// [`Class::new`]: super::log::Class::new
/// [`eval::parse_and_eval`]: super::eval::parse_and_eval
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BareObject<'c> {
    /// The object's first and only member, a `const AVClass *`: a class
    /// handle is one non-NULL pointer to it.
    class: ClassRef<'c>,
}

impl<'c> BareObject<'c> {
    /// An object of `class`; `None` when the class has an option, a
    /// `child_next` callback or a log level, parent context or state flags
    /// offset, which C would read in an object larger than a class pointer,
    /// or no name, which C would print.
    pub fn new(class: ClassRef<'c>) -> Option<Self> {
        let bare = class.name().is_some()
            && class.options().is_empty()
            && class.child_next().is_none()
            && class.log_level_offset_offset().is_none()
            && class.parent_log_context_offset().is_none()
            && class.state_flags_offset().is_none();
        bare.then_some(Self { class })
    }

    /// The object, borrowed shared.
    pub fn as_object(&self) -> ObjectRef<'_> {
        ObjectRef {
            obj: NonNull::from(self).cast(),
            _borrow: PhantomData,
        }
    }

    /// The object, borrowed exclusively.
    pub fn as_object_mut(&mut self) -> ObjectMut<'_> {
        ObjectMut {
            obj: NonNull::from(self).cast(),
            _borrow: PhantomData,
        }
    }
}

impl fmt::Debug for BareObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BareObject")
            .field(&self.class.name())
            .finish()
    }
}

/// Undefined-behaviour checks for values the setters hand to C.
impl ObjectRef<'_> {
    /// The error [`ObjectMut::set`] refuses `val` for option `name` with,
    /// when C's parse of it could reach undefined behaviour; `Err` for a
    /// lookup that fails as C's own evaluation of the value would not.
    fn value_refusal(
        &self,
        name: &CStr,
        val: &[u8],
        search_flags: c_int,
    ) -> Result<Option<i32>, i32> {
        let Some((o, target)) = self.find(name, None, search_flags) else {
            return Ok(None);
        };
        if o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int != 0 {
            return Ok(None);
        }
        let ty = o.option_type();
        if !ty.is_array() {
            return elem_refusal(target, o, ty, val);
        }
        let sep = match o.default_value() {
            OptionDefault::Array(Some(def)) => def.separator(),
            _ => b',',
        };
        // C splits at each unescaped separator and drops each `\` before a
        // byte, as below.
        let mut rest = val;
        while !rest.is_empty() {
            let mut elem = Vec::new();
            while let Some((&c, tail)) = rest.split_first() {
                rest = tail;
                if c == b'\\' && !tail.is_empty() {
                    elem.push(tail[0]);
                    rest = &tail[1..];
                } else if c == sep {
                    break;
                } else {
                    elem.push(c);
                }
            }
            if let Some(err) = elem_refusal(target, o, ty.base(), &elem)? {
                return Ok(Some(err));
            }
        }
        Ok(None)
    }

    /// The address, bytes and size of option `name`'s value when it is a
    /// color (4 bytes) or an image size (8 bytes), which C's failed parse
    /// overwrites with an uninitialised buffer, for [`ObjectMut::set`] to put
    /// back.
    fn clobbered_field(
        &self,
        name: &CStr,
        search_flags: c_int,
    ) -> Option<(NonNull<u8>, [u8; 8], usize)> {
        let (o, target) = self.find(name, None, search_flags)?;
        let len = match o.option_type() {
            OptionType::COLOR => 4,
            OptionType::IMAGE_SIZE => 8,
            _ => return None,
        };
        // SAFETY: the contract makes the option's offset address its value
        // in `target`, the object or a child.
        let field = unsafe { target.cast::<u8>().add(usize::try_from(o.offset()).ok()?) };
        let mut bytes = [0; 8];
        // SAFETY: `field` addresses `len` readable bytes of the value.
        unsafe { ptr::copy_nonoverlapping(field.as_ptr(), bytes.as_mut_ptr(), len) };
        Some((field, bytes, len))
    }

    /// The error [`ObjectMut::set_int`] and [`ObjectMut::set_double`] refuse
    /// to store `num * intnum` (C's `write_number` operands, denominator 1)
    /// in option `name` with.
    fn number_refusal(
        &self,
        name: &CStr,
        num: f64,
        intnum: i64,
        search_flags: c_int,
    ) -> Option<i32> {
        let (o, _) = self.find(name, None, search_flags)?;
        if o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int != 0 {
            return None;
        }
        if o.option_type().is_array() {
            return Some(EINVAL);
        }
        if !matches!(
            o.option_type(),
            OptionType::RATIONAL | OptionType::VIDEO_RATE
        ) {
            return None;
        }
        let value = num * intnum as f64;
        if o.max() < value || o.min() > value {
            // C's own `AVERROR(ERANGE)`.
            return None;
        }
        // C tests `(int) num == num`, then converts `num * intnum` to `int`
        // when it holds.
        let ub = !fits_int(num) || (num.trunc() == num && !fits_int(value));
        ub.then_some(ERANGE)
    }

    /// The error [`ObjectMut::set_bin`] and [`ObjectMut::set_image_size`]
    /// refuse a `required` type store in option `name` with: an array option,
    /// whose type index C's mismatch message reads past `opt_type_desc`.
    fn type_refusal(&self, name: &CStr, required: OptionType, search_flags: c_int) -> Option<i32> {
        let (o, _) = self.find(name, None, search_flags)?;
        let refused = o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int == 0
            && o.option_type() != required
            && o.option_type().is_array();
        refused.then_some(EINVAL)
    }

    /// The error [`ObjectMut::set_from_string`] refuses `opts` with: the
    /// first of [`value_refusal`](Self::value_refusal)'s for a pair, read as
    /// C reads them.
    fn pairs_refusal(
        &self,
        opts: &CStr,
        shorthand: &[&CStr],
        key_val_sep: &CStr,
        pairs_sep: &CStr,
    ) -> Result<Option<i32>, i32> {
        let mut rest = opts;
        let mut implied = shorthand.iter();
        while !rest.is_empty() {
            let Ok(pair) =
                parse_key_value(rest, key_val_sep, pairs_sep, !implied.as_slice().is_empty())
            else {
                // C stops at the pair it cannot parse.
                return Ok(None);
            };
            // C steps over the separator that ended the value.
            rest = pair.rest;
            if !rest.is_empty() {
                let tail = &rest.to_bytes_with_nul()[1..];
                rest = CStr::from_bytes_with_nul(tail).map_err(|_| AVERROR_BUG)?;
            }
            let key = match &pair.key {
                Some(key) => {
                    implied = [].iter();
                    key.as_c_str()
                }
                None => implied.next().copied().ok_or(AVERROR_BUG)?,
            };
            if let Some(err) = self.value_refusal(key, pair.value.as_bytes(), 0)? {
                return Ok(Some(err));
            }
        }
        Ok(None)
    }
}

/// The error a value of option `o` of `target`, of scalar type `ty`, is
/// refused with, as [`ObjectMut::set`] lists them; `Err` when a quiet
/// evaluation cannot run.
fn elem_refusal(
    target: NonNull<c_void>,
    o: OptionDefRef<'_>,
    ty: OptionType,
    val: &[u8],
) -> Result<Option<i32>, i32> {
    match ty {
        OptionType::FLAGS
        | OptionType::INT
        | OptionType::UINT
        | OptionType::INT64
        | OptionType::UINT64
        | OptionType::FLOAT
        | OptionType::DOUBLE
        | OptionType::RATIONAL => {
            // C evaluates the value, or each flag token of it, as an
            // expression: each token is a part of `val`, so `val` covers
            // every call C could parse.
            if eval::refuses(val, 0) {
                return Ok(Some(EINVAL));
            }
            if ty == OptionType::FLAGS {
                return flags_refusal(target, o, val);
            }
            // A rational's bounds within the `int` range keep C's range
            // check ahead of every unchecked conversion.
            if ty != OptionType::RATIONAL || (fits_int(o.min()) && fits_int(o.max())) {
                return Ok(None);
            }
            Ok(match number_value(target, o, val)? {
                Some(d) if !(o.max() < d || o.min() > d) && !fits_int(d) => Some(ERANGE),
                _ => None,
            })
        }
        // `av_parse_video_rate` evaluates what is no `num:den` or
        // abbreviation quietly, with `AV_LOG_MAX_OFFSET`.
        OptionType::VIDEO_RATE => Ok(eval::refuses(val, LOG_MAX_OFFSET).then_some(EINVAL)),
        OptionType::COLOR => Ok(parseutils::color_alpha_is_nan(val).then_some(EINVAL)),
        OptionType::CHANNEL_LAYOUT => {
            Ok(channel_layout::ambisonic_order_overflows(val).then_some(EINVAL))
        }
        _ => Ok(None),
    }
}

/// The error a flags value is refused with: `AVERROR(ERANGE)` when C's
/// `set_string_number` reaches a `+` or `-` token whose value is NaN or
/// outside the `int64_t` range, which it converts unchecked.
///
/// Splits `val` as C does: an optional `+` or `-`, then up to 255 bytes up
/// to the next `+` or `-`; an empty token evaluates the whole rest and ends
/// the value.
fn flags_refusal(
    target: NonNull<c_void>,
    o: OptionDefRef<'_>,
    val: &[u8],
) -> Result<Option<i32>, i32> {
    let mut rest = val;
    loop {
        let cmd = matches!(rest.first(), Some(b'+' | b'-'));
        if cmd {
            rest = &rest[1..];
        }
        let len = rest
            .iter()
            .take(255)
            .take_while(|&&c| c != b'+' && c != b'-')
            .count();
        let token = if len > 0 { &rest[..len] } else { rest };
        if cmd {
            match number_value(target, o, token)? {
                Some(d) if !fits_i64(d) => return Ok(Some(ERANGE)),
                Some(_) => {}
                // C stops at a token it cannot evaluate.
                None => return Ok(None),
            }
        }
        rest = &rest[len..];
        if len == 0 || rest.is_empty() {
            return Ok(None);
        }
    }
}

/// The value C's `set_string_number` evaluates `token` to for option `o` of
/// `target`: a named constant of the option's unit, or an expression over
/// the unit's constants and `default`, `max`, `min`, `none` and `all`,
/// evaluated quietly (`AV_LOG_MAX_OFFSET`, no log context). `None` when C
/// fails to evaluate it; `Err(AVERROR(EINVAL))` for a token the quiet
/// evaluation must refuse.
fn number_value(
    target: NonNull<c_void>,
    o: OptionDefRef<'_>,
    token: &[u8],
) -> Result<Option<f64>, i32> {
    let token = CString::new(token).map_err(|_| EINVAL)?;
    let unit = o.unit();
    let unit_ptr = unit.map_or(ptr::null(), CStr::as_ptr);
    let children = if o.flags() & ffi::AV_OPT_FLAG_CHILD_CONSTS as c_int != 0 {
        ffi::AV_OPT_SEARCH_CHILDREN as c_int
    } else {
        0
    };
    // SAFETY: `target` is the object C found the option in, live and
    // well-formed for the borrow; `token` and `unit` are live C strings.
    let named = unsafe { ffi::av_opt_find(target.as_ptr(), token.as_ptr(), unit_ptr, 0, children) };
    // SAFETY: a found option is an entry of a live, unwritten table.
    if let Some(named) = unsafe { OptionDefRef::from_ptr(named.cast_mut()) }
        && named.option_type() == OptionType::CONST
    {
        return Ok(Some(default_number(named)));
    }
    let mut names = Vec::new();
    let mut values = Vec::new();
    if let Some(unit) = unit {
        let mut prev = ptr::null();
        loop {
            // SAFETY: `target` is live and well-formed; `prev` is NULL or the
            // entry `av_opt_next` returned last.
            prev = unsafe { ffi::av_opt_next(target.as_ptr(), prev) };
            // SAFETY: as above, a table entry.
            let Some(c) = (unsafe { OptionDefRef::from_ptr(prev.cast_mut()) }) else {
                break;
            };
            if c.option_type() == OptionType::CONST && c.unit() == Some(unit) {
                if names.len() + 6 >= 64 {
                    // C's `AVERROR_PATCHWELCOME`.
                    return Ok(None);
                }
                names.push(c.name().map_or(ptr::null(), CStr::as_ptr));
                values.push(default_number(c));
            }
        }
    }
    names.extend([c"default", c"max", c"min", c"none", c"all"].map(CStr::as_ptr));
    values.extend([default_number(o), o.max(), o.min(), 0.0, -1.0]);
    names.push(ptr::null());
    values.push(0.0);
    if eval::refuses(token.to_bytes(), LOG_MAX_OFFSET) {
        return Err(EINVAL);
    }
    let mut d = 0.0;
    // SAFETY: `names` is NULL-terminated and `values` as long; every string
    // is live; no functions are passed; `refuses` rejected every token whose
    // evaluation could reach undefined behaviour at this level offset.
    let ret = unsafe {
        ffi::av_expr_parse_and_eval(
            &mut d,
            token.as_ptr(),
            names.as_ptr(),
            values.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            LOG_MAX_OFFSET,
            ptr::null_mut(),
        )
    };
    Ok((ret >= 0).then_some(d))
}

/// `DEFAULT_NUMVAL(o)` for the options C evaluates it on (flags, rationals,
/// named constants): `default_val.i64` or `default_val.dbl`, as a double.
fn default_number(o: OptionDefRef<'_>) -> f64 {
    match o.default_value() {
        OptionDefault::Int(i) => i as f64,
        OptionDefault::Double(d) => d,
        _ => 0.0,
    }
}

/// A key-value pair [`parse_key_value`] read from the front of a string.
#[derive(Debug)]
pub struct KeyValue<'s> {
    /// The key; `None` for a pair without one, read with `implicit_key`.
    pub key: Option<HeapString>,
    /// The value, unescaped and with surrounding whitespace removed, as
    /// `av_get_token` reads it.
    pub value: HeapString,
    /// The rest of the string: the pair separator that ended the value and
    /// what follows, or the empty string.
    pub rest: &'s CStr,
}

/// Wraps: av_opt_get_key_value
///
/// Reads the key-value pair at the front of `opts`: a key of the bytes
/// option names use (`a-z A-Z 0-9 - . / _`, whitespace around it ignored)
/// ended by any byte of `key_val_sep`, then a value up to any byte of
/// `pairs_sep`. With `implicit_key` (`AV_OPT_FLAG_IMPLICIT_KEY`), a pair
/// without a key yields the whole token as its value.
///
/// # Errors
///
/// `AVERROR(EINVAL)` for a pair without a key (unless `implicit_key`),
/// `AVERROR(ENOMEM)`.
pub fn parse_key_value<'s>(
    opts: &'s CStr,
    key_val_sep: &CStr,
    pairs_sep: &CStr,
    implicit_key: bool,
) -> Result<KeyValue<'s>, i32> {
    let mut rest = opts.as_ptr();
    let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
    let flags = if implicit_key {
        ffi::AV_OPT_FLAG_IMPLICIT_KEY.0
    } else {
        0
    };
    // SAFETY: `rest`, `key` and `value` are writable slots, and the strings
    // live C strings C only reads during the call.
    let ret = unsafe {
        ffi::av_opt_get_key_value(
            &mut rest,
            key_val_sep.as_ptr(),
            pairs_sep.as_ptr(),
            flags,
            &mut key,
            &mut value,
        )
    };
    if ret < 0 {
        return Err(ret);
    }
    // SAFETY: on success `key` (NULL for an implicit key) and `value` are
    // fresh `av_malloc` strings the caller frees with `av_free`, and `rest`
    // points into `opts` at a separator or its NUL, so it is a suffix of the
    // same string, live for `'s`.
    unsafe {
        Ok(KeyValue {
            key: HeapString::from_raw(key),
            value: HeapString::from_raw(value).ok_or(AVERROR_BUG)?,
            rest: CStr::from_ptr(rest),
        })
    }
}

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
        // SAFETY: `def` is a live local, only read, that outlives the handle.
        let def_ref = unsafe { OptionArrayDefRef::from_ptr((&raw const def).cast_mut()) };
        for base in [
            T::AV_OPT_TYPE_INT,
            T::AV_OPT_TYPE_STRING,
            T::AV_OPT_TYPE_DOUBLE,
        ] {
            let ty = T(base.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0);
            let o = option(ty, ffi::AVOption__bindgen_ty_1 { arr: &def });
            assert_default(o, OptionDefault::Array(def_ref));
            let o = option(
                ty,
                ffi::AVOption__bindgen_ty_1 {
                    arr: core::ptr::null(),
                },
            );
            assert_default(o, OptionDefault::Array(None));
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

        let OptionDefault::Array(Some(def)) = named(c"array").default_value() else {
            panic!()
        };
        assert_eq!(def.as_ptr(), &raw const fixture::ARRAY_DEF.0);
        assert_eq!(def.def(), Some(c"1,2"));
        assert_eq!(ctx.array_len, 2 as c_uint);
        // SAFETY: `array` holds `array_len` elements parsed from `ARRAY_DEF`.
        let array = unsafe { core::slice::from_raw_parts(ctx.array, 2) };
        assert_eq!(array, [1, 2]);
        let OptionDefault::Array(def) = named(c"empty_array").default_value() else {
            panic!()
        };
        assert!(def.is_none());
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

    #[test]
    fn array_def_new_is_the_all_zero_definition() {
        for d in [
            OptionArrayDef::new(),
            OptionArrayDef::default(),
            OptionArrayDef::zeroed(),
        ] {
            let r = d.as_ref();
            assert_eq!(r.def(), None);
            assert_eq!(r.size_min(), 0);
            assert_eq!(r.size_max(), None);
            assert_eq!(r.separator(), b',');
            // SAFETY: a read of the plain field we just built.
            let raw = unsafe { r.as_ptr().read() };
            assert!(raw.def.is_null() && raw.size_min == 0 && raw.size_max == 0 && raw.sep == 0);
        }
        assert_eq!(
            size_of::<OptionArrayDef>(),
            size_of::<ffi::AVOptionArrayDef>()
        );
    }

    #[test]
    fn array_def_setters_write_the_fields_getters_read() {
        let mut d = OptionArrayDef::new();
        let mut m = d.as_mut();
        m.set_def(Some(c"1|2|3"));
        m.set_size_min(2);
        m.set_size_max(NonZeroU32::new(u32::MAX));
        m.set_separator(b'|').unwrap();
        let r = m.as_ref();
        assert_eq!(r.def(), Some(c"1|2|3"));
        assert_eq!(
            (r.size_min(), r.size_max().map(NonZeroU32::get)),
            (2, Some(u32::MAX))
        );
        assert_eq!(r.separator(), b'|');
        // SAFETY: a read of the plain struct behind the handle.
        let raw = unsafe { r.as_ptr().read() };
        assert_eq!(raw.def, c"1|2|3".as_ptr());
        assert_eq!(
            (raw.size_min, raw.size_max, raw.sep as u8),
            (2, u32::MAX, b'|')
        );

        // Back to none: NULL, unlimited and the comma.
        m.set_def(None);
        m.set_size_max(None);
        m.set_separator(0).unwrap();
        // SAFETY: as above.
        let raw = unsafe { m.as_ref().as_ptr().read() };
        assert!(raw.def.is_null() && raw.size_max == 0 && raw.sep == 0);
        assert_eq!(d.as_ref().separator(), b',');
    }

    #[test]
    fn array_def_separators_follow_opt_h() {
        let mut d = OptionArrayDef::new();
        let mut m = d.as_mut();
        let mut accepted = Vec::new();
        for b in 0..=u8::MAX {
            match m.set_separator(b) {
                Ok(()) => {
                    accepted.push(b);
                    assert_eq!(m.as_ref().separator(), if b == 0 { b',' } else { b });
                }
                Err(e) => {
                    assert_eq!(e, InvalidSeparator(b));
                    // A refused byte leaves the last accepted one.
                    let last = *accepted.last().unwrap();
                    assert_eq!(m.as_ref().separator(), if last == 0 { b',' } else { last });
                }
            }
        }
        // 0, and the 32 printable ASCII characters (space included) that are
        // neither alphanumeric nor the backslash.
        assert_eq!(accepted.len(), 1 + 32);
        for &b in b",| ;:~/" {
            assert!(accepted.contains(&b), "{b}");
        }
        for b in [b'a', b'Z', b'0', b'\\', b'\n', 0x7f, 0x80, 0xff] {
            assert!(!accepted.contains(&b), "{b}");
        }
        assert_eq!(
            InvalidSeparator(b'\\').to_string(),
            r"invalid array option separator '\\'"
        );
    }

    #[test]
    fn array_defaults_compare_by_definition() {
        let a = OptionArrayDef::new();
        let b = OptionArrayDef::new();
        let (a, b) = (a.as_ref(), b.as_ref());
        assert!(a.ptr_eq(a) && !a.ptr_eq(b));
        assert_eq!(OptionDefault::Array(Some(a)), OptionDefault::Array(Some(a)));
        assert_ne!(OptionDefault::Array(Some(a)), OptionDefault::Array(Some(b)));
        assert_ne!(OptionDefault::Array(Some(a)), OptionDefault::Array(None));
        assert_eq!(OptionDefault::Array(None), OptionDefault::Array(None));
        assert_ne!(OptionDefault::Array(None), OptionDefault::Unsupported);
        assert_ne!(OptionDefault::Int(0), OptionDefault::Double(0.0));
        assert_eq!(OptionDefault::Unsupported, OptionDefault::Unsupported);
        assert_ne!(
            OptionDefault::Double(f64::NAN),
            OptionDefault::Double(f64::NAN)
        );
    }

    #[test]
    fn array_def_debug_lists_the_fields() {
        let OptionDefault::Array(Some(def)) = named(c"array").default_value() else {
            panic!()
        };
        assert_eq!(
            format!("{def:?}"),
            r#"OptionArrayDef { def: Some("1,2"), size_min: 0, size_max: None, separator: ',' }"#
        );
        assert!(
            format!("{:?}", OptionDefault::Array(Some(def)))
                .starts_with("Array(Some(OptionArrayDef {")
        );
    }

    #[test]
    fn array_defs_are_send_and_sync() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<OptionArrayDef>();
        send_sync::<OptionArrayDefRef<'static>>();
        send_sync::<OptionArrayDefMut<'static>>();

        let OptionDefault::Array(Some(def)) = named(c"array").default_value() else {
            panic!()
        };
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..64 {
                        assert_eq!(def.def(), Some(c"1,2"));
                        assert_eq!(def.separator(), b',');
                    }
                });
            }
        });
    }

    /// Options wave 0, batch 3: `ObjectRef` / `ObjectMut` internals and the
    /// corrections of C's defects.
    mod objects {
        use super::*;
        use crate::libavutil::log::Class;
        use fixture::{OBJ_CLASS, Owned};

        fn under_bsan() -> bool {
            std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
        }

        /// The exclusive handle of a fixture object.
        fn object(o: &Owned) -> ObjectMut<'_> {
            // SAFETY: `o` is a live, defaulted object of the well-formed
            // `OBJ_CLASS`, used through no other handle while this one lives.
            unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
        }

        fn refusal(o: &Owned, name: &CStr, val: &CStr) -> Option<i32> {
            object(o)
                .as_ref()
                .value_refusal(name, val.to_bytes(), 0)
                .unwrap()
        }

        #[test]
        fn adoption_refuses_null_objects_and_classes() {
            // SAFETY: NULL is accepted.
            assert!(unsafe { ObjectRef::from_ptr(ptr::null_mut()) }.is_none());
            // SAFETY: as above.
            assert!(unsafe { ObjectMut::from_ptr(ptr::null_mut()) }.is_none());
            let mut no_class: *const ffi::AVClass = ptr::null();
            let p = (&raw mut no_class).cast::<c_void>();
            // SAFETY: a live object whose class pointer is NULL.
            assert!(unsafe { ObjectRef::from_ptr(p) }.is_none());
            // SAFETY: as above.
            assert!(unsafe { ObjectMut::from_ptr(p) }.is_none());
            let o = Owned::new();
            let obj = object(&o);
            assert_eq!(obj.as_ref().class().name(), Some(c"crustify-opt-obj"));
            assert!(format!("{obj:?}").contains("crustify-opt-obj"));
        }

        #[test]
        fn free_options_resets_binary_lengths() {
            let o = Owned::new();
            let mut obj = object(&o);
            obj.set_bin(c"binary", &[1, 2, 3], SearchFlags::NONE)
                .unwrap();
            obj.free_options();
            let p = o.ptr();
            // SAFETY: a raw read of the live object's fields.
            let binary = unsafe { ((*p).binary, (*p).binary_len) };
            assert_eq!(binary, (ptr::null_mut(), 0));
            // C's own `av_opt_free` keeps the length 3, after which this read
            // dereferences the NULL buffer.
            assert_eq!(
                obj.as_ref()
                    .get(c"binary", SearchFlags::NONE)
                    .unwrap()
                    .as_bytes(),
                b""
            );
            assert_eq!(
                obj.as_ref().is_set_to_default(c"binary", SearchFlags::NONE),
                Ok(false)
            );
            let copy = Owned::zeroed();
            object(&copy).copy_from(obj.as_ref()).unwrap();
            // SAFETY: as above.
            assert_eq!(unsafe { (*copy.ptr()).binary_len }, 0);
        }

        #[test]
        fn flag_is_set_sees_bits_above_31() {
            let o = Owned::new();
            let mut obj = object(&o);
            obj.set_int(c"big", 1 << 40, SearchFlags::NONE).unwrap();
            let (big, high) = (c"big".as_ptr(), c"high".as_ptr());
            // SAFETY: the live object and C strings.
            let c = unsafe { ffi::av_opt_flag_is_set(o.as_obj(), big, high) };
            assert_eq!(c, 0, "C truncates the and to an int");
            assert!(obj.as_ref().flag_is_set(c"big", c"high"));
            assert!(obj.as_ref().flag_is_set(c"big", c"mixed"));
            assert!(!obj.as_ref().flag_is_set(c"big", c"low"));
            assert!(!obj.as_ref().flag_is_set(c"big", c"notconst"));
            assert!(!obj.as_ref().flag_is_set(c"big", c"bit"));
            assert!(!obj.as_ref().flag_is_set(c"int64", c"high"));
            assert!(!obj.as_ref().flag_is_set(c"missing", c"high"));
        }

        #[test]
        fn flag_is_set_reads_an_unconvertible_field_as_unset() {
            let o = Owned::new();
            let mut obj = object(&o);
            obj.set_double(c"dbl", 1e300, SearchFlags::NONE).unwrap();
            assert!(!obj.as_ref().flag_is_set(c"dbl_unit", c"dflag"));
            obj.set_double(c"dbl", 2.0, SearchFlags::NONE).unwrap();
            assert!(obj.as_ref().flag_is_set(c"dbl_unit", c"dflag"));
        }

        #[test]
        fn get_int_refuses_unconvertible_values() {
            let o = Owned::new();
            let mut obj = object(&o);
            obj.set_double(c"dbl", 1e300, SearchFlags::NONE).unwrap();
            assert_eq!(obj.as_ref().get_int(c"dbl", SearchFlags::NONE), Err(ERANGE));
            obj.set_double(c"dbl", -9.3e18, SearchFlags::NONE).unwrap();
            assert_eq!(obj.as_ref().get_int(c"dbl", SearchFlags::NONE), Err(ERANGE));
            obj.set_double(c"dbl", -9.2e18, SearchFlags::NONE).unwrap();
            assert_eq!(
                obj.as_ref().get_int(c"dbl", SearchFlags::NONE),
                Ok(-9_200_000_000_000_000_000)
            );
            obj.set(c"q", c"1:0", SearchFlags::NONE).unwrap_err();
            // SAFETY: a raw write of the live object's field.
            unsafe { (*o.ptr()).q = ffi::AVRational { num: 1, den: 0 } };
            assert_eq!(obj.as_ref().get_int(c"q", SearchFlags::NONE), Err(ERANGE));
            assert_eq!(
                obj.as_ref().get_double(c"q", SearchFlags::NONE),
                Ok(f64::INFINITY)
            );
            // SAFETY: as above.
            unsafe { (*o.ptr()).q = ffi::AVRational { num: 7, den: 2 } };
            assert_eq!(obj.as_ref().get_int(c"q", SearchFlags::NONE), Ok(3));
        }

        #[test]
        fn values_reaching_unchecked_conversions_are_refused() {
            let o = Owned::with_child();
            for (name, val, err) in [
                (c"flags", c"+1e30", Some(ERANGE)),
                (c"flags", c"-1e30", Some(ERANGE)),
                (c"flags", c"one+huge", Some(ERANGE)),
                (c"flags", c"+huge", Some(ERANGE)),
                (c"flags", c"+max*1e10", Some(ERANGE)),
                (c"flags", c"1e30", None),
                (c"flags", c"+one-bit+8", None),
                (c"flags", c"+-1e30", Some(ERANGE)),
                (c"flags", c"+", None),
                (c"flags", c"+nosuch", None),
                (c"flags", c"+ld(1e300)", Some(EINVAL)),
                (c"int", c"ld(1e300)", Some(EINVAL)),
                (c"int", c"gcd(12,18)", None),
                (c"dbl", c"st(0,-1);random(0)", Some(EINVAL)),
                (c"q", c"1e10", None),
                (c"wide_q", c"1e10", Some(ERANGE)),
                (c"wide_q", c"-2147483649", Some(ERANGE)),
                (c"wide_q", c"2147483647", None),
                (c"wide_q", c"1e301", None),
                (c"wide_q", c"3/2", None),
                (c"rate", c"ld(1e300)", Some(EINVAL)),
                (c"rate", c"ntsc", None),
                (c"color", c"red@nan", Some(EINVAL)),
                (c"color", c"red@0.5", None),
                (c"layout", c"ambisonic 2147483647", Some(EINVAL)),
                (c"layout", c"stereo", None),
                (c"array", c"1,ld(1e300)", Some(EINVAL)),
                (c"array", c"1,2", None),
                (c"strings", c"ld(1e300)|x", None),
                (c"ro", c"ld(1e300)", None),
                (c"missing", c"ld(1e300)", None),
                (c"string", c"ld(1e300)", None),
            ] {
                assert_eq!(refusal(&o, name, val), err, "{name:?} = {val:?}");
            }
            // A child's option is checked with `CHILDREN`.
            let obj = object(&o);
            assert_eq!(
                obj.as_ref()
                    .value_refusal(c"flags", b"+1e30", SearchFlags::CHILDREN.0),
                Ok(Some(ERANGE))
            );
        }

        #[test]
        fn failed_color_and_size_parses_keep_the_value() {
            // BSan reports C's reads through the end pointers glibc's uninstrumented
            // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
            // as accesses of freed or foreign allocations, depending on earlier
            // frames (the shared recipes' stale-shadow class); the parses here are
            // C's own, so BSan covers these paths in the typed-setter tests instead.
            if under_bsan() {
                return;
            }
            let o = Owned::with_child();
            let mut obj = object(&o);
            for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                obj.set(c"color", c"#01020304", flags).unwrap();
                obj.set(c"size", c"7x9", flags).unwrap();
                assert!(obj.set(c"color", c"nocolor", flags).is_err());
                assert!(obj.set(c"color", c"red@2", flags).is_err());
                assert!(obj.set(c"size", c"bad", flags).is_err());
                assert!(obj.set(c"size", c"0x0", flags).is_err());
                let r = obj.as_ref();
                assert_eq!(r.get(c"color", flags).unwrap().as_bytes(), b"0x01020304");
                assert_eq!(r.get_image_size(c"size", flags), Ok((7, 9)));
            }
        }

        #[test]
        fn array_elements_split_as_c_splits_them() {
            let o = Owned::new();
            // An escaped separator stays in its element.
            assert_eq!(refusal(&o, c"array", c"1\\,ld(1e300)"), Some(EINVAL));
            assert_eq!(refusal(&o, c"array", c"ld(0\\,1e300)"), None);
            let mut obj = object(&o);
            obj.set(c"strings", c"x\\|y|z", SearchFlags::NONE).unwrap();
            assert_eq!(
                obj.as_ref().get_array_size(c"strings", SearchFlags::NONE),
                Ok(2)
            );
        }

        #[test]
        fn scalar_stores_into_arrays_and_rationals_are_refused() {
            let o = Owned::new();
            let obj = object(&o);
            let r = obj.as_ref();
            assert_eq!(r.number_refusal(c"array", 1.0, 5, 0), Some(EINVAL));
            assert_eq!(r.number_refusal(c"wide_q", f64::NAN, 1, 0), Some(ERANGE));
            assert_eq!(r.number_refusal(c"wide_q", 1e10, 1, 0), Some(ERANGE));
            assert_eq!(r.number_refusal(c"wide_q", 1.0, 1 << 40, 0), Some(ERANGE));
            assert_eq!(r.number_refusal(c"wide_q", 2.5, 1, 0), None);
            assert_eq!(r.number_refusal(c"wide_q", 1e10 + 0.5, 1, 0), Some(ERANGE));
            assert_eq!(r.number_refusal(c"q", 1e10, 1, 0), None);
            assert_eq!(r.number_refusal(c"rate", 1.0, 1 << 40, 0), None);
            assert_eq!(r.number_refusal(c"int", f64::NAN, 1, 0), None);
            assert_eq!(
                r.type_refusal(c"array", OptionType::BINARY, 0),
                Some(EINVAL)
            );
            assert_eq!(
                r.type_refusal(c"strings", OptionType::IMAGE_SIZE, 0),
                Some(EINVAL)
            );
            assert_eq!(r.type_refusal(c"int", OptionType::BINARY, 0), None);
            assert_eq!(r.type_refusal(c"binary", OptionType::BINARY, 0), None);
        }

        #[test]
        fn a_refused_pair_sets_nothing() {
            let o = Owned::new();
            let mut obj = object(&o);
            assert_eq!(
                obj.set_from_string(c"int=5:flags=+1e30", &[], c"=", c":"),
                Err(ERANGE)
            );
            assert_eq!(obj.as_ref().get_int(c"int", SearchFlags::NONE), Ok(7));
            assert_eq!(
                obj.set_from_string(c"9:+1e30", &[c"int", c"flags"], c"=", c":"),
                Err(ERANGE)
            );
            assert_eq!(
                obj.set_from_string(c"9:+eight", &[c"int", c"flags"], c"=", c":"),
                Ok(2)
            );
            assert_eq!(obj.as_ref().get_int(c"flags", SearchFlags::NONE), Ok(11));
            // After a named key, keyless pairs fail in C, before any check.
            assert!(
                obj.set_from_string(c"int=1:+1e30", &[c"flags"], c"=", c":")
                    .is_err()
            );
        }

        #[test]
        fn flags_expose_their_bits_and_children_end() {
            assert_eq!(SearchFlags::NONE.bits(), 0);
            assert_eq!(
                SearchFlags::CHILDREN.bits(),
                ffi::AV_OPT_SEARCH_CHILDREN as c_int
            );
            assert_eq!(SearchFlags::default(), SearchFlags::NONE);
            let o = Owned::with_child();
            let obj = object(&o);
            let mut children = obj.as_ref().children();
            assert!(children.next().is_some());
            assert!(children.next().is_none());
            assert!(children.next().is_none());
        }

        #[test]
        fn arrays_without_a_definition_split_at_commas() {
            let o = Owned::new();
            assert_eq!(refusal(&o, c"empty", c"1,ld(1e300)"), Some(EINVAL));
            assert_eq!(refusal(&o, c"empty", c"1,2"), None);
            let mut obj = object(&o);
            obj.set(c"empty", c"1,2,3", SearchFlags::NONE).unwrap();
            assert_eq!(
                obj.as_ref().get_array_size(c"empty", SearchFlags::NONE),
                Ok(3)
            );
        }

        #[test]
        fn child_constants_are_looked_up_in_children() {
            let o = Owned::with_child();
            assert_eq!(refusal(&o, c"cflags", c"+one+1e30"), Some(ERANGE));
            assert_eq!(refusal(&o, c"cflags", c"+one+bit"), None);
            let mut obj = object(&o);
            obj.set(c"cflags", c"one+eight", SearchFlags::NONE).unwrap();
            assert_eq!(obj.as_ref().get_int(c"flags", SearchFlags::NONE), Ok(9));
        }

        #[test]
        fn too_many_unit_constants_fail_as_in_c() {
            // A flags option whose unit has 59 named constants: C's
            // expression table has room for 58 (`AVERROR_PATCHWELCOME`).
            let mut table: Vec<ffi::AVOption> = Vec::new();
            let entry = |name: &CStr| {
                let found = fixture::OBJ_OPTIONS.0.iter().find(|o| {
                    // SAFETY: the static table's names are NULL or literals.
                    !o.name.is_null() && unsafe { CStr::from_ptr(o.name) } == name
                });
                *found.unwrap()
            };
            let mut flags = entry(c"flags");
            flags.unit = c"many".as_ptr();
            table.push(flags);
            let names: Vec<CString> = (0..59)
                .map(|i| CString::new(format!("c{i}")).unwrap())
                .collect();
            for n in &names {
                let mut c = entry(c"one");
                c.name = n.as_ptr();
                c.unit = c"many".as_ptr();
                table.push(c);
            }
            table.push(fixture::OBJ_OPTIONS.0[fixture::OBJ_OPTIONS.0.len() - 1]);
            let mut class = OBJ_CLASS.0;
            class.option = table.as_ptr();
            class.child_next = None;
            let o = Owned::new();
            // SAFETY: `o` is live and owned; its class pointer is swapped for
            // a local class over the same fields, live until it is restored.
            unsafe { (*o.ptr()).class = &class };
            let obj = object(&o);
            let (opt, target) = obj.as_ref().find(c"flags", None, 0).unwrap();
            assert_eq!(number_value(target, opt, b"c1"), Ok(Some(1.0)));
            assert_eq!(number_value(target, opt, b"1+1"), Ok(None));
            assert_eq!(refusal(&o, c"flags", c"+1e30"), None);
            // SAFETY: as above, restoring the static class.
            unsafe { (*o.ptr()).class = &OBJ_CLASS.0 };
        }

        #[test]
        fn quiet_evaluation_uses_the_unit_constants() {
            let o = Owned::new();
            let obj = object(&o);
            let (flags, target) = obj.as_ref().find(c"flags", None, 0).unwrap();
            let value = |t: &[u8]| number_value(target, flags, t).unwrap();
            assert_eq!(value(b"one"), Some(1.0));
            assert_eq!(value(b"huge"), Some(i64::MAX as f64));
            assert_eq!(value(b"bit*eight+default"), Some(19.0));
            assert_eq!(value(b"max"), Some(u32::MAX as f64));
            assert_eq!(value(b"all"), Some(-1.0));
            assert_eq!(value(b"seven"), None);
            assert_eq!(
                number_value(target, flags, b"print(1,2147483647)"),
                Err(EINVAL)
            );
        }

        #[test]
        fn bare_objects_need_a_class_without_options() {
            static BARE: Class = Class::new(c"bare", crate::libavutil::log::ClassCategory::NA);
            let mut bare = BareObject::new(BARE.as_ref()).unwrap();
            assert!(format!("{bare:?}").contains("bare"));
            let mut other = bare;
            other.as_object_mut().copy_from(bare.as_object()).unwrap();
            bare.as_object_mut().free_options();
            let class = (&raw const OBJ_CLASS.0).cast_mut();
            // SAFETY: `OBJ_CLASS` is a live static class.
            let with_options = unsafe { ClassRef::from_ptr(class) }.unwrap();
            assert!(BareObject::new(with_options).is_none());
        }
    }
}
