//! A test `AVClass` with one option of each type `libavutil/opt.h` defaults,
//! shared by the `opt` suites: the crate's unit tests and the `equiv` and
//! `ub_unsafe` integration tests include it with `#[path]`, after bringing the
//! `-sys` crate into scope as `ffi`.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::mem::{offset_of, size_of};
use core::ptr::null;

use ffi::AVOptionType as T;

use super::ffi;

/// The object [`CLASS`] describes; its fields hold the option values.
#[repr(C)]
pub(crate) struct Ctx {
    pub(crate) class: *const ffi::AVClass,
    pub(crate) int: c_int,
    pub(crate) int64: i64,
    pub(crate) uint64: u64,
    pub(crate) dbl: f64,
    pub(crate) flt: f32,
    pub(crate) string: *mut c_char,
    pub(crate) q: ffi::AVRational,
    pub(crate) boolean: c_int,
    pub(crate) flags: c_int,
    pub(crate) size: [c_int; 2],
    pub(crate) rate: ffi::AVRational,
    pub(crate) color: [u8; 4],
    pub(crate) duration: i64,
    pub(crate) binary: *mut u8,
    pub(crate) binary_len: c_int,
    pub(crate) dict: *mut c_void,
    pub(crate) array: *mut c_int,
    pub(crate) array_len: c_uint,
    pub(crate) empty_array: *mut c_int,
    pub(crate) empty_array_len: c_uint,
    pub(crate) bare: c_int,
    pub(crate) nostr: *mut c_char,
    pub(crate) bogus: c_int,
}

impl Ctx {
    /// An object of `class` with every option zeroed, as `av_opt_set_defaults`
    /// expects it.
    pub(crate) fn new(class: &'static Class) -> Self {
        // SAFETY: every field is an integer, a float or a raw pointer, for
        // which all-zero is a valid value.
        let mut ctx: Self = unsafe { core::mem::zeroed() };
        ctx.class = &raw const class.0;
        ctx
    }

    /// `self` as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_obj(&mut self) -> *mut c_void {
        std::ptr::from_mut::<Self>(self).cast()
    }
}

/// An option table, `Sync` to live in a `static`.
pub(crate) struct Table<const N: usize>(pub [ffi::AVOption; N]);
// SAFETY: the table is immutable and its pointers address immutable statics.
unsafe impl<const N: usize> Sync for Table<N> {}

/// A class, `Sync` to live in a `static`.
pub(crate) struct Class(pub ffi::AVClass);
// SAFETY: as for `Table`.
unsafe impl Sync for Class {}

/// A `const AVClass *`, `Sync` to live in a `static`: the fake object
/// `AV_OPT_SEARCH_FAKE_OBJ` queries take.
pub(crate) struct FakeObj(pub *const ffi::AVClass);
// SAFETY: as for `Table`.
unsafe impl Sync for FakeObj {}

impl FakeObj {
    /// `self` as the `void *` the `av_opt_*` routines take with
    /// `AV_OPT_SEARCH_FAKE_OBJ`; they only read it.
    pub(crate) fn as_obj(&'static self) -> *mut c_void {
        std::ptr::from_ref::<Self>(self).cast_mut().cast()
    }
}

/// One option: `name`, `help`, the value at `offset`, of type `ty`, defaulting
/// to `default`, within `[min, max]`.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the arguments of the C routine it stands for"
)]
const fn opt(
    name: *const c_char,
    help: *const c_char,
    offset: usize,
    ty: T,
    default: ffi::AVOption__bindgen_ty_1,
    min: f64,
    max: f64,
    unit: *const c_char,
) -> ffi::AVOption {
    ffi::AVOption {
        name,
        help,
        offset: offset as c_int,
        type_: ty,
        default_val: default,
        min,
        max,
        flags: 0,
        unit,
    }
}

/// A flag on the options whose defaults C builds by growing an `av_realloc`
/// block (`dict`, `array`), so that suites can select them apart from the
/// scalar defaults with `av_opt_set_defaults2`.
pub(crate) const GROWS: c_int = 1 << 30;

/// `o`, flagged [`GROWS`].
const fn grows(mut o: ffi::AVOption) -> ffi::AVOption {
    o.flags = GROWS;
    o
}

const fn int(v: i64) -> ffi::AVOption__bindgen_ty_1 {
    ffi::AVOption__bindgen_ty_1 { i64_: v }
}
const fn dbl(v: f64) -> ffi::AVOption__bindgen_ty_1 {
    ffi::AVOption__bindgen_ty_1 { dbl: v }
}
const fn str_(v: *const c_char) -> ffi::AVOption__bindgen_ty_1 {
    ffi::AVOption__bindgen_ty_1 { str_: v }
}

/// The definition of the `array` option: two elements by default.
pub(crate) struct ArrayDef(pub ffi::AVOptionArrayDef);
// SAFETY: as for `Table`.
unsafe impl Sync for ArrayDef {}
pub(crate) static ARRAY_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"1,2".as_ptr(),
    size_min: 0,
    size_max: 0,
    sep: 0,
});

const MAX: f64 = i32::MAX as f64;

/// The options of [`CLASS`], in `Ctx` order, then a named constant and the
/// terminator.
#[rustfmt::skip]
pub(crate) static OPTIONS: Table<23> = Table([
    opt(c"int".as_ptr(), c"an int".as_ptr(), offset_of!(Ctx, int), ffi::AV_OPT_TYPE_INT, int(7), -10.0, 100.0, c"mode".as_ptr()),
    opt(c"int64".as_ptr(), c"an int64".as_ptr(), offset_of!(Ctx, int64), ffi::AV_OPT_TYPE_INT64, int(-5_000_000_000), i64::MIN as f64, i64::MAX as f64, null()),
    opt(c"uint64".as_ptr(), c"a uint64".as_ptr(), offset_of!(Ctx, uint64), ffi::AV_OPT_TYPE_UINT64, int(42), 0.0, u64::MAX as f64, null()),
    opt(c"dbl".as_ptr(), c"a double".as_ptr(), offset_of!(Ctx, dbl), ffi::AV_OPT_TYPE_DOUBLE, dbl(2.5), -1e9, 1e9, null()),
    opt(c"flt".as_ptr(), c"a float".as_ptr(), offset_of!(Ctx, flt), ffi::AV_OPT_TYPE_FLOAT, dbl(0.25), 0.0, 1.0, null()),
    opt(c"string".as_ptr(), c"a string".as_ptr(), offset_of!(Ctx, string), ffi::AV_OPT_TYPE_STRING, str_(c"hello".as_ptr()), 0.0, 0.0, null()),
    opt(c"q".as_ptr(), c"a rational".as_ptr(), offset_of!(Ctx, q), ffi::AV_OPT_TYPE_RATIONAL, dbl(0.5), 0.0, MAX, null()),
    opt(c"boolean".as_ptr(), c"a bool".as_ptr(), offset_of!(Ctx, boolean), ffi::AV_OPT_TYPE_BOOL, int(1), -1.0, 1.0, null()),
    opt(c"flags".as_ptr(), c"some flags".as_ptr(), offset_of!(Ctx, flags), ffi::AV_OPT_TYPE_FLAGS, int(3), 0.0, MAX, c"fl".as_ptr()),
    opt(c"size".as_ptr(), c"an image size".as_ptr(), offset_of!(Ctx, size), ffi::AV_OPT_TYPE_IMAGE_SIZE, str_(c"640x480".as_ptr()), 0.0, MAX, null()),
    opt(c"rate".as_ptr(), c"a video rate".as_ptr(), offset_of!(Ctx, rate), ffi::AV_OPT_TYPE_VIDEO_RATE, str_(c"25".as_ptr()), 0.0, MAX, null()),
    opt(c"color".as_ptr(), c"a color".as_ptr(), offset_of!(Ctx, color), ffi::AV_OPT_TYPE_COLOR, str_(c"red".as_ptr()), 0.0, 0.0, null()),
    opt(c"duration".as_ptr(), c"a duration".as_ptr(), offset_of!(Ctx, duration), ffi::AV_OPT_TYPE_DURATION, int(1_500_000), 0.0, i64::MAX as f64, null()),
    opt(c"binary".as_ptr(), c"some bytes".as_ptr(), offset_of!(Ctx, binary), ffi::AV_OPT_TYPE_BINARY, str_(c"0a0b".as_ptr()), 0.0, 0.0, null()),
    grows(opt(c"dict".as_ptr(), c"a dictionary".as_ptr(), offset_of!(Ctx, dict), ffi::AV_OPT_TYPE_DICT, str_(c"k=v".as_ptr()), 0.0, 0.0, null())),
    grows(opt(c"array".as_ptr(), c"an int array".as_ptr(), offset_of!(Ctx, array), ffi::AV_OPT_TYPE_INT | ffi::AV_OPT_TYPE_FLAG_ARRAY, ffi::AVOption__bindgen_ty_1 { arr: &raw const ARRAY_DEF.0 }, 0.0, 10.0, null())),
    opt(c"empty_array".as_ptr(), c"an array without a default".as_ptr(), offset_of!(Ctx, empty_array), ffi::AV_OPT_TYPE_INT | ffi::AV_OPT_TYPE_FLAG_ARRAY, ffi::AVOption__bindgen_ty_1 { arr: null() }, 0.0, 10.0, null()),
    opt(c"bare".as_ptr(), null(), offset_of!(Ctx, bare), ffi::AV_OPT_TYPE_INT, int(0), 0.0, 1.0, null()),
    opt(c"nostr".as_ptr(), c"a string without a default".as_ptr(), offset_of!(Ctx, nostr), ffi::AV_OPT_TYPE_STRING, str_(null()), 0.0, 0.0, null()),
    opt(c"bogus".as_ptr(), c"a type opt.h does not define".as_ptr(), offset_of!(Ctx, bogus), 99, int(0), 0.0, 0.0, null()),
    opt(c"seven".as_ptr(), c"a named constant".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(7), 0.0, 0.0, c"mode".as_ptr()),
    opt(c"bit".as_ptr(), c"a flag constant".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"fl".as_ptr()),
    opt(null(), null(), 0, 0, int(0), 0.0, 0.0, null()),
]);

/// The class of [`Ctx`], with [`OPTIONS`] and no `query_ranges` callback.
pub(crate) static CLASS: Class = Class(class(None));

/// `CLASS` as a fake object.
pub(crate) static FAKE: FakeObj = FakeObj(&raw const CLASS.0);

/// [`CLASS`] with `query_ranges` set to `callback`.
pub(crate) const fn class(
    callback: Option<
        unsafe extern "C" fn(
            *mut *mut ffi::AVOptionRanges,
            *mut c_void,
            *const c_char,
            c_int,
        ) -> c_int,
    >,
) -> ffi::AVClass {
    ffi::AVClass {
        class_name: c"crustify-opt-test".as_ptr(),
        item_name: None,
        option: &raw const OPTIONS.0[0],
        version: 0,
        log_level_offset_offset: 0,
        parent_log_context_offset: 0,
        category: ffi::AV_CLASS_CATEGORY_NA,
        get_category: None,
        query_ranges: callback,
        child_next: None,
        child_class_iterate: None,
        state_flags_offset: 0,
    }
}

/// The number of option names [`OPTIONS`] holds before its terminator.
pub(crate) const NAMED: usize = OPTIONS.0.len() - 1;

/// Keys a range query succeeds for with the default ranges.
pub(crate) const RANGED: [&core::ffi::CStr; 14] = [
    c"int",
    c"int64",
    c"uint64",
    c"dbl",
    c"flt",
    c"string",
    c"q",
    c"boolean",
    c"size",
    c"rate",
    c"color",
    c"duration",
    c"bare",
    c"nostr",
];

/// Keys a default range query rejects with `AVERROR(ENOSYS)`: types without
/// ranges.
pub(crate) const UNRANGED: [&core::ffi::CStr; 6] = [
    c"flags",
    c"binary",
    c"dict",
    c"array",
    c"empty_array",
    c"bogus",
];

/// Keys a range query rejects with `AVERROR(ENOMEM)`: unknown names, and named
/// constants, which a lookup without a unit skips.
pub(crate) const UNKNOWN: [&core::ffi::CStr; 4] = [c"missing", c"seven", c"bit", c""];

/// `AVERROR(EINVAL)`, which [`query_ranges`] fails with for `"fail"`.
pub(crate) const EINVAL: c_int = -22;

/// A `query_ranges` callback of [`CB_CLASS`], building its lists as a class
/// would, through the `av_malloc` family:
///
/// - `"size"`: 2 ranges of 3 components with `AV_OPT_MULTI_COMPONENT_RANGE`, else of 1, each with
///   an owned `str`;
/// - `"sparse"`: 3 ranges of 1 component, the middle one NULL;
/// - `"fail"`: `AVERROR(EINVAL)`, storing NULL;
/// - any other key: the default ranges.
///
/// # Safety
///
/// As `av_opt_query_ranges`' callback.
pub(crate) unsafe extern "C" fn query_ranges(
    out: *mut *mut ffi::AVOptionRanges,
    obj: *mut c_void,
    key: *const c_char,
    flags: c_int,
) -> c_int {
    // SAFETY: C passes a live NUL-terminated key.
    let key = unsafe { core::ffi::CStr::from_ptr(key) };
    let (nb_ranges, nb_components, hole) = match key.to_bytes() {
        b"size" if flags & ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int != 0 => (2, 3, None),
        b"size" => (2, 1, None),
        b"sparse" => (3, 1, Some(1)),
        b"fail" => {
            // SAFETY: `out` is C's writable slot.
            unsafe { *out = core::ptr::null_mut() };
            return EINVAL;
        }
        // SAFETY: the callback's arguments, forwarded unchanged.
        _ => return unsafe { ffi::av_opt_query_ranges_default(out, obj, key.as_ptr(), flags) },
    };
    let n = (nb_ranges * nb_components) as usize;
    // SAFETY: `av_mallocz` has no preconditions; failures abort the test.
    let list =
        unsafe { ffi::av_mallocz(size_of::<ffi::AVOptionRanges>()) }.cast::<ffi::AVOptionRanges>();
    // SAFETY: as above.
    let array = unsafe { ffi::av_mallocz(n * size_of::<*mut ffi::AVOptionRange>()) }
        .cast::<*mut ffi::AVOptionRange>();
    assert!(!list.is_null() && !array.is_null());
    for i in 0..n {
        if hole == Some(i) {
            continue;
        }
        // SAFETY: as above.
        let r = unsafe { ffi::av_mallocz(size_of::<ffi::AVOptionRange>()) }
            .cast::<ffi::AVOptionRange>();
        assert!(!r.is_null());
        // For the unsafe operations below: `r` is a fresh zeroed range, `array` holds `n` slots,
        // and `av_strdup` reads a literal.
        {
            // SAFETY: as the comment above this block states.
            let label = unsafe { ffi::av_strdup(c"range".as_ptr()) };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).str_ = label };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).value_min = i as f64 };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).value_max = i as f64 + 10.0 };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).component_min = -(i as f64) };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).component_max = 255.0 };
            // SAFETY: as the comment above this block states.
            unsafe { (*r).is_range = (i % 2) as c_int };
            // SAFETY: as the comment below states, for this part of the operation.
            let array_add = unsafe { array.add(i) };
            // SAFETY: as the comment above this block states.
            unsafe { *array_add = r };
        }
    }
    // For the unsafe operations below: `list` is a fresh zeroed list and `out` C's writable slot.
    {
        // SAFETY: as the comment above this block states.
        unsafe { (*list).range = array };
        // SAFETY: as the comment above this block states.
        unsafe { (*list).nb_ranges = nb_ranges };
        // SAFETY: as the comment above this block states.
        unsafe { (*list).nb_components = nb_components };
        // SAFETY: as the comment above this block states.
        unsafe { *out = list };
    }
    nb_components
}

/// [`CLASS`] with [`query_ranges`] as its callback.
pub(crate) static CB_CLASS: Class = Class(class(Some(query_ranges)));

/// `CB_CLASS` as a fake object.
pub(crate) static CB_FAKE: FakeObj = FakeObj(&raw const CB_CLASS.0);

/// Keys [`query_ranges`] builds its own lists for.
pub(crate) const CB_KEYS: [&core::ffi::CStr; 3] = [c"size", c"sparse", c"fail"];

// Options wave 0, batch 3: a well-formed object for `ObjectRef` / `ObjectMut`.
// `CLASS` above has an option of a type opt.h does not define, outside their
// contract; `OBJ_CLASS` describes `Obj`, every field of a defined type, with a
// child of its own class reached through `child_next`.

/// The object [`OBJ_CLASS`] describes.
#[repr(C)]
pub(crate) struct Obj {
    pub(crate) class: *const ffi::AVClass,
    pub(crate) int: c_int,
    pub(crate) int64: i64,
    pub(crate) uint64: u64,
    pub(crate) uint: c_uint,
    pub(crate) dbl: f64,
    pub(crate) flt: f32,
    pub(crate) string: *mut c_char,
    pub(crate) q: ffi::AVRational,
    pub(crate) wide_q: ffi::AVRational,
    pub(crate) boolean: c_int,
    pub(crate) flags: c_int,
    pub(crate) big: i64,
    pub(crate) size: [c_int; 2],
    pub(crate) rate: ffi::AVRational,
    pub(crate) color: [u8; 4],
    pub(crate) duration: i64,
    pub(crate) binary: *mut u8,
    pub(crate) binary_len: c_int,
    pub(crate) dict: *mut c_void,
    pub(crate) array: *mut c_int,
    pub(crate) array_len: c_uint,
    pub(crate) strings: *mut *mut c_char,
    pub(crate) strings_len: c_uint,
    pub(crate) empty: *mut c_int,
    pub(crate) empty_len: c_uint,
    pub(crate) layout: ffi::AVChannelLayout,
    pub(crate) pix: c_int,
    pub(crate) sample: c_int,
    pub(crate) ro: c_int,
    pub(crate) nostr: *mut c_char,
    pub(crate) child: *mut Obj,
}

/// The definition of the `strings` option: `|`-separated, two by default.
pub(crate) static STRINGS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"a|b\\|c".as_ptr(),
    size_min: 0,
    size_max: 4,
    sep: b'|' as c_char,
});

/// `o` with `flags`.
const fn flagged(mut o: ffi::AVOption, flags: u32) -> ffi::AVOption {
    o.flags = flags as c_int;
    o
}

const INT_MAX: f64 = i32::MAX as f64;

/// The options of [`OBJ_CLASS`], in `Obj` order, then the named constants
/// and the terminator.
#[rustfmt::skip]
pub(crate) static OBJ_OPTIONS: Table<40> = Table([
    opt(c"int".as_ptr(), c"an int".as_ptr(), offset_of!(Obj, int), ffi::AV_OPT_TYPE_INT, int(7), -10.0, 100.0, c"mode".as_ptr()),
    opt(c"int64".as_ptr(), c"an int64".as_ptr(), offset_of!(Obj, int64), ffi::AV_OPT_TYPE_INT64, int(-5_000_000_000), i64::MIN as f64, i64::MAX as f64, null()),
    opt(c"uint64".as_ptr(), c"a uint64".as_ptr(), offset_of!(Obj, uint64), ffi::AV_OPT_TYPE_UINT64, int(42), 0.0, u64::MAX as f64, null()),
    opt(c"uint".as_ptr(), c"a uint".as_ptr(), offset_of!(Obj, uint), ffi::AV_OPT_TYPE_UINT, int(9), 0.0, u32::MAX as f64, null()),
    opt(c"dbl".as_ptr(), c"a double".as_ptr(), offset_of!(Obj, dbl), ffi::AV_OPT_TYPE_DOUBLE, dbl(2.5), f64::MIN, f64::MAX, null()),
    opt(c"flt".as_ptr(), c"a float".as_ptr(), offset_of!(Obj, flt), ffi::AV_OPT_TYPE_FLOAT, dbl(0.25), 0.0, 1.0, null()),
    opt(c"string".as_ptr(), c"a string".as_ptr(), offset_of!(Obj, string), ffi::AV_OPT_TYPE_STRING, str_(c"hello".as_ptr()), 0.0, 0.0, null()),
    opt(c"q".as_ptr(), c"a rational".as_ptr(), offset_of!(Obj, q), ffi::AV_OPT_TYPE_RATIONAL, dbl(0.5), 0.0, INT_MAX, null()),
    opt(c"wide_q".as_ptr(), c"a rational of wide bounds".as_ptr(), offset_of!(Obj, wide_q), ffi::AV_OPT_TYPE_RATIONAL, dbl(0.25), -1e300, 1e300, null()),
    opt(c"boolean".as_ptr(), c"a bool".as_ptr(), offset_of!(Obj, boolean), ffi::AV_OPT_TYPE_BOOL, int(1), -1.0, 1.0, null()),
    opt(c"flags".as_ptr(), c"some flags".as_ptr(), offset_of!(Obj, flags), ffi::AV_OPT_TYPE_FLAGS, int(3), 0.0, u32::MAX as f64, c"fl".as_ptr()),
    opt(c"big".as_ptr(), c"64-bit flags".as_ptr(), offset_of!(Obj, big), ffi::AV_OPT_TYPE_INT64, int(0), i64::MIN as f64, i64::MAX as f64, c"big".as_ptr()),
    opt(c"size".as_ptr(), c"an image size".as_ptr(), offset_of!(Obj, size), ffi::AV_OPT_TYPE_IMAGE_SIZE, str_(c"640x480".as_ptr()), 0.0, INT_MAX, null()),
    opt(c"rate".as_ptr(), c"a video rate".as_ptr(), offset_of!(Obj, rate), ffi::AV_OPT_TYPE_VIDEO_RATE, str_(c"25".as_ptr()), 0.0, INT_MAX, null()),
    opt(c"color".as_ptr(), c"a color".as_ptr(), offset_of!(Obj, color), ffi::AV_OPT_TYPE_COLOR, str_(c"red".as_ptr()), 0.0, 0.0, null()),
    opt(c"duration".as_ptr(), c"a duration".as_ptr(), offset_of!(Obj, duration), ffi::AV_OPT_TYPE_DURATION, int(1_500_000), 0.0, i64::MAX as f64, null()),
    opt(c"binary".as_ptr(), c"some bytes".as_ptr(), offset_of!(Obj, binary), ffi::AV_OPT_TYPE_BINARY, str_(c"0a0b".as_ptr()), 0.0, 0.0, null()),
    opt(c"dict".as_ptr(), c"a dictionary".as_ptr(), offset_of!(Obj, dict), ffi::AV_OPT_TYPE_DICT, str_(c"k=v".as_ptr()), 0.0, 0.0, null()),
    opt(c"array".as_ptr(), c"an int array".as_ptr(), offset_of!(Obj, array), ffi::AV_OPT_TYPE_INT | ffi::AV_OPT_TYPE_FLAG_ARRAY, ffi::AVOption__bindgen_ty_1 { arr: &raw const ARRAY_DEF.0 }, 0.0, 10.0, null()),
    opt(c"strings".as_ptr(), c"a string array".as_ptr(), offset_of!(Obj, strings), ffi::AV_OPT_TYPE_STRING | ffi::AV_OPT_TYPE_FLAG_ARRAY, ffi::AVOption__bindgen_ty_1 { arr: &raw const STRINGS_DEF.0 }, 0.0, 0.0, null()),
    opt(c"empty".as_ptr(), c"an int array without a definition".as_ptr(), offset_of!(Obj, empty), ffi::AV_OPT_TYPE_INT | ffi::AV_OPT_TYPE_FLAG_ARRAY, ffi::AVOption__bindgen_ty_1 { arr: null() }, 0.0, 10.0, null()),
    opt(c"layout".as_ptr(), c"a channel layout".as_ptr(), offset_of!(Obj, layout), ffi::AV_OPT_TYPE_CHLAYOUT, str_(c"stereo".as_ptr()), 0.0, 0.0, null()),
    opt(c"pix".as_ptr(), c"a pixel format".as_ptr(), offset_of!(Obj, pix), ffi::AV_OPT_TYPE_PIXEL_FMT, int(0), -1.0, INT_MAX, null()),
    opt(c"sample".as_ptr(), c"a sample format".as_ptr(), offset_of!(Obj, sample), ffi::AV_OPT_TYPE_SAMPLE_FMT, int(1), -1.0, INT_MAX, null()),
    flagged(opt(c"ro".as_ptr(), c"read-only".as_ptr(), offset_of!(Obj, ro), ffi::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, null()), ffi::AV_OPT_FLAG_READONLY | ffi::AV_OPT_FLAG_EXPORT),
    flagged(opt(c"nostr".as_ptr(), c"a string without a default".as_ptr(), offset_of!(Obj, nostr), ffi::AV_OPT_TYPE_STRING, str_(null()), 0.0, 0.0, null()), ffi::AV_OPT_FLAG_ENCODING_PARAM),
    flagged(opt(c"cflags".as_ptr(), c"flags with child constants".as_ptr(), offset_of!(Obj, flags), ffi::AV_OPT_TYPE_FLAGS, int(3), 0.0, u32::MAX as f64, c"fl".as_ptr()), ffi::AV_OPT_FLAG_CHILD_CONSTS),
    flagged(opt(c"old".as_ptr(), c"use ro".as_ptr(), offset_of!(Obj, ro), ffi::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, null()), ffi::AV_OPT_FLAG_DEPRECATED),
    opt(c"seven".as_ptr(), c"a named constant".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(7), 0.0, 0.0, c"mode".as_ptr()),
    opt(c"one".as_ptr(), c"flag 1".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"bit".as_ptr(), c"flag 2".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"eight".as_ptr(), c"flag 8".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(8), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"huge".as_ptr(), c"a flag beyond int64_t as a double".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(i64::MAX), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"low".as_ptr(), c"bit 0".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"big".as_ptr()),
    opt(c"high".as_ptr(), c"bit 40".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(1 << 40), 0.0, 0.0, c"big".as_ptr()),
    opt(c"mixed".as_ptr(), c"bits 0 and 40".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(1 | 1 << 40), 0.0, 0.0, c"big".as_ptr()),
    opt(c"notconst".as_ptr(), c"an int in unit big".as_ptr(), offset_of!(Obj, ro), ffi::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, c"big".as_ptr()),
    opt(c"dbl_unit".as_ptr(), c"a double with a unit".as_ptr(), offset_of!(Obj, dbl), ffi::AV_OPT_TYPE_DOUBLE, dbl(2.5), f64::MIN, f64::MAX, c"dunit".as_ptr()),
    opt(c"dflag".as_ptr(), c"a constant of unit dunit".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"dunit".as_ptr()),
    opt(null(), null(), 0, 0, int(0), 0.0, 0.0, null()),
]);

/// `child_next` of [`OBJ_CLASS`]: an object's `child`, once.
///
/// # Safety
///
/// `obj` must address a live [`Obj`].
pub(crate) unsafe extern "C" fn obj_child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
    if !prev.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: C passes the live `Obj` it found this class in.
    unsafe { (*obj.cast::<Obj>()).child.cast() }
}

/// The class of [`Obj`].
pub(crate) static OBJ_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-obj".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &raw const OBJ_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: Some(obj_child_next),
    child_class_iterate: None,
    state_flags_offset: 0,
});

/// A heap [`Obj`] of [`OBJ_CLASS`], reached only through its raw pointer
/// (so that C's writes are never invalidated by a Rust reborrow), with an
/// optional child; dropping it frees what its options own, then it.
pub(crate) struct Owned(core::ptr::NonNull<Obj>);

impl Owned {
    /// A zeroed object, before `av_opt_set_defaults`.
    pub(crate) fn zeroed() -> Self {
        // SAFETY: every field of `Obj` is an integer, a float, a raw pointer
        // or a plain C struct of those, for which all-zero is valid.
        let obj: Box<Obj> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // SAFETY: `p` is the fresh allocation above.
        unsafe { (*p).class = &raw const OBJ_CLASS.0 };
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// An object with its defaults set by C.
    pub(crate) fn new() -> Self {
        let o = Self::zeroed();
        // SAFETY: a live zeroed object of `OBJ_CLASS`.
        unsafe { ffi::av_opt_set_defaults(o.as_obj()) };
        o
    }

    /// An object with defaults and a child object with defaults.
    pub(crate) fn with_child() -> Self {
        let o = Self::new();
        let child = Self::new();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        let _ = core::mem::ManuallyDrop::new(child);
        o
    }

    /// A zeroed object with a zeroed child, before any default.
    pub(crate) fn zeroed_with_child() -> Self {
        let o = Self::zeroed();
        let child = Self::zeroed();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        let _ = core::mem::ManuallyDrop::new(child);
        o
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_obj(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The object, for raw field reads.
    pub(crate) fn ptr(&self) -> *mut Obj {
        self.0.as_ptr()
    }

    /// The child object, if any.
    pub(crate) fn child(&self) -> *mut Obj {
        // SAFETY: `self` is live.
        unsafe { (*self.0.as_ptr()).child }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        let p = self.0.as_ptr();
        // For the unsafe operations below: `p` and its child are live objects this owner allocated,
        // whose options own what `av_opt_free` releases.
        {
            // SAFETY: as the comment above this block states.
            let child = unsafe { (*p).child };
            // For the unsafe operations below: as the comment above this block states.
            if !child.is_null() {
                // SAFETY: as the comment above this statement states.
                unsafe { ffi::av_opt_free(child.cast()) };
                // SAFETY: as the comment at the head of this block states.
                drop(unsafe { Box::from_raw(child) });
            }
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_opt_free(p.cast()) };
            // SAFETY: as the comment above this block states.
            unsafe { drop(Box::from_raw(p)) };
        }
    }
}

// Options wave 1, batch 0: an array of each option type `av_opt_get_array`
// reads, for `ObjectRef::get_array`, besides `Obj`'s `int` and `string`
// arrays. The binary array has no default: C parses a binary element into
// a pointer and a length, which an array element has no room for.

/// The object [`ARR_CLASS`] describes: each array is a pointer followed by
/// its element count.
#[repr(C)]
pub(crate) struct ArrObj {
    pub(crate) class: *const ffi::AVClass,
    pub(crate) ints: *mut c_int,
    pub(crate) ints_len: c_uint,
    pub(crate) uints: *mut c_uint,
    pub(crate) uints_len: c_uint,
    pub(crate) i64s: *mut i64,
    pub(crate) i64s_len: c_uint,
    pub(crate) u64s: *mut u64,
    pub(crate) u64s_len: c_uint,
    pub(crate) dbls: *mut f64,
    pub(crate) dbls_len: c_uint,
    pub(crate) flts: *mut f32,
    pub(crate) flts_len: c_uint,
    pub(crate) strs: *mut *mut c_char,
    pub(crate) strs_len: c_uint,
    pub(crate) qs: *mut ffi::AVRational,
    pub(crate) qs_len: c_uint,
    pub(crate) rates: *mut ffi::AVRational,
    pub(crate) rates_len: c_uint,
    pub(crate) bools: *mut c_int,
    pub(crate) bools_len: c_uint,
    pub(crate) sizes: *mut [c_int; 2],
    pub(crate) sizes_len: c_uint,
    pub(crate) colors: *mut [u8; 4],
    pub(crate) colors_len: c_uint,
    pub(crate) durs: *mut i64,
    pub(crate) durs_len: c_uint,
    pub(crate) pixs: *mut c_int,
    pub(crate) pixs_len: c_uint,
    pub(crate) samples: *mut c_int,
    pub(crate) samples_len: c_uint,
    pub(crate) dicts: *mut *mut c_void,
    pub(crate) dicts_len: c_uint,
    pub(crate) layouts: *mut ffi::AVChannelLayout,
    pub(crate) layouts_len: c_uint,
    pub(crate) bins: *mut *mut u8,
    pub(crate) bins_len: c_uint,
}

/// An array definition with default `def` and separator `sep`.
const fn arr_def(def: *const c_char, sep: u8) -> ArrayDef {
    ArrayDef(ffi::AVOptionArrayDef {
        def,
        size_min: 0,
        size_max: 0,
        sep: sep as c_char,
    })
}

pub(crate) static INTS_DEF: ArrayDef = arr_def(c"1,-2,2147483647,-2147483648".as_ptr(), 0);
pub(crate) static UINTS_DEF: ArrayDef = arr_def(c"0,4294967295,9".as_ptr(), 0);
pub(crate) static I64S_DEF: ArrayDef = arr_def(c"-5000000000,123456789012".as_ptr(), 0);
pub(crate) static U64S_DEF: ArrayDef = arr_def(c"42,7".as_ptr(), 0);
pub(crate) static DBLS_DEF: ArrayDef = arr_def(c"1.5,-2,1e300,0.1".as_ptr(), 0);
pub(crate) static FLTS_DEF: ArrayDef = arr_def(c"0.25,-3e38,1".as_ptr(), 0);
pub(crate) static STRS_DEF: ArrayDef = arr_def(c"x,,y\\,z".as_ptr(), 0);
pub(crate) static QS_DEF: ArrayDef = arr_def(c"1/3,5,-7/2".as_ptr(), 0);
pub(crate) static RATES_DEF: ArrayDef = arr_def(c"25,30000/1001".as_ptr(), 0);
pub(crate) static BOOLS_DEF: ArrayDef = arr_def(c"true,false,auto".as_ptr(), 0);
pub(crate) static SIZES_DEF: ArrayDef = arr_def(c"640x480,hd720".as_ptr(), 0);
pub(crate) static COLORS_DEF: ArrayDef = arr_def(c"red,0x00ff0080".as_ptr(), 0);
pub(crate) static DURS_DEF: ArrayDef = arr_def(c"1.5,00:01:00".as_ptr(), 0);
pub(crate) static PIXS_DEF: ArrayDef = arr_def(c"yuv420p,rgb24,none".as_ptr(), 0);
pub(crate) static SAMPLES_DEF: ArrayDef = arr_def(c"s16,fltp".as_ptr(), 0);
pub(crate) static DICTS_DEF: ArrayDef = arr_def(c"a=1:b=2:A=3,k=v".as_ptr(), 0);
pub(crate) static LAYOUTS_DEF: ArrayDef = arr_def(c"stereo|5.1(side)|FL+FR".as_ptr(), b'|');

/// An array option `name` of element type `ty` at `offset`, defaulting to
/// `def`, within `[min, max]`.
const fn arr(
    name: *const c_char,
    offset: usize,
    ty: T,
    def: Option<&'static ArrayDef>,
    min: f64,
    max: f64,
) -> ffi::AVOption {
    let def = match def {
        Some(d) => &raw const d.0,
        None => null(),
    };
    opt(
        name,
        c"an array".as_ptr(),
        offset,
        ty | ffi::AV_OPT_TYPE_FLAG_ARRAY,
        ffi::AVOption__bindgen_ty_1 { arr: def },
        min,
        max,
        null(),
    )
}

/// The options of [`ARR_CLASS`], in `ArrObj` order, then the terminator.
#[rustfmt::skip]
pub(crate) static ARR_OPTIONS: Table<19> = Table([
    arr(c"ints".as_ptr(), offset_of!(ArrObj, ints), ffi::AV_OPT_TYPE_INT, Some(&INTS_DEF), i32::MIN as f64, INT_MAX),
    arr(c"uints".as_ptr(), offset_of!(ArrObj, uints), ffi::AV_OPT_TYPE_UINT, Some(&UINTS_DEF), 0.0, u32::MAX as f64),
    arr(c"i64s".as_ptr(), offset_of!(ArrObj, i64s), ffi::AV_OPT_TYPE_INT64, Some(&I64S_DEF), -1e18, 1e18),
    arr(c"u64s".as_ptr(), offset_of!(ArrObj, u64s), ffi::AV_OPT_TYPE_UINT64, Some(&U64S_DEF), 0.0, 1e18),
    arr(c"dbls".as_ptr(), offset_of!(ArrObj, dbls), ffi::AV_OPT_TYPE_DOUBLE, Some(&DBLS_DEF), f64::MIN, f64::MAX),
    arr(c"flts".as_ptr(), offset_of!(ArrObj, flts), ffi::AV_OPT_TYPE_FLOAT, Some(&FLTS_DEF), -f32::MAX as f64, f32::MAX as f64),
    arr(c"strs".as_ptr(), offset_of!(ArrObj, strs), ffi::AV_OPT_TYPE_STRING, Some(&STRS_DEF), 0.0, 0.0),
    arr(c"qs".as_ptr(), offset_of!(ArrObj, qs), ffi::AV_OPT_TYPE_RATIONAL, Some(&QS_DEF), -1e9, 1e9),
    arr(c"rates".as_ptr(), offset_of!(ArrObj, rates), ffi::AV_OPT_TYPE_VIDEO_RATE, Some(&RATES_DEF), 0.0, INT_MAX),
    arr(c"bools".as_ptr(), offset_of!(ArrObj, bools), ffi::AV_OPT_TYPE_BOOL, Some(&BOOLS_DEF), -1.0, 1.0),
    arr(c"sizes".as_ptr(), offset_of!(ArrObj, sizes), ffi::AV_OPT_TYPE_IMAGE_SIZE, Some(&SIZES_DEF), 0.0, INT_MAX),
    arr(c"colors".as_ptr(), offset_of!(ArrObj, colors), ffi::AV_OPT_TYPE_COLOR, Some(&COLORS_DEF), 0.0, 0.0),
    arr(c"durs".as_ptr(), offset_of!(ArrObj, durs), ffi::AV_OPT_TYPE_DURATION, Some(&DURS_DEF), 0.0, 1e18),
    arr(c"pixs".as_ptr(), offset_of!(ArrObj, pixs), ffi::AV_OPT_TYPE_PIXEL_FMT, Some(&PIXS_DEF), -1.0, INT_MAX),
    arr(c"samples".as_ptr(), offset_of!(ArrObj, samples), ffi::AV_OPT_TYPE_SAMPLE_FMT, Some(&SAMPLES_DEF), -1.0, INT_MAX),
    arr(c"dicts".as_ptr(), offset_of!(ArrObj, dicts), ffi::AV_OPT_TYPE_DICT, Some(&DICTS_DEF), 0.0, 0.0),
    arr(c"layouts".as_ptr(), offset_of!(ArrObj, layouts), ffi::AV_OPT_TYPE_CHLAYOUT, Some(&LAYOUTS_DEF), 0.0, 0.0),
    arr(c"bins".as_ptr(), offset_of!(ArrObj, bins), ffi::AV_OPT_TYPE_BINARY, None, 0.0, 0.0),
    opt(null(), null(), 0, 0, int(0), 0.0, 0.0, null()),
]);

/// The names of [`ARR_OPTIONS`], in order.
pub(crate) const ARR_NAMES: [&core::ffi::CStr; 18] = [
    c"ints", c"uints", c"i64s", c"u64s", c"dbls", c"flts", c"strs", c"qs", c"rates", c"bools",
    c"sizes", c"colors", c"durs", c"pixs", c"samples", c"dicts", c"layouts", c"bins",
];

/// `child_class_iterate` of [`ARR_CLASS`]: [`OBJ_CLASS`], then
/// [`ARR_CLASS`] itself, then nothing.
///
/// # Safety
///
/// `iter` must address a state this function alone advanced from NULL.
pub(crate) unsafe extern "C" fn arr_child_class_iterate(
    iter: *mut *mut c_void,
) -> *const ffi::AVClass {
    // SAFETY: C passes its writable state slot.
    let state = unsafe { &mut *iter };
    let i = *state as usize;
    *state = (i + 1) as *mut c_void;
    match i {
        0 => &raw const OBJ_CLASS.0,
        1 => &raw const ARR_CLASS.0,
        _ => null(),
    }
}

/// The class of [`ArrObj`], whose potential children are listed by
/// [`arr_child_class_iterate`].
pub(crate) static ARR_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-arrays".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &raw const ARR_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: None,
    child_class_iterate: Some(arr_child_class_iterate),
    state_flags_offset: 0,
});

/// A heap [`ArrObj`] of [`ARR_CLASS`] with its defaults set by C, reached
/// only through its raw pointer; dropping it frees its arrays, then it.
pub(crate) struct ArrOwned(core::ptr::NonNull<ArrObj>);

impl ArrOwned {
    pub(crate) fn new() -> Self {
        // SAFETY: every field of `ArrObj` is a raw pointer or an integer, for
        // which all-zero is valid.
        let obj: Box<ArrObj> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // For the unsafe operations below: `p` is the fresh allocation above; C fills each array
        // from its definition.
        {
            // SAFETY: as the comment above this block states.
            unsafe { (*p).class = &raw const ARR_CLASS.0 };
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_opt_set_defaults(p.cast()) };
        }
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_obj(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The object, for raw field access.
    pub(crate) fn ptr(&self) -> *mut ArrObj {
        self.0.as_ptr()
    }
}

impl Default for ArrOwned {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ArrOwned {
    fn drop(&mut self) {
        let p = self.0.as_ptr();
        // For the unsafe operations below: `p` is a live object this owner allocated, whose arrays
        // `av_opt_free` releases with their elements.
        {
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_opt_free(p.cast()) };
            // SAFETY: as the comment above this block states.
            unsafe { drop(Box::from_raw(p)) };
        }
    }
}

// Options wave 1, batch 1: an object with an array option of each element
// type `av_opt_set_array` stores, for the array editors, with what they need
// beyond batch 0's `ArrObj`: a child of its own class, size bounds, wide
// rational bounds, a read-only array and a scalar. `bins` is an array of
// binary elements, which C cannot store (it writes each element's length
// past its pointer): it stays empty.

/// An array field: the elements and their count, as C lays them out.
#[repr(C)]
pub(crate) struct Arr<T> {
    pub(crate) elems: *mut T,
    pub(crate) len: c_uint,
}

/// The object [`ARRAYS_CLASS`] describes.
#[repr(C)]
pub(crate) struct Arrays {
    pub(crate) class: *const ffi::AVClass,
    pub(crate) ints: Arr<c_int>,
    pub(crate) i64s: Arr<i64>,
    pub(crate) uints: Arr<c_uint>,
    pub(crate) u64s: Arr<u64>,
    pub(crate) dbls: Arr<f64>,
    pub(crate) flts: Arr<f32>,
    pub(crate) qs: Arr<ffi::AVRational>,
    pub(crate) wide_qs: Arr<ffi::AVRational>,
    pub(crate) rates: Arr<ffi::AVRational>,
    pub(crate) flags: Arr<c_uint>,
    pub(crate) bools: Arr<c_int>,
    pub(crate) durations: Arr<i64>,
    pub(crate) sizes: Arr<[c_int; 2]>,
    pub(crate) colors: Arr<[u8; 4]>,
    pub(crate) pix: Arr<c_int>,
    pub(crate) samples: Arr<c_int>,
    pub(crate) layouts: Arr<ffi::AVChannelLayout>,
    pub(crate) dicts: Arr<*mut c_void>,
    pub(crate) strs: Arr<*mut c_char>,
    pub(crate) bins: Arr<*mut u8>,
    pub(crate) ro: Arr<c_int>,
    pub(crate) scalar: c_int,
    pub(crate) child: *mut Arrays,
}

/// `strs`: `;`-separated, one to three elements, two by default.
pub(crate) static EDIT_STRS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"a;b".as_ptr(),
    size_min: 1,
    size_max: 3,
    sep: b';' as c_char,
});

/// `layouts`: `|`-separated (layout names hold `,` and `+`), two by default.
pub(crate) static EDIT_LAYOUTS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"stereo|mono".as_ptr(),
    size_min: 0,
    size_max: 0,
    sep: b'|' as c_char,
});

/// `ints`: two by default, at most six.
pub(crate) static EDIT_INTS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"1,2".as_ptr(),
    size_min: 0,
    size_max: 6,
    sep: 0,
});

/// An array option `name` of element type `ty` at `offset`, with definition
/// `def`, within `[min, max]`.
const fn edit_arr(
    name: &'static core::ffi::CStr,
    offset: usize,
    ty: T,
    def: *const ffi::AVOptionArrayDef,
    min: f64,
    max: f64,
) -> ffi::AVOption {
    opt(
        name.as_ptr(),
        c"an array".as_ptr(),
        offset,
        ty | ffi::AV_OPT_TYPE_FLAG_ARRAY,
        ffi::AVOption__bindgen_ty_1 { arr: def },
        min,
        max,
        null(),
    )
}

/// The options of [`ARRAYS_CLASS`], in `Arrays` order, then a flag constant
/// and the terminator.
#[rustfmt::skip]
pub(crate) static ARRAYS_OPTIONS: Table<25> = Table([
    edit_arr(c"ints", offset_of!(Arrays, ints), ffi::AV_OPT_TYPE_INT, &raw const EDIT_INTS_DEF.0, -10.0, 100.0),
    edit_arr(c"i64s", offset_of!(Arrays, i64s), ffi::AV_OPT_TYPE_INT64, null(), i64::MIN as f64, i64::MAX as f64),
    edit_arr(c"uints", offset_of!(Arrays, uints), ffi::AV_OPT_TYPE_UINT, null(), 0.0, u32::MAX as f64),
    edit_arr(c"u64s", offset_of!(Arrays, u64s), ffi::AV_OPT_TYPE_UINT64, null(), 0.0, u64::MAX as f64),
    edit_arr(c"dbls", offset_of!(Arrays, dbls), ffi::AV_OPT_TYPE_DOUBLE, null(), -1e9, 1e9),
    edit_arr(c"flts", offset_of!(Arrays, flts), ffi::AV_OPT_TYPE_FLOAT, null(), 0.0, 1.0),
    edit_arr(c"qs", offset_of!(Arrays, qs), ffi::AV_OPT_TYPE_RATIONAL, null(), 0.0, INT_MAX),
    edit_arr(c"wide_qs", offset_of!(Arrays, wide_qs), ffi::AV_OPT_TYPE_RATIONAL, null(), -1e300, 1e300),
    edit_arr(c"rates", offset_of!(Arrays, rates), ffi::AV_OPT_TYPE_VIDEO_RATE, null(), 0.0, INT_MAX),
    ffi::AVOption { unit: c"afl".as_ptr(), ..edit_arr(c"flags", offset_of!(Arrays, flags), ffi::AV_OPT_TYPE_FLAGS, null(), 0.0, u32::MAX as f64) },
    edit_arr(c"bools", offset_of!(Arrays, bools), ffi::AV_OPT_TYPE_BOOL, null(), -1.0, 1.0),
    edit_arr(c"durations", offset_of!(Arrays, durations), ffi::AV_OPT_TYPE_DURATION, null(), 0.0, i64::MAX as f64),
    edit_arr(c"sizes", offset_of!(Arrays, sizes), ffi::AV_OPT_TYPE_IMAGE_SIZE, null(), 0.0, INT_MAX),
    edit_arr(c"colors", offset_of!(Arrays, colors), ffi::AV_OPT_TYPE_COLOR, null(), 0.0, 0.0),
    edit_arr(c"pix", offset_of!(Arrays, pix), ffi::AV_OPT_TYPE_PIXEL_FMT, null(), -1.0, INT_MAX),
    edit_arr(c"samples", offset_of!(Arrays, samples), ffi::AV_OPT_TYPE_SAMPLE_FMT, null(), -1.0, INT_MAX),
    edit_arr(c"layouts", offset_of!(Arrays, layouts), ffi::AV_OPT_TYPE_CHLAYOUT, &raw const EDIT_LAYOUTS_DEF.0, 0.0, 0.0),
    edit_arr(c"dicts", offset_of!(Arrays, dicts), ffi::AV_OPT_TYPE_DICT, null(), 0.0, 0.0),
    edit_arr(c"strs", offset_of!(Arrays, strs), ffi::AV_OPT_TYPE_STRING, &raw const EDIT_STRS_DEF.0, 0.0, 0.0),
    edit_arr(c"bins", offset_of!(Arrays, bins), ffi::AV_OPT_TYPE_BINARY, null(), 0.0, 0.0),
    flagged(edit_arr(c"ro", offset_of!(Arrays, ro), ffi::AV_OPT_TYPE_INT, null(), 0.0, 10.0), ffi::AV_OPT_FLAG_READONLY),
    opt(c"scalar".as_ptr(), c"not an array".as_ptr(), offset_of!(Arrays, scalar), ffi::AV_OPT_TYPE_INT, int(1), 0.0, 10.0, null()),
    opt(c"one".as_ptr(), c"flag 1".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"afl".as_ptr()),
    opt(c"four".as_ptr(), c"flag 4".as_ptr(), 0, ffi::AV_OPT_TYPE_CONST, int(4), 0.0, 0.0, c"afl".as_ptr()),
    opt(null(), null(), 0, 0, int(0), 0.0, 0.0, null()),
]);

/// `child_next` of [`ARRAYS_CLASS`]: an object's `child`, once.
///
/// # Safety
///
/// `obj` must address a live [`Arrays`].
pub(crate) unsafe extern "C" fn arrays_child_next(
    obj: *mut c_void,
    prev: *mut c_void,
) -> *mut c_void {
    if !prev.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: C passes the live `Arrays` it found this class in.
    unsafe { (*obj.cast::<Arrays>()).child.cast() }
}

/// The class of [`Arrays`].
pub(crate) static ARRAYS_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-edits".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &raw const ARRAYS_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: Some(arrays_child_next),
    child_class_iterate: None,
    state_flags_offset: 0,
});

/// Every option name of [`ARRAYS_CLASS`] that holds a value, and one that
/// names nothing.
pub(crate) const ARRAYS_NAMES: [&core::ffi::CStr; 23] = [
    c"ints",
    c"i64s",
    c"uints",
    c"u64s",
    c"dbls",
    c"flts",
    c"qs",
    c"wide_qs",
    c"rates",
    c"flags",
    c"bools",
    c"durations",
    c"sizes",
    c"colors",
    c"pix",
    c"samples",
    c"layouts",
    c"dicts",
    c"strs",
    c"bins",
    c"ro",
    c"scalar",
    c"missing",
];

/// A heap [`Arrays`], reached only through its raw pointer, with an optional
/// child; dropping it frees what its options own, then it.
pub(crate) struct OwnedArrays(core::ptr::NonNull<Arrays>);

impl OwnedArrays {
    /// A zeroed object, before `av_opt_set_defaults`.
    pub(crate) fn zeroed() -> Self {
        // SAFETY: every field of `Arrays` is an integer, a raw pointer or a
        // struct of those, for which all-zero is valid.
        let obj: Box<Arrays> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // SAFETY: `p` is the fresh allocation above.
        unsafe { (*p).class = &raw const ARRAYS_CLASS.0 };
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// An object with its defaults set by C.
    pub(crate) fn new() -> Self {
        let o = Self::zeroed();
        // SAFETY: a live zeroed object of `ARRAYS_CLASS`.
        unsafe { ffi::av_opt_set_defaults(o.as_obj()) };
        o
    }

    /// An object with defaults and a child object with defaults.
    pub(crate) fn with_child() -> Self {
        let o = Self::new();
        let child = Self::new();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        let _ = core::mem::ManuallyDrop::new(child);
        o
    }

    /// A zeroed object with a zeroed child, before any default: no option
    /// value is parsed, so C's evaluator never runs (`BorrowSanitizer`
    /// misreports its `av_strtod` end pointers).
    pub(crate) fn zeroed_with_child() -> Self {
        let o = Self::zeroed();
        let child = Self::zeroed();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        let _ = core::mem::ManuallyDrop::new(child);
        o
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub(crate) fn as_obj(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The object, for raw field reads.
    pub(crate) fn ptr(&self) -> *mut Arrays {
        self.0.as_ptr()
    }

    /// The child object, if any.
    pub(crate) fn child(&self) -> *mut Arrays {
        // SAFETY: `self` is live.
        unsafe { (*self.0.as_ptr()).child }
    }
}

impl Drop for OwnedArrays {
    fn drop(&mut self) {
        let p = self.0.as_ptr();
        // For the unsafe operations below: `p` and its child are live objects this owner allocated,
        // whose options own what `av_opt_free` releases.
        {
            // SAFETY: as the comment above this block states.
            let child = unsafe { (*p).child };
            // For the unsafe operations below: as the comment above this block states.
            if !child.is_null() {
                // SAFETY: as the comment above this statement states.
                unsafe { ffi::av_opt_free(child.cast()) };
                // SAFETY: as the comment at the head of this block states.
                drop(unsafe { Box::from_raw(child) });
            }
            // SAFETY: as the comment above this block states.
            unsafe { ffi::av_opt_free(p.cast()) };
            // SAFETY: as the comment above this block states.
            unsafe { drop(Box::from_raw(p)) };
        }
    }
}
