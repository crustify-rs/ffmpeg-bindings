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
//! an [`OptionRangesBox`], built by [`ObjectRef::option_ranges`] or
//! [`ClassRef::option_ranges`] (or their `default_option_ranges` forms, which
//! skip a class's callback) and released by `av_opt_freep_ranges`,
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
//! (setters: parsed or typed values, defaults, key-value strings and
//! dictionaries, array edits, copies, freeing). The objects themselves are
//! wrapped by later layers; until then a handle is adopted from a raw
//! pointer under [`ObjectRef`]'s contract. The typed getters read a value
//! as a [`Rational`], a [`PixelFormat`], a [`SampleFormat`], a copied
//! [`ChannelLayoutVal`] or [`Dict`], and an array option's elements as any
//! [`ArrayElement`]; array edits store values of any [`ArrayInput`] type;
//! [`ClassRef::potential_children`] lists the classes of the children an
//! object of a class may have.
//! Several C setters and getters reach undefined behaviour for some values
//! (unchecked `double` to integer conversions, a scalar store over an array,
//! an out-of-bounds type name lookup); the wrappers refuse those values
//! before C runs, as each documents. [`parse_key_value`] reads one
//! `key=value` pair of an options string.
//!
//! [`ObjectRef::find`] and [`ClassRef::find`] look an option up (the latter
//! without an object, as C's `AV_OPT_SEARCH_FAKE_OBJ` does),
//! [`ObjectRef::options`] walks a class's options, and
//! [`ObjectRef::is_option_set_to_default`] checks one option found in the
//! object. [`OptionDefRef::eval_int`] and its siblings evaluate a string as
//! an option of their type parses it, in an [`EvalContext`]: an object, or a
//! class alone.

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::fmt;
use core::marker::PhantomData;
use core::mem::size_of;
use core::num::NonZeroU32;
use core::ops::BitOr;
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use std::ffi::CString;

use ffibox::{CBox, CDrop, CSlice, CZeroable, define_ctype};

use super::channel_layout::{ChannelLayout, ChannelLayoutRef, ChannelLayoutVal};
use super::dict::{Dict, DictionaryBox, DictionaryRef};
use super::ffi;
use super::log::{ClassRef, ItemNameFn};
use super::mem::HeapString;
use super::pixfmt::PixelFormat;
use super::rational::Rational;
use super::samplefmt::SampleFormat;
use super::{avstring, channel_layout, eval, parseutils};

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

/// Which query builds a range list: the class's `query_ranges` callback,
/// when it has one (`av_opt_query_ranges`), or the default ranges
/// (`av_opt_query_ranges_default`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RangeQuery {
    Class,
    Default,
}

/// What a range query inspects: an object, or a class alone, which C reads
/// through a fake object (`AV_OPT_SEARCH_FAKE_OBJ`), a pointer to the class
/// pointer.
#[derive(Clone, Copy)]
enum RangeTarget<'a> {
    Object(ObjectRef<'a>),
    Class(ClassRef<'a>),
}

/// The allowed ranges of option `key` of `target`, as `query` builds them.
fn query_ranges(
    query: RangeQuery,
    target: RangeTarget<'_>,
    key: &CStr,
    flags: RangeFlags,
) -> Result<OptionRangesBox, i32> {
    let class;
    let (obj, flags) = match target {
        RangeTarget::Object(obj) => (obj.as_ptr(), flags.0),
        RangeTarget::Class(c) => {
            class = c.as_ptr();
            let fake = ptr::from_ref(&class).cast_mut().cast::<c_void>();
            (fake, flags.0 | SEARCH_FAKE_OBJ)
        }
    };
    let mut ranges = ptr::null_mut();
    let key = key.as_ptr();
    // An object handle's contract makes the object live and unwritten during
    // the call, its class well-formed and its callbacks valid for it. A
    // well-formed class's callback accepts the fake object with FAKE_OBJ, as
    // `av_opt_query_ranges` documents, and the default query reads only the
    // class (and its potential children's) through it.
    // SAFETY: `ranges` is a writable slot, `key` a live C string, and `obj`
    // an object or a live fake object with FAKE_OBJ, as above.
    let ret = unsafe {
        match query {
            RangeQuery::Class => ffi::av_opt_query_ranges(&mut ranges, obj, key, flags),
            RangeQuery::Default => ffi::av_opt_query_ranges_default(&mut ranges, obj, key, flags),
        }
    };
    if ret < 0 {
        // On failure the list is not the caller's: the default query
        // stores NULL, and a class callback keeps what it allocated.
        return Err(ret);
    }
    // SAFETY: on success `ranges` is a fresh list the caller owns, released
    // by `av_opt_freep_ranges`; `av_opt_query_ranges` dereferences it before
    // returning, so NULL is a broken class callback, reported as a bug.
    unsafe { OptionRangesBox::from_c(ranges) }.ok_or(AVERROR_BUG)
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

/// `AVERROR(EINVAL)`.
const EINVAL: i32 = -(ffi::EINVAL as i32);

/// `AVERROR(ERANGE)`.
const ERANGE: i32 = -(ffi::ERANGE as i32);

/// `AVERROR(ENOMEM)`.
const ENOMEM: i32 = -(ffi::ENOMEM as i32);

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
        if let Some((o, _)) = self.lookup(name, None, flags.0)
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
        let Some((field_opt, _)) = self.lookup(field, None, 0) else {
            return false;
        };
        // C looks the flag up as a constant of the field's unit, and as an
        // option that is no constant (never a match) when it has none.
        let Some(unit) = field_opt.unit() else {
            return false;
        };
        // A lookup with a unit finds only named constants, so C's own
        // `flag->type != AV_OPT_TYPE_CONST` test always passes here.
        let Some((flag_opt, _)) = self.lookup(flag, Some(unit), 0) else {
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
    fn lookup(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        search_flags: c_int,
    ) -> Option<(OptionDefRef<'a>, NonNull<c_void>)> {
        self.lookup_with(name, unit, 0, search_flags)
    }

    /// As [`lookup`](Self::lookup), matching only options whose flags
    /// include `opt_flags`.
    fn lookup_with(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        opt_flags: c_int,
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
                opt_flags,
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
    /// [`eval::parse_and_eval`] refuses (for flags, any token C evaluates on
    /// its own: each `+` / `-` part, cut every 255 bytes), a color whose alpha reads as NaN
    /// (`"red@nan"`), a channel layout of an ambisonic order of exactly
    /// `INT_MAX`, or any element of an array of binary elements, whose length
    /// C writes past each element's pointer; `AVERROR(ERANGE)` for a `+` / `-` flag operand whose value
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

    /// Wraps: av_set_options_string
    ///
    /// Sets options of the object or its children from the list `opts`:
    /// `key` and `value` pairs, each key followed by one byte of
    /// `key_val_sep` and its value, the pairs separated by one byte of
    /// `pairs_sep`. Keys and values are tokens as `av_get_token` reads them:
    /// surrounding whitespace dropped, `\` escaping one byte and `'...'`
    /// quoting a run. Each pair is set as [`set`](Self::set) with
    /// [`SearchFlags::CHILDREN`] sets it, in order. Returns how many pairs
    /// were set; an empty `opts` sets nothing.
    ///
    /// # Errors
    ///
    /// Fails at the first pair that does not parse (an empty key, or a key
    /// without a separator after it) with `AVERROR(EINVAL)`, or that C's
    /// `av_opt_set` rejects, with its error (`AVERROR_OPTION_NOT_FOUND` for
    /// an unknown key, after logging it); the pairs before it stay set.
    /// Also, before C runs, the error of [`set`](Self::set)'s refusal of any
    /// pair's value, with no pair set.
    ///
    /// A color or image size pair that does not parse leaves C's
    /// uninitialised parse buffer in its option, as C does: the pair C
    /// stopped at is not known here, so [`set`](Self::set)'s restore does
    /// not apply.
    pub fn set_options_string(
        &mut self,
        opts: &CStr,
        key_val_sep: &CStr,
        pairs_sep: &CStr,
    ) -> Result<usize, i32> {
        if let Some(err) = self
            .as_ref()
            .options_refusal(opts, key_val_sep, pairs_sep)?
        {
            return Err(err);
        }
        // SAFETY: as for `set`, for each pair (C sets them with
        // `AV_OPT_SEARCH_CHILDREN`); the strings are live and NUL-terminated,
        // and C only reads them.
        let ret = unsafe {
            ffi::av_set_options_string(
                self.as_mut_ptr(),
                opts.as_ptr(),
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
    /// an `item_name` other than `av_default_item_name` or a `get_category`
    /// callback, which logging calls on the object and which a class's own
    /// callbacks may read as one of its full-sized objects, or no name,
    /// which C would print.
    pub fn new(class: ClassRef<'c>) -> Option<Self> {
        let bare = logs_as_bare(class)
            && class.options().is_empty()
            && class.child_next().is_none()
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

/// Whether C logs in the context of an object of `class` that is only its
/// class pointer without reading past it or running class code on it: the
/// class has a name, which C prints, `av_default_item_name` or no
/// `item_name`, no `get_category` callback, and no log level or parent
/// context offset, which C would read in the object.
fn logs_as_bare(class: ClassRef<'_>) -> bool {
    class.name().is_some()
        && class.item_name().is_none_or(ItemNameFn::is_default)
        && class.get_category().is_none()
        && class.log_level_offset_offset().is_none()
        && class.parent_log_context_offset().is_none()
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
        let Some((o, target)) = self.lookup(name, None, search_flags) else {
            return Ok(None);
        };
        if o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int != 0 {
            return Ok(None);
        }
        let ty = o.option_type();
        if !ty.is_array() {
            return elem_refusal(target, o, ty, val);
        }
        if ty.base() == OptionType::BINARY && !val.is_empty() {
            // C writes each element's length past its pointer.
            return Ok(Some(EINVAL));
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
        let (o, target) = self.lookup(name, None, search_flags)?;
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
        let (o, _) = self.lookup(name, None, search_flags)?;
        if o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int != 0 {
            return None;
        }
        if o.option_type().is_array() {
            return Some(EINVAL);
        }
        write_refusal(o, num, 1, intnum)
    }

    /// The error [`ObjectMut::set_bin`] and [`ObjectMut::set_image_size`]
    /// refuse a `required` type store in option `name` with: an array option,
    /// whose type index C's mismatch message reads past `opt_type_desc`.
    fn type_refusal(&self, name: &CStr, required: OptionType, search_flags: c_int) -> Option<i32> {
        let (o, _) = self.lookup(name, None, search_flags)?;
        let refused = o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int == 0
            && o.option_type() != required
            && o.option_type().is_array();
        refused.then_some(EINVAL)
    }

    /// The error [`ObjectMut::set_options_string`] refuses `opts` with: the
    /// first of [`value_refusal`](Self::value_refusal)'s for a pair, read as
    /// C's `parse_key_value_pair` reads them; `Err` as there.
    fn options_refusal(
        &self,
        opts: &CStr,
        key_val_sep: &CStr,
        pairs_sep: &CStr,
    ) -> Result<Option<i32>, i32> {
        let children = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
        let mut rest = opts;
        while !rest.is_empty() {
            // C stops at a pair it cannot allocate or parse: an empty key,
            // or one not followed by a byte of `key_val_sep`.
            let Some(key) = avstring::get_token(&mut rest, key_val_sep) else {
                return Ok(None);
            };
            let separated = rest
                .to_bytes()
                .first()
                .is_some_and(|c| key_val_sep.to_bytes().contains(c));
            if key.as_bytes().is_empty() || !separated {
                return Ok(None);
            }
            rest = skip_byte(rest)?;
            let Some(value) = avstring::get_token(&mut rest, pairs_sep) else {
                return Ok(None);
            };
            if let Some(err) = self.value_refusal(key.as_c_str(), value.as_bytes(), children)? {
                return Ok(Some(err));
            }
            // C steps over the separator that ended the value.
            if !rest.is_empty() {
                rest = skip_byte(rest)?;
            }
        }
        Ok(None)
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
                rest = skip_byte(rest)?;
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

/// The error a store of `num * intnum / den` (C's `write_number` operands)
/// into option `o`, or an element of its array, is refused with:
/// `AVERROR(ERANGE)` for a rational or video rate value within the option's
/// bounds that C converts to `int` unchecked.
fn write_refusal(o: OptionDefRef<'_>, num: f64, den: c_int, intnum: i64) -> Option<i32> {
    if !matches!(
        o.option_type().base(),
        OptionType::RATIONAL | OptionType::VIDEO_RATE
    ) {
        return None;
    }
    let value = num * intnum as f64;
    if den == 0 || o.max() * f64::from(den) < value || o.min() * f64::from(den) > value {
        // C's own `AVERROR(ERANGE)`.
        return None;
    }
    // C tests `(int) num == num`, then converts `num * intnum` to `int`
    // when it holds.
    let ub = !fits_int(num) || (num.trunc() == num && !fits_int(value));
    ub.then_some(ERANGE)
}

/// `s` without its first byte, which is not its NUL.
fn skip_byte(s: &CStr) -> Result<&CStr, i32> {
    CStr::from_bytes_with_nul(&s.to_bytes_with_nul()[1..]).map_err(|_| AVERROR_BUG)
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

/// The error a flags value is refused with: `AVERROR(EINVAL)` for a token
/// whose evaluation could reach undefined behaviour, and `AVERROR(ERANGE)`
/// when C's `set_string_number` reaches a `+` or `-` token whose value is
/// NaN or outside the `int64_t` range, which it converts unchecked.
///
/// Splits `val` as C does: an optional `+` or `-`, then up to 255 bytes up
/// to the next `+` or `-`; an empty token evaluates the whole rest and ends
/// the value. C parses each token as an expression of its own, and a token
/// cut at the 255-byte limit can start inside what the whole value reads as
/// one name or number (`0` x 255 then `ld(1e300)`), so every token is checked
/// on its own.
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
        if eval::refuses(token, 0) {
            return Ok(Some(EINVAL));
        }
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
/// evaluation must refuse, or that calls `time()`, whose value C's own
/// evaluation of the token, later, need not share.
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
    if eval::refuses(token.to_bytes(), LOG_MAX_OFFSET) || eval::reads_clock(token.to_bytes()) {
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
/// arrays of them, named constants): `default_val.i64` or `default_val.dbl`,
/// as a double. C selects the member by comparing the whole type with the
/// integer types, which an array type never equals, so for an array option
/// it reads the definition's address as `default_val.dbl`: a non-zero
/// (subnormal) value whenever the option has a definition.
fn default_number(o: OptionDefRef<'_>) -> f64 {
    match o.default_value() {
        OptionDefault::Int(i) => i as f64,
        OptionDefault::Double(d) => d,
        OptionDefault::Array(def) => f64::from_bits(def.map_or(0, |d| d.as_ptr().addr() as u64)),
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

// Options wave 1, batch 0: the typed getters of `opt.h` and the iteration of
// a class's potential children.

/// The typed getters.
impl ObjectRef<'_> {
    /// Wraps: av_opt_get_q
    ///
    /// The value of numeric option `name` as a rational: a rational as
    /// stored, an integer whose value fits an `int` over 1, anything else
    /// (a float, a double, a wider integer) approximated within `int` parts
    /// (`av_d2q`), which is `0/0` for NaN and `±1/0` beyond the `int` range.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for an
    /// array or an option of another type (a video rate included; see
    /// [`get_video_rate`](Self::get_video_rate)).
    pub fn get_rational(&self, name: &CStr, flags: SearchFlags) -> Result<Rational, i32> {
        let mut out = ffi::AVRational { num: 0, den: 0 };
        // SAFETY: the handle's contract covers C's reads of the object, its
        // children and its class; `name` is a live C string and `out` a
        // writable `AVRational`. C converts only through `double`s and
        // `av_d2q`, which are defined for every value.
        let ret = unsafe { ffi::av_opt_get_q(self.as_ptr(), name.as_ptr(), flags.0, &mut out) };
        if ret < 0 {
            Err(ret)
        } else {
            Ok(Rational::from(out))
        }
    }

    /// Wraps: av_opt_get_video_rate
    ///
    /// The value of video rate option `name`, as stored; for any other
    /// option, as [`get_rational`](Self::get_rational) reads it.
    ///
    /// Beyond C: `av_opt_get_video_rate` reads every option as `av_opt_get_q`
    /// does, which fails with `AVERROR(EINVAL)` for a video rate option (its
    /// numeric reader has no case for the type), so C can read a video rate
    /// back from everything but a video rate option. This reads the stored
    /// rate, as `av_opt_get` formats it.
    ///
    /// # Errors
    ///
    /// As [`get_rational`](Self::get_rational) for any other option.
    pub fn get_video_rate(&self, name: &CStr, flags: SearchFlags) -> Result<Rational, i32> {
        if let Some((o, target)) = self.lookup(name, None, flags.0)
            && o.option_type() == OptionType::VIDEO_RATE
        {
            let offset = usize::try_from(o.offset()).map_err(|_| AVERROR_BUG)?;
            // SAFETY: the handle's contract makes the option's offset address
            // its `AVRational` value in `target`, the object or a child,
            // which nothing writes during the borrow.
            let rate = unsafe {
                target
                    .cast::<u8>()
                    .add(offset)
                    .cast::<ffi::AVRational>()
                    .read()
            };
            return Ok(Rational::from(rate));
        }
        let (obj, mut out) = (self.as_ptr(), ffi::AVRational { num: 0, den: 0 });
        // SAFETY: as for `get_rational`, which C forwards to.
        let ret = unsafe { ffi::av_opt_get_video_rate(obj, name.as_ptr(), flags.0, &mut out) };
        if ret < 0 {
            Err(ret)
        } else {
            Ok(Rational::from(out))
        }
    }

    /// Wraps: av_opt_get_pixel_fmt
    ///
    /// The value of pixel format option `name`, as stored.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)`
    /// (logged) for an option of another type, arrays included.
    pub fn get_pixel_format(&self, name: &CStr, flags: SearchFlags) -> Result<PixelFormat, i32> {
        let (obj, mut out) = (self.as_ptr(), ffi::AVPixelFormat(0));
        // SAFETY: as for `get_rational`; `out` is a writable
        // `enum AVPixelFormat`, whose every value the newtype holds.
        let ret = unsafe { ffi::av_opt_get_pixel_fmt(obj, name.as_ptr(), flags.0, &mut out) };
        if ret < 0 {
            Err(ret)
        } else {
            Ok(PixelFormat::from_raw(out))
        }
    }

    /// Wraps: av_opt_get_sample_fmt
    ///
    /// The value of sample format option `name`, as stored.
    ///
    /// # Errors
    ///
    /// As [`get_pixel_format`](Self::get_pixel_format).
    pub fn get_sample_format(&self, name: &CStr, flags: SearchFlags) -> Result<SampleFormat, i32> {
        let (obj, mut out) = (self.as_ptr(), ffi::AVSampleFormat(0));
        // SAFETY: as for `get_pixel_format`, for an `enum AVSampleFormat`.
        let ret = unsafe { ffi::av_opt_get_sample_fmt(obj, name.as_ptr(), flags.0, &mut out) };
        if ret < 0 {
            Err(ret)
        } else {
            Ok(SampleFormat::from_raw(out))
        }
    }

    /// Wraps: av_opt_get_chlayout
    ///
    /// A copy of the value of channel layout option `name`
    /// (`av_channel_layout_copy`): a custom map is duplicated.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)`
    /// (logged) for an option of another type, arrays included, and the
    /// errors of [`ChannelLayoutRef::try_clone`](channel_layout::ChannelLayoutRef::try_clone).
    pub fn get_channel_layout(
        &self,
        name: &CStr,
        flags: SearchFlags,
    ) -> Result<ChannelLayoutVal, i32> {
        let mut out = ChannelLayout::unspecified(0);
        let dst = out.as_mut().as_mut_ptr();
        // SAFETY: as for `get_rational`; `dst` is a live, initialised
        // (empty) layout C may uninit and overwrite with a copy the value
        // then owns, and on failure leaves empty or disposable.
        let ret = unsafe { ffi::av_opt_get_chlayout(self.as_ptr(), name.as_ptr(), flags.0, dst) };
        if ret < 0 { Err(ret) } else { Ok(out) }
    }

    /// Wraps: av_opt_get_dict_val
    ///
    /// A copy of the value of dictionary option `name`, empty for an unset
    /// one. As C copies it (`av_dict_copy` with no flags), entries whose
    /// keys match an earlier one case-insensitively replace it.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for an
    /// option of another type, arrays included, `AVERROR(ENOMEM)`.
    pub fn get_dict(&self, name: &CStr, flags: SearchFlags) -> Result<Dict, i32> {
        let (obj, mut out) = (self.as_ptr(), ptr::null_mut());
        // SAFETY: as for `get_rational`; `out` is a NULL dictionary slot, into
        // which C copies the option's dictionary, entry by entry.
        let ret = unsafe { ffi::av_opt_get_dict_val(obj, name.as_ptr(), flags.0, &mut out) };
        // SAFETY: the slot holds NULL or the dictionary C built, every entry
        // through `av_dict_set`, which nothing else references; a partial
        // copy left on failure is released here.
        let copy = Dict::from(unsafe { DictionaryBox::from_c(out) });
        if ret < 0 { Err(ret) } else { Ok(copy) }
    }

    /// Wraps: av_opt_get_array
    ///
    /// Elements `start..start + count` of array option `name`, read as `T`,
    /// as the scalar getter of `T`'s option type reads a value: as stored
    /// when `T` is the array's own element type, formatted as `av_opt_get`
    /// formats it for `Option<HeapString>` (`None` for an unset string),
    /// and converted for `i64`, `f64` and [`Rational`] from a numeric array.
    /// Strings, dictionaries and layouts are copies the result owns.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option; `AVERROR(EINVAL)` for an
    /// option that is not an array, or a range that does not lie in the array
    /// (`start` must be an element, even for `count` 0); `AVERROR(ENOSYS)`
    /// for a conversion C does not make (to `i32` from anything but an `int`
    /// array, say); `AVERROR(ENOMEM)`; the error of an element that does not
    /// format. Also, before C reads it, `AVERROR(EINVAL)` for an array of
    /// binary buffers, whose elements C reads a length past (`opt.h` gives
    /// such an array no layout), and `AVERROR(ERANGE)` for an `i64` read of a
    /// float, double or rational element whose value is NaN or outside the
    /// `i64` range, which C converts unchecked.
    pub fn get_array<T: ArrayElement>(
        &self,
        name: &CStr,
        flags: SearchFlags,
        start: u32,
        count: u32,
    ) -> Result<Vec<T>, i32> {
        // C's own checks, made first so that the output is sized for a range
        // that exists: no such option, not an array, outside the array.
        let size = self.get_array_size(name, flags)?;
        let (start_at, count_at) = (start as usize, count as usize);
        if start_at >= size || size - start_at < count_at {
            return Err(EINVAL);
        }
        let (o, _) = self.lookup(name, None, flags.0).ok_or(AVERROR_BUG)?;
        let base = o.option_type().base();
        if base == OptionType::BINARY {
            return Err(EINVAL);
        }
        if T::TYPE == OptionType::INT64
            && matches!(
                base,
                OptionType::FLOAT | OptionType::DOUBLE | OptionType::RATIONAL
            )
        {
            // C's `int64_t` result of these is `(int64_t)` of the `double`
            // its `double` result is, so preview that.
            let values = self.get_array::<f64>(name, flags, start, count)?;
            if !values.into_iter().all(fits_i64) {
                return Err(ERANGE);
            }
        }
        let mut out: Vec<T::Raw> = (0..count).map(|_| T::zero()).collect();
        // SAFETY: as for `get_rational`; the range lies in the array, which
        // is no binary array; `out` holds `count` zeroed (empty) elements of
        // the C type `T::TYPE` stores, as C requires of its output; and an
        // `int64_t` conversion C makes was previewed in range above.
        let ret = unsafe {
            ffi::av_opt_get_array(
                self.as_ptr(),
                name.as_ptr(),
                flags.0,
                start,
                count,
                T::TYPE.into(),
                out.as_mut_ptr().cast(),
            )
        };
        if ret < 0 {
            // C released every element, leaving each empty.
            return Err(ret);
        }
        // C wrote each element as `T::TYPE` stores it, handing over what it
        // allocated.
        let adopt = |raw| {
            // SAFETY: an element a successful call wrote, owned by nothing
            // else, as `adopt` requires.
            unsafe { T::adopt(raw) }
        };
        Ok(out.into_iter().map(adopt).collect())
    }
}

/// An element type [`ObjectRef::get_array`] reads an array option's
/// elements as, standing for an option type (`AV_OPT_TYPE_*`) whose C type
/// it holds: `i32` (`INT`), `u32` (`UINT`), `i64` (`INT64`), `u64`
/// (`UINT64`), `f32` (`FLOAT`), `f64` (`DOUBLE`), [`Rational`] (`RATIONAL`),
/// `Option<HeapString>` (`STRING`), [`PixelFormat`] (`PIXEL_FMT`),
/// [`SampleFormat`] (`SAMPLE_FMT`), [`Dict`] (`DICT`) and
/// [`ChannelLayoutVal`] (`CHLAYOUT`).
///
/// Sealed: the crate implements it for the types whose C layout it checks.
pub trait ArrayElement: element::Element {}

mod element {
    use super::OptionType;

    /// The C side of an [`ArrayElement`](super::ArrayElement).
    ///
    /// # Safety
    ///
    /// `Raw` must have the size and alignment of the C type `opt.h` documents
    /// for `TYPE`, every value `zero` returns must be the empty value C
    /// expects of an output element (NULL, zero, or an empty layout), and
    /// `adopt` must take over what `av_opt_get_array` writes there for `TYPE`.
    pub unsafe trait Element: Sized {
        /// The option type C converts the elements to.
        const TYPE: OptionType;
        /// The C type of one element of C's output.
        type Raw;
        /// An empty output element.
        fn zero() -> Self::Raw;
        /// The element C wrote.
        ///
        /// # Safety
        ///
        /// `raw` must be an element a successful `av_opt_get_array` wrote for
        /// `TYPE`, owned by nothing else.
        unsafe fn adopt(raw: Self::Raw) -> Self;
    }
}

/// `ArrayElement` for a type that is its own C element, plain data with no
/// resources: `$ty` for option type `$opt`, C type `$c`.
macro_rules! plain_elements {
    ($($ty:ty => $opt:ident, $c:ty);* $(;)?) => {$(
        // SAFETY: `$ty` is `$c`, plain data with no resources, whose zero is
        // a value; C writes a value of it.
        unsafe impl element::Element for $ty {
            const TYPE: OptionType = OptionType::$opt;
            type Raw = $c;
            fn zero() -> $c {
                <$c>::default()
            }
            unsafe fn adopt(raw: $c) -> Self {
                Self::from(raw)
            }
        }
        impl ArrayElement for $ty {}
    )*};
}

plain_elements! {
    i32 => INT, c_int;
    u32 => UINT, c_uint;
    i64 => INT64, i64;
    u64 => UINT64, u64;
    f32 => FLOAT, f32;
    f64 => DOUBLE, f64;
}

// SAFETY: `AVRational` is `opt.h`'s C type of a rational, plain data whose
// zero (`0/0`) is a value.
unsafe impl element::Element for Rational {
    const TYPE: OptionType = OptionType::RATIONAL;
    type Raw = ffi::AVRational;
    fn zero() -> ffi::AVRational {
        ffi::AVRational { num: 0, den: 0 }
    }
    unsafe fn adopt(raw: ffi::AVRational) -> Self {
        Self::from(raw)
    }
}
impl ArrayElement for Rational {}

// SAFETY: a string element is a `char *`, NULL when empty, and C writes NULL
// or a fresh `av_malloc`-family string the caller frees.
unsafe impl element::Element for Option<HeapString> {
    const TYPE: OptionType = OptionType::STRING;
    type Raw = *mut c_char;
    fn zero() -> *mut c_char {
        ptr::null_mut()
    }
    unsafe fn adopt(raw: *mut c_char) -> Self {
        // SAFETY: NULL or a fresh string the caller owns, per `adopt`'s
        // contract.
        unsafe { HeapString::from_raw(raw) }
    }
}
impl ArrayElement for Option<HeapString> {}

// SAFETY: `enum AVPixelFormat` is a C `int`, plain data whose every value
// the newtype holds.
unsafe impl element::Element for PixelFormat {
    const TYPE: OptionType = OptionType::PIXEL_FORMAT;
    type Raw = ffi::AVPixelFormat;
    fn zero() -> ffi::AVPixelFormat {
        ffi::AVPixelFormat(0)
    }
    unsafe fn adopt(raw: ffi::AVPixelFormat) -> Self {
        Self::from_raw(raw)
    }
}
impl ArrayElement for PixelFormat {}

// SAFETY: as for `PixelFormat`, for `enum AVSampleFormat`.
unsafe impl element::Element for SampleFormat {
    const TYPE: OptionType = OptionType::SAMPLE_FORMAT;
    type Raw = ffi::AVSampleFormat;
    fn zero() -> ffi::AVSampleFormat {
        ffi::AVSampleFormat(0)
    }
    unsafe fn adopt(raw: ffi::AVSampleFormat) -> Self {
        Self::from_raw(raw)
    }
}
impl ArrayElement for SampleFormat {}

// SAFETY: a dictionary element is an `AVDictionary *`, NULL when empty, and
// C writes NULL or a copy built by `av_dict_set` the caller frees.
unsafe impl element::Element for Dict {
    const TYPE: OptionType = OptionType::DICT;
    type Raw = *mut ffi::AVDictionary;
    fn zero() -> *mut ffi::AVDictionary {
        ptr::null_mut()
    }
    unsafe fn adopt(raw: *mut ffi::AVDictionary) -> Self {
        // SAFETY: NULL or a non-empty dictionary the caller owns, per
        // `adopt`'s contract.
        Self::from(unsafe { DictionaryBox::from_c(raw) })
    }
}
impl ArrayElement for Dict {}

const _: () = assert!(size_of::<ChannelLayoutVal>() == size_of::<ffi::AVChannelLayout>());
const _: () = assert!(align_of::<ChannelLayoutVal>() == align_of::<ffi::AVChannelLayout>());

// SAFETY: a `ChannelLayoutVal` is an `AVChannelLayout` (`#[repr(C)]`, the
// value first, then a zero-sized policy; sizes asserted above), whose
// all-zero value is the empty layout C's `av_channel_layout_copy` uninits
// and overwrites with a copy the value then owns and disposes.
unsafe impl element::Element for ChannelLayoutVal {
    const TYPE: OptionType = OptionType::CHANNEL_LAYOUT;
    type Raw = ChannelLayoutVal;
    fn zero() -> ChannelLayoutVal {
        ChannelLayout::unspecified(0)
    }
    unsafe fn adopt(raw: ChannelLayoutVal) -> Self {
        raw
    }
}
impl ArrayElement for ChannelLayoutVal {}

impl<'a> ClassRef<'a> {
    /// Wraps: av_opt_child_class_iterate
    ///
    /// The classes of the `AVOptions`-enabled children an object of this
    /// class may have, which a search of its options with children
    /// (`AV_OPT_SEARCH_CHILDREN` on a class, without an object) visits:
    /// `av_opt_child_class_iterate`, driven from a NULL state until it
    /// returns NULL. None for a class without a `child_class_iterate`
    /// callback. The same classes as [`child_classes`](Self::child_classes),
    /// through `opt.h`'s entry point.
    pub fn potential_children(&self) -> PotentialChildren<'a> {
        PotentialChildren {
            parent: Some(*self),
            state: ptr::null_mut(),
        }
    }
}

/// The classes of a class's potential children, from
/// [`ClassRef::potential_children`].
#[derive(Debug)]
pub struct PotentialChildren<'a> {
    /// The parent class; `None` once C has returned NULL.
    parent: Option<ClassRef<'a>>,
    /// C's iteration state, which only the parent's callback reads.
    state: *mut c_void,
}

impl<'a> Iterator for PotentialChildren<'a> {
    type Item = ClassRef<'a>;

    fn next(&mut self) -> Option<ClassRef<'a>> {
        let parent = self.parent?;
        // SAFETY: the parent is a well-formed class, whose
        // `child_class_iterate` (when it has one) takes the opaque state it
        // alone advanced from NULL, which `state` is.
        let child = unsafe { ffi::av_opt_child_class_iterate(parent.as_ptr(), &mut self.state) };
        // SAFETY: the callback returns NULL or a well-formed class that lives
        // as long as the parent class (`'a`): every FFmpeg iterator returns a
        // `static const` class.
        let child = unsafe { ClassRef::from_ptr(child.cast_mut()) };
        if child.is_none() {
            // C callers stop at the first NULL; so does the iterator.
            self.parent = None;
        }
        child
    }
}

impl core::iter::FusedIterator for PotentialChildren<'_> {}

// Options wave 1, batch 1: the typed setters (rationals, video rates, pixel
// and sample formats, channel layouts, dictionaries), the array editor and
// the dictionary appliers.

/// Typed setters, array edits and dictionary appliers.
impl ObjectMut<'_> {
    /// Wraps: av_opt_set_q
    ///
    /// Stores `val` in numeric option `name`, converted to its type: kept as
    /// is in a rational or video rate, its quotient (rounded to nearest for
    /// an integer) in the other types.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for a
    /// read-only option, a non-numeric one, or a non-runtime option of an
    /// initialized object, `AVERROR(ERANGE)` (logged) for a value out of the
    /// option's range (a zero denominator is out of every range but a flags
    /// option's) or not a valid set of flags. Also, before C runs,
    /// `AVERROR(EINVAL)` for an array option, which C would overwrite as a
    /// scalar.
    pub fn set_rational(
        &mut self,
        name: &CStr,
        val: Rational,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        // A rational's parts are `int`s, so only the array store is refused.
        if let Some(err) = self.as_ref().number_refusal(name, 1.0, 1, flags.0) {
            return Err(err);
        }
        let obj = self.as_mut_ptr();
        // SAFETY: as for `set`; the check above refused an array option, and
        // every conversion C makes of an `int` pair is defined.
        let ret = unsafe { ffi::av_opt_set_q(obj, name.as_ptr(), val.into_raw(), flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_video_rate
    ///
    /// Stores `val`, as is, in video rate option `name`.
    ///
    /// # Errors
    ///
    /// As [`set_bin`](Self::set_bin), and `AVERROR(ERANGE)` (logged) for a
    /// rate out of the option's range or with a zero denominator.
    pub fn set_video_rate(
        &mut self,
        name: &CStr,
        val: Rational,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::VIDEO_RATE, flags.0)
        {
            return Err(err);
        }
        let (obj, val) = (self.as_mut_ptr(), val.into_raw());
        // SAFETY: as for `set_bin`; C stores the `int` pair as is.
        let ret = unsafe { ffi::av_opt_set_video_rate(obj, name.as_ptr(), val, flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_pixel_fmt
    ///
    /// Stores `fmt` in pixel format option `name`.
    ///
    /// # Errors
    ///
    /// As [`set_bin`](Self::set_bin), and `AVERROR(ERANGE)` (logged) for a
    /// format outside the option's range or the formats this libavutil
    /// knows ([`PixelFormat::NONE`] is in range when the option's minimum
    /// allows it).
    pub fn set_pixel_format(
        &mut self,
        name: &CStr,
        fmt: PixelFormat,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::PIXEL_FORMAT, flags.0)
        {
            return Err(err);
        }
        let obj = self.as_mut_ptr();
        // SAFETY: as for `set_bin`; the contract keeps the option's bounds,
        // which C converts to `int`, within its range.
        let ret = unsafe { ffi::av_opt_set_pixel_fmt(obj, name.as_ptr(), fmt.into_raw(), flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_sample_fmt
    ///
    /// Stores `fmt` in sample format option `name`.
    ///
    /// # Errors
    ///
    /// As [`set_pixel_format`](Self::set_pixel_format), for sample formats.
    pub fn set_sample_format(
        &mut self,
        name: &CStr,
        fmt: SampleFormat,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::SAMPLE_FORMAT, flags.0)
        {
            return Err(err);
        }
        let (obj, fmt) = (self.as_mut_ptr(), fmt.into_raw());
        // SAFETY: as for `set_pixel_format`.
        let ret = unsafe { ffi::av_opt_set_sample_fmt(obj, name.as_ptr(), fmt, flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_chlayout
    ///
    /// Stores a copy of `layout` in channel layout option `name`, releasing
    /// the layout it replaces; `layout` stays the caller's.
    ///
    /// # Errors
    ///
    /// As [`set_bin`](Self::set_bin), and `AVERROR(ENOMEM)` when the copy of
    /// a custom map fails. Also, before C runs, `AVERROR(EINVAL)` for a
    /// layout C's readers of the option cannot read (a custom map name
    /// without a NUL, which only [`ChannelCustom::from_raw`] builds, or an
    /// ambisonic count whose order overflows), which [`ObjectRef::get`]
    /// would later describe.
    ///
    /// Beyond C: when the copy of a custom map fails, C leaves the option a
    /// custom layout without a map, which its readers dereference; it is
    /// left the empty layout of unspecified order here.
    ///
    /// [`ChannelCustom::from_raw`]: super::channel_layout::ChannelCustom::from_raw
    pub fn set_channel_layout(
        &mut self,
        name: &CStr,
        layout: ChannelLayoutRef<'_>,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self
            .as_ref()
            .type_refusal(name, OptionType::CHANNEL_LAYOUT, flags.0)
        {
            return Err(err);
        }
        layout.describable()?;
        let obj = self.as_mut_ptr();
        // SAFETY: as for `set_bin`; `layout` is a live layout every safe path
        // keeps valid, which C only reads (a custom map is copied), and no
        // handle to the object's own field exists while `self` borrows it.
        let ret = unsafe { ffi::av_opt_set_chlayout(obj, name.as_ptr(), layout.as_ptr(), flags.0) };
        if ret < 0 {
            self.reset_unmapped_layout(name, flags.0);
            return Err(ret);
        }
        Ok(())
    }

    /// Resets channel layout option `name` to the zeroed layout
    /// (unspecified order, no channels) when it is a custom layout without
    /// a map, as `av_channel_layout_copy` leaves its destination when it
    /// cannot allocate the map: C's readers would dereference the NULL map.
    fn reset_unmapped_layout(&mut self, name: &CStr, search_flags: c_int) {
        let Some(field) = self.as_ref().layout_field(name, search_flags) else {
            return;
        };
        let p = field.as_ptr();
        // SAFETY: `p` addresses the option's layout in the object or a child,
        // live for the exclusive borrow; the union member is read as the
        // pointer C's custom order stores.
        let (order, map) = unsafe { (addr_of!((*p).order).read(), addr_of!((*p).u.map).read()) };
        if order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM && map.is_null() {
            // SAFETY: as above, writable through the exclusive borrow; the
            // layout owns no map to leak, and all-zero is the empty layout
            // every reader accepts.
            unsafe { p.write(core::mem::zeroed()) };
        }
    }

    /// Wraps: av_opt_set_dict_val
    ///
    /// Stores a copy of `val` in dictionary option `name`, freeing the
    /// dictionary it replaces; `None` is the empty dictionary. `val` stays
    /// the caller's. The copy is made with no flags: an entry whose key
    /// equals an earlier one's, case-insensitively, replaces it.
    ///
    /// # Errors
    ///
    /// As [`set_bin`](Self::set_bin), and `AVERROR(ENOMEM)`, after which the
    /// option holds the entries copied so far.
    pub fn set_dictionary(
        &mut self,
        name: &CStr,
        val: Option<DictionaryRef<'_>>,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        if let Some(err) = self.as_ref().type_refusal(name, OptionType::DICT, flags.0) {
            return Err(err);
        }
        let (obj, val) = (self.as_mut_ptr(), val.map_or(ptr::null(), |d| d.as_ptr()));
        // SAFETY: as for `set_bin`; `val` is NULL or a live dictionary C only
        // iterates, distinct from the object's own, which `self` borrows
        // exclusively.
        let ret = unsafe { ffi::av_opt_set_dict_val(obj, name.as_ptr(), val, flags.0) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_array
    ///
    /// Inserts `values` into array option `name` before element `start`
    /// (`start` equal to the array's size appends), shifting the elements
    /// from `start` on. Each value is converted as the scalar setter of its
    /// type would: a string is parsed as by [`set`](Self::set) (a string
    /// array stores a copy of it), an integer or float converted as by
    /// [`set_int`](Self::set_int) or [`set_double`](Self::set_double), and a
    /// value of the array's own element type copied (dictionaries and
    /// channel layouts deeply). The array is left as it was when any value
    /// fails.
    ///
    /// A rational converts as C's `av_opt_set_array` converts it, which is
    /// not as [`set_rational`](Self::set_rational) does: C passes its
    /// numerator as the integer factor of `1 / den`, so a rational or video
    /// rate element stores it as is and a float or double element gets its
    /// quotient, but an integer, flags, boolean, duration or format element
    /// gets `llrint(1 / den) * num` (`0` for `3/2`, where `set_rational`
    /// stores `2`).
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for
    /// a read-only option, one that is not an array, a `start` past the
    /// array's end, a resulting size outside the definition's bounds or not
    /// below `u32::MAX`, `AVERROR(ENOSYS)` for a value type that does not
    /// convert to the element type, the error of the first value that does
    /// not convert or is out of range (logged), `AVERROR(ENOMEM)`.
    ///
    /// Also, before C runs: `AVERROR(EINVAL)` for an array of binary
    /// elements, whose length C writes past each element, and for any value
    /// the scalar setter of its type refuses ([`set`](Self::set),
    /// [`set_int`](Self::set_int), [`set_double`](Self::set_double),
    /// [`set_channel_layout`](Self::set_channel_layout)), with that
    /// setter's error.
    pub fn insert_array<T: ArrayInput>(
        &mut self,
        name: &CStr,
        start: usize,
        values: &[T],
        flags: SearchFlags,
    ) -> Result<(), i32> {
        self.store_array(name, start, values, flags.0)
    }

    /// Wraps: av_opt_set_array
    ///
    /// As [`insert_array`](Self::insert_array), but `values` replace the
    /// elements from `start` on (`AV_OPT_ARRAY_REPLACE`), whose values are
    /// released, and extend the array where they run past its end.
    ///
    /// # Errors
    ///
    /// As [`insert_array`](Self::insert_array).
    pub fn replace_array<T: ArrayInput>(
        &mut self,
        name: &CStr,
        start: usize,
        values: &[T],
        flags: SearchFlags,
    ) -> Result<(), i32> {
        self.store_array(name, start, values, flags.0 | ARRAY_REPLACE)
    }

    /// Wraps: av_opt_set_array
    ///
    /// Removes `count` elements of array option `name` from element `start`
    /// on, releasing their values and shifting the later ones down.
    ///
    /// # Errors
    ///
    /// `AVERROR_OPTION_NOT_FOUND` for no such option, `AVERROR(EINVAL)` for
    /// a read-only option, one that is not an array, a range past the
    /// array's end or a resulting size below the definition's minimum,
    /// `AVERROR(ENOMEM)` when C cannot shrink the array's block (under an
    /// [`av_max_alloc`](super::mem::set_max_alloc) limit below the new
    /// size), after which the elements are removed all the same.
    ///
    /// Beyond C: C releases and shifts the elements before it shrinks the
    /// block, and on that failure keeps the old count, so the array's last
    /// elements repeat or address released values, which a later read or
    /// `av_opt_free` uses again (a double free); the count is set to the new
    /// size here.
    pub fn remove_array(
        &mut self,
        name: &CStr,
        start: usize,
        count: usize,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        let start = c_uint::try_from(start).map_err(|_| EINVAL)?;
        let count = c_uint::try_from(count).map_err(|_| EINVAL)?;
        let size = self.as_ref().array_count_field(name, flags.0);
        // SAFETY: the count of an array of the object or a child, which the
        // exclusive borrow keeps unwritten by anything else.
        let old = size.map(|p| unsafe { p.read() });
        let obj = self.as_mut_ptr();
        let ty = ffi::AVOptionType::AV_OPT_TYPE_INT;
        // SAFETY: as for `set`; with a NULL `val` C only releases and shifts
        // elements of the array the object owns, after checking the range
        // against its count, and ignores `val_type`.
        let ret = unsafe {
            ffi::av_opt_set_array(obj, name.as_ptr(), flags.0, start, count, ty, ptr::null())
        };
        // A removal fails with `ENOMEM` only once C has released the range
        // and shifted the rest down, when shrinking the block fails.
        if let (ENOMEM, Some(p), Some(old)) = (ret, size, old) {
            // SAFETY: as above, writable through the exclusive borrow; the
            // first `old - count` elements are the array's remaining ones,
            // and the block C kept is larger than they need.
            unsafe { p.write(old.saturating_sub(count)) };
        }
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// `av_opt_set_array` of `values` with `search_flags`.
    fn store_array<T: ArrayInput>(
        &mut self,
        name: &CStr,
        start: usize,
        values: &[T],
        search_flags: c_int,
    ) -> Result<(), i32> {
        let start = c_uint::try_from(start).map_err(|_| EINVAL)?;
        let count = c_uint::try_from(values.len()).map_err(|_| EINVAL)?;
        if let Some(err) = self.as_ref().array_refusal(name, values, search_flags)? {
            return Err(err);
        }
        let elems: Vec<T::C> = values.iter().map(|&v| v.to_c()).collect();
        let (obj, ty) = (self.as_mut_ptr(), T::TYPE.into());
        // SAFETY: as for `set`, for each element; `elems` holds `count`
        // values of the C type of `ty`, each live for the call (strings,
        // dictionaries and channel layout maps borrowed from `values`), which
        // C only reads; the check above refused binary arrays and every value
        // whose conversion could reach undefined behaviour.
        let ret = unsafe {
            ffi::av_opt_set_array(
                obj,
                name.as_ptr(),
                search_flags,
                start,
                count,
                ty,
                elems.as_ptr().cast(),
            )
        };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_opt_set_dict
    ///
    /// Sets each entry of `options`, in order, as [`set`](Self::set) sets
    /// its key to its value, on the object itself; on success `options` is
    /// left holding the entries no option is named by, in order.
    ///
    /// # Errors
    ///
    /// The error of the first entry that does not set (logged, with
    /// `AVERROR_OPTION_NOT_FOUND` not counting), after the entries before it
    /// have been set, or `AVERROR(ENOMEM)`; `options` is left unchanged.
    /// Also, before C runs, the error of [`set`](Self::set)'s refusal of any
    /// entry's value, with no entry set.
    ///
    /// A color or image size entry that does not parse leaves C's
    /// uninitialised parse buffer in its option, as C does: the entry C
    /// stopped at is not known here, so [`set`](Self::set)'s restore does
    /// not apply.
    pub fn set_from_dict(&mut self, options: &mut Dict) -> Result<(), i32> {
        self.apply_dict(options, 0, |obj, slot| {
            // SAFETY: as for `set`, for each entry; `slot` is the dictionary
            // slot `apply_dict` hands over.
            unsafe { ffi::av_opt_set_dict(obj, slot) }
        })
    }

    /// Wraps: av_opt_set_dict2
    ///
    /// As [`set_from_dict`](Self::set_from_dict), setting each entry with
    /// `flags` ([`SearchFlags::CHILDREN`] reaches the children's options).
    ///
    /// # Errors
    ///
    /// As [`set_from_dict`](Self::set_from_dict).
    pub fn set_from_dict_with(
        &mut self,
        options: &mut Dict,
        flags: SearchFlags,
    ) -> Result<(), i32> {
        self.apply_dict(options, flags.0, |obj, slot| {
            // SAFETY: as for `set_from_dict`; no FAKE_OBJ flag reaches C.
            unsafe { ffi::av_opt_set_dict2(obj, slot, flags.0) }
        })
    }

    /// Refuses `options` as [`set`](Self::set) refuses each entry's value
    /// with `search_flags`, then hands `apply` the object and the
    /// dictionary's slot, and takes back the dictionary C leaves in it.
    fn apply_dict(
        &mut self,
        options: &mut Dict,
        search_flags: c_int,
        apply: impl FnOnce(*mut c_void, *mut *mut ffi::AVDictionary) -> c_int,
    ) -> Result<(), i32> {
        for e in options.iter() {
            let (key, value) = (e.key(), e.value().to_bytes());
            if let Some(err) = self.as_ref().value_refusal(key, value, search_flags)? {
                return Err(err);
            }
        }
        let mut slot = Option::<DictionaryBox>::from(core::mem::take(options))
            .map_or(ptr::null_mut(), CBox::into_c);
        let ret = apply(self.as_mut_ptr(), &mut slot);
        // SAFETY: C leaves NULL or a dictionary only the slot references in
        // it: the one handed over when it fails, else the fresh dictionary
        // of the entries no option names, after freeing the one handed over.
        *options = Dict::from(unsafe { DictionaryBox::from_c(slot) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

/// `AV_OPT_ARRAY_REPLACE`.
const ARRAY_REPLACE: c_int = ffi::AV_OPT_ARRAY_REPLACE as c_int;

/// Undefined-behaviour checks for array edits and channel layouts.
impl ObjectRef<'_> {
    /// The error [`ObjectMut::insert_array`] and [`ObjectMut::replace_array`]
    /// refuse to store `values` in option `name` with; `Err` as in
    /// [`value_refusal`](Self::value_refusal).
    fn array_refusal<T: ArrayInput>(
        &self,
        name: &CStr,
        values: &[T],
        search_flags: c_int,
    ) -> Result<Option<i32>, i32> {
        let Some((o, target)) = self.lookup(name, None, search_flags & !ARRAY_REPLACE) else {
            return Ok(None);
        };
        let ty = o.option_type();
        if o.flags() & ffi::AV_OPT_FLAG_READONLY as c_int != 0 || !ty.is_array() {
            // C's own `AVERROR(EINVAL)`.
            return Ok(None);
        }
        if ty.base() == OptionType::BINARY {
            return Ok(Some(EINVAL));
        }
        for &v in values {
            if let Some(err) = v.refusal(target, o, ty.base())? {
                return Ok(Some(err));
            }
        }
        Ok(None)
    }

    /// The address of the element count of option `name` when it is an
    /// array, for [`ObjectMut::remove_array`] to repair after a failed
    /// shrink.
    fn array_count_field(&self, name: &CStr, search_flags: c_int) -> Option<NonNull<c_uint>> {
        let (o, target) = self.lookup(name, None, search_flags)?;
        if !o.option_type().is_array() {
            return None;
        }
        // C stores an array as its elements' pointer, then their `unsigned`
        // count (`opt_array_pcount`).
        let offset = usize::try_from(o.offset()).ok()? + size_of::<*mut c_void>();
        // SAFETY: the contract makes the option's offset address its
        // `{ T *, unsigned }` array field in `target`, the object or a child.
        Some(unsafe { target.cast::<u8>().add(offset) }.cast())
    }

    /// The address of option `name`'s value when it is a channel layout,
    /// for [`ObjectMut::set_channel_layout`] to repair after a failed copy.
    fn layout_field(
        &self,
        name: &CStr,
        search_flags: c_int,
    ) -> Option<NonNull<ffi::AVChannelLayout>> {
        let (o, target) = self.lookup(name, None, search_flags)?;
        if o.option_type() != OptionType::CHANNEL_LAYOUT {
            return None;
        }
        // SAFETY: the contract makes the option's offset address its layout
        // in `target`, the object or a child.
        Some(unsafe { target.cast::<u8>().add(usize::try_from(o.offset()).ok()?) }.cast())
    }
}

/// A value type of [`ObjectMut::insert_array`] and
/// [`ObjectMut::replace_array`]: the C type `av_opt_set_array` reads, which
/// C copies into an array of the same element type and otherwise converts.
///
/// | Rust | C (`val_type`) | stores into |
/// |---|---|---|
/// | `&CStr`, `Option<&CStr>` | `char *` (`STRING`) | a copy into a string array; parsed into the others (`None` as C's NULL) |
/// | `i32`, `i64` | `int`, `int64_t` (`INT`, `INT64`) | a copy, or converted into the numeric types |
/// | `f32`, `f64` | `float`, `double` (`FLOAT`, `DOUBLE`) | a copy, or converted into the numeric types |
/// | [`Rational`] | `AVRational` (`RATIONAL`) | a copy, or converted into the numeric types |
/// | `u32`, `u64` | `unsigned`, `uint64_t` (`UINT`, `UINT64`) | a copy only |
/// | [`PixelFormat`], [`SampleFormat`] | their enums | a copy only |
/// | `(i32, i32)` | `int[2]` (`IMAGE_SIZE`) | a copy only |
/// | `[u8; 4]` | `uint8_t[4]` (`COLOR`) | a copy only |
/// | [`ChannelLayoutRef`] | `AVChannelLayout` (`CHLAYOUT`) | a deep copy only |
/// | `Option<DictionaryRef>` | `AVDictionary *` (`DICT`) | a deep copy only |
///
/// "A copy only": into an array of another element type C fails with
/// `AVERROR(ENOSYS)`. Sealed. The types differ from the owned
/// [`ArrayElement`]s [`ObjectRef::get_array`] reads out where C's values own
/// something: strings, layouts and dictionaries go in borrowed.
///
/// ```
/// use ffmpeg::libavutil::log::{Class, ClassCategory};
/// use ffmpeg::libavutil::opt::{BareObject, SearchFlags};
/// static CLASS: Class = Class::new(c"MyContext", ClassCategory::NA);
/// let mut obj = BareObject::new(CLASS.as_ref()).unwrap();
/// let mut m = obj.as_object_mut();
/// // A bare object has no array option to edit.
/// assert!(m.insert_array(c"list", 0, &[1i32, 2], SearchFlags::NONE).is_err());
/// assert!(m.replace_array(c"list", 0, &[c"1", c"2"], SearchFlags::NONE).is_err());
/// ```
///
/// Other value types do not convert:
///
/// ```compile_fail,E0277
/// # use ffmpeg::libavutil::log::{Class, ClassCategory};
/// # use ffmpeg::libavutil::opt::{BareObject, SearchFlags};
/// # static CLASS: Class = Class::new(c"MyContext", ClassCategory::NA);
/// # let mut obj = BareObject::new(CLASS.as_ref()).unwrap();
/// let _ = obj.as_object_mut().insert_array(c"list", 0, &["text"], SearchFlags::NONE);
/// ```
pub trait ArrayInput: array_input::Sealed {}

mod array_input {
    use super::*;

    pub trait Sealed: Copy {
        /// The C type of an element of `val`.
        type C;
        /// The `val_type` naming [`C`](Self::C).
        const TYPE: OptionType;
        /// The element as C reads it, pointing into what `self` borrows.
        fn to_c(self) -> Self::C;
        /// The error `av_opt_set_array` of this value into option `o` of
        /// `target`, of element type `base`, is refused with; `Err` as in
        /// `value_refusal`.
        fn refusal(
            self,
            target: NonNull<c_void>,
            o: OptionDefRef<'_>,
            base: OptionType,
        ) -> Result<Option<i32>, i32>;
    }
}

/// Declares the element types C copies into an array of their own type and
/// converts into no other, which C's copy and range check handle for every
/// value.
macro_rules! copied_elements {
    ($($t:ty => $c:ty, $ty:ident, |$v:ident| $to_c:expr;)*) => {
        $(
            impl array_input::Sealed for $t {
                type C = $c;
                const TYPE: OptionType = OptionType::$ty;
                fn to_c(self) -> $c {
                    let $v = self;
                    $to_c
                }
                fn refusal(
                    self,
                    _: NonNull<c_void>,
                    _: OptionDefRef<'_>,
                    _: OptionType,
                ) -> Result<Option<i32>, i32> {
                    Ok(None)
                }
            }
            impl ArrayInput for $t {}
        )*
    };
}

copied_elements! {
    u32 => c_uint, UINT, |v| v;
    u64 => u64, UINT64, |v| v;
    PixelFormat => ffi::AVPixelFormat, PIXEL_FORMAT, |v| v.into_raw();
    SampleFormat => ffi::AVSampleFormat, SAMPLE_FORMAT, |v| v.into_raw();
    (i32, i32) => [c_int; 2], IMAGE_SIZE, |v| [v.0, v.1];
    [u8; 4] => [u8; 4], COLOR, |v| v;
    Option<DictionaryRef<'_>> => *const ffi::AVDictionary, DICT, |v| v.map_or(ptr::null(), |d| d.as_ptr());
}

/// Declares the numeric element types C also converts, through
/// `write_number` with the operands `$num`, `$den` and `$intnum`.
macro_rules! numeric_elements {
    ($($t:ty => $c:ty, $ty:ident, |$v:ident| ($num:expr, $den:expr, $intnum:expr), $to_c:expr;)*) => {
        $(
            impl array_input::Sealed for $t {
                type C = $c;
                const TYPE: OptionType = OptionType::$ty;
                fn to_c(self) -> $c {
                    let $v = self;
                    $to_c
                }
                fn refusal(
                    self,
                    _: NonNull<c_void>,
                    o: OptionDefRef<'_>,
                    base: OptionType,
                ) -> Result<Option<i32>, i32> {
                    let $v = self;
                    // C copies a value of the element type, and range-checks
                    // it as a double without converting it back.
                    if base == Self::TYPE {
                        return Ok(None);
                    }
                    Ok(write_refusal(o, $num, $den, $intnum))
                }
            }
            impl ArrayInput for $t {}
        )*
    };
}

numeric_elements! {
    i32 => c_int, INT, |v| (1.0, 1, i64::from(v)), v;
    i64 => i64, INT64, |v| (1.0, 1, v), v;
    f32 => f32, FLOAT, |v| (f64::from(v), 1, 1), v;
    f64 => f64, DOUBLE, |v| (v, 1, 1), v;
    Rational => ffi::AVRational, RATIONAL, |v| (1.0, v.as_ref().den(), i64::from(v.as_ref().num())), v.into_raw();
}

impl array_input::Sealed for Option<&CStr> {
    type C = *const c_char;
    const TYPE: OptionType = OptionType::STRING;
    fn to_c(self) -> *const c_char {
        self.map_or(ptr::null(), CStr::as_ptr)
    }
    fn refusal(
        self,
        target: NonNull<c_void>,
        o: OptionDefRef<'_>,
        base: OptionType,
    ) -> Result<Option<i32>, i32> {
        // C copies a string into a string array, and parses it into any
        // other as `av_opt_set` parses an element; NULL parses to a default
        // or fails, without evaluating anything.
        match self {
            Some(s) if base != OptionType::STRING => elem_refusal(target, o, base, s.to_bytes()),
            _ => Ok(None),
        }
    }
}
impl ArrayInput for Option<&CStr> {}

impl array_input::Sealed for &CStr {
    type C = *const c_char;
    const TYPE: OptionType = OptionType::STRING;
    fn to_c(self) -> *const c_char {
        self.as_ptr()
    }
    fn refusal(
        self,
        target: NonNull<c_void>,
        o: OptionDefRef<'_>,
        base: OptionType,
    ) -> Result<Option<i32>, i32> {
        Some(self).refusal(target, o, base)
    }
}
impl ArrayInput for &CStr {}

impl array_input::Sealed for ChannelLayoutRef<'_> {
    type C = ffi::AVChannelLayout;
    const TYPE: OptionType = OptionType::CHANNEL_LAYOUT;
    fn to_c(self) -> ffi::AVChannelLayout {
        // SAFETY: the handle covers a live layout; the bitwise copy shares
        // its map, which C only reads (to copy it) while `self` is borrowed,
        // and is never released.
        unsafe { self.as_ptr().read() }
    }
    fn refusal(
        self,
        _: NonNull<c_void>,
        _: OptionDefRef<'_>,
        base: OptionType,
    ) -> Result<Option<i32>, i32> {
        // A layout C copies into the array must stay readable by the
        // option's describers, as `ObjectMut::set_channel_layout` requires.
        if base != OptionType::CHANNEL_LAYOUT {
            return Ok(None);
        }
        Ok(self.describable().err())
    }
}
impl ArrayInput for ChannelLayoutRef<'_> {}

// Options wave 2, batch 0: option lookups and the walk of a class's options,
// the default check of one option, the numeric evaluators, and the range
// queries on objects and on classes without an object.

/// The flags of a range query: [`ObjectRef::option_ranges`],
/// [`ClassRef::option_ranges`] and their default forms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RangeFlags(c_int);

impl RangeFlags {
    /// Look the option up in the object (or class) itself only, and list
    /// one component.
    pub const NONE: Self = Self(0);
    /// `AV_OPT_SEARCH_CHILDREN`: look the option up in the children first,
    /// recursively: an object's ([`ObjectRef::children`]), or a class's
    /// potential children ([`ClassRef::potential_children`]).
    pub const CHILDREN: Self = Self(ffi::AV_OPT_SEARCH_CHILDREN as c_int);
    /// `AV_OPT_MULTI_COMPONENT_RANGE`: list as many components as a class's
    /// `query_ranges` callback returns (an image size's pixel count, width
    /// and height); without it a list has one.
    pub const MULTI_COMPONENT: Self = Self(ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int);

    /// The C flag bits.
    pub const fn bits(self) -> c_int {
        self.0
    }
}

impl BitOr for RangeFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// `AV_OPT_SEARCH_FAKE_OBJ`: the object a lookup takes is a pointer to a
/// class pointer.
const SEARCH_FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;

/// Lookups, the option walk, the default check of one option and range
/// queries.
impl<'a> ObjectRef<'a> {
    /// Wraps: av_opt_find
    ///
    /// The option `name` of the object or, with [`SearchFlags::CHILDREN`],
    /// of a child, whose options are searched first, recursively: an option
    /// that is no named constant when `unit` is `None`, else a named constant
    /// of `unit`, and only one whose flags include `opt_flags`
    /// (`AV_OPT_FLAG_*`). `None` when no option matches.
    pub fn find(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        opt_flags: i32,
        flags: SearchFlags,
    ) -> Option<OptionDefRef<'a>> {
        let unit = unit.map_or(ptr::null(), CStr::as_ptr);
        let (obj, name) = (self.as_ptr(), name.as_ptr());
        // SAFETY: as for `get`: the handle's contract covers C's reads of the
        // object, its children and their classes; `name` and `unit` are live
        // C strings or NULL. No FAKE_OBJ flag reaches C.
        let o = unsafe { ffi::av_opt_find(obj, name, unit, opt_flags, flags.0) };
        // SAFETY: a found option is an entry of the option table of the
        // object's class or a child's, which outlives the borrow and is never
        // written.
        unsafe { OptionDefRef::from_ptr(o.cast_mut()) }
    }

    /// Wraps: av_opt_find2
    ///
    /// As [`find`](Self::find), with the object that holds the option's
    /// value: this object, or the child the option was found in, on which
    /// [`is_option_set_to_default`](Self::is_option_set_to_default) checks it.
    pub fn find_with_target(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        opt_flags: i32,
        flags: SearchFlags,
    ) -> Option<(OptionDefRef<'a>, ObjectRef<'a>)> {
        let (o, target) = self.lookup_with(name, unit, opt_flags, flags.0)?;
        // SAFETY: C found the option in this object or one of its children,
        // each of which satisfies the handle's contract for `'a`.
        let target = unsafe { ObjectRef::from_ptr(target.as_ptr()) }?;
        Some((o, target))
    }

    /// Wraps: av_opt_next
    ///
    /// The options of the object's class, in table order, named constants
    /// included: the entries `av_opt_next` walks, which [`ClassRef::options`]
    /// lists too.
    pub fn options(&self) -> Options<'a> {
        Options {
            obj: *self,
            prev: None,
            done: false,
        }
    }

    /// Wraps: av_opt_is_set_to_default
    ///
    /// Whether option `o` holds its default value in the object: as
    /// [`is_set_to_default`](Self::is_set_to_default), for an option already
    /// found. A named constant always does.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)`, before C runs, when `o` is not an entry of the
    /// option table of the object's own class, whose offset C would read in
    /// this object: an option found in a child is checked on that child (the
    /// target [`find_with_target`](Self::find_with_target) returns).
    /// Otherwise as [`is_set_to_default`](Self::is_set_to_default).
    pub fn is_option_set_to_default(&self, o: OptionDefRef<'_>) -> Result<bool, i32> {
        if !self.has_option(o) {
            return Err(EINVAL);
        }
        // SAFETY: as for `get`; `o` is an entry of the table of the object's
        // class, so its offset addresses its value in the object, as the
        // contract makes it.
        let ret = unsafe { ffi::av_opt_is_set_to_default(self.as_ptr(), o.as_ptr()) };
        if ret < 0 { Err(ret) } else { Ok(ret > 0) }
    }

    /// Whether `o` is an entry of the option table of the object's class.
    fn has_option(&self, o: OptionDefRef<'_>) -> bool {
        let table = self.class().options();
        let size = size_of::<ffi::AVOption>();
        let at = o.as_ptr().addr().wrapping_sub(table.as_ptr().addr());
        at < table.len() * size && at.is_multiple_of(size)
    }

    /// Wraps: av_opt_query_ranges
    ///
    /// The allowed ranges of option `key` of the object (or, with
    /// [`RangeFlags::CHILDREN`], of a child), from its class's `query_ranges`
    /// callback, or as
    /// [`default_option_ranges`](Self::default_option_ranges) builds them
    /// when the class has none. Without [`RangeFlags::MULTI_COMPONENT`], the
    /// list has one component.
    ///
    /// # Errors
    ///
    /// The callback's `AVERROR` code: the default one fails with
    /// `AVERROR(ENOMEM)` for an unknown key (a named constant included) as for
    /// an allocation failure, and `AVERROR(ENOSYS)` for a type with no
    /// ranges. `AVERROR_BUG` for a callback that succeeds without a list.
    pub fn option_ranges(&self, key: &CStr, flags: RangeFlags) -> Result<OptionRangesBox, i32> {
        query_ranges(RangeQuery::Class, RangeTarget::Object(*self), key, flags)
    }

    /// Wraps: av_opt_query_ranges_default
    ///
    /// The allowed ranges of option `key` of the object (or, with
    /// [`RangeFlags::CHILDREN`], of a child), built from the option's `min` /
    /// `max` and type without the class's callback: one range of one
    /// component, whatever the flags.
    ///
    /// # Errors
    ///
    /// As the default query of [`option_ranges`](Self::option_ranges).
    pub fn default_option_ranges(
        &self,
        key: &CStr,
        flags: RangeFlags,
    ) -> Result<OptionRangesBox, i32> {
        query_ranges(RangeQuery::Default, RangeTarget::Object(*self), key, flags)
    }
}

/// The options of an object's class, from [`ObjectRef::options`].
#[derive(Clone)]
pub struct Options<'a> {
    obj: ObjectRef<'a>,
    /// The entry C returned last; `None` before the first.
    prev: Option<OptionDefRef<'a>>,
    done: bool,
}

impl<'a> Iterator for Options<'a> {
    type Item = OptionDefRef<'a>;

    fn next(&mut self) -> Option<OptionDefRef<'a>> {
        if self.done {
            return None;
        }
        let prev = self.prev.map_or(ptr::null(), |o| o.as_ptr());
        // SAFETY: the object is live and its class well-formed for `'a`, so
        // its table is NULL or terminated; `prev` is NULL or the entry
        // `av_opt_next` returned last, which is not the terminator.
        let next = unsafe { ffi::av_opt_next(self.obj.as_ptr(), prev) };
        // SAFETY: NULL or an entry of the class's table, which outlives the
        // borrow and is never written.
        let next = unsafe { OptionDefRef::from_ptr(next.cast_mut()) };
        match next {
            Some(o) => self.prev = Some(o),
            None => self.done = true,
        }
        next
    }
}

impl core::iter::FusedIterator for Options<'_> {}

impl fmt::Debug for Options<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("class", &self.obj.class().name())
            .field("prev", &self.prev.and_then(|o| o.name()))
            .finish()
    }
}

/// Lookups and range queries on a class without an object: C's fake object
/// (`AV_OPT_SEARCH_FAKE_OBJ`), a pointer to the class pointer, through which
/// C reads only the class.
impl<'a> ClassRef<'a> {
    /// Wraps: av_opt_find
    ///
    /// The option `name` of the class or, with [`SearchFlags::CHILDREN`], of
    /// the class of a potential child
    /// ([`potential_children`](Self::potential_children)), whose options are
    /// searched first, recursively; matched as [`ObjectRef::find`] matches.
    pub fn find(
        &self,
        name: &CStr,
        unit: Option<&CStr>,
        opt_flags: i32,
        flags: SearchFlags,
    ) -> Option<OptionDefRef<'a>> {
        let class = self.as_ptr();
        let fake = ptr::from_ref(&class).cast_mut().cast::<c_void>();
        let unit = unit.map_or(ptr::null(), CStr::as_ptr);
        let search = flags.0 | SEARCH_FAKE_OBJ;
        // SAFETY: with FAKE_OBJ, C reads only the class pointer `fake`
        // addresses, live for the call, then the class's table and, with
        // children, its `child_class_iterate` callback and the classes it
        // returns, all well-formed for `'a`; `name` and `unit` are live C
        // strings or NULL.
        let o = unsafe { ffi::av_opt_find(fake, name.as_ptr(), unit, opt_flags, search) };
        // SAFETY: a found option is an entry of the table of this class or
        // of a potential child's, each of which lives, unwritten, for `'a`.
        unsafe { OptionDefRef::from_ptr(o.cast_mut()) }
    }

    /// Wraps: av_opt_query_ranges
    ///
    /// The allowed ranges of option `key` of the class (or, with
    /// [`RangeFlags::CHILDREN`], of a potential child's class), from its
    /// `query_ranges` callback, which C hands the fake object, or as
    /// [`default_option_ranges`](Self::default_option_ranges) builds them
    /// when the class has none. Errors as for [`ObjectRef::option_ranges`].
    pub fn option_ranges(&self, key: &CStr, flags: RangeFlags) -> Result<OptionRangesBox, i32> {
        query_ranges(RangeQuery::Class, RangeTarget::Class(*self), key, flags)
    }

    /// Wraps: av_opt_query_ranges_default
    ///
    /// The allowed ranges of option `key` of the class (or, with
    /// [`RangeFlags::CHILDREN`], of a potential child's class), built from
    /// the option's `min` / `max` and type without the class's callback: one
    /// range of one component. Errors as for
    /// [`ObjectRef::default_option_ranges`].
    pub fn default_option_ranges(
        &self,
        key: &CStr,
        flags: RangeFlags,
    ) -> Result<OptionRangesBox, i32> {
        query_ranges(RangeQuery::Default, RangeTarget::Class(*self), key, flags)
    }
}

/// The context a numeric evaluator ([`OptionDefRef::eval_int`] and its
/// siblings) runs in: the object or class whose named constants a value may
/// use, in whose name C logs.
#[derive(Clone, Copy, Debug)]
pub enum EvalContext<'a> {
    /// An object: the constants of its class's table and, for an option with
    /// `AV_OPT_FLAG_CHILD_CONSTS`, its children's.
    Object(ObjectRef<'a>),
    /// A class alone, as C's own callers pass a pointer to a class pointer:
    /// the constants of its table. C logs in the name of that fake object,
    /// so the evaluators refuse a class whose logging would read an object
    /// (one that is not valid for a [`BareObject`] for its name, `item_name`,
    /// `get_category` and offsets), and, for an option with
    /// `AV_OPT_FLAG_CHILD_CONSTS`, one with a `child_next` callback.
    Class(ClassRef<'a>),
}

impl<'a> From<ObjectRef<'a>> for EvalContext<'a> {
    fn from(obj: ObjectRef<'a>) -> Self {
        Self::Object(obj)
    }
}

impl<'a> From<ClassRef<'a>> for EvalContext<'a> {
    fn from(class: ClassRef<'a>) -> Self {
        Self::Class(class)
    }
}

/// A C numeric evaluator, `av_opt_eval_*`.
type EvalFn<T> =
    unsafe extern "C" fn(*mut c_void, *const ffi::AVOption, *const c_char, *mut T) -> c_int;

/// Numeric evaluators: a value string read as this option reads it, into a
/// Rust value rather than an object's field.
///
/// Each evaluates as [`ObjectMut::set`] parses the option, with the
/// context's named constants, `default`, `max`, `min`, `none` and `all`, and
/// checks the result against the option's bounds. The option need not
/// belong to the context's class.
///
/// # Errors
///
/// `AVERROR(EINVAL)` for an option whose type is not exactly the
/// evaluator's (an array of it included) or that is read-only, and for a
/// class context the evaluators refuse (see [`EvalContext::Class`]); the
/// error of an expression C cannot parse; `AVERROR(ERANGE)` (logged) for a
/// value outside the option's bounds. Also the values [`ObjectMut::set`]
/// refuses before C parses them for the same type: `AVERROR(EINVAL)` for an
/// expression (for flags, any token of it) whose evaluation C converts
/// unchecked, and `AVERROR(ERANGE)` for a flag operand outside the `int64_t`
/// range or a rational within wide bounds outside the `int` range.
impl OptionDefRef<'_> {
    /// Wraps: av_opt_eval_flags
    ///
    /// Evaluates `val` as this flags option's value into `flags`: `+` / `-`
    /// separated tokens, each a named constant of the option's unit or an
    /// expression over them, setting (`+`) or clearing (`-`) its bits, or
    /// replacing `flags` (unsigned). `flags` is read first and written after
    /// each token, so a token that fails leaves the earlier ones applied, as
    /// in C. A result must be a set of 32-bit flags.
    pub fn eval_flags<'c>(
        &self,
        ctx: impl Into<EvalContext<'c>>,
        val: &CStr,
        flags: &mut i32,
    ) -> Result<(), i32> {
        self.eval(
            ctx.into(),
            OptionType::FLAGS,
            val,
            flags,
            ffi::av_opt_eval_flags,
        )
    }

    /// Wraps: av_opt_eval_int
    ///
    /// Evaluates `val` as this `INT` option's value, rounded to the nearest
    /// integer.
    pub fn eval_int<'c>(&self, ctx: impl Into<EvalContext<'c>>, val: &CStr) -> Result<i32, i32> {
        let mut out = 0;
        self.eval(
            ctx.into(),
            OptionType::INT,
            val,
            &mut out,
            ffi::av_opt_eval_int,
        )?;
        Ok(out)
    }

    /// Wraps: av_opt_eval_uint
    ///
    /// Evaluates `val` as this `UINT` option's value, rounded to the nearest
    /// integer.
    pub fn eval_uint<'c>(&self, ctx: impl Into<EvalContext<'c>>, val: &CStr) -> Result<u32, i32> {
        let mut out = 0;
        self.eval(
            ctx.into(),
            OptionType::UINT,
            val,
            &mut out,
            ffi::av_opt_eval_uint,
        )?;
        Ok(out)
    }

    /// Wraps: av_opt_eval_int64
    ///
    /// Evaluates `val` as this `INT64` option's value, rounded to the nearest
    /// integer.
    pub fn eval_int64<'c>(&self, ctx: impl Into<EvalContext<'c>>, val: &CStr) -> Result<i64, i32> {
        let mut out = 0;
        self.eval(
            ctx.into(),
            OptionType::INT64,
            val,
            &mut out,
            ffi::av_opt_eval_int64,
        )?;
        Ok(out)
    }

    /// Wraps: av_opt_eval_float
    ///
    /// Evaluates `val` as this `FLOAT` option's value.
    pub fn eval_float<'c>(&self, ctx: impl Into<EvalContext<'c>>, val: &CStr) -> Result<f32, i32> {
        let mut out = 0.0;
        self.eval(
            ctx.into(),
            OptionType::FLOAT,
            val,
            &mut out,
            ffi::av_opt_eval_float,
        )?;
        Ok(out)
    }

    /// Wraps: av_opt_eval_double
    ///
    /// Evaluates `val` as this `DOUBLE` option's value.
    pub fn eval_double<'c>(&self, ctx: impl Into<EvalContext<'c>>, val: &CStr) -> Result<f64, i32> {
        let mut out = 0.0;
        self.eval(
            ctx.into(),
            OptionType::DOUBLE,
            val,
            &mut out,
            ffi::av_opt_eval_double,
        )?;
        Ok(out)
    }

    /// Wraps: av_opt_eval_q
    ///
    /// Evaluates `val` as this `RATIONAL` option's value: `num:den` or
    /// `num/den` as written when within the bounds, else an expression's
    /// value, kept as `value/1` when it is an integer and approximated within
    /// `int` parts (`av_d2q`) otherwise.
    pub fn eval_rational<'c>(
        &self,
        ctx: impl Into<EvalContext<'c>>,
        val: &CStr,
    ) -> Result<Rational, i32> {
        let mut out = ffi::AVRational { num: 0, den: 0 };
        self.eval(
            ctx.into(),
            OptionType::RATIONAL,
            val,
            &mut out,
            ffi::av_opt_eval_q,
        )?;
        Ok(Rational::from(out))
    }

    /// `eval`, the evaluator of type `ty`, on `val` into `out`, after the
    /// refusals the evaluators list.
    fn eval<T>(
        &self,
        ctx: EvalContext<'_>,
        ty: OptionType,
        val: &CStr,
        out: &mut T,
        eval: EvalFn<T>,
    ) -> Result<(), i32> {
        let child_consts = self.flags() & ffi::AV_OPT_FLAG_CHILD_CONSTS as c_int != 0;
        let class_ptr;
        let obj = match ctx {
            EvalContext::Object(obj) => obj.obj,
            EvalContext::Class(class) => {
                if !logs_as_bare(class) || (child_consts && class.child_next().is_some()) {
                    return Err(EINVAL);
                }
                class_ptr = class.as_ptr();
                NonNull::from(&class_ptr).cast()
            }
        };
        // C rejects every other option before it reads `val`.
        let parsed =
            self.option_type() == ty && self.flags() & ffi::AV_OPT_FLAG_READONLY as c_int == 0;
        if parsed && let Some(err) = elem_refusal(obj, *self, ty, val.to_bytes())? {
            return Err(err);
        }
        // A class context is a live pointer to a class whose logging reads
        // nothing past it, and which has no `child_next` for C to call on it
        // when the option asks for its children's constants.
        // SAFETY: `obj` is a live object under `ObjectRef`'s contract or such
        // a class pointer; the option is a table entry, `val` a live C string
        // and `out` a writable `T`; `elem_refusal` refused every value whose
        // evaluation could reach undefined behaviour.
        let ret = unsafe { eval(obj.as_ptr(), self.as_ptr(), val.as_ptr(), out) };
        if ret < 0 { Err(ret) } else { Ok(()) }
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn flag_is_set_reads_an_unconvertible_field_as_unset() {
            let o = Owned::new();
            let mut obj = object(&o);
            obj.set_double(c"dbl", 1e300, SearchFlags::NONE).unwrap();
            assert!(!obj.as_ref().flag_is_set(c"dbl_unit", c"dflag"));
            obj.set_double(c"dbl", 2.0, SearchFlags::NONE).unwrap();
            assert!(obj.as_ref().flag_is_set(c"dbl_unit", c"dflag"));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
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
        #[cfg_attr(miri, ignore = "calls C")]
        fn child_constants_are_looked_up_in_children() {
            let o = Owned::with_child();
            assert_eq!(refusal(&o, c"cflags", c"+one+1e30"), Some(ERANGE));
            assert_eq!(refusal(&o, c"cflags", c"+one+bit"), None);
            let mut obj = object(&o);
            obj.set(c"cflags", c"one+eight", SearchFlags::NONE).unwrap();
            assert_eq!(obj.as_ref().get_int(c"flags", SearchFlags::NONE), Ok(9));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
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
            let (opt, target) = obj.as_ref().lookup(c"flags", None, 0).unwrap();
            assert_eq!(number_value(target, opt, b"c1"), Ok(Some(1.0)));
            assert_eq!(number_value(target, opt, b"1+1"), Ok(None));
            assert_eq!(refusal(&o, c"flags", c"+1e30"), None);
            // C itself fails the same way (`AVERROR_PATCHWELCOME`), leaving
            // the value as it was.
            let patch_welcome = -i32::from_le_bytes(*b"PAWE");
            assert_eq!(
                object(&o).set(c"flags", c"1+1", SearchFlags::NONE),
                Err(patch_welcome)
            );
            assert_eq!(object(&o).set(c"flags", c"c2", SearchFlags::NONE), Ok(()));
            // SAFETY: as above, restoring the static class.
            unsafe { (*o.ptr()).class = &OBJ_CLASS.0 };
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn quiet_evaluation_uses_the_unit_constants() {
            let o = Owned::new();
            let obj = object(&o);
            let (flags, target) = obj.as_ref().lookup(c"flags", None, 0).unwrap();
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

        /// Options wave 0, batch-1 review: `time()` makes C's own evaluation
        /// of a previewed operand differ from the preview.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn quiet_evaluation_refuses_the_clock() {
            let o = Owned::new();
            let obj = object(&o);
            let (flags, target) = obj.as_ref().lookup(c"flags", None, 0).unwrap();
            for token in [
                &b"time(0)"[..],
                b"ti me (0)",
                b"1+time(1)*0",
                b"if(time(0),1,2)",
            ] {
                assert_eq!(number_value(target, flags, token), Err(EINVAL));
            }
            // Other names that merely contain it evaluate (or fail) as before.
            assert_eq!(number_value(target, flags, b"timer"), Ok(None));
            assert_eq!(number_value(target, flags, b"1"), Ok(Some(1.0)));
            let previewed = |name: &CStr, val: &[u8]| obj.as_ref().value_refusal(name, val, 0);
            assert_eq!(previewed(c"flags", b"+time(0)"), Err(EINVAL));
            assert_eq!(previewed(c"wide_q", b"time(0)"), Err(EINVAL));
            // No preview, no refusal: C's range check precedes the
            // conversion of an `int`-bounded rational.
            assert_eq!(previewed(c"q", b"time(0)"), Ok(None));
            assert_eq!(previewed(c"flags", b"time(0)"), Ok(None));
        }

        /// Options wave 0, batch-1 review: C reads an array option's
        /// `default_val.dbl`, the bits of its definition's address, as
        /// `default`; none reads as 0.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn array_defaults_read_the_definition_address() {
            let o = Owned::new();
            let obj = object(&o);
            let (array, _) = obj.as_ref().lookup(c"array", None, 0).unwrap();
            let address = (&raw const fixture::ARRAY_DEF.0).addr() as u64;
            let d = default_number(array);
            assert_eq!(d.to_bits(), address);
            assert!(d != 0.0 && d.is_subnormal());
            let (empty, _) = obj.as_ref().lookup(c"empty", None, 0).unwrap();
            assert_eq!(default_number(empty).to_bits(), 0);
            let (int, _) = obj.as_ref().lookup(c"int", None, 0).unwrap();
            assert_eq!(default_number(int), 7.0);
        }

        /// Options wave 0, batch-1 review: the pairs of
        /// `set_options_string` are read as C's `parse_key_value_pair` reads
        /// them, in the object and its children.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn option_lists_are_refused_pair_by_pair() {
            let o = Owned::with_child();
            let obj = object(&o);
            let refusal = |opts: &CStr, kv: &CStr, pairs: &CStr| {
                obj.as_ref().options_refusal(opts, kv, pairs).unwrap()
            };
            assert_eq!(refusal(c"int=1:flags=+1e30", c"=", c":"), Some(ERANGE));
            assert_eq!(refusal(c"fl\\ags = '+1e30'", c"=", c":"), Some(ERANGE));
            assert_eq!(refusal(c"int;flags@+1e30", c"@", c";"), None);
            assert_eq!(refusal(c"int@1;flags@+1e30", c"@", c";"), Some(ERANGE));
            assert_eq!(refusal(c"color=red@nan", c"=", c":"), Some(EINVAL));
            // C stops at a pair it cannot parse, before a later refusal.
            assert_eq!(refusal(c"int:flags=+1e30", c"=", c":"), None);
            assert_eq!(refusal(c"=1:flags=+1e30", c"=", c":"), None);
            assert_eq!(refusal(c"int=1:flags=+1", c"=", c":"), None);
            assert_eq!(refusal(c"", c"=", c":"), None);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
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
            // Options wave 0, batch-1 review: logging calls `item_name` and
            // `get_category` on the object, so only callbacks that read no
            // more than the class pointer are allowed.
            unsafe extern "C" fn name(_: *mut c_void) -> *const c_char {
                c"other".as_ptr()
            }
            unsafe extern "C" fn category(_: *mut c_void) -> ffi::AVClassCategory {
                ffi::AVClassCategory::AV_CLASS_CATEGORY_NA
            }
            // SAFETY: a copy of the live static class.
            let mut c = unsafe { BARE.as_ref().as_ptr().read() };
            for (item_name, get_category, bare) in [
                // Recognised by address, which Miri does not keep stable: there
                // it reads as another callback and is refused (the safe side).
                (Some(ffi::av_default_item_name as ItemName), None, cfg!(not(miri))),
                (None, None, true),
                (Some(name as ItemName), None, false),
                (None, Some(category as GetCategory), false),
            ] {
                c.item_name = item_name;
                c.get_category = get_category;
                // SAFETY: `c` is a live local class with no options.
                let class = unsafe { ClassRef::from_ptr(&raw mut c) }.unwrap();
                assert_eq!(BareObject::new(class).is_some(), bare);
            }
        }

        type ItemName = unsafe extern "C" fn(*mut c_void) -> *const c_char;
        type GetCategory = unsafe extern "C" fn(*mut c_void) -> ffi::AVClassCategory;
    }

    /// Options wave 1, batch 0: the typed getters on the fixture's objects,
    /// the corrected video rate getter, the refusals `get_array` makes before
    /// C runs, its element plumbing, and the potential children of a class.
    mod getters {
        use super::super::element::Element;
        use super::*;
        use crate::libavutil::channel_layout::ChannelOrder;
        use fixture::{ARR_CLASS, ARR_NAMES, ArrOwned, OBJ_CLASS, Owned};

        /// `AVERROR_OPTION_NOT_FOUND`.
        const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);
        const ENOSYS: i32 = -38;

        fn obj(o: *mut c_void) -> ObjectRef<'static> {
            // SAFETY: the callers' fixture objects are well-formed and outlive
            // every use of the handle in these tests, which only read them.
            unsafe { ObjectRef::from_ptr(o) }.unwrap()
        }

        fn class(c: &'static fixture::Class) -> ClassRef<'static> {
            // SAFETY: the fixture classes are well-formed statics.
            unsafe { ClassRef::from_ptr((&raw const c.0).cast_mut()) }.unwrap()
        }

        fn strings(v: Vec<Option<HeapString>>) -> Vec<Option<String>> {
            v.into_iter()
                .map(|s| s.map(|s| s.to_str().unwrap().to_owned()))
                .collect()
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn the_fixture_arrays_hold_their_defaults() {
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let sizes: Vec<usize> = ARR_NAMES
                .iter()
                .map(|n| r.get_array_size(n, SearchFlags::NONE).unwrap())
                .collect();
            assert_eq!(
                sizes,
                [4, 3, 2, 2, 4, 3, 3, 3, 2, 3, 2, 2, 2, 3, 2, 2, 3, 0]
            );
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn arrays_read_as_their_own_type() {
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let n = SearchFlags::NONE;
            assert_eq!(
                r.get_array::<i32>(c"ints", n, 0, 4),
                Ok(vec![1, -2, i32::MAX, i32::MIN])
            );
            assert_eq!(r.get_array::<u32>(c"uints", n, 1, 2), Ok(vec![u32::MAX, 9]));
            assert_eq!(
                r.get_array::<i64>(c"i64s", n, 0, 2),
                Ok(vec![-5_000_000_000, 123_456_789_012])
            );
            assert_eq!(r.get_array::<u64>(c"u64s", n, 0, 2), Ok(vec![42, 7]));
            assert_eq!(
                r.get_array::<f64>(c"dbls", n, 0, 3),
                Ok(vec![1.5, -2.0, 1e300])
            );
            assert_eq!(r.get_array::<f32>(c"flts", n, 0, 2), Ok(vec![0.25, -3e38]));
            let q = r.get_array::<Rational>(c"qs", n, 0, 3).unwrap();
            let q: Vec<(i32, i32)> = q
                .iter()
                .map(|q| (q.as_ref().num(), q.as_ref().den()))
                .collect();
            assert_eq!(q, [(1, 3), (5, 1), (-7, 2)]);
            let s = r.get_array::<Option<HeapString>>(c"strs", n, 0, 3).unwrap();
            assert_eq!(
                strings(s),
                [Some("x".into()), Some("".into()), Some("y,z".into())]
            );
            // An unset element, which C's parser never stores, reads as `None`.
            // SAFETY: `strs` holds three owned strings; nothing else accesses
            // the object during the write.
            unsafe { ffi::av_freep((*o.ptr()).strs.add(1).cast()) };
            let s = r.get_array::<Option<HeapString>>(c"strs", n, 1, 2).unwrap();
            assert_eq!(strings(s), [None, Some("y,z".into())]);
            let p = r.get_array::<PixelFormat>(c"pixs", n, 0, 3).unwrap();
            assert_eq!(
                p,
                [PixelFormat::YUV420P, PixelFormat::RGB24, PixelFormat::NONE]
            );
            let f = r.get_array::<SampleFormat>(c"samples", n, 0, 2).unwrap();
            assert_eq!(f, [SampleFormat::S16, SampleFormat::FLTP]);
            // C copies a dictionary element without flags: `A` replaces `a`.
            let d = r.get_array::<Dict>(c"dicts", n, 0, 2).unwrap();
            let d: Vec<String> = d
                .iter()
                .map(|d| {
                    d.serialize(b'=', b';')
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned()
                })
                .collect();
            assert_eq!(d, ["b=2;A=3", "k=v"]);
            let l = r
                .get_array::<ChannelLayoutVal>(c"layouts", n, 0, 3)
                .unwrap();
            let orders: Vec<_> = l
                .iter()
                .map(|l| (l.as_ref().order(), l.as_ref().channel_count()))
                .collect();
            assert_eq!(
                orders,
                [
                    (ChannelOrder::NATIVE, 2),
                    (ChannelOrder::NATIVE, 6),
                    (ChannelOrder::NATIVE, 2)
                ]
            );
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn arrays_convert_as_c_converts_them() {
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let n = SearchFlags::NONE;
            assert_eq!(r.get_array::<i64>(c"dbls", n, 0, 2), Ok(vec![1, -2]));
            assert_eq!(r.get_array::<i64>(c"qs", n, 0, 3), Ok(vec![0, 5, -3]));
            assert_eq!(r.get_array::<f64>(c"ints", n, 0, 2), Ok(vec![1.0, -2.0]));
            assert_eq!(r.get_array::<i64>(c"bools", n, 0, 3), Ok(vec![1, 0, -1]));
            let s = r
                .get_array::<Option<HeapString>>(c"sizes", n, 0, 2)
                .unwrap();
            assert_eq!(
                strings(s),
                [Some("640x480".into()), Some("1280x720".into())]
            );
            // C converts to `int` only from an `int` array.
            assert_eq!(r.get_array::<i32>(c"dbls", n, 0, 1), Err(ENOSYS));
            // A video rate has no numeric value in C's reader.
            assert_eq!(r.get_array::<f64>(c"rates", n, 0, 1), Err(EINVAL));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn ranges_outside_the_array_and_missing_options_fail_before_any_output() {
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let n = SearchFlags::NONE;
            assert_eq!(r.get_array::<i32>(c"ints", n, 4, 0), Err(EINVAL));
            assert_eq!(r.get_array::<i32>(c"ints", n, 3, 2), Err(EINVAL));
            assert_eq!(r.get_array::<i32>(c"ints", n, 0, u32::MAX), Err(EINVAL));
            assert_eq!(r.get_array::<i32>(c"ints", n, u32::MAX, 1), Err(EINVAL));
            assert_eq!(r.get_array::<i32>(c"ints", n, 3, 0), Ok(vec![]));
            assert_eq!(r.get_array::<i32>(c"missing", n, 0, 0), Err(NOT_FOUND));
            assert_eq!(r.get_array::<i32>(c"bins", n, 0, 0), Err(EINVAL));
            let plain = Owned::new();
            assert_eq!(
                obj(plain.as_obj()).get_array::<i32>(c"int", n, 0, 1),
                Err(EINVAL)
            );
        }

        /// C's `int64_t` result of a double element out of range is
        /// undefined, so it is refused with `ERANGE` after a `double`
        /// preview, whose values C computes the same way.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn out_of_range_int64_conversions_are_refused() {
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let n = SearchFlags::NONE;
            assert_eq!(r.get_array::<i64>(c"dbls", n, 0, 3), Err(ERANGE));
            assert_eq!(r.get_array::<i64>(c"dbls", n, 2, 1), Err(ERANGE));
            assert_eq!(r.get_array::<i64>(c"flts", n, 0, 2), Err(ERANGE));
            assert_eq!(r.get_array::<i64>(c"flts", n, 0, 1), Ok(vec![0]));
            // A rational of zero denominator is an infinity, or NaN for 0/0.
            for (num, den) in [(1, 0), (0, 0), (-1, 0)] {
                // SAFETY: `qs` holds three elements; nothing else accesses
                // the object during the write.
                unsafe { (*o.ptr()).qs.write(ffi::AVRational { num, den }) };
                assert_eq!(r.get_array::<i64>(c"qs", n, 0, 1), Err(ERANGE));
                assert_eq!(r.get_array::<i64>(c"qs", n, 1, 2), Ok(vec![5, -3]));
                let v = r.get_array::<f64>(c"qs", n, 0, 1).unwrap()[0];
                assert!(v.is_infinite() || v.is_nan());
            }
            // The extremes of the range.
            // SAFETY: as above, for `dbls`, which holds four.
            unsafe {
                (*o.ptr()).dbls.write(-9_223_372_036_854_775_808.0);
                (*o.ptr()).dbls.add(1).write(9_223_372_036_854_774_784.0);
                (*o.ptr()).dbls.add(2).write(9_223_372_036_854_775_808.0);
            }
            assert_eq!(
                r.get_array::<i64>(c"dbls", n, 0, 2),
                Ok(vec![i64::MIN, 9_223_372_036_854_774_784])
            );
            assert_eq!(r.get_array::<i64>(c"dbls", n, 1, 2), Err(ERANGE));
        }

        /// C reads a binary element's length past it, out of the array for
        /// the last one: refused before C runs.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn binary_arrays_are_refused_before_c() {
            let o = ArrOwned::new();
            // SAFETY: an `av_mallocz` array of two NULL buffers, which the
            // object owns from here and `av_opt_free` releases.
            unsafe {
                let bins = ffi::av_mallocz(2 * size_of::<*mut u8>()).cast::<*mut u8>();
                assert!(!bins.is_null());
                (*o.ptr()).bins = bins;
                (*o.ptr()).bins_len = 2;
            }
            let r = obj(o.as_obj());
            assert_eq!(r.get_array_size(c"bins", SearchFlags::NONE), Ok(2));
            let n = SearchFlags::NONE;
            let read = |start, count| r.get_array::<Option<HeapString>>(c"bins", n, start, count);
            assert_eq!(read(0, 2).err(), Some(EINVAL));
            assert_eq!(read(1, 1).err(), Some(EINVAL));
            assert_eq!(r.get_array::<i64>(c"bins", n, 0, 0), Err(EINVAL));
        }

        /// Runs the test `name` alone in a child process, where it may lower
        /// the process-wide allocation limit; true in that child.
        fn isolated(name: &str) -> bool {
            const KEY: &str = "CRUSTIFY_UNIT_OPT_GETTERS_ISOLATED";
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

        /// C's allocation failures part-way through an array read, under an
        /// `av_max_alloc` limit: C releases the elements it wrote (as
        /// LeakSanitizer checks) and the read fails with `ENOMEM`.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn allocation_failures_release_the_elements_written() {
            if !isolated("libavutil::opt::unit_tests::getters::allocation_failures_release_the_elements_written") {
                return;
            }
            let o = ArrOwned::new();
            let r = obj(o.as_obj());
            let n = SearchFlags::NONE;
            let enomem = Some(-(ffi::ENOMEM as i32));
            // `x` and `` copy within 3 bytes; `y,z` does not.
            crate::libavutil::mem::set_max_alloc(3);
            let strings = r.get_array::<Option<HeapString>>(c"strs", n, 0, 3);
            assert_eq!(strings.err(), enomem);
            // A formatted number is copied out of C's stack buffer: `1` and
            // `-2` fit, `2147483647` does not.
            let formatted = r.get_array::<Option<HeapString>>(c"ints", n, 0, 3);
            assert_eq!(formatted.err(), enomem);
            // A dictionary formatted by `av_dict_get_string` (`b=2:A=3` takes
            // 8 bytes), and copied (a dictionary takes 16).
            crate::libavutil::mem::set_max_alloc(7);
            let formatted = r.get_array::<Option<HeapString>>(c"dicts", n, 0, 2);
            assert_eq!(formatted.err(), enomem);
            assert_eq!(r.get_array::<Dict>(c"dicts", n, 0, 2).err(), enomem);
            crate::libavutil::mem::set_max_alloc(i32::MAX as usize);
            let strings = r.get_array::<Option<HeapString>>(c"strs", n, 0, 3).unwrap();
            assert_eq!(strings.len(), 3);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn element_plumbing() {
            assert_eq!(<i32 as Element>::zero(), 0);
            assert!(<Option<HeapString> as Element>::zero().is_null());
            assert!(<Dict as Element>::zero().is_null());
            // SAFETY: NULL is the empty value of each.
            assert!(unsafe { <Option<HeapString> as Element>::adopt(ptr::null_mut()) }.is_none());
            // SAFETY: as above.
            assert!(unsafe { <Dict as Element>::adopt(ptr::null_mut()) }.is_empty());
            let l = <ChannelLayoutVal as Element>::zero();
            assert_eq!(
                (l.as_ref().order(), l.as_ref().channel_count()),
                (ChannelOrder::UNSPEC, 0)
            );
            // SAFETY: the layout is all zero, as C leaves its output elements.
            let zero = unsafe { core::mem::zeroed::<ffi::AVChannelLayout>() };
            // SAFETY: both are `size_of::<AVChannelLayout>()` readable bytes.
            let same = unsafe {
                core::slice::from_raw_parts(
                    ptr::from_ref(&l).cast::<u8>(),
                    size_of::<ffi::AVChannelLayout>(),
                ) == core::slice::from_raw_parts(
                    ptr::from_ref(&zero).cast::<u8>(),
                    size_of::<ffi::AVChannelLayout>(),
                )
            };
            assert!(same);
            assert_eq!(<Rational as Element>::TYPE, OptionType::RATIONAL);
            assert_eq!(
                <ChannelLayoutVal as Element>::TYPE,
                OptionType::CHANNEL_LAYOUT
            );
        }

        /// Options wave 1, batch 0: `av_opt_get_video_rate` forwards to
        /// `av_opt_get_q`, which has no reader for a video rate option and
        /// fails with `EINVAL`; the wrapper reads the stored rate instead.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn video_rates_read_back_where_c_fails() {
            let o = Owned::new();
            let r = obj(o.as_obj());
            let mut c = ffi::AVRational { num: 0, den: 0 };
            let rate = c"rate".as_ptr();
            // SAFETY: a well-formed object, a live name and a writable rate.
            let ret = unsafe { ffi::av_opt_get_video_rate(o.as_obj(), rate, 0, &mut c) };
            assert_eq!(ret, EINVAL);
            let rate = r.get_video_rate(c"rate", SearchFlags::NONE).unwrap();
            assert_eq!((rate.as_ref().num(), rate.as_ref().den()), (25, 1));
            assert_eq!(
                r.get_rational(c"rate", SearchFlags::NONE).err(),
                Some(EINVAL)
            );
            // Other options read as C reads them.
            let q = r.get_video_rate(c"q", SearchFlags::NONE).unwrap();
            assert_eq!((q.as_ref().num(), q.as_ref().den()), (1, 2));
            assert_eq!(
                r.get_video_rate(c"missing", SearchFlags::NONE).err(),
                Some(NOT_FOUND)
            );
            assert_eq!(
                r.get_video_rate(c"string", SearchFlags::NONE).err(),
                Some(EINVAL)
            );
            // In a child, too.
            let p = Owned::with_child();
            // SAFETY: the child is live and well-formed; a rate is 8 bytes.
            unsafe {
                (*p.child()).rate = ffi::AVRational {
                    num: 30000,
                    den: 1001,
                }
            };
            let r = obj(p.as_obj());
            let rate = r.get_video_rate(c"rate", SearchFlags::CHILDREN).unwrap();
            assert_eq!((rate.as_ref().num(), rate.as_ref().den()), (30000, 1001));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn potential_children_follow_the_class_callback() {
            let arr = class(&ARR_CLASS);
            let children: Vec<_> = arr.potential_children().collect();
            assert_eq!(children.len(), 2);
            assert!(children[0].ptr_eq(class(&OBJ_CLASS)));
            assert!(children[1].ptr_eq(arr));
            let via_field: Vec<_> = arr.child_classes().collect();
            assert!(via_field.iter().zip(&children).all(|(a, b)| a.ptr_eq(*b)));
            let mut it = arr.potential_children();
            assert_eq!(it.by_ref().count(), 2);
            // Fused: C would start again from the state's end.
            assert!(it.next().is_none() && it.next().is_none());
            assert_eq!(class(&OBJ_CLASS).potential_children().count(), 0);
        }
    }

    /// Options wave 1, batch 1: the typed setters' and array editors'
    /// refusals, element conversions and the corrections of C's defects.
    mod typed_setters {
        use super::*;
        use crate::libavutil::channel_layout::ChannelLayoutVal;
        use crate::libavutil::channel_layout::{ChannelCustom, ChannelLayout};
        use fixture::{Owned, OwnedArrays};

        /// The exclusive handle of a fixture object.
        fn object(o: &Owned) -> ObjectMut<'_> {
            // SAFETY: `o` is a live, defaulted object of the well-formed
            // `OBJ_CLASS`, used through no other handle while this one lives.
            unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
        }

        /// The exclusive handle of an array fixture object.
        fn arrays(o: &OwnedArrays) -> ObjectMut<'_> {
            // SAFETY: `o` is a live, defaulted object of the well-formed
            // `ARRAYS_CLASS`, used through no other handle while this one
            // lives.
            unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
        }

        /// The option `name` of `OBJ_CLASS`.
        fn option(name: &CStr) -> OptionDefRef<'static> {
            fixture::OBJ_OPTIONS
                .0
                .iter()
                // SAFETY: an entry of the immutable static table.
                .map(|o| unsafe { OptionDefRef::from_ptr(ptr::from_ref(o).cast_mut()) }.unwrap())
                .find(|o| o.name() == Some(name))
                .unwrap()
        }

        /// A custom layout of two channels whose first name fills its field
        /// without a NUL.
        fn unterminated() -> ChannelLayoutVal {
            let mut l = ChannelLayout::custom(2).unwrap();
            let mut map = l.as_mut().into_custom_map_mut().unwrap();
            map.get_mut(0)
                .unwrap()
                .set(ChannelCustom::from_raw(ffi::AVChannelCustom {
                    id: ffi::AVChannel::AV_CHAN_FRONT_LEFT,
                    name: [b'n' as c_char; 16],
                    opaque: ptr::null_mut(),
                }));
            map.get_mut(1)
                .unwrap()
                .set_id(crate::libavutil::channel_layout::Channel::FRONT_RIGHT);
            l
        }

        #[test]
        fn write_refusal_mirrors_write_number() {
            let (q, wide, rate, int) = (
                option(c"q"),
                option(c"wide_q"),
                option(c"rate"),
                option(c"int"),
            );
            // Only rationals and video rates convert to `int` unchecked.
            assert_eq!(write_refusal(int, f64::NAN, 1, 1), None);
            // NaN passes C's range check, then `(int) num` is undefined.
            assert_eq!(write_refusal(q, f64::NAN, 1, 1), Some(ERANGE));
            assert_eq!(write_refusal(rate, f64::NAN, 1, 1), Some(ERANGE));
            // Out of bounds: C's own `AVERROR(ERANGE)`, before converting.
            assert_eq!(write_refusal(q, 1e10, 1, 1), None);
            assert_eq!(write_refusal(q, -1.0, 1, 1), None);
            // A zero denominator is out of range.
            assert_eq!(write_refusal(wide, f64::NAN, 0, 1), None);
            // A negative denominator flips C's bounds: `max * den < num`.
            // With symmetric bounds, every finite value fails it.
            assert_eq!(write_refusal(q, -5.0, -1, 1), None);
            assert_eq!(write_refusal(wide, 1e10, -1, 1), None);
            assert_eq!(write_refusal(wide, f64::NAN, -1, 1), Some(ERANGE));
            // Within wide bounds: `num` beyond `int`, or an integral `num`
            // whose product with `intnum` is.
            assert_eq!(write_refusal(wide, 1e10, 1, 1), Some(ERANGE));
            assert_eq!(write_refusal(wide, 1.0, 1, 1 << 40), Some(ERANGE));
            assert_eq!(write_refusal(wide, 2.5, 1, 1 << 40), None);
            assert_eq!(write_refusal(wide, 2.5, 1, 1), None);
            assert_eq!(write_refusal(wide, -2_147_483_648.0, 1, 1), None);
            assert_eq!(write_refusal(wide, 2_147_483_647.0, 7, 1), None);
            assert_eq!(write_refusal(wide, 2_147_483_648.0, 7, 1), Some(ERANGE));
        }

        #[test]
        fn array_elements_name_their_c_types() {
            use array_input::Sealed;
            assert_eq!(<&CStr as Sealed>::TYPE, OptionType::STRING);
            assert_eq!(<Option<&CStr> as Sealed>::TYPE, OptionType::STRING);
            assert_eq!(<i32 as Sealed>::TYPE, OptionType::INT);
            assert_eq!(<i64 as Sealed>::TYPE, OptionType::INT64);
            assert_eq!(<u32 as Sealed>::TYPE, OptionType::UINT);
            assert_eq!(<u64 as Sealed>::TYPE, OptionType::UINT64);
            assert_eq!(<f32 as Sealed>::TYPE, OptionType::FLOAT);
            assert_eq!(<f64 as Sealed>::TYPE, OptionType::DOUBLE);
            assert_eq!(<Rational as Sealed>::TYPE, OptionType::RATIONAL);
            assert_eq!(<PixelFormat as Sealed>::TYPE, OptionType::PIXEL_FORMAT);
            assert_eq!(<SampleFormat as Sealed>::TYPE, OptionType::SAMPLE_FORMAT);
            assert_eq!(<(i32, i32) as Sealed>::TYPE, OptionType::IMAGE_SIZE);
            assert_eq!(<[u8; 4] as Sealed>::TYPE, OptionType::COLOR);
            assert_eq!(
                <ChannelLayoutRef<'_> as Sealed>::TYPE,
                OptionType::CHANNEL_LAYOUT
            );
            assert_eq!(
                <Option<DictionaryRef<'_>> as Sealed>::TYPE,
                OptionType::DICT
            );
            // Each C type is the size `opt_type_desc` gives the type.
            assert_eq!(size_of::<<(i32, i32) as Sealed>::C>(), 8);
            assert_eq!(size_of::<<Rational as Sealed>::C>(), 8);
            assert_eq!(size_of::<<ChannelLayoutRef<'_> as Sealed>::C>(), 24);
            assert_eq!(size_of::<<PixelFormat as Sealed>::C>(), size_of::<c_int>());

            let s = c"abc";
            assert_eq!(s.to_c(), s.as_ptr());
            assert_eq!(Some(s).to_c(), s.as_ptr());
            assert!(None::<&CStr>.to_c().is_null());
            assert_eq!((3, -4).to_c(), [3, -4]);
            assert_eq!([1u8, 2, 3, 4].to_c(), [1, 2, 3, 4]);
            let q = Rational::from_raw(ffi::AVRational { num: 3, den: -7 });
            assert_eq!((q.to_c().num, q.to_c().den), (3, -7));
            assert_eq!(
                PixelFormat::NONE.to_c(),
                ffi::AVPixelFormat::AV_PIX_FMT_NONE
            );
            assert_eq!(
                SampleFormat::S16.to_c(),
                ffi::AVSampleFormat::AV_SAMPLE_FMT_S16
            );
            assert!(None::<DictionaryRef<'_>>.to_c().is_null());
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn borrowed_elements_point_into_their_values() {
            use array_input::Sealed;
            let d = Dict::from_entries([(c"k", c"v")]).unwrap();
            assert_eq!(d.as_ref().to_c(), d.as_ref().unwrap().as_ptr());
            let l = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
            let raw = l.as_ref().to_c();
            // The copy shares the map; it is never released.
            // SAFETY: a union read of the custom map pointer of a live layout.
            assert_eq!(unsafe { raw.u.map }, unsafe {
                (*l.as_ref().as_ptr()).u.map
            });
            assert_eq!(raw.nb_channels, 2);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn binary_arrays_are_refused_before_c_overruns_them() {
            let o = OwnedArrays::new();
            let mut obj = arrays(&o);
            let r = obj.as_ref();
            assert_eq!(r.array_refusal(c"bins", &[c"0a"], 0), Ok(Some(EINVAL)));
            assert_eq!(r.array_refusal::<&CStr>(c"bins", &[], 0), Ok(Some(EINVAL)));
            assert_eq!(r.value_refusal(c"bins", b"0a,0b", 0), Ok(Some(EINVAL)));
            // An empty value splits into no element: C stores nothing.
            assert_eq!(r.value_refusal(c"bins", b"", 0), Ok(None));
            // C writes each element's 4-byte length past its 8-byte slot
            // (ASan: heap-buffer-overflow, opt.c:373); refused instead.
            assert_eq!(
                obj.insert_array(c"bins", 0, &[c"0a"], SearchFlags::NONE),
                Err(EINVAL)
            );
            assert_eq!(
                obj.replace_array(c"bins", 0, &[None::<&CStr>], SearchFlags::NONE),
                Err(EINVAL)
            );
            assert_eq!(obj.set(c"bins", c"0a0b", SearchFlags::NONE), Err(EINVAL));
            assert_eq!(obj.set(c"bins", c"", SearchFlags::NONE), Ok(()));
            assert_eq!(obj.remove_array(c"bins", 0, 0, SearchFlags::NONE), Ok(()));
            // SAFETY: a raw read of the live object's field.
            let bins = unsafe { (*o.ptr()).bins.len };
            assert_eq!(bins, 0);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn array_values_reaching_undefined_conversions_are_refused() {
            let o = OwnedArrays::new();
            let obj = arrays(&o);
            let r = obj.as_ref();
            let refused = Ok(Some(ERANGE));
            assert_eq!(r.array_refusal(c"qs", &[f64::NAN], 0), refused);
            assert_eq!(r.array_refusal(c"rates", &[f32::NAN], 0), refused);
            assert_eq!(r.array_refusal(c"wide_qs", &[1e10], 0), refused);
            assert_eq!(r.array_refusal(c"wide_qs", &[1i64 << 40], 0), refused);
            assert_eq!(r.array_refusal(c"wide_qs", &[i32::MAX], 0), Ok(None));
            // The second value is refused: the whole edit is.
            assert_eq!(r.array_refusal(c"wide_qs", &[2.5, 1e10], 0), refused);
            // Rationals keep their `int` parts.
            let q = [Rational::new(i32::MIN, -1)];
            assert_eq!(r.array_refusal(c"wide_qs", &q, 0), Ok(None));
            // Values of the element type are copied and range-checked as
            // doubles: no conversion.
            let q = [Rational::new(1, 0)];
            assert_eq!(r.array_refusal(c"qs", &q, 0), Ok(None));
            assert_eq!(r.array_refusal(c"dbls", &[f64::NAN], 0), Ok(None));
            assert_eq!(r.array_refusal(c"ints", &[i32::MIN], 0), Ok(None));
            // Strings are refused as `set` refuses them.
            let einval = Ok(Some(EINVAL));
            assert_eq!(r.array_refusal(c"dbls", &[c"ld(1e300)"], 0), einval);
            assert_eq!(r.array_refusal(c"colors", &[c"red@nan"], 0), einval);
            assert_eq!(r.array_refusal(c"flags", &[c"+1e30"], 0), refused);
            let amb = [c"ambisonic 2147483647"];
            assert_eq!(r.array_refusal(c"layouts", &amb, 0), einval);
            // A string array copies its strings, unparsed.
            assert_eq!(r.array_refusal(c"strs", &[c"ld(1e300)"], 0), Ok(None));
            assert_eq!(r.array_refusal(c"dbls", &[None::<&CStr>], 0), Ok(None));
            // A layout the option's describers cannot read.
            let bad = unterminated();
            assert_eq!(r.array_refusal(c"layouts", &[bad.as_ref()], 0), einval);
            assert_eq!(r.array_refusal(c"ints", &[bad.as_ref()], 0), Ok(None));
            // C's own checks: no such option, read-only, not an array.
            for name in [c"missing", c"ro", c"scalar"] {
                assert_eq!(r.array_refusal(name, &[f64::NAN], 0), Ok(None), "{name:?}");
            }
            // The replace flag does not hide the option.
            assert_eq!(r.array_refusal(c"qs", &[f64::NAN], ARRAY_REPLACE), refused);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn refused_edits_leave_the_array() {
            let o = OwnedArrays::new();
            let mut obj = arrays(&o);
            obj.insert_array(c"wide_qs", 0, &[Rational::new(1, 2)], SearchFlags::NONE)
                .unwrap();
            assert_eq!(
                obj.insert_array(c"wide_qs", 0, &[0.5, f64::NAN], SearchFlags::NONE),
                Err(ERANGE)
            );
            assert_eq!(
                obj.replace_array(c"wide_qs", 0, &[i64::MAX], SearchFlags::NONE),
                Err(ERANGE)
            );
            assert_eq!(
                obj.as_ref()
                    .get(c"wide_qs", SearchFlags::NONE)
                    .unwrap()
                    .as_bytes(),
                b"1/2"
            );
            // Indices beyond `unsigned` never reach C.
            if usize::BITS > 32 {
                let far = u32::MAX as usize + 1;
                assert_eq!(
                    obj.insert_array(c"wide_qs", far, &[1.0], SearchFlags::NONE),
                    Err(EINVAL)
                );
                assert_eq!(
                    obj.remove_array(c"wide_qs", 0, far, SearchFlags::NONE),
                    Err(EINVAL)
                );
                assert_eq!(
                    obj.remove_array(c"wide_qs", far, 0, SearchFlags::NONE),
                    Err(EINVAL)
                );
            }
            assert_eq!(
                obj.as_ref().get_array_size(c"wide_qs", SearchFlags::NONE),
                Ok(1)
            );
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn unreadable_layouts_are_refused() {
            let o = Owned::new();
            let mut obj = object(&o);
            let bad = unterminated();
            assert_eq!(
                obj.set_channel_layout(c"layout", bad.as_ref(), SearchFlags::NONE),
                Err(EINVAL)
            );
            assert_eq!(
                obj.as_ref()
                    .get(c"layout", SearchFlags::NONE)
                    .unwrap()
                    .as_bytes(),
                b"stereo"
            );
            let good = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
            obj.set_channel_layout(c"layout", good.as_ref(), SearchFlags::NONE)
                .unwrap();
            assert_eq!(
                obj.as_ref()
                    .get(c"layout", SearchFlags::NONE)
                    .unwrap()
                    .as_bytes(),
                b"2 channels (FL@a+FR@b)"
            );
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn layouts_left_without_a_map_are_reset() {
            let o = Owned::with_child();
            let p = o.ptr();
            // SAFETY: raw writes of the live object's layout, which owns no
            // map in native order, before any handle exists: the state C's
            // failed copy leaves.
            unsafe {
                (*p).layout = ffi::AVChannelLayout {
                    order: ffi::AVChannelOrder::AV_CHANNEL_ORDER_CUSTOM,
                    nb_channels: 3,
                    opaque: ptr::dangling_mut(),
                    ..Default::default()
                };
            }
            let mut obj = object(&o);
            // The child, searched first, holds a readable layout: left alone.
            obj.reset_unmapped_layout(c"layout", ffi::AV_OPT_SEARCH_CHILDREN as c_int);
            obj.reset_unmapped_layout(c"int", 0);
            obj.reset_unmapped_layout(c"missing", 0);
            // SAFETY: a raw read of the live object's layout.
            assert_eq!(unsafe { (*p).layout.nb_channels }, 3);
            obj.reset_unmapped_layout(c"layout", 0);
            // SAFETY: as above.
            let l = unsafe { (*p).layout };
            assert_eq!(l.order, ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC);
            assert_eq!(l.nb_channels, 0);
            assert!(l.opaque.is_null());
            // A custom layout with a map is left alone.
            let custom = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
            obj.set_channel_layout(c"layout", custom.as_ref(), SearchFlags::NONE)
                .unwrap();
            obj.reset_unmapped_layout(c"layout", 0);
            let got = obj.as_ref().get(c"layout", SearchFlags::NONE).unwrap();
            assert_eq!(got.as_bytes(), b"2 channels (FL@a+FR@b)");
        }

        /// The corrected behaviour: when `av_channel_layout_copy` cannot
        /// allocate a custom map, C leaves the option a custom layout of two
        /// channels and a NULL map, which `av_opt_get` would describe through
        /// the NULL pointer; the wrapper leaves the empty layout. Runs alone
        /// in a child: the allocation limit is process-wide.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn failed_layout_copies_leave_the_empty_layout() {
            const KEY: &str = "CRUSTIFY_OPT_UNIT_LAYOUT_ENOMEM";
            if std::env::var_os(KEY).is_none() {
                let name = "libavutil::opt::unit_tests::typed_setters::failed_layout_copies_leave_the_empty_layout";
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--test-threads=1"])
                    .env(KEY, "1")
                    .status()
                    .unwrap();
                assert!(status.success(), "isolated run failed: {status}");
                return;
            }
            let o = Owned::with_child();
            let mut obj = object(&o);
            let custom = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
            // SAFETY: `av_max_alloc` takes a size by value; this process
            // runs nothing else.
            unsafe { ffi::av_max_alloc(16) };
            let ret = obj.set_channel_layout(c"layout", custom.as_ref(), SearchFlags::CHILDREN);
            // SAFETY: as above.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            assert_eq!(ret, Err(-(ffi::ENOMEM as i32)));
            // The child's layout was found first.
            for p in [o.child(), o.ptr()] {
                // SAFETY: raw reads of the live objects' layouts.
                let (order, n) = unsafe { ((*p).layout.order, (*p).layout.nb_channels) };
                if p == o.child() {
                    assert_eq!(
                        (order, n),
                        (ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC, 0)
                    );
                } else {
                    assert_eq!(
                        (order, n),
                        (ffi::AVChannelOrder::AV_CHANNEL_ORDER_NATIVE, 2)
                    );
                }
            }
            let got = obj.as_ref().get(c"layout", SearchFlags::CHILDREN).unwrap();
            assert_eq!(got.as_bytes(), b"0 channels");
        }

        /// The corrected behaviour (options wave 1 review): when C cannot
        /// shrink an array's block after a removal, it has released the
        /// range and shifted the rest down but keeps the old count; the
        /// wrapper writes the new count back. Runs alone in a child: the
        /// allocation limit is process-wide.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn failed_shrinks_store_the_new_count() {
            const KEY: &str = "CRUSTIFY_OPT_UNIT_SHRINK_ENOMEM";
            if std::env::var_os(KEY).is_none() {
                let name =
                    "libavutil::opt::unit_tests::typed_setters::failed_shrinks_store_the_new_count";
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--test-threads=1"])
                    .env(KEY, "1")
                    .status()
                    .unwrap();
                assert!(status.success(), "isolated run failed: {status}");
                return;
            }
            let o = OwnedArrays::zeroed_with_child();
            let mut obj = arrays(&o);
            for f in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                obj.insert_array(c"ints", 0, &[1, 2, 3, 4], f).unwrap();
                crate::libavutil::mem::set_max_alloc(1);
                let ret = obj.remove_array(c"ints", 1, 2, f);
                crate::libavutil::mem::set_max_alloc(i32::MAX as usize);
                assert_eq!(ret, Err(ENOMEM));
                let p = if f == SearchFlags::NONE {
                    o.ptr()
                } else {
                    o.child()
                };
                // SAFETY: raw reads of the live object's `ints` array, whose
                // count elements are initialised.
                let ints = unsafe {
                    let a = &raw const (*p).ints;
                    core::slice::from_raw_parts((*a).elems, (*a).len as usize).to_vec()
                };
                assert_eq!(ints, [1, 4]);
                // Removals that succeed still shrink the array normally.
                obj.remove_array(c"ints", 0, 2, f).unwrap();
                assert_eq!(obj.as_ref().get_array_size(c"ints", f), Ok(0));
            }
            // No count to repair: no such option, or not an array.
            assert!(obj.as_ref().array_count_field(c"nosuch", 0).is_none());
            assert!(obj.as_ref().array_count_field(c"scalar", 0).is_none());
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn refused_dictionary_entries_set_nothing() {
            let o = Owned::new();
            let mut obj = object(&o);
            let mut options =
                Dict::from_entries([(c"int", c"5"), (c"dbl", c"ld(1e300)"), (c"nosuch", c"x")])
                    .unwrap();
            assert_eq!(obj.set_from_dict(&mut options), Err(EINVAL));
            assert_eq!(obj.as_ref().get_int(c"int", SearchFlags::NONE), Ok(7));
            // The dictionary is left as it was, every entry in order.
            let before = options.as_ref().unwrap().as_ptr();
            assert_eq!(
                obj.set_from_dict_with(&mut options, SearchFlags::CHILDREN),
                Err(EINVAL)
            );
            assert_eq!(options.as_ref().unwrap().as_ptr(), before);
            let mut empty = Dict::new();
            assert_eq!(obj.set_from_dict(&mut empty), Ok(()));
            assert!(empty.as_ref().is_none());
        }
    }

    /// Options wave 2, batch 0: the table check behind
    /// `is_option_set_to_default`, the evaluators' refusals before C (class
    /// contexts whose logging reads an object, values whose evaluation C
    /// converts unchecked), the option walk's iterator plumbing and the range
    /// flags.
    mod lookups {
        use super::*;
        use fixture::{OBJ_CLASS, OBJ_OPTIONS, Owned};

        /// The shared handle of a fixture object.
        fn shared(o: &Owned) -> ObjectRef<'_> {
            // SAFETY: `o` is a live object of the well-formed `OBJ_CLASS`,
            // which nothing writes while the handle lives.
            unsafe { ObjectRef::from_ptr(o.as_obj()) }.unwrap()
        }

        /// The handle of `class`, a static fixture class.
        fn class(class: &'static fixture::Class) -> ClassRef<'static> {
            // SAFETY: the fixture classes are well-formed statics.
            unsafe { ClassRef::from_ptr((&raw const class.0).cast_mut()) }.unwrap()
        }

        /// The option `name` of `OBJ_CLASS`.
        fn option(name: &CStr) -> OptionDefRef<'static> {
            class(&OBJ_CLASS)
                .find(name, None, 0, SearchFlags::NONE)
                .unwrap()
        }

        #[test]
        fn only_entries_of_the_object_class_table_are_its_options() {
            // A zeroed stack object: the check reads only the class, so the
            // test needs no C (and runs under Miri).
            // SAFETY: every field of `Obj` is valid all-zero.
            let mut raw: fixture::Obj = unsafe { core::mem::zeroed() };
            raw.class = &OBJ_CLASS.0;
            // SAFETY: a live object of the well-formed `OBJ_CLASS`, unwritten
            // while the handle lives.
            let obj = unsafe { ObjectRef::from_ptr((&raw mut raw).cast()) }.unwrap();
            let n = OBJ_OPTIONS.0.len() - 1;
            for i in 0..n {
                // SAFETY: an entry of the live static table.
                let e = unsafe { OptionDefRef::from_ptr((&raw const OBJ_OPTIONS.0[i]).cast_mut()) };
                assert!(obj.has_option(e.unwrap()), "entry {i}");
            }
            // Entries of another table, the terminator and the entry after
            // it are not.
            assert!(!obj.has_option(entry(0)));
            assert!(!obj.has_option(entry(NAMED - 1)));
            // SAFETY: the terminator, read only by address here.
            let end = unsafe { OptionDefRef::from_ptr((&raw const OBJ_OPTIONS.0[n]).cast_mut()) };
            assert!(!obj.has_option(end.unwrap()));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn foreign_options_are_refused_and_child_options_checked_on_the_child() {
            let o = Owned::with_child();
            let obj = shared(&o);
            // A foreign option is refused before C reads its offset.
            assert_eq!(obj.is_option_set_to_default(entry(0)), Err(EINVAL));
            // An option found in the child is one of the child's class, which
            // is the parent's class too.
            let (o_child, target) = obj
                .find_with_target(c"int", None, 0, SearchFlags::CHILDREN)
                .unwrap();
            assert!(target.has_option(o_child) && obj.has_option(o_child));
            assert_eq!(target.is_option_set_to_default(o_child), Ok(true));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn class_contexts_that_log_through_an_object_are_refused() {
            let flags = option(c"flags");
            let cflags = option(c"cflags");
            let mut f = 0;
            // `OBJ_CLASS` has a `child_next` callback: refused only for an
            // option that searches its children for constants.
            let obj_class = class(&OBJ_CLASS);
            assert_eq!(flags.eval_flags(obj_class, c"one", &mut f), Ok(()));
            assert_eq!(f, 1);
            assert_eq!(cflags.eval_flags(obj_class, c"one", &mut f), Err(EINVAL));
            // Classes whose logging would read the fake object or run class
            // code on it.
            let mut base = OBJ_CLASS.0;
            base.child_next = None;
            let mut cases = Vec::new();
            for edit in 0..5 {
                let mut c = base;
                match edit {
                    0 => c.class_name = ptr::null(),
                    1 => c.log_level_offset_offset = 8,
                    2 => c.parent_log_context_offset = 8,
                    3 => {
                        unsafe extern "C" fn cat(_: *mut c_void) -> ffi::AVClassCategory {
                            ffi::AVClassCategory::AV_CLASS_CATEGORY_NA
                        }
                        c.get_category = Some(cat);
                    }
                    _ => {
                        unsafe extern "C" fn name(_: *mut c_void) -> *const c_char {
                            c"item".as_ptr()
                        }
                        c.item_name = Some(name);
                    }
                }
                cases.push(c);
            }
            for c in &cases {
                // SAFETY: a live local class, with the fixture's table.
                let c = unsafe { ClassRef::from_ptr(ptr::from_ref(c).cast_mut()) }.unwrap();
                assert_eq!(flags.eval_flags(c, c"one", &mut f), Err(EINVAL));
                assert_eq!(option(c"int").eval_int(c, c"1"), Err(EINVAL));
            }
            // Without them the same class evaluates, child constants too.
            // SAFETY: as above.
            let bare = unsafe { ClassRef::from_ptr(ptr::from_ref(&base).cast_mut()) }.unwrap();
            assert_eq!(cflags.eval_flags(bare, c"bit", &mut f), Ok(()));
            assert_eq!(f, 2);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn values_reaching_unchecked_conversions_are_refused_before_c() {
            let o = Owned::with_child();
            let obj = shared(&o);
            let mut f = 0;
            // An evaluation builtin converting a double unchecked.
            assert_eq!(option(c"dbl").eval_double(obj, c"ld(1e300)"), Err(EINVAL));
            assert_eq!(option(c"int").eval_int(obj, c"st(1e30, 1)"), Err(EINVAL));
            assert_eq!(
                option(c"flags").eval_flags(obj, c"gcd(1e300, 2)", &mut f),
                Err(EINVAL)
            );
            // A flag operand outside the `int64_t` range.
            assert_eq!(
                option(c"flags").eval_flags(obj, c"+huge", &mut f),
                Err(ERANGE)
            );
            assert_eq!(
                option(c"flags").eval_flags(obj, c"one-1e30", &mut f),
                Err(ERANGE)
            );
            assert_eq!(f, 0, "refused before C wrote anything");
            // ... or one reading the clock, which a preview cannot predict.
            assert_eq!(
                option(c"flags").eval_flags(obj, c"+time(0)", &mut f),
                Err(EINVAL)
            );
            // A rational within wide bounds outside the `int` range.
            assert_eq!(option(c"wide_q").eval_rational(obj, c"1e20"), Err(ERANGE));
            assert_eq!(option(c"wide_q").eval_rational(obj, c"-3e9"), Err(ERANGE));
            assert_eq!(
                option(c"wide_q").eval_rational(obj, c"3"),
                Ok(Rational::new(3, 1))
            );
            // Options C rejects itself are not previewed: no refusal, C's
            // own `AVERROR(EINVAL)`.
            assert_eq!(option(c"ro").eval_int(obj, c"ld(1e300)"), Err(EINVAL));
            assert_eq!(option(c"array").eval_int(obj, c"1"), Err(EINVAL));
            assert_eq!(option(c"dbl").eval_int(obj, c"1"), Err(EINVAL));
            assert_eq!(option(c"int").eval_int64(obj, c"1"), Err(EINVAL));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn evaluators_read_constants_and_bounds_of_the_context() {
            let o = Owned::with_child();
            let obj = shared(&o);
            let int = option(c"int");
            assert_eq!(int.eval_int(obj, c"seven"), Ok(7));
            assert_eq!(int.eval_int(obj, c"seven*2+min"), Ok(4));
            assert_eq!(int.eval_int(obj, c"101"), Err(ERANGE));
            assert_eq!(int.eval_int(class(&OBJ_CLASS), c"default"), Ok(7));
            let mut f = 8;
            let flags = option(c"flags");
            assert_eq!(flags.eval_flags(obj, c"+one+bit-eight", &mut f), Ok(()));
            assert_eq!(f, 3);
            // A failing token keeps what the earlier ones wrote.
            assert!(flags.eval_flags(obj, c"+eight+nosuch", &mut f).is_err());
            assert_eq!(f, 11);
            assert_eq!(option(c"uint").eval_uint(obj, c"4294967295"), Ok(u32::MAX));
            assert_eq!(
                option(c"int64").eval_int64(obj, c"-5e9"),
                Ok(-5_000_000_000)
            );
            assert_eq!(option(c"flt").eval_float(obj, c"1/4"), Ok(0.25));
            assert_eq!(option(c"dbl").eval_double(obj, c"1k"), Ok(1000.0));
            assert_eq!(
                option(c"q").eval_rational(obj, c"30000:1001"),
                Ok(Rational::new(30000, 1001))
            );
            // The option need not belong to the context: an option of the
            // older `CLASS` reads `OBJ_CLASS`'s `mode` constants.
            assert_eq!(entry(0).eval_int(obj, c"seven"), Ok(7));
        }

        /// An object of a class whose options have no defaults, and whose
        /// unit has a deprecated named constant.
        #[repr(C)]
        struct Plain {
            class: *const ffi::AVClass,
            size: [c_int; 2],
            rate: ffi::AVRational,
            color: [u8; 4],
            text: *mut c_char,
            n: c_int,
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn absent_defaults_and_deprecated_constants() {
            use core::mem::offset_of;
            let opt =
                |name: &'static CStr, offset: usize, ty: T, flags: u32, def: i64| ffi::AVOption {
                    name: name.as_ptr(),
                    help: c"help".as_ptr(),
                    offset: offset as c_int,
                    type_: ty,
                    default_val: ffi::AVOption__bindgen_ty_1 { i64_: def },
                    min: 0.0,
                    max: 10.0,
                    flags: flags as c_int,
                    unit: if ty == T::AV_OPT_TYPE_INT || ty == T::AV_OPT_TYPE_CONST {
                        c"u".as_ptr()
                    } else {
                        ptr::null()
                    },
                };
            let table = [
                opt(
                    c"size",
                    offset_of!(Plain, size),
                    T::AV_OPT_TYPE_IMAGE_SIZE,
                    0,
                    0,
                ),
                opt(
                    c"rate",
                    offset_of!(Plain, rate),
                    T::AV_OPT_TYPE_VIDEO_RATE,
                    0,
                    0,
                ),
                opt(
                    c"color",
                    offset_of!(Plain, color),
                    T::AV_OPT_TYPE_COLOR,
                    0,
                    0,
                ),
                opt(
                    c"text",
                    offset_of!(Plain, text),
                    T::AV_OPT_TYPE_STRING,
                    0,
                    0,
                ),
                opt(c"n", offset_of!(Plain, n), T::AV_OPT_TYPE_INT, 0, 0),
                opt(
                    c"old",
                    0,
                    T::AV_OPT_TYPE_CONST,
                    ffi::AV_OPT_FLAG_DEPRECATED,
                    3,
                ),
                // SAFETY: the all-zero terminator.
                unsafe { core::mem::zeroed() },
            ];
            let mut class = OBJ_CLASS.0;
            class.option = table.as_ptr();
            class.child_next = None;
            // SAFETY: every field of `Plain` is valid all-zero.
            let mut raw: Plain = unsafe { core::mem::zeroed() };
            raw.class = &raw const class;
            // SAFETY: a live object of a well-formed local class (no
            // defaults, a unit on its constant), unwritten while borrowed.
            let obj = unsafe { ObjectRef::from_ptr((&raw mut raw).cast()) }.unwrap();
            let checked: Vec<_> = obj
                .options()
                .map(|o| obj.is_option_set_to_default(o))
                .collect();
            // A NULL size, color and string default is zero, as the object
            // holds; a NULL video rate is `0/0`, which C never compares
            // equal.
            let expected = [true, false, true, true, true, true].map(Ok);
            assert_eq!(checked, expected);
            // C warns about the deprecated constant and evaluates it.
            let n = obj.find(c"n", None, 0, SearchFlags::NONE).unwrap();
            assert_eq!(n.eval_int(obj, c"old"), Ok(3));
            assert_eq!(n.eval_int(obj.class(), c"old+1"), Ok(4));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn options_walk_ends_and_stays_ended() {
            let o = Owned::new();
            let obj = shared(&o);
            let mut walk = obj.options();
            let first = walk.next().unwrap();
            assert_eq!(first.name(), Some(c"int"));
            let resumed = walk.clone();
            assert!(format!("{walk:?}").contains("crustify-opt-obj"));
            assert_eq!(walk.by_ref().count(), OBJ_OPTIONS.0.len() - 2);
            assert!(walk.next().is_none() && walk.next().is_none());
            assert_eq!(resumed.count(), OBJ_OPTIONS.0.len() - 2);
            let names: Vec<_> = obj.options().map(|o| o.name()).collect();
            let table: Vec<_> = obj.class().options().iter().map(|o| o.name()).collect();
            assert_eq!(names, table);
        }

        /// Each token C evaluates on its own is checked, one that starts
        /// after the 255-byte cut included; tokens without `+` or `-` are
        /// checked without C, so the target is never read (options wave-2
        /// review, `ub_safe/eval-flags-token-split-hides-evaluator-call`).
        #[test]
        fn flags_refusal_checks_every_cut_token() {
            let o = (0..NAMED).map(entry).find(|o| o.name() == Some(c"flags"));
            let o = o.unwrap();
            let target = NonNull::dangling();
            let cut = |zeros: usize, tail: &[u8]| [&b"0*"[..], &vec![b'0'; zeros], tail].concat();
            for tail in [&b"ld(1e300)"[..], b"print(1,1e300)", b"taylor(1,2,1e300)"] {
                let refusal = flags_refusal(target, o, &cut(253, tail));
                assert_eq!(refusal, Ok(Some(EINVAL)), "{tail:?}");
            }
            for tail in [&b"ld(1)"[..], b"1", b"abc"] {
                assert_eq!(flags_refusal(target, o, &cut(253, tail)), Ok(None));
            }
            // One zero fewer and the cut falls after the `l`: C then parses
            // `d(1e300)`, which fails without calling anything.
            assert_eq!(flags_refusal(target, o, &cut(252, b"ld(1e300)")), Ok(None));
        }

        #[test]
        fn range_flags_combine_as_c_bits() {
            let all = RangeFlags::CHILDREN | RangeFlags::MULTI_COMPONENT;
            assert_eq!(
                all.bits(),
                (ffi::AV_OPT_SEARCH_CHILDREN | ffi::AV_OPT_MULTI_COMPONENT_RANGE) as c_int
            );
            assert_eq!(RangeFlags::default(), RangeFlags::NONE);
            assert_eq!(
                RangeFlags::NONE | RangeFlags::CHILDREN,
                RangeFlags::CHILDREN
            );
        }
    }
}
