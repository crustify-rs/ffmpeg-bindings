//! Safe wrappers over `libavutil/log.h`.
//!
//! [`ClassCategory`] and [`ClassStateFlags`] are plain values: C stores them
//! by value and never allocates or frees them, so each is a `Copy` newtype over
//! its raw enum with no handles or owner.
//!
//! A raw seam remains where the dependency is wrapped by a later batch:
//! [`default_item_name`] takes the `AVClass`-enabled object as a `void *`.

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::fmt;
use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Sub, SubAssign};

use super::ffi;

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

/// Wraps: av_vlog
///
/// Sends `message` to the general log, with no context, at `level`: an
/// `ffi::AV_LOG_*` level, optionally with `AV_LOG_C` colour bits. It goes
/// to the installed log callback; the default one prints it to stderr when
/// `level` is within `av_log_get_level()`. `message` is logged verbatim:
/// `%` in it is not a format directive.
pub fn log(level: c_int, message: &CStr) {
    // SAFETY: a NULL context selects the general log, which `av_vlog` and the
    // default callback accept from any thread, as `av_log_set_callback`
    // requires of any other; the format `%s` consumes exactly one argument,
    // `message`, a live NUL-terminated string.
    unsafe {
        ffi::crustify_av_vlog(
            core::ptr::null_mut(),
            level,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    }
}

/// Wraps: av_vlog
///
/// Sends `message` to the log at `level` in the context of `obj`, as
/// [`log`] does without one. When `obj`'s class is recent enough (version
/// 50.15.2 or later) and has a `log_level_offset_offset`, the `int` at that
/// offset in `obj` is added to any `level` at or above `AV_LOG_FATAL`. The
/// default callback prefixes the line with the object's name, and its
/// parent's.
///
/// # Safety
///
/// `obj` must be NULL or address a live `AVClass`-enabled object: a struct
/// whose first member is a `const AVClass *` that is NULL or addresses a
/// live class with a valid `class_name` string. The class's `item_name` and
/// `get_category` callbacks, when set, must be callable with `obj`. When the
/// class's version is 50.15.2 or later and its `log_level_offset_offset` is
/// non-zero, `obj` must hold at that offset an `int` whose sum with `level`
/// does not overflow, for a `level` at or above `AV_LOG_FATAL`. When its
/// `parent_log_context_offset` is non-zero, `obj` must hold at that offset a
/// pointer that is NULL or addresses a parent: another `AVClass`-enabled
/// object, whose class pointer may be NULL, with a class meeting the same
/// requirements for the parent. No thread may
/// write any of these during the call. A callback installed with
/// `av_log_set_callback` may require more of `obj`.
pub unsafe fn log_with_context(obj: *mut c_void, level: c_int, message: &CStr) {
    // SAFETY: the caller vouches for `obj`, which `av_vlog` and the default
    // callback only read; the format is as for `log`.
    unsafe { ffi::crustify_av_vlog(obj, level, c"%s".as_ptr(), message.as_ptr()) }
}

/// Wraps: av_default_item_name
///
/// The `class_name` of the `AVClass` that the object `ctx` points to: the
/// name `av_log` prints for an object whose class has no `item_name`
/// callback. `None` when the class stores NULL there.
///
/// # Safety
///
/// `ctx` must address a live `AVClass`-enabled object: readable memory that
/// starts with a non-NULL `const AVClass *` to a live class. The class's
/// `class_name`, when non-NULL, must be a NUL-terminated string that stays
/// alive and unmodified for `'a`; a string literal, which every FFmpeg class
/// stores, satisfies any `'a`. No other thread may write the object's class
/// pointer or the class's `class_name` during the call.
pub unsafe fn default_item_name<'a>(ctx: *const c_void) -> Option<&'a CStr> {
    // SAFETY: the caller vouches that `ctx` starts with a valid class pointer
    // to a live class; `av_default_item_name` only reads both.
    let name = unsafe { ffi::av_default_item_name(ctx.cast_mut()) };
    // SAFETY: a non-NULL `name` is the class's `class_name`, a NUL-terminated
    // string the caller guarantees alive and unmodified for `'a`.
    (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
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
}
