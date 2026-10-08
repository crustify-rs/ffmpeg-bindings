//! Safe wrappers over `libavutil/opt.h`.
//!
//! [`OptionDef`] is one entry of an `AVClass`'s option table (`AVOption`).
//! Every table is a `static const` C array that no C path writes, allocates
//! or frees, so an option is only ever borrowed: an [`OptionDefRef`] carries
//! the getters, and its strings borrow from the table for the handle's
//! lifetime. [`OptionDefMut`] exists only as the layout type's exclusive
//! handle and has no setters.
//!
//! [`OptionRanges`] is the list `av_opt_query_ranges` and
//! `av_opt_query_ranges_default` allocate (`AVOptionRanges`). It is owned as
//! an [`OptionRangesBox`], built by [`OptionRanges::query`] or
//! [`OptionRanges::query_default`] and released by `av_opt_freep_ranges`,
//! which also frees every range it lists. C has no copy routine, so the box is
//! not `Clone`, and the list is read-only: its counts bound the release loop.
//!
//! Raw seams remain where the dependency is wrapped by a later batch: the
//! object a range query inspects (an `AVClass`-enabled `void *`), the
//! `AVOptionRange` elements, an array option's `AVOptionArrayDef`, and the
//! `AVOptionType` value.

use core::ffi::{CStr, c_char, c_void};
use core::ptr::{NonNull, addr_of};

use ffibox::{CBox, CDrop, CSlice, define_ctype};

use super::ffi;

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
    /// The option's type, an `AV_OPT_TYPE_*` value possibly combined with
    /// `AV_OPT_TYPE_FLAG_ARRAY`.
    pub fn option_type(&self) -> ffi::AVOptionType {
        // SAFETY: a read of a projected field through the shared handle; every
        // `c_uint` is a valid `AVOptionType`.
        unsafe { addr_of!((*self.as_ptr()).type_).read() }
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
        use ffi::AVOptionType as T;
        let ty = self.option_type();
        let val = self.as_ptr();
        if ty.0 & T::AV_OPT_TYPE_FLAG_ARRAY.0 != 0 {
            // SAFETY: a read of a projected union member through the shared
            // handle; an array option's table entry initialises `arr`, as
            // `av_opt_set_defaults` relies on, and a pointer has no invalid
            // bit patterns.
            return OptionDefault::Array(unsafe { addr_of!((*val).default_val.arr).read() });
        }
        match ty {
            T::AV_OPT_TYPE_BOOL
            | T::AV_OPT_TYPE_FLAGS
            | T::AV_OPT_TYPE_INT
            | T::AV_OPT_TYPE_UINT
            | T::AV_OPT_TYPE_INT64
            | T::AV_OPT_TYPE_UINT64
            | T::AV_OPT_TYPE_DURATION
            | T::AV_OPT_TYPE_PIXEL_FMT
            | T::AV_OPT_TYPE_SAMPLE_FMT
            | T::AV_OPT_TYPE_CONST => {
                // SAFETY: a read of a projected union member through the
                // shared handle; all 8 bytes of the union are initialised by
                // its table entry, and every bit pattern is a valid `i64`.
                OptionDefault::Int(unsafe { addr_of!((*val).default_val.i64_).read() })
            }
            T::AV_OPT_TYPE_DOUBLE | T::AV_OPT_TYPE_FLOAT | T::AV_OPT_TYPE_RATIONAL => {
                // SAFETY: as above, for an `f64`.
                OptionDefault::Double(unsafe { addr_of!((*val).default_val.dbl).read() })
            }
            T::AV_OPT_TYPE_STRING
            | T::AV_OPT_TYPE_COLOR
            | T::AV_OPT_TYPE_IMAGE_SIZE
            | T::AV_OPT_TYPE_VIDEO_RATE
            | T::AV_OPT_TYPE_BINARY
            | T::AV_OPT_TYPE_CHLAYOUT
            | T::AV_OPT_TYPE_DICT => {
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
// SAFETY: as for `OptionRanges`: `OptionRangesMut` has no setters, so it
// reaches only the same bytes, read-only.
unsafe impl Send for OptionRangesMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `OptionRangesRef`
// has getters only, the list has no interior mutability, and no C path writes
// a list after its query returns, so concurrent shared access reads
// unchanging bytes.
unsafe impl Sync for OptionRanges {}
// SAFETY: as for `Sync for OptionRanges`: copies of a shared handle only read.
unsafe impl Send for OptionRangesRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for OptionRangesRef<'_> {}
// SAFETY: `&OptionRangesMut` reaches the list only through `as_ref`, a shared
// handle, and the exclusive handle has no setters either.
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
const AVERROR_BUG: i32 = -i32::from_le_bytes(*b"BUG!");

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
    /// `c * range_count() + i`, as [`get`](Self::get) indexes. An element may
    /// be NULL when a class callback left it so.
    ///
    /// Raw elements until `AVOptionRange` is wrapped: each addresses a range
    /// the list owns, live and unwritten for `'a`.
    pub fn ranges(&self) -> CSlice<'a, *const ffi::AVOptionRange> {
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

    /// Field: AVOptionRanges.range
    ///
    /// Range `index` of component `component`; `None` when either is out of
    /// bounds. Raw, as for [`ranges`](Self::ranges).
    pub fn get(&self, index: usize, component: usize) -> Option<*const ffi::AVOptionRange> {
        if index >= self.range_count() || component >= self.component_count() {
            return None;
        }
        self.ranges().elem(component * self.range_count() + index)
    }
}

#[cfg(test)]
#[path = "../tests/common/opt.rs"]
mod fixture;

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
        assert_eq!(o.option_type(), T::AV_OPT_TYPE_INT);
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
                assert_eq!(o.option_type(), T::AV_OPT_TYPE_CONST);
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

    /// Reads the value bounds of the range at `p`.
    fn bounds(p: *const ffi::AVOptionRange) -> (f64, f64) {
        // SAFETY: each caller passes an element of a live `Local`.
        unsafe { ((*p).value_min, (*p).value_max) }
    }

    #[test]
    fn ranges_index_component_major() {
        let l = Local::new(2, 3, None);
        let r = l.handle();
        assert_eq!((r.range_count(), r.component_count()), (2, 3));
        let all = r.ranges();
        assert_eq!(all.len(), 6);
        assert_eq!(
            all.elems().map(bounds).map(|b| b.0).collect::<Vec<_>>(),
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
        assert!(r.ranges().is_empty());
        assert!(r.get(0, 0).is_none());

        // An empty list owns no array.
        let l = Local::new(0, 1, None).without_array();
        assert!(l.handle().ranges().is_empty());
    }

    #[test]
    fn null_elements_pass_through() {
        let l = Local::new(3, 1, Some(1));
        let r = l.handle();
        assert!(r.get(1, 0).unwrap().is_null());
        assert_eq!(r.ranges().elems().filter(|p| p.is_null()).count(), 1);
    }
}
