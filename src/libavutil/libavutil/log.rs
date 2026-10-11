//! Safe wrappers over `libavutil/log.h`.
//!
//! [`ClassCategory`] and [`ClassStateFlags`] are plain values: C stores them
//! by value and never allocates or frees them, so each is a `Copy` newtype over
//! its raw enum with no handles or owner.
//!
//! Logging goes through one process-wide callback. [`log`], [`log_fmt`] and
//! [`log_once`] send a message to it, [`set_callback`] installs a Rust
//! [`Callback`], which receives each message as a [`Record`] it can format or
//! forward to FFmpeg's default callback, and [`set_default_callback`]
//! restores that one. The
//! default callback filters by [`level()`] and shapes its output by
//! [`flags()`]; C keeps all three settings in relaxed atomics, so every
//! accessor is safe from any thread. Messages are passed to C with a `"%s"`
//! format, so Rust never hands C a format string to interpret.
//!
//! [`Class`] is the class of an `AVClass`-enabled object (`AVClass`): a
//! descriptor immutable while borrowed, viewed through a [`ClassRef`]
//! whose getters read its name, category, offsets and option table, iterate
//! its child classes, and hand out its callbacks as callable handles. A Rust
//! object's own class can be a local or `static` built by [`Class::new`];
//! [`Record::class`] reads the class of a message's context.
//!
//! A logging context, an `AVClass`-enabled object, stays a raw `void *` until
//! the types that carry an `AVClass` are wrapped by a later batch:
//! [`log_with_context`], [`log_fmt_with_context`], [`log_once_with_context`],
//! [`default_item_name`] and [`default_category`] take it, [`Record::context`]
//! returns it, and the class callbacks' handles take it.

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::fmt;
use core::marker::PhantomData;
use core::mem::align_of;
use core::num::NonZeroI32;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Sub, SubAssign};
use core::ptr::{self, NonNull, addr_of, addr_of_mut};
use std::ffi::CString;

use ffibox::{CSlice, define_ctype};

use super::avstring::text_until_nul;
use super::ffi;
use super::opt::{AVERROR_BUG, OptionDef, OptionRangesBox};

/// Wraps: AVClassCategory
/// Handwritten
///
/// The category an `AVClass` reports for visualization (the log colour).
///
/// Open, like the C enum: C hands back whatever an `AVClass` stores in
/// `category` or returns from `get_category`, so any `c_uint` is a valid
/// value, and the associated constants name the known ones.
/// `AV_CLASS_CATEGORY_NB` is not part of FFmpeg's ABI or API and has no
/// constant. The default is [`ClassCategory::NA`], the zeroed C value.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassCategory(ffi::AVClassCategory);

impl ClassCategory {
    pub const NA: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_NA);
    pub const INPUT: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_INPUT);
    pub const OUTPUT: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_OUTPUT);
    pub const MUXER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_MUXER);
    pub const DEMUXER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEMUXER);
    pub const ENCODER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_ENCODER);
    pub const DECODER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DECODER);
    pub const FILTER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_FILTER);
    pub const BITSTREAM_FILTER: Self =
        Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_BITSTREAM_FILTER);
    pub const SWSCALER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_SWSCALER);
    pub const SWRESAMPLER: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_SWRESAMPLER);
    pub const HWDEVICE: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_HWDEVICE);
    pub const DEVICE_VIDEO_OUTPUT: Self =
        Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_VIDEO_OUTPUT);
    pub const DEVICE_VIDEO_INPUT: Self =
        Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_VIDEO_INPUT);
    pub const DEVICE_AUDIO_OUTPUT: Self =
        Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_AUDIO_OUTPUT);
    pub const DEVICE_AUDIO_INPUT: Self =
        Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_AUDIO_INPUT);
    pub const DEVICE_OUTPUT: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_OUTPUT);
    pub const DEVICE_INPUT: Self = Self(ffi::AVClassCategory::AV_CLASS_CATEGORY_DEVICE_INPUT);

    /// Every named category with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 18] = [
        (Self::NA, "NA"),
        (Self::INPUT, "INPUT"),
        (Self::OUTPUT, "OUTPUT"),
        (Self::MUXER, "MUXER"),
        (Self::DEMUXER, "DEMUXER"),
        (Self::ENCODER, "ENCODER"),
        (Self::DECODER, "DECODER"),
        (Self::FILTER, "FILTER"),
        (Self::BITSTREAM_FILTER, "BITSTREAM_FILTER"),
        (Self::SWSCALER, "SWSCALER"),
        (Self::SWRESAMPLER, "SWRESAMPLER"),
        (Self::HWDEVICE, "HWDEVICE"),
        (Self::DEVICE_VIDEO_OUTPUT, "DEVICE_VIDEO_OUTPUT"),
        (Self::DEVICE_VIDEO_INPUT, "DEVICE_VIDEO_INPUT"),
        (Self::DEVICE_AUDIO_OUTPUT, "DEVICE_AUDIO_OUTPUT"),
        (Self::DEVICE_AUDIO_INPUT, "DEVICE_AUDIO_INPUT"),
        (Self::DEVICE_OUTPUT, "DEVICE_OUTPUT"),
        (Self::DEVICE_INPUT, "DEVICE_INPUT"),
    ];

    /// The category with the raw value `value`, named or not.
    #[inline]
    pub const fn from_value(value: c_uint) -> Self {
        Self(ffi::AVClassCategory(value))
    }

    /// The raw value C stores for this category.
    #[inline]
    pub const fn value(self) -> c_uint {
        self.0.0
    }

    /// The name of this category's constant, or `None` for an unnamed value.
    pub fn name(self) -> Option<&'static str> {
        Self::KNOWN
            .iter()
            .find(|(c, _)| *c == self)
            .map(|&(_, name)| name)
    }

    /// Wraps: AV_IS_INPUT_DEVICE
    ///
    /// Whether this is one of the input device categories:
    /// [`DEVICE_VIDEO_INPUT`](Self::DEVICE_VIDEO_INPUT),
    /// [`DEVICE_AUDIO_INPUT`](Self::DEVICE_AUDIO_INPUT) or
    /// [`DEVICE_INPUT`](Self::DEVICE_INPUT).
    pub fn is_input_device(self) -> bool {
        // SAFETY: a plain value comparison; any category value is accepted.
        unsafe { ffi::crustify_AV_IS_INPUT_DEVICE(self.0) != 0 }
    }

    /// Wraps: AV_IS_OUTPUT_DEVICE
    ///
    /// Whether this is one of the output device categories:
    /// [`DEVICE_VIDEO_OUTPUT`](Self::DEVICE_VIDEO_OUTPUT),
    /// [`DEVICE_AUDIO_OUTPUT`](Self::DEVICE_AUDIO_OUTPUT) or
    /// [`DEVICE_OUTPUT`](Self::DEVICE_OUTPUT).
    pub fn is_output_device(self) -> bool {
        // SAFETY: a plain value comparison; any category value is accepted.
        unsafe { ffi::crustify_AV_IS_OUTPUT_DEVICE(self.0) != 0 }
    }
}

impl Default for ClassCategory {
    #[inline]
    fn default() -> Self {
        Self::NA
    }
}

impl fmt::Debug for ClassCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "ClassCategory::{name}"),
            None => write!(f, "ClassCategory({})", self.value()),
        }
    }
}

impl From<ffi::AVClassCategory> for ClassCategory {
    #[inline]
    fn from(raw: ffi::AVClassCategory) -> Self {
        Self(raw)
    }
}

impl From<ClassCategory> for ffi::AVClassCategory {
    #[inline]
    fn from(category: ClassCategory) -> Self {
        category.0
    }
}

/// Wraps: AVClassStateFlags
/// Handwritten
///
/// A set of `AVClassStateFlags` bits, the state an object exposes through its
/// `AVClass`'s `state_flags_offset`.
///
/// C keeps the set in an `unsigned` and combines the flags with bitwise
/// operators, so any bit pattern is a valid value: [`from_bits_retain`]
/// keeps bits without a constant, and the set operations carry them along.
///
/// [`from_bits_retain`]: ClassStateFlags::from_bits_retain
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassStateFlags(ffi::AVClassStateFlags);

impl ClassStateFlags {
    /// Initialization has finished and the object is in its runtime stage, so
    /// only options flagged as runtime parameters can still be set on it.
    pub const INITIALIZED: Self = Self(ffi::AVClassStateFlags::AV_CLASS_STATE_INITIALIZED);

    /// Every named flag with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 1] = [(Self::INITIALIZED, "INITIALIZED")];

    /// The set with no flag.
    #[inline]
    pub const fn empty() -> Self {
        Self::from_bits_retain(0)
    }

    /// The set of every named flag.
    pub const fn all() -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < Self::KNOWN.len() {
            bits |= Self::KNOWN[i].0.bits();
            i += 1;
        }
        Self::from_bits_retain(bits)
    }

    /// The set's raw bits, as C stores them.
    #[inline]
    pub const fn bits(self) -> c_uint {
        self.0.0
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    pub const fn from_bits_retain(bits: c_uint) -> Self {
        Self(ffi::AVClassStateFlags(bits))
    }

    /// The set with `bits`, or `None` when a bit has no constant.
    #[inline]
    pub const fn from_bits(bits: c_uint) -> Option<Self> {
        if bits & !Self::all().bits() == 0 {
            Some(Self::from_bits_retain(bits))
        } else {
            None
        }
    }

    /// The named flags among `bits`, the others dropped.
    #[inline]
    pub const fn from_bits_truncate(bits: c_uint) -> Self {
        Self::from_bits_retain(bits & Self::all().bits())
    }

    /// Whether no bit is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.bits() == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
    pub const fn contains(self, other: Self) -> bool {
        self.bits() & other.bits() == other.bits()
    }

    /// Whether `self` and `other` share a bit.
    #[inline]
    pub const fn intersects(self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }

    /// Sets every bit of `other`.
    #[inline]
    pub fn insert(&mut self, other: Self) {
        *self |= other;
    }

    /// Clears every bit of `other`.
    #[inline]
    pub fn remove(&mut self, other: Self) {
        *self -= other;
    }

    /// Sets every bit of `other` when `value`, and clears them otherwise.
    #[inline]
    pub fn set(&mut self, other: Self, value: bool) {
        if value {
            self.insert(other)
        } else {
            self.remove(other)
        }
    }
}

impl Default for ClassStateFlags {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for ClassStateFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClassStateFlags(")?;
        let mut rest = self.bits();
        let mut first = true;
        for &(flag, name) in &Self::KNOWN {
            if self.contains(flag) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                rest &= !flag.bits();
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

impl BitOr for ClassStateFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() | rhs.bits())
    }
}

impl BitOrAssign for ClassStateFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl BitAnd for ClassStateFlags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() & rhs.bits())
    }
}

impl BitAndAssign for ClassStateFlags {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

/// The bits of `self` not in `rhs`.
impl Sub for ClassStateFlags {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::from_bits_retain(self.bits() & !rhs.bits())
    }
}

impl SubAssign for ClassStateFlags {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl From<ffi::AVClassStateFlags> for ClassStateFlags {
    #[inline]
    fn from(raw: ffi::AVClassStateFlags) -> Self {
        Self(raw)
    }
}

impl From<ClassStateFlags> for ffi::AVClassStateFlags {
    #[inline]
    fn from(flags: ClassStateFlags) -> Self {
        flags.0
    }
}

define_ctype!(
    /// Wraps: AVClass
    ///
    /// The class of an `AVClass`-enabled object: its name, log category and
    /// level offset, parent context, option table and children, which
    /// logging and the `av_opt_*` routines read through the object's first
    /// member, a `const AVClass *`.
    ///
    /// Published FFmpeg classes are usually `static const`, but a logging
    /// adapter may build a temporary descriptor (for example `vs2av_log`
    /// borrows its caller's tag as the name). A class is immutable while
    /// borrowed, not necessarily static: [`ClassRef`] bounds its getters by
    /// the actual descriptor lifetime ([`ClassMut`] has no setters). There
    /// is no class-specific C allocator or destructor. Rust-owned local or
    /// static descriptors can be built by [`Class::new`].
    ///
    /// Every handle addresses a well-formed class for its lifetime: a
    /// `class_name` that is NULL or a NUL-terminated string, an option table
    /// that is NULL or terminated by an entry with a NULL name, both live as
    /// long as the class, and callbacks honouring the contracts their
    /// callable handles document.
    Class,
    ClassRef,
    ClassMut,
    ffi::AVClass
);

// SAFETY: a class is plain data (integers, an enum), pointers to immutable
// strings and option tables that live as long as it does, and function
// pointers C calls from whichever thread logs or sets options; nothing in it
// is tied to a thread, so a value moves between threads as a struct of
// shared immutable borrows and `fn` pointers does. Those borrows remain
// valid for the descriptor's lifetime, including for temporary descriptors.
unsafe impl Send for Class {}
// SAFETY: nothing writes a class: `ClassRef` has getters only, `ClassMut` no
// setters, the struct has no interior mutability, and no C path writes one
// once published, so concurrent shared access reads unchanging bytes.
unsafe impl Sync for Class {}
// SAFETY: as `&Class`, which is `Send` because `Class` is `Sync`.
unsafe impl Send for ClassRef<'_> {}
// SAFETY: as above: copies of a shared handle only read.
unsafe impl Sync for ClassRef<'_> {}
// SAFETY: as `&mut Class`; the exclusive handle has no setters, so it reaches
// only the same immutable bytes.
unsafe impl Send for ClassMut<'_> {}
// SAFETY: `&ClassMut` reaches the class only through `as_ref`, a shared
// handle.
unsafe impl Sync for ClassMut<'_> {}

/// `LIBAVUTIL_VERSION_INT`: `AV_VERSION_INT(major, minor, micro)`, the
/// version a class records it was built against.
const VERSION_INT: c_int = ((ffi::LIBAVUTIL_VERSION_MAJOR << 16)
    | (ffi::LIBAVUTIL_VERSION_MINOR << 8)
    | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

impl Class {
    /// A class named `name`, of `category`, for a Rust object that logs as
    /// an `AVClass`-enabled context: named by `av_default_item_name` (its
    /// class name), recording the libavutil version it is built against, with
    /// no options, children, parent or log level offset. A `const fn`, so it
    /// builds a `static`:
    ///
    /// ```
    /// use ffmpeg::libavutil::log::{Class, ClassCategory};
    /// static CLASS: Class = Class::new(c"MyContext", ClassCategory::FILTER);
    /// assert_eq!(CLASS.as_ref().name(), Some(c"MyContext"));
    /// ```
    pub const fn new(name: &'static CStr, category: ClassCategory) -> Self {
        Self(
            ffi::AVClass {
                class_name: name.as_ptr(),
                item_name: Some(ffi::av_default_item_name),
                option: ptr::null(),
                version: VERSION_INT,
                log_level_offset_offset: 0,
                parent_log_context_offset: 0,
                category: category.0,
                get_category: None,
                query_ranges: None,
                child_next: None,
                child_class_iterate: None,
                state_flags_offset: 0,
            },
            PhantomData,
        )
    }

    /// The class as the `const AVClass *` an object's first member holds,
    /// for an object of this class handed to C as a logging context.
    pub fn as_c_ptr(&self) -> *const ffi::AVClass {
        (&raw const *self).cast()
    }
}

impl<'a> ClassRef<'a> {
    /// Field: AVClass.class_name
    ///
    /// The class's name, usually its context structure's (`"AVCodecContext"`);
    /// `None` when it has none.
    pub fn name(&self) -> Option<&'a CStr> {
        // SAFETY: a read of a projected field through the shared handle.
        let name = unsafe { addr_of!((*self.as_ptr()).class_name).read() };
        // SAFETY: a well-formed class's name is NULL or a NUL-terminated
        // string that lives, unwritten, as long as the class (`'a`).
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }

    /// Field: AVClass.item_name
    ///
    /// The callback naming an object of the class, which logging calls
    /// (falling back to `av_default_item_name` when it is `None`).
    pub fn item_name(&self) -> Option<ItemNameFn> {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).item_name).read() }.map(ItemNameFn)
    }

    /// Field: AVClass.option
    ///
    /// The class's options, the entries of its table before the terminating
    /// one; empty for a class without options.
    pub fn options(&self) -> CSlice<'a, OptionDef> {
        // SAFETY: a read of a projected field through the shared handle.
        let table = unsafe { addr_of!((*self.as_ptr()).option).read() };
        let Some(first) = NonNull::new(table.cast_mut()) else {
            // SAFETY: an empty run reads nothing; a dangling pointer is
            // aligned and non-NULL.
            return unsafe { CSlice::from_raw_parts(NonNull::dangling(), 0) };
        };
        let mut len = 0;
        // SAFETY: a well-formed class's table ends with an entry whose name
        // is NULL, so every entry up to it is in bounds; only names are read.
        while !unsafe { addr_of!((*table.add(len)).name).read() }.is_null() {
            len += 1;
        }
        // SAFETY: the table holds `len` initialised options before its
        // terminator, living, unwritten, as long as the class (`'a`);
        // `OptionDef` is `#[repr(transparent)]` over `AVOption`.
        unsafe { CSlice::from_raw_parts(first.cast(), len) }
    }

    /// Field: AVClass.version
    ///
    /// The libavutil version the class was built against,
    /// `AV_VERSION_INT(major, minor, micro)`, which C compares before reading
    /// some members (the log level offset needs 50.15.2, the category
    /// callback 51.56.0).
    pub fn version(&self) -> i32 {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).version).read() }
    }

    /// Field: AVClass.log_level_offset_offset
    ///
    /// The byte offset, in an object of the class, of the `int` added to the
    /// level of every message at or above `AV_LOG_FATAL` it logs; `None`
    /// when objects have none.
    pub fn log_level_offset_offset(&self) -> Option<NonZeroI32> {
        // SAFETY: a read of a projected field through the shared handle.
        NonZeroI32::new(unsafe { addr_of!((*self.as_ptr()).log_level_offset_offset).read() })
    }

    /// Field: AVClass.parent_log_context_offset
    ///
    /// The byte offset, in an object of the class, of a pointer to the
    /// parent context the default log callback prints before it; `None`
    /// when objects have none.
    pub fn parent_log_context_offset(&self) -> Option<NonZeroI32> {
        // SAFETY: a read of a projected field through the shared handle.
        NonZeroI32::new(unsafe { addr_of!((*self.as_ptr()).parent_log_context_offset).read() })
    }

    /// Field: AVClass.category
    ///
    /// The category of every object of the class, used when it has no
    /// [`get_category`](Self::get_category) callback.
    pub fn category(&self) -> ClassCategory {
        // SAFETY: a read of a projected field through the shared handle;
        // every `c_uint` is a valid `AVClassCategory`.
        ClassCategory(unsafe { addr_of!((*self.as_ptr()).category).read() })
    }

    /// Field: AVClass.get_category
    ///
    /// The callback returning an object's category, for a class whose objects
    /// differ.
    pub fn get_category(&self) -> Option<GetCategoryFn> {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).get_category).read() }.map(GetCategoryFn)
    }

    /// Field: AVClass.query_ranges
    ///
    /// The callback returning an option's allowed ranges, which
    /// `av_opt_query_ranges` prefers to the default query.
    pub fn query_ranges(&self) -> Option<QueryRangesFn> {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).query_ranges).read() }.map(QueryRangesFn)
    }

    /// Field: AVClass.child_next
    ///
    /// The callback iterating an object's `AVClass`-enabled children.
    pub fn child_next(&self) -> Option<ChildNextFn> {
        // SAFETY: a read of a projected field through the shared handle.
        unsafe { addr_of!((*self.as_ptr()).child_next).read() }.map(ChildNextFn)
    }

    /// Field: AVClass.child_class_iterate
    ///
    /// The classes of the children an object of this class may have, as
    /// `av_opt_child_class_iterate` yields them; empty for a class without
    /// a `child_class_iterate` callback. Unlike
    /// [`child_next`](Self::child_next), this needs no object.
    pub fn child_classes(&self) -> ChildClasses<'a> {
        ChildClasses {
            // SAFETY: a read of a projected field through the shared handle.
            iterate: unsafe { addr_of!((*self.as_ptr()).child_class_iterate).read() },
            state: ptr::null_mut(),
            _class: PhantomData,
        }
    }

    /// Field: AVClass.state_flags_offset
    ///
    /// The byte offset, in an object of the class, of the `unsigned`
    /// [`ClassStateFlags`] the object updates; `None` when objects have none.
    pub fn state_flags_offset(&self) -> Option<NonZeroI32> {
        // SAFETY: a read of a projected field through the shared handle.
        NonZeroI32::new(unsafe { addr_of!((*self.as_ptr()).state_flags_offset).read() })
    }

    /// Whether `self` and `other` are the same class, as C compares classes:
    /// by address.
    pub fn ptr_eq(self, other: ClassRef<'_>) -> bool {
        ptr::eq(self.as_ptr(), other.as_ptr())
    }
}

impl fmt::Debug for ClassRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClassRef")
            .field("name", &self.name())
            .field("category", &self.category())
            .finish_non_exhaustive()
    }
}

/// A class's `item_name` callback, from [`ClassRef::item_name`].
#[derive(Clone, Copy, Debug)]
pub struct ItemNameFn(unsafe extern "C" fn(*mut c_void) -> *const c_char);

impl ItemNameFn {
    /// The name of `obj`; `None` when the callback returns NULL.
    ///
    /// The object is raw until the types that carry an `AVClass` are
    /// wrapped.
    ///
    /// # Safety
    ///
    /// `obj` must be an object the callback accepts for the whole call: for
    /// FFmpeg's classes, a live object of the class the callback came from
    /// (for `av_default_item_name`, any live object whose first member is an
    /// aligned, non-NULL `const AVClass *`), which no other thread writes
    /// during the call. The string the callback returns, if any, must stay
    /// alive and unwritten for `'o`; FFmpeg's callbacks return the class
    /// name or a string of the object, alive while the object is.
    pub unsafe fn call<'o>(self, obj: *mut c_void) -> Option<&'o CStr> {
        // SAFETY: the caller vouches that the callback accepts `obj`.
        let name = unsafe { (self.0)(obj) };
        // SAFETY: a non-NULL result is a NUL-terminated string the caller
        // guarantees alive and unwritten for `'o`.
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
    }

    /// Whether the callback is `av_default_item_name`, which reads only the
    /// object's class pointer.
    pub(crate) fn is_default(self) -> bool {
        let default: unsafe extern "C" fn(*mut c_void) -> *const c_char = ffi::av_default_item_name;
        core::ptr::fn_addr_eq(self.0, default)
    }
}

/// A class's `get_category` callback, from [`ClassRef::get_category`].
#[derive(Clone, Copy, Debug)]
pub struct GetCategoryFn(unsafe extern "C" fn(*mut c_void) -> ffi::AVClassCategory);

impl GetCategoryFn {
    /// The category of `obj`.
    ///
    /// The object is raw until the types that carry an `AVClass` are
    /// wrapped.
    ///
    /// # Safety
    ///
    /// `obj` must be an object the callback accepts for the whole call: for
    /// FFmpeg's classes, a live object of the class the callback came from,
    /// which no other thread writes during the call.
    pub unsafe fn call(self, obj: *mut c_void) -> ClassCategory {
        // SAFETY: the caller vouches that the callback accepts `obj`; any
        // returned value is a valid (open) category.
        ClassCategory(unsafe { (self.0)(obj) })
    }
}

/// A class's `query_ranges` callback, from [`ClassRef::query_ranges`].
#[derive(Clone, Copy, Debug)]
pub struct QueryRangesFn(
    unsafe extern "C" fn(*mut *mut ffi::AVOptionRanges, *mut c_void, *const c_char, c_int) -> c_int,
);

impl QueryRangesFn {
    /// The allowed ranges of option `key` of `obj`, as `av_opt_query_ranges`
    /// returns them for a class with this callback: the list's component
    /// count is the callback's return value with
    /// `AV_OPT_MULTI_COMPONENT_RANGE` in `flags`, and 1 without. Fails with
    /// the callback's `AVERROR` code, or `AVERROR_BUG` when it succeeds
    /// without a list.
    ///
    /// The object is raw until the types that carry an `AVClass` are
    /// wrapped.
    ///
    /// # Safety
    ///
    /// `obj` must be an object the callback accepts for the whole call: for
    /// FFmpeg's classes, a live object of the class the callback came from
    /// or, with `AV_OPT_SEARCH_FAKE_OBJ` in `flags`, a pointer to a live
    /// `const AVClass *` addressing that class. No other thread may write
    /// it during the call. The callback must honour `av_opt_query_ranges`'
    /// contract: on success, store a fresh list that `av_opt_freep_ranges`
    /// releases and that holds `nb_ranges` ranges for each of the components
    /// counted as above.
    pub unsafe fn call(
        self,
        obj: *mut c_void,
        key: &CStr,
        flags: i32,
    ) -> Result<OptionRangesBox, i32> {
        let mut ranges = ptr::null_mut();
        // SAFETY: `ranges` is a writable slot, `key` a live NUL-terminated
        // string, and the caller vouches for `obj` and the callback.
        let ret = unsafe { (self.0)(&mut ranges, obj, key.as_ptr(), flags) };
        if ret < 0 {
            return Err(ret);
        }
        // SAFETY: on success `ranges` is a fresh list the caller owns,
        // released by `av_opt_freep_ranges`; NULL is reported as a bug.
        let list = unsafe { OptionRangesBox::from_c(ranges) }.ok_or(AVERROR_BUG)?;
        let multi = flags & ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int != 0;
        // SAFETY: a write of a projected field of the list we own, as
        // `av_opt_query_ranges` records the count; the callback allocated
        // that many components, per the caller.
        unsafe {
            addr_of_mut!((*list.as_c_ptr()).nb_components).write(if multi { ret } else { 1 })
        };
        Ok(list)
    }
}

/// A class's `child_next` callback, from [`ClassRef::child_next`].
#[derive(Clone, Copy, Debug)]
pub struct ChildNextFn(unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void);

impl ChildNextFn {
    /// The child of `obj` after `prev`, or its first child when `prev` is
    /// NULL; `None` after the last one. The child is borrowed from `obj`.
    ///
    /// The objects are raw until the types that carry an `AVClass` are
    /// wrapped.
    ///
    /// # Safety
    ///
    /// `obj` must be an object the callback accepts for the whole call: for
    /// FFmpeg's classes, a live object of the class the callback came from,
    /// which no other thread writes during the call. `prev` must be NULL or a
    /// child an earlier call returned for `obj`, which `obj` still holds.
    pub unsafe fn call(self, obj: *mut c_void, prev: *mut c_void) -> Option<NonNull<c_void>> {
        // SAFETY: the caller vouches for `obj` and `prev`.
        NonNull::new(unsafe { (self.0)(obj, prev) })
    }
}

/// The classes of a class's potential children, from
/// [`ClassRef::child_classes`]: its `child_class_iterate` callback driven
/// from a NULL state until it returns NULL.
#[derive(Debug)]
pub struct ChildClasses<'a> {
    iterate: Option<unsafe extern "C" fn(*mut *mut c_void) -> *const ffi::AVClass>,
    state: *mut c_void,
    _class: PhantomData<ClassRef<'a>>,
}

impl<'a> Iterator for ChildClasses<'a> {
    type Item = ClassRef<'a>;

    fn next(&mut self) -> Option<ClassRef<'a>> {
        let iterate = self.iterate?;
        // SAFETY: a well-formed class's `child_class_iterate` takes the
        // opaque state it alone advanced from NULL, which `state` is.
        let child = unsafe { iterate(&mut self.state) };
        // SAFETY: the callback returns NULL or a well-formed class that lives
        // as long as the parent class (`'a`): every FFmpeg iterator returns a
        // `static const` class.
        let child = unsafe { ClassRef::from_ptr(child.cast_mut()) };
        if child.is_none() {
            // C callers stop at the first NULL; so does the iterator.
            self.iterate = None;
        }
        child
    }
}

impl core::iter::FusedIterator for ChildClasses<'_> {}

/// The `va_list` parameter of a C log callback. On x86-64 SysV, the ABI the
/// bindings are generated for, a `va_list` is a one-element array of
/// `__va_list_tag`, so a parameter receives a pointer to its element.
type VaList = *mut ffi::__va_list_tag;

/// A log message's importance (`AV_LOG_*`): lower is more severe.
///
/// Open, like the `int` C takes: any value is valid, and C reads the bits
/// above the low byte of a non-negative level as a colour tint
/// (`AV_LOG_C`), so the associated constants name only the plain levels.
/// The order is C's: a message is printed by the default callback when its
/// level is at most [`level()`], the default callback comparing a
/// non-negative level with its tint bits cleared (`level & 0xff`), which the
/// derived order does not.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Level(c_int);

impl Level {
    /// Print no output.
    pub const QUIET: Self = Self(ffi::AV_LOG_QUIET);
    /// Something went really wrong and the program will crash now.
    pub const PANIC: Self = Self(ffi::AV_LOG_PANIC as c_int);
    /// Something went wrong and recovery is not possible.
    pub const FATAL: Self = Self(ffi::AV_LOG_FATAL as c_int);
    /// Something went wrong and cannot losslessly be recovered.
    pub const ERROR: Self = Self(ffi::AV_LOG_ERROR as c_int);
    /// Something does not look correct, which may or may not cause problems.
    pub const WARNING: Self = Self(ffi::AV_LOG_WARNING as c_int);
    /// Standard information; the default [`level()`].
    pub const INFO: Self = Self(ffi::AV_LOG_INFO as c_int);
    /// Detailed information.
    pub const VERBOSE: Self = Self(ffi::AV_LOG_VERBOSE as c_int);
    /// Information only useful to FFmpeg developers.
    pub const DEBUG: Self = Self(ffi::AV_LOG_DEBUG as c_int);
    /// Extremely verbose debugging, useful for FFmpeg development.
    pub const TRACE: Self = Self(ffi::AV_LOG_TRACE as c_int);

    /// Every named level with its constant's name, most severe first.
    const KNOWN: [(Self, &'static str); 9] = [
        (Self::QUIET, "QUIET"),
        (Self::PANIC, "PANIC"),
        (Self::FATAL, "FATAL"),
        (Self::ERROR, "ERROR"),
        (Self::WARNING, "WARNING"),
        (Self::INFO, "INFO"),
        (Self::VERBOSE, "VERBOSE"),
        (Self::DEBUG, "DEBUG"),
        (Self::TRACE, "TRACE"),
    ];

    /// The level C passes as `raw`, named or not.
    #[inline]
    pub const fn from_raw(raw: c_int) -> Self {
        Self(raw)
    }

    /// The `int` C takes for this level.
    #[inline]
    pub const fn raw(self) -> c_int {
        self.0
    }

    /// The name of this level's constant, or `None` for an unnamed value.
    pub fn name(self) -> Option<&'static str> {
        Self::KNOWN
            .iter()
            .find(|(l, _)| *l == self)
            .map(|&(_, name)| name)
    }
}

impl fmt::Debug for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "Level::{name}"),
            None => write!(f, "Level({})", self.0),
        }
    }
}

/// The set of `AV_LOG_*` flags that shape the default callback's output.
///
/// C keeps them in an `int`, so any bit pattern is a valid value and the set
/// operations carry unnamed bits along.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Flags(c_int);

impl Flags {
    /// Collapse repeated messages into a "Last message repeated" line.
    pub const SKIP_REPEATED: Self = Self(ffi::AV_LOG_SKIP_REPEATED as c_int);
    /// Print the level name in each message's prefix.
    pub const PRINT_LEVEL: Self = Self(ffi::AV_LOG_PRINT_LEVEL as c_int);
    /// Print the time of day before each message.
    pub const PRINT_TIME: Self = Self(ffi::AV_LOG_PRINT_TIME as c_int);
    /// Print the date and time before each message.
    pub const PRINT_DATETIME: Self = Self(ffi::AV_LOG_PRINT_DATETIME as c_int);

    /// Every named flag with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 4] = [
        (Self::SKIP_REPEATED, "SKIP_REPEATED"),
        (Self::PRINT_LEVEL, "PRINT_LEVEL"),
        (Self::PRINT_TIME, "PRINT_TIME"),
        (Self::PRINT_DATETIME, "PRINT_DATETIME"),
    ];

    /// The set with no flag.
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

impl fmt::Debug for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Flags(")?;
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

impl BitOr for Flags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Flags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for Flags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// Wraps: av_log_get_level
///
/// The level above which the default callback drops messages, process-wide.
/// C keeps it in a relaxed atomic.
pub fn level() -> Level {
    // SAFETY: `av_log_get_level` takes nothing and loads an atomic.
    Level(unsafe { ffi::av_log_get_level() })
}

/// Wraps: av_log_set_level
///
/// Sets the level above which the default callback drops messages,
/// process-wide. Other callbacks receive every message whatever the level.
pub fn set_level(level: Level) {
    // SAFETY: `av_log_set_level` takes its argument by value and stores it
    // atomically.
    unsafe { ffi::av_log_set_level(level.0) }
}

/// Wraps: av_log_get_flags
///
/// The flags shaping the default callback's output, process-wide.
pub fn flags() -> Flags {
    // SAFETY: `av_log_get_flags` takes nothing and loads an atomic.
    Flags(unsafe { ffi::av_log_get_flags() })
}

/// Wraps: av_log_set_flags
///
/// Sets the flags shaping the default callback's output, process-wide.
pub fn set_flags(flags: Flags) {
    // SAFETY: `av_log_set_flags` takes its argument by value and stores it
    // atomically.
    unsafe { ffi::av_log_set_flags(flags.0) }
}

/// Wraps: av_vlog
///
/// Sends `message` to the general log, with no context, at `level`. It goes
/// to the installed log callback; the default one prints it to stderr when
/// `level` is within [`level()`]. `message` is logged verbatim: `%` in it is
/// not a format directive.
pub fn log(level: Level, message: &CStr) {
    // SAFETY: a NULL context selects the general log, which `av_vlog` and the
    // default callback accept from any thread, as `av_log_set_callback`
    // requires of any other; the format `%s` consumes exactly one argument,
    // `message`, a live NUL-terminated string.
    unsafe { ffi::crustify_av_vlog(ptr::null_mut(), level.0, c"%s".as_ptr(), message.as_ptr()) }
}

/// Wraps: av_vlog
///
/// Sends `message` to the log at `level` in the context of `obj`, as
/// [`log`] does without one. When `obj`'s class is recent enough (version
/// 50.15.2 or later) and has a `log_level_offset_offset`, the `int` at that
/// offset in `obj` is added to any `level` at or above [`Level::FATAL`]. The
/// default callback prefixes the line with the object's name and address,
/// and its parent's, and colours it by category.
///
/// The context is an `AVClass`-enabled object, raw until the types that
/// carry an `AVClass` are wrapped.
///
/// # Safety
///
/// `obj` must be NULL, or address a live object whose first member is a
/// `const AVClass *` that is NULL or points to a live `AVClass` such that,
/// for the whole call:
///
/// - its `item_name` callback, if any, may be called with `obj` and
///   returns a NUL-terminated string; without one, its `class_name` is one;
/// - its `get_category` callback, if any, may be called with `obj` and
///   returns a category below `AV_CLASS_CATEGORY_NB` (the default callback
///   indexes its colour table with it, unchecked);
/// - neither callback logs: the default callback calls them holding its
///   lock, which C does not let a thread take twice;
/// - a non-zero `log_level_offset_offset` (in a class of version 50.15.2 or
///   later) is the offset of a readable `int` within the object which, when
///   `level` is at least [`Level::FATAL`], C adds to `level` without
///   overflowing an `int`;
/// - a non-zero `parent_log_context_offset` is the offset of a readable
///   pointer within the object that is NULL or addresses a parent object,
///   whose first member is a `const AVClass *` that is NULL or meets the
///   three callback requirements above for the parent.
///
/// No other thread may write those members while the call runs. The
/// installed callback receives `obj` as is: the default and Rust
/// callbacks only read it as above, and a raw C callback installed through
/// `ffi` must accept it.
pub unsafe fn log_with_context(obj: *mut c_void, level: Level, message: &CStr) {
    // SAFETY: the caller vouches for `obj`, which `av_vlog` and the default
    // callback only read; the format is as for `log`.
    unsafe { ffi::crustify_av_vlog(obj, level.0, c"%s".as_ptr(), message.as_ptr()) }
}

/// Wraps: av_log
///
/// Sends the text `args` formats to the general log at `level`, as [`log`]
/// sends a string: Rust formats the text, and C receives it with a `"%s"`
/// format. The text ends at its first NUL, as C's `%s` reads it.
///
/// ```
/// use ffmpeg::libavutil::log::{self, Level};
/// log::log_fmt(Level::DEBUG, format_args!("decoded {} frames\n", 42));
/// ```
pub fn log_fmt(level: Level, args: fmt::Arguments<'_>) {
    let text = text_until_nul(args);
    // SAFETY: a NULL context is the general log, which every callback
    // accepts; `"%s"` consumes exactly the one argument passed, a live
    // NUL-terminated string.
    unsafe { ffi::av_log(ptr::null_mut(), level.0, c"%s".as_ptr(), text.as_ptr()) }
}

/// Wraps: av_log
///
/// [`log_fmt`] with a logging context, as [`log_with_context`] sends a
/// string with one.
///
/// # Safety
///
/// As [`log_with_context`].
pub unsafe fn log_fmt_with_context(obj: *mut c_void, level: Level, args: fmt::Arguments<'_>) {
    let text = text_until_nul(args);
    // SAFETY: the caller vouches for `obj`; `"%s"` consumes exactly the one
    // argument passed, a live NUL-terminated string.
    unsafe { ffi::av_log(obj, level.0, c"%s".as_ptr(), text.as_ptr()) }
}

/// Wraps: av_log_once
///
/// Sends `message` as [`log`] does, at `initial_level` while `state` is
/// `false` and at `subsequent_level` once it is `true`, then sets `state`.
/// Start every message's state at `false`; the exclusive borrow keeps two
/// threads from sharing it, as C requires.
pub fn log_once(initial_level: Level, subsequent_level: Level, state: &mut bool, message: &CStr) {
    let mut c_state = c_int::from(*state);
    // SAFETY: a NULL context is the general log; `c_state` is a writable
    // `int` for the call; `"%s"` consumes exactly the one live
    // NUL-terminated string passed.
    unsafe {
        ffi::av_log_once(
            ptr::null_mut(),
            initial_level.0,
            subsequent_level.0,
            &mut c_state,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    }
    *state = c_state != 0;
}

/// Wraps: av_log_once
///
/// [`log_once`] with a logging context, as [`log_with_context`].
///
/// # Safety
///
/// As [`log_with_context`].
pub unsafe fn log_once_with_context(
    obj: *mut c_void,
    initial_level: Level,
    subsequent_level: Level,
    state: &mut bool,
    message: &CStr,
) {
    let mut c_state = c_int::from(*state);
    // SAFETY: the caller vouches for `obj`; `c_state` is a writable
    // `int` for the call; `"%s"` consumes exactly the one live
    // NUL-terminated string passed.
    unsafe {
        ffi::av_log_once(
            obj,
            initial_level.0,
            subsequent_level.0,
            &mut c_state,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    }
    *state = c_state != 0;
}

/// A Rust log callback, installed with [`set_callback`].
///
/// The handler is a function of the implementing type, not of a value, so
/// installing one needs no storage: each implementation gets its own C entry
/// point. C calls it for every message any FFmpeg library logs, at every
/// level ([`level()`] filters only the default callback), on whichever thread
/// logs, possibly on several at once. It must not panic: a panic cannot
/// unwind into C, and aborts the process.
///
/// ```
/// use std::sync::atomic::{AtomicUsize, Ordering};
/// use ffmpeg::libavutil::log::{self, Callback, Level, Record};
///
/// static ERRORS: AtomicUsize = AtomicUsize::new(0);
///
/// struct CountErrors;
/// impl Callback for CountErrors {
///     fn log(record: Record<'_>) {
///         if record.level() <= Level::ERROR {
///             ERRORS.fetch_add(1, Ordering::Relaxed);
///         }
///         record.log_default();
///     }
/// }
///
/// log::set_callback::<CountErrors>();
/// log::log(Level::ERROR, c"counted\n");
/// log::set_default_callback();
/// assert!(ERRORS.load(Ordering::Relaxed) >= 1);
/// ```
pub trait Callback {
    /// Handles one message.
    fn log(record: Record<'_>);
}

/// One message on its way through a log callback: its level, its context,
/// and its format string with the arguments, which C formats on request.
///
/// It borrows the logging call's arguments, so it lives only as long as the
/// callback runs; it cannot be kept:
///
/// ```compile_fail,E0521
/// use std::cell::RefCell;
/// use ffmpeg::libavutil::log::{Callback, Record};
/// thread_local!(static KEPT: RefCell<Option<Record<'static>>> = RefCell::new(None));
/// struct Keep;
/// impl Callback for Keep {
///     fn log(record: Record<'_>) {
///         KEPT.with(|k| *k.borrow_mut() = Some(record));
///     }
/// }
/// ```
///
/// nor leave the thread that logs:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::log::{Callback, Record};
/// fn sent<T: Send>(_: T) {}
/// struct Send_;
/// impl Callback for Send_ {
///     fn log(record: Record<'_>) {
///         sent(record);
///     }
/// }
/// ```
pub struct Record<'a> {
    context: *mut c_void,
    level: c_int,
    format: *const c_char,
    args: VaList,
    _call: PhantomData<&'a ()>,
}

impl Record<'_> {
    /// The message's level, as the callback received it: shifted by the
    /// context's level offset, if any, and with any colour tint.
    pub fn level(&self) -> Level {
        Level(self.level)
    }

    /// The `AVClass`-enabled object the message was logged with, or NULL for
    /// the general log. Raw until the types that carry an `AVClass` are
    /// wrapped; it is live while the callback runs.
    pub fn context(&self) -> *mut c_void {
        self.context
    }

    /// A private copy of the arguments for one C call: on x86-64 SysV,
    /// `va_copy` is a copy of the `__va_list_tag`, whose pointers address the
    /// logging call's frame, live while the callback runs.
    fn args(&self) -> ffi::__va_list_tag {
        // SAFETY: `args` addresses the logging call's `va_list` element,
        // live and unmodified for the whole callback (every C consumer works
        // on a copy).
        unsafe { self.args.read() }
    }

    /// Wraps: av_log_format_line2
    ///
    /// Formats the message as the default callback prints it, with the
    /// context's prefix when `print_prefix` is set, into `line`, truncated to
    /// fit with its NUL. Returns the length of the whole line without the
    /// NUL, which is at least `line.len()` when it was truncated, or `None`
    /// when C fails to format it. `print_prefix` is the state that carries
    /// across messages: start it at `true`; C sets it when a message ends a
    /// line. An empty `line` measures the line without writing it.
    pub fn format_line(&self, line: &mut [u8], print_prefix: &mut bool) -> Option<usize> {
        let mut args = self.args();
        let mut c_prefix = c_int::from(*print_prefix);
        let (ptr, size) = line_buffer(line);
        // SAFETY: the record's context, format and arguments are those C
        // passed the callback, which `av_log`'s contract makes valid for the
        // default formatting while the callback runs; `args` is a private
        // copy; `ptr` is NULL with size 0 or valid for `size` bytes of
        // writes; `c_prefix` is a writable `int`.
        let ret = unsafe {
            ffi::av_log_format_line2(
                self.context,
                self.level,
                self.format,
                &mut args,
                ptr,
                size,
                &mut c_prefix,
            )
        };
        *print_prefix = c_prefix != 0;
        usize::try_from(ret).ok()
    }

    /// Wraps: av_log_format_line
    ///
    /// Formats the message into `line` as [`format_line`](Self::format_line)
    /// does, and returns the possibly truncated line it wrote; an empty
    /// `line` yields an empty string.
    pub fn write_line<'b>(&self, line: &'b mut [u8], print_prefix: &mut bool) -> &'b CStr {
        let mut args = self.args();
        let mut c_prefix = c_int::from(*print_prefix);
        let (ptr, size) = line_buffer(line);
        // SAFETY: as for `format_line`.
        unsafe {
            ffi::av_log_format_line(
                self.context,
                self.level,
                self.format,
                &mut args,
                ptr,
                size,
                &mut c_prefix,
            )
        };
        *print_prefix = c_prefix != 0;
        if size == 0 {
            return Default::default();
        }
        // `snprintf` NUL-terminates a non-empty buffer, also on failure;
        // a missing NUL reads as no line.
        CStr::from_bytes_until_nul(line).unwrap_or_default()
    }

    /// The whole formatted line, as [`format_line`](Self::format_line) would
    /// write it into a buffer large enough, or `None` when C fails to format
    /// it or the line exceeds `c_int::MAX` bytes.
    pub fn to_line(&self, print_prefix: &mut bool) -> Option<CString> {
        // Measuring updates the prefix state too; only the real pass may.
        let mut probe = *print_prefix;
        let len = self.format_line(&mut [], &mut probe)?;
        let mut line = vec![0; len.checked_add(1)?];
        self.format_line(&mut line, print_prefix)?;
        // A context whose name changed between the passes may leave the
        // line shorter than measured.
        let end = line.iter().position(|&b| b == 0).unwrap_or(line.len());
        line.truncate(end);
        CString::new(line).ok()
    }

    /// Wraps: av_log_default_callback
    ///
    /// Hands the message to FFmpeg's default callback: printed to stderr when
    /// its level is at most [`level()`], prefixed, coloured and collapsed
    /// with repeats per [`flags()`].
    pub fn log_default(&self) {
        let mut args = self.args();
        // SAFETY: the record's context, format and arguments are those C
        // passed the callback, valid for the default callback while it runs;
        // `args` is a private copy.
        unsafe { ffi::av_log_default_callback(self.context, self.level, self.format, &mut args) }
    }
}

impl<'a> Record<'a> {
    /// The class of the message's context: the `const AVClass *` the
    /// context's first member holds; `None` for the general log, or a
    /// context whose class pointer is NULL. Borrowed for the callback.
    pub fn class(&self) -> Option<ClassRef<'a>> {
        if self.context.is_null() {
            return None;
        }
        // SAFETY: a non-NULL context is an `AVClass`-enabled object, as
        // `av_log` requires of every caller: its first member is an aligned
        // `const AVClass *`, which `av_vlog` itself reads, and no thread
        // writes it while the message is logged.
        let class = unsafe { self.context.cast::<*const ffi::AVClass>().read() };
        // SAFETY: the pointer is NULL or a well-formed class live for the
        // logging call (`'a`), as `av_log`'s contract requires.
        unsafe { ClassRef::from_ptr(class.cast_mut()) }
    }
}

impl fmt::Debug for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Record")
            .field("level", &self.level())
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

/// `line` as C's `(char *line, int line_size)`: NULL for an empty buffer,
/// and at most `c_int::MAX` bytes of a longer one.
fn line_buffer(line: &mut [u8]) -> (*mut c_char, c_int) {
    if line.is_empty() {
        (ptr::null_mut(), 0)
    } else {
        let size = c_int::try_from(line.len()).unwrap_or(c_int::MAX);
        (line.as_mut_ptr().cast(), size)
    }
}

/// The C entry point of the [`Callback`] `C`.
///
/// # Safety
///
/// The arguments are those of a log callback call: `context` is NULL or a
/// context valid as [`log_with_context`] requires, `format` a printf format
/// matching the arguments `args` addresses, all live for the call.
unsafe extern "C" fn trampoline<C: Callback>(
    context: *mut c_void,
    level: c_int,
    format: *const c_char,
    args: VaList,
) {
    C::log(Record {
        context,
        level,
        format,
        args,
        _call: PhantomData,
    });
}

/// Wraps: av_log_set_callback
///
/// Installs the [`Callback`] `C` as the process-wide log callback, in place
/// of the current one. Messages being handled by the previous callback on
/// other threads still finish there.
pub fn set_callback<C: Callback>() {
    // SAFETY: `trampoline::<C>` has the callback signature, lives for the
    // program, and accepts every call C makes through it from any thread.
    unsafe { ffi::av_log_set_callback(Some(trampoline::<C>)) }
}

/// Wraps: av_log_set_callback
///
/// Installs FFmpeg's default callback, which prints to stderr, as the
/// process-wide log callback: the initial one.
pub fn set_default_callback() {
    // SAFETY: `av_log_default_callback` is C's own thread-safe callback.
    unsafe { ffi::av_log_set_callback(Some(ffi::av_log_default_callback)) }
}

/// Wraps: av_log_set_callback
///
/// Removes the log callback, process-wide: every message is dropped.
pub fn clear_callback() {
    // SAFETY: `av_vlog` checks for a NULL callback before calling it.
    unsafe { ffi::av_log_set_callback(None) }
}

/// Wraps: av_default_item_name
///
/// The `class_name` of the `AVClass` that the object `ctx` points to: the
/// name `av_log` prints for an object whose class has no `item_name`
/// callback. `None` when the class stores NULL there.
///
/// # Safety
///
/// `ctx` must address a live `AVClass`-enabled object: a `const AVClass *`
/// first member, readable and aligned for a pointer (C loads it as
/// `*(AVClass **) ctx`, so a byte offset into a buffer does not qualify),
/// holding a non-NULL pointer to a live class. The class's `class_name`, when
/// non-NULL, must be a NUL-terminated string that stays alive and unmodified
/// for `'a`; a string literal, which every FFmpeg class stores, satisfies any
/// `'a`. No other thread may write the object's class pointer or the class's
/// `class_name` during the call.
pub unsafe fn default_item_name<'a>(ctx: *const c_void) -> Option<&'a CStr> {
    debug_assert!(
        ctx.addr().is_multiple_of(align_of::<*const ffi::AVClass>()),
        "context {ctx:p} is not aligned for its class pointer"
    );
    // SAFETY: the caller vouches that `ctx` is an aligned, readable class
    // pointer slot holding a valid pointer to a live class;
    // `av_default_item_name` only reads both.
    let name = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
    // SAFETY: a non-NULL `name` is the class's `class_name`, a NUL-terminated
    // string the caller guarantees alive and unmodified for `'a`.
    (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
}

/// Wraps: av_default_get_category
///
/// The `category` of the `AVClass` that the object `ctx` points to: the
/// category `av_log` colours a message by when the class has no
/// `get_category` callback. Any value the class stores is returned, named or
/// not.
///
/// # Safety
///
/// `ctx` must address a live `AVClass`-enabled object: a `const AVClass *`
/// first member, readable and aligned for a pointer (C loads it as
/// `*(AVClass **) ctx`, so a byte offset into a buffer does not qualify),
/// holding a non-NULL pointer to a live class. No other thread may write the
/// object's class pointer or the class's `category` during the call.
pub unsafe fn default_category(ctx: *const c_void) -> ClassCategory {
    debug_assert!(
        ctx.addr().is_multiple_of(align_of::<*const ffi::AVClass>()),
        "context {ctx:p} is not aligned for its class pointer"
    );
    // SAFETY: the caller vouches that `ctx` is an aligned, readable class
    // pointer slot holding a valid pointer to a live class;
    // `av_default_get_category` only reads both, and returns the enum by
    // value.
    ClassCategory::from(unsafe { ffi::av_default_get_category(ctx.cast_mut()) })
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::mem::{align_of, size_of};

    /// A class named `name`, every other member zero.
    fn class(name: Option<&'static CStr>) -> ffi::AVClass {
        ffi::AVClass {
            class_name: name.map_or(core::ptr::null(), CStr::as_ptr),
            ..Default::default()
        }
    }

    #[test]
    fn default_item_name_reads_the_class_name() {
        let named = class(Some(c"unit-class"));
        let obj: *const ffi::AVClass = &named;
        // SAFETY: `obj` is a live object starting with a non-NULL class
        // pointer to `named`, whose name is a literal.
        let name = unsafe { default_item_name((&raw const obj).cast()) };
        assert_eq!(name, Some(c"unit-class"));
        // The name is the class's own string, not a copy.
        assert_eq!(name.unwrap().as_ptr(), named.class_name);

        let nameless = class(None);
        let obj: *const ffi::AVClass = &nameless;
        // SAFETY: as above; the class stores NULL as its name.
        assert_eq!(unsafe { default_item_name((&raw const obj).cast()) }, None);
    }

    /// A context whose class pointer slot is misaligned is outside the
    /// contract; the debug assertion reports it before C loads the pointer,
    /// so no misaligned read happens.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "not aligned for its class pointer")]
    fn default_item_name_rejects_a_misaligned_context_before_c() {
        #[repr(C, align(8))]
        struct Buf([u8; 16]);

        let named = class(Some(c"unit-class"));
        let mut buf = Buf([0; 16]);
        let slot = buf
            .0
            .as_mut_ptr()
            .wrapping_add(1)
            .cast::<*const ffi::AVClass>();
        // SAFETY: `slot` addresses bytes 1..9 of `buf`, inside the buffer.
        unsafe { slot.write_unaligned(&raw const named) };
        // SAFETY: `slot` breaks the alignment obligation on purpose; the
        // debug assertion panics before the FFI call, so C never reads it.
        let _ = unsafe { default_item_name(slot.cast()) };
    }

    #[test]
    fn layouts_match_the_c_enums() {
        assert_eq!(
            size_of::<ClassCategory>(),
            size_of::<ffi::AVClassCategory>()
        );
        assert_eq!(
            align_of::<ClassCategory>(),
            align_of::<ffi::AVClassCategory>()
        );
        assert_eq!(
            size_of::<ClassStateFlags>(),
            size_of::<ffi::AVClassStateFlags>()
        );
        assert_eq!(
            align_of::<ClassStateFlags>(),
            align_of::<ffi::AVClassStateFlags>()
        );
    }

    #[test]
    fn category_constants_carry_the_c_values() {
        let values: Vec<c_uint> = ClassCategory::KNOWN
            .iter()
            .map(|(c, _)| c.value())
            .collect();
        assert_eq!(
            values,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 40, 41, 42, 43, 44, 45]
        );
    }

    #[test]
    fn category_round_trips_any_value() {
        for v in [0, 11, 12, 39, 45, 46, 1000, c_uint::MAX] {
            let c = ClassCategory::from_value(v);
            assert_eq!(c.value(), v);
            let raw: ffi::AVClassCategory = c.into();
            assert_eq!(raw.0, v);
            assert_eq!(ClassCategory::from(raw), c);
        }
    }

    #[test]
    fn category_names_only_the_named_values() {
        for &(c, name) in &ClassCategory::KNOWN {
            assert_eq!(c.name(), Some(name));
        }
        // The gap between HWDEVICE and the device categories, and NB.
        for v in [12, 39, 46, c_uint::MAX] {
            assert_eq!(ClassCategory::from_value(v).name(), None);
        }
    }

    #[test]
    fn device_predicates_name_the_device_categories() {
        let inputs = [
            ClassCategory::DEVICE_VIDEO_INPUT,
            ClassCategory::DEVICE_AUDIO_INPUT,
            ClassCategory::DEVICE_INPUT,
        ];
        let outputs = [
            ClassCategory::DEVICE_VIDEO_OUTPUT,
            ClassCategory::DEVICE_AUDIO_OUTPUT,
            ClassCategory::DEVICE_OUTPUT,
        ];
        for &(c, _) in &ClassCategory::KNOWN {
            assert_eq!(c.is_input_device(), inputs.contains(&c), "{c:?}");
            assert_eq!(c.is_output_device(), outputs.contains(&c), "{c:?}");
        }
        // Unnamed values around the device range and beyond are neither.
        for v in [12, 39, 46, 1000, c_uint::MAX] {
            let c = ClassCategory::from_value(v);
            assert!(!c.is_input_device() && !c.is_output_device(), "{c:?}");
        }
    }

    #[test]
    fn category_debug_and_default() {
        assert_eq!(ClassCategory::default(), ClassCategory::NA);
        assert_eq!(
            format!("{:?}", ClassCategory::DEMUXER),
            "ClassCategory::DEMUXER"
        );
        assert_eq!(
            format!("{:?}", ClassCategory::from_value(46)),
            "ClassCategory(46)"
        );
    }

    #[test]
    fn flags_bits_and_constructors() {
        assert_eq!(ClassStateFlags::INITIALIZED.bits(), 1);
        assert_eq!(ClassStateFlags::all(), ClassStateFlags::INITIALIZED);
        assert_eq!(ClassStateFlags::default(), ClassStateFlags::empty());
        assert!(ClassStateFlags::empty().is_empty());
        assert!(!ClassStateFlags::INITIALIZED.is_empty());

        assert_eq!(
            ClassStateFlags::from_bits(1),
            Some(ClassStateFlags::INITIALIZED)
        );
        assert_eq!(
            ClassStateFlags::from_bits(0),
            Some(ClassStateFlags::empty())
        );
        assert_eq!(ClassStateFlags::from_bits(2), None);
        assert_eq!(ClassStateFlags::from_bits(c_uint::MAX), None);
        assert_eq!(
            ClassStateFlags::from_bits_truncate(c_uint::MAX),
            ClassStateFlags::all()
        );
        assert_eq!(
            ClassStateFlags::from_bits_retain(c_uint::MAX).bits(),
            c_uint::MAX
        );

        let raw: ffi::AVClassStateFlags = ClassStateFlags::from_bits_retain(5).into();
        assert_eq!(raw.0, 5);
        assert_eq!(ClassStateFlags::from(raw).bits(), 5);
    }

    #[test]
    fn flags_set_operations_keep_unnamed_bits() {
        let init = ClassStateFlags::INITIALIZED;
        let other = ClassStateFlags::from_bits_retain(0x80);

        let both = init | other;
        assert_eq!(both.bits(), 0x81);
        assert!(both.contains(init) && both.contains(other));
        assert!(both.intersects(init));
        assert!(!init.intersects(other));
        assert!(!init.contains(both));
        assert!(init.contains(ClassStateFlags::empty()));
        assert_eq!(both & init, init);
        assert_eq!(both - init, other);

        let mut f = ClassStateFlags::empty();
        f.insert(init);
        assert_eq!(f, init);
        f |= other;
        assert_eq!(f, both);
        f.remove(init);
        assert_eq!(f, other);
        f.set(init, true);
        assert_eq!(f, both);
        f.set(init, false);
        assert_eq!(f, other);
        f -= other;
        assert!(f.is_empty());
        f = both;
        f &= init;
        assert_eq!(f, init);
    }

    #[test]
    fn flags_debug() {
        let init = ClassStateFlags::INITIALIZED;
        assert_eq!(
            format!("{:?}", ClassStateFlags::empty()),
            "ClassStateFlags(0x0)"
        );
        assert_eq!(format!("{init:?}"), "ClassStateFlags(INITIALIZED)");
        let unnamed = ClassStateFlags::from_bits_retain(0x80);
        assert_eq!(format!("{unnamed:?}"), "ClassStateFlags(0x80)");
        assert_eq!(
            format!("{:?}", init | unnamed),
            "ClassStateFlags(INITIALIZED | 0x80)"
        );
    }

    /// A writer that fails on its `limit`-th write, as a full sink does.
    struct FailAt {
        limit: usize,
        writes: usize,
    }

    impl fmt::Write for FailAt {
        fn write_str(&mut self, _: &str) -> fmt::Result {
            self.writes += 1;
            if self.writes == self.limit {
                Err(fmt::Error)
            } else {
                Ok(())
            }
        }
    }

    /// Whether formatting `value` reports the error of a sink failing on its
    /// `limit`-th write.
    fn debug_fails_at(value: &dyn fmt::Debug, limit: usize) -> bool {
        use core::fmt::Write;
        write!(FailAt { limit, writes: 0 }, "{value:?}").is_err()
    }

    #[test]
    fn debug_propagates_sink_errors() {
        let flags = ClassStateFlags::INITIALIZED | ClassStateFlags::from_bits_retain(0x80);
        // "ClassStateFlags(", "INITIALIZED", " | ", "0x80", ")".
        for limit in 1..=5 {
            assert!(debug_fails_at(&flags, limit), "write {limit}");
        }
        for value in [ClassCategory::NA, ClassCategory::from_value(46)] {
            assert!(debug_fails_at(&value, 1), "{value:?}");
        }
    }

    #[test]
    fn level_constants_carry_the_c_values() {
        let values: Vec<c_int> = Level::KNOWN.iter().map(|(l, _)| l.raw()).collect();
        assert_eq!(values, [-8, 0, 8, 16, 24, 32, 40, 48, 56]);
        assert!(Level::KNOWN.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(Level::ERROR < Level::INFO);
        assert_eq!(size_of::<Level>(), size_of::<c_int>());
    }

    #[test]
    fn level_names_debug_and_raw_round_trip() {
        for &(l, name) in &Level::KNOWN {
            assert_eq!(l.name(), Some(name));
            assert_eq!(format!("{l:?}"), format!("Level::{name}"));
            assert_eq!(Level::from_raw(l.raw()), l);
        }
        for raw in [c_int::MIN, -9, 1, 33, 57, 0x1_0020, c_int::MAX] {
            let l = Level::from_raw(raw);
            assert_eq!((l.raw(), l.name()), (raw, None));
            assert_eq!(format!("{l:?}"), format!("Level({raw})"));
        }
    }

    #[test]
    fn log_flags_bits_ops_and_debug() {
        assert_eq!(
            [
                Flags::SKIP_REPEATED,
                Flags::PRINT_LEVEL,
                Flags::PRINT_TIME,
                Flags::PRINT_DATETIME
            ]
            .map(Flags::bits),
            [1, 2, 4, 8]
        );
        assert!(Flags::default().is_empty() && Flags::empty().is_empty());
        let mut f = Flags::SKIP_REPEATED | Flags::from_bits_retain(0x40);
        assert_eq!(f.bits(), 0x41);
        assert!(f.contains(Flags::SKIP_REPEATED) && !f.contains(Flags::PRINT_LEVEL));
        f |= Flags::PRINT_LEVEL;
        assert_eq!((f & Flags::PRINT_LEVEL), Flags::PRINT_LEVEL);
        assert_eq!(format!("{:?}", Flags::empty()), "Flags(0x0)");
        assert_eq!(
            format!("{f:?}"),
            "Flags(SKIP_REPEATED | PRINT_LEVEL | 0x40)"
        );
        assert_eq!(
            format!("{:?}", Flags::PRINT_TIME | Flags::PRINT_DATETIME),
            "Flags(PRINT_TIME | PRINT_DATETIME)"
        );
        assert_eq!(
            format!("{:?}", Flags::from_bits_retain(-16)),
            "Flags(0xfffffff0)"
        );
        let flags = Flags::SKIP_REPEATED | Flags::from_bits_retain(0x40);
        // "Flags(", "SKIP_REPEATED", " | ", "0x40", ")".
        for limit in 1..=5 {
            assert!(debug_fails_at(&flags, limit), "write {limit}");
        }
        assert!(debug_fails_at(&Level::from_raw(3), 1));
    }

    #[test]
    fn line_buffer_passes_null_for_empty() {
        let (p, n) = line_buffer(&mut []);
        assert!(p.is_null() && n == 0);
        let mut b = [0u8; 3];
        let base = b.as_mut_ptr();
        let (p, n) = line_buffer(&mut b);
        assert_eq!((p.cast::<u8>(), n), (base, 3));
    }

    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, PoisonError};

    /// Serializes the tests that install a callback, which is process-wide.
    static CALLBACK: Mutex<()> = Mutex::new(());

    /// What [`Capture`] saw of one record.
    #[derive(Debug)]
    struct Seen {
        level: Level,
        context: *mut c_void,
        line: CString,
        measured: Option<usize>,
        short: CString,
        prefix_after: bool,
    }

    thread_local! {
        /// The records this thread logged while capturing.
        static SEEN: RefCell<Option<Vec<Seen>>> = const { RefCell::new(None) };
    }

    /// Records every message logged on a capturing thread, formatting it
    /// through each `Record` method with a fresh prefix state.
    struct Capture;
    impl Callback for Capture {
        fn log(record: Record<'_>) {
            if SEEN.with(|s| s.borrow().is_none()) {
                return;
            }
            let mut prefix = true;
            let line = record.to_line(&mut prefix).unwrap();
            let prefix_after = prefix;
            let mut fresh = true;
            let measured = record.format_line(&mut [], &mut fresh);
            let mut buf = [0xaa_u8; 4];
            let mut fresh = true;
            let short = record.write_line(&mut buf, &mut fresh).to_owned();
            assert!(format!("{record:?}").starts_with("Record { level: "));
            let seen = Seen {
                level: record.level(),
                context: record.context(),
                line,
                measured,
                short,
                prefix_after,
            };
            SEEN.with(|s| s.borrow_mut().as_mut().unwrap().push(seen));
        }
    }

    /// Runs `f` with [`Capture`] installed, and returns what this thread
    /// logged.
    fn capture(f: impl FnOnce()) -> Vec<Seen> {
        let _g = CALLBACK.lock().unwrap_or_else(PoisonError::into_inner);
        SEEN.with(|s| *s.borrow_mut() = Some(Vec::new()));
        set_callback::<Capture>();
        f();
        set_default_callback();
        SEEN.with(|s| s.borrow_mut().take().unwrap())
    }

    #[test]
    fn records_format_measure_and_truncate() {
        let seen = capture(|| {
            log(Level::WARNING, c"50% done\n");
            log(Level::from_raw(77), c"partial");
        });
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].level, Level::WARNING);
        assert!(seen[0].context.is_null());
        assert_eq!(seen[0].line.as_bytes(), b"50% done\n");
        assert_eq!(seen[0].measured, Some(9));
        assert_eq!(seen[0].short.as_bytes(), b"50%");
        assert!(seen[0].prefix_after);
        assert_eq!(seen[1].level.raw(), 77);
        assert_eq!(seen[1].line.as_bytes(), b"partial");
        assert!(!seen[1].prefix_after);
    }

    #[test]
    fn write_line_into_an_empty_buffer_is_empty() {
        struct Empty;
        impl Callback for Empty {
            fn log(record: Record<'_>) {
                if SEEN.with(|s| s.borrow().is_none()) {
                    return;
                }
                let mut prefix = true;
                assert_eq!(record.write_line(&mut [], &mut prefix), c"");
                assert!(prefix);
                let mut one = [0xaa];
                assert_eq!(record.write_line(&mut one, &mut prefix), c"");
                assert!(prefix, "a line ending in a newline keeps the prefix");
            }
        }
        let _g = CALLBACK.lock().unwrap_or_else(PoisonError::into_inner);
        SEEN.with(|s| *s.borrow_mut() = Some(Vec::new()));
        set_callback::<Empty>();
        log(Level::INFO, c"x\n");
        set_default_callback();
        SEEN.with(|s| s.borrow_mut().take());
    }

    /// Alternates the name C prints for a context between a long and a short
    /// one, so the two formatting passes of `to_line` disagree.
    unsafe extern "C" fn flip_name(_: *mut c_void) -> *const c_char {
        static SHORT: AtomicBool = AtomicBool::new(false);
        if SHORT.fetch_xor(true, Ordering::Relaxed) {
            c"a".as_ptr()
        } else {
            c"longer-name".as_ptr()
        }
    }

    #[test]
    fn to_line_keeps_what_the_second_pass_wrote() {
        let class = ffi::AVClass {
            class_name: c"flip".as_ptr(),
            item_name: Some(flip_name),
            ..Default::default()
        };
        let mut ctx: *const ffi::AVClass = &class;
        let ctx_ptr = (&raw mut ctx).cast::<c_void>();
        let seen = capture(|| {
            // SAFETY: `ctx` starts with a pointer to `class`, which outlives
            // the call; its `item_name` returns static strings and it has no
            // offsets for C to follow.
            unsafe { log_with_context(ctx_ptr, Level::ERROR, c"m\n") };
        });
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].context, ctx_ptr);
        let line = seen[0].line.to_str().unwrap();
        // The measuring pass may name the context either way; the line is
        // whatever the writing pass produced, never padded.
        assert!(
            line.starts_with("[a @ ") || line.starts_with("[longer-name @ "),
            "{line}"
        );
        assert!(line.ends_with("] m\n"), "{line}");
    }

    use crate::libavutil::opt::fixture as opt_fixture;

    #[test]
    fn new_builds_a_named_class_of_this_version() {
        static NEW: Class = Class::new(c"unit-new", ClassCategory::FILTER);
        let c = NEW.as_ref();
        assert_eq!(c.name(), Some(c"unit-new"));
        assert_eq!(c.category(), ClassCategory::FILTER);
        assert_eq!(c.version(), (61 << 16) | (6 << 8) | 100);
        assert!(c.item_name().is_some());
        assert!(c.options().is_empty());
        assert_eq!(c.child_classes().count(), 0);
        assert!(c.get_category().is_none() && c.query_ranges().is_none());
        assert!(c.child_next().is_none());
        assert_eq!(c.log_level_offset_offset(), None);
        assert_eq!(c.parent_log_context_offset(), None);
        assert_eq!(c.state_flags_offset(), None);
        assert_eq!(NEW.as_c_ptr(), c.as_ptr());
        assert!(c.ptr_eq(NEW.as_ref()));
        assert_eq!(
            format!("{c:?}"),
            "ClassRef { name: Some(\"unit-new\"), category: ClassCategory::FILTER, .. }"
        );
    }

    #[test]
    fn a_temporary_class_borrows_its_callers_name() {
        let name = CString::new("temporary tag").unwrap();
        let raw = ffi::AVClass {
            class_name: name.as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            ..Default::default()
        };
        // SAFETY: the initialized local descriptor and its caller-owned
        // NUL-terminated name outlive all handles below and are not written.
        let class = unsafe { ClassRef::from_ptr((&raw const raw).cast_mut()) }.unwrap();
        let mut obj = &raw const raw;
        assert_eq!(class.name(), Some(name.as_c_str()));
        // SAFETY: `obj` is an aligned class-pointer slot; its descriptor and
        // the borrowed tag remain valid and immutable through the result's use.
        let item = unsafe { class.item_name().unwrap().call((&raw mut obj).cast()) };
        assert_eq!(item, class.name());
        assert_eq!(item.unwrap().as_ptr(), name.as_ptr());
    }

    unsafe extern "C" fn unit_item_name(_: *mut c_void) -> *const c_char {
        c"item".as_ptr()
    }

    unsafe extern "C" fn unit_category(_: *mut c_void) -> ffi::AVClassCategory {
        ffi::AVClassCategory::AV_CLASS_CATEGORY_DECODER
    }

    unsafe extern "C" fn unit_child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
        if prev.is_null() { obj } else { ptr::null_mut() }
    }

    static CHILD_A: opt_fixture::Class = opt_fixture::Class(ffi::AVClass {
        class_name: c"child-a".as_ptr(),
        item_name: None,
        option: ptr::null(),
        version: 0,
        log_level_offset_offset: 0,
        parent_log_context_offset: 0,
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
        get_category: None,
        query_ranges: None,
        child_next: None,
        child_class_iterate: None,
        state_flags_offset: 0,
    });

    std::thread_local!(static ITERATIONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) });

    /// Yields `CHILD_A` twice, then NULL, then (were it asked again) more.
    unsafe extern "C" fn unit_children(state: *mut *mut c_void) -> *const ffi::AVClass {
        ITERATIONS.with(|n| n.set(n.get() + 1));
        // SAFETY: `ChildClasses` passes its own state slot.
        let i = unsafe { state.read() }.addr();
        // SAFETY: as above.
        unsafe { state.write(ptr::without_provenance_mut(i + 1)) };
        if i == 2 { ptr::null() } else { &CHILD_A.0 }
    }

    #[test]
    fn getters_read_every_field() {
        let raw = ffi::AVClass {
            class_name: c"unit-all".as_ptr(),
            item_name: Some(unit_item_name),
            option: opt_fixture::CLASS.0.option,
            version: 0x0001_0203,
            log_level_offset_offset: 8,
            parent_log_context_offset: -16,
            category: ffi::AVClassCategory::AV_CLASS_CATEGORY_MUXER,
            get_category: Some(unit_category),
            query_ranges: None,
            child_next: Some(unit_child_next),
            child_class_iterate: Some(unit_children),
            state_flags_offset: 24,
        };
        // SAFETY: `raw` is a well-formed class outliving the handle.
        let c = unsafe { ClassRef::from_ptr((&raw const raw).cast_mut()) }.unwrap();
        assert_eq!(c.name(), Some(c"unit-all"));
        assert_eq!(c.version(), 0x0001_0203);
        assert_eq!(c.log_level_offset_offset().map(NonZeroI32::get), Some(8));
        assert_eq!(
            c.parent_log_context_offset().map(NonZeroI32::get),
            Some(-16)
        );
        assert_eq!(c.state_flags_offset().map(NonZeroI32::get), Some(24));
        assert_eq!(c.category(), ClassCategory::MUXER);
        let mut obj = 0_u8;
        let obj = (&raw mut obj).cast::<c_void>();
        // SAFETY: the callbacks above accept any object and return statics.
        unsafe {
            assert_eq!(c.item_name().unwrap().call(obj), Some(c"item"));
            assert_eq!(c.get_category().unwrap().call(obj), ClassCategory::DECODER);
            let next = c.child_next().unwrap();
            assert_eq!(
                next.call(obj, ptr::null_mut()).map(NonNull::as_ptr),
                Some(obj)
            );
            assert_eq!(next.call(obj, obj), None);
        }
        let names: Vec<_> = opt_fixture::OPTIONS
            .0
            .iter()
            .map_while(|o| {
                // SAFETY: the fixture's names are NULL or literals.
                (!o.name.is_null()).then(|| unsafe { CStr::from_ptr(o.name) })
            })
            .collect();
        let options = c.options();
        assert_eq!(options.len(), names.len());
        assert!(options.iter().map(|o| o.name().unwrap()).eq(names));
    }

    #[test]
    fn child_classes_stop_at_the_first_null() {
        let raw = ffi::AVClass {
            child_class_iterate: Some(unit_children),
            ..Default::default()
        };
        // SAFETY: `raw` is a well-formed class outliving the handle.
        let c = unsafe { ClassRef::from_ptr((&raw const raw).cast_mut()) }.unwrap();
        ITERATIONS.with(|n| n.set(0));
        let mut children = c.child_classes();
        assert_eq!(children.next().unwrap().as_ptr(), &raw const CHILD_A.0);
        assert_eq!(children.next().unwrap().name(), Some(c"child-a"));
        assert!(children.next().is_none());
        // Fused: the callback is not asked past its NULL.
        assert!(children.next().is_none());
        assert_eq!(ITERATIONS.with(|n| n.get()), 3);
        // Every iteration starts afresh.
        assert_eq!(c.child_classes().count(), 2);
    }

    #[test]
    fn record_class_reads_the_contexts_first_member() {
        let record = |context: *mut c_void| Record {
            context,
            level: 0,
            format: ptr::null(),
            args: ptr::null_mut(),
            _call: PhantomData,
        };
        assert!(record(ptr::null_mut()).class().is_none());

        let mut classless: *const ffi::AVClass = ptr::null();
        assert!(record((&raw mut classless).cast()).class().is_none());

        static OWN: Class = Class::new(c"unit-record", ClassCategory::NA);
        let mut obj = OWN.as_c_ptr();
        let class = record((&raw mut obj).cast()).class().unwrap();
        assert!(class.ptr_eq(OWN.as_ref()));
        assert_eq!(class.name(), Some(c"unit-record"));
    }

    #[test]
    fn default_category_reads_the_class_member() {
        for category in [
            ClassCategory::NA,
            ClassCategory::DEMUXER,
            ClassCategory::from_value(4242),
        ] {
            let class = ffi::AVClass {
                category: category.into(),
                ..Default::default()
            };
            let ctx: *const ffi::AVClass = &class;
            // SAFETY: `ctx` is a live, aligned class pointer slot holding a
            // pointer to the live `class`; nothing writes either.
            let got = unsafe { default_category((&raw const ctx).cast()) };
            assert_eq!(got, category);
        }
    }
}
