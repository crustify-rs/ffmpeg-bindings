//! A test `AVClass` with one option of each type `libavutil/opt.h` defaults,
//! shared by the `opt` suites: the crate's unit tests and the `equiv` and
//! `ub_unsafe` integration tests include it with `#[path]`, after bringing the
//! `-sys` crate into scope as `ffi`.
#![allow(dead_code)]

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::mem::{offset_of, size_of};
use core::ptr::null;

use super::ffi;
use ffi::AVOptionType as T;

/// The object [`CLASS`] describes; its fields hold the option values.
#[repr(C)]
pub struct Ctx {
    pub class: *const ffi::AVClass,
    pub int: c_int,
    pub int64: i64,
    pub uint64: u64,
    pub dbl: f64,
    pub flt: f32,
    pub string: *mut c_char,
    pub q: ffi::AVRational,
    pub boolean: c_int,
    pub flags: c_int,
    pub size: [c_int; 2],
    pub rate: ffi::AVRational,
    pub color: [u8; 4],
    pub duration: i64,
    pub binary: *mut u8,
    pub binary_len: c_int,
    pub dict: *mut c_void,
    pub array: *mut c_int,
    pub array_len: c_uint,
    pub empty_array: *mut c_int,
    pub empty_array_len: c_uint,
    pub bare: c_int,
    pub nostr: *mut c_char,
    pub bogus: c_int,
}

impl Ctx {
    /// An object of `class` with every option zeroed, as `av_opt_set_defaults`
    /// expects it.
    pub fn new(class: &'static Class) -> Self {
        // SAFETY: every field is an integer, a float or a raw pointer, for
        // which all-zero is a valid value.
        let mut ctx: Self = unsafe { core::mem::zeroed() };
        ctx.class = &class.0;
        ctx
    }

    /// `self` as the `void *` the `av_opt_*` routines take.
    pub fn as_obj(&mut self) -> *mut c_void {
        (self as *mut Self).cast()
    }
}

/// An option table, `Sync` to live in a `static`.
pub struct Table<const N: usize>(pub [ffi::AVOption; N]);
// SAFETY: the table is immutable and its pointers address immutable statics.
unsafe impl<const N: usize> Sync for Table<N> {}

/// A class, `Sync` to live in a `static`.
pub struct Class(pub ffi::AVClass);
// SAFETY: as for `Table`.
unsafe impl Sync for Class {}

/// A `const AVClass *`, `Sync` to live in a `static`: the fake object
/// `AV_OPT_SEARCH_FAKE_OBJ` queries take.
pub struct FakeObj(pub *const ffi::AVClass);
// SAFETY: as for `Table`.
unsafe impl Sync for FakeObj {}

impl FakeObj {
    /// `self` as the `void *` the `av_opt_*` routines take with
    /// `AV_OPT_SEARCH_FAKE_OBJ`; they only read it.
    pub fn as_obj(&'static self) -> *mut c_void {
        (self as *const Self).cast_mut().cast()
    }
}

/// One option: `name`, `help`, the value at `offset`, of type `ty`, defaulting
/// to `default`, within `[min, max]`.
#[allow(clippy::too_many_arguments)]
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
pub const GROWS: c_int = 1 << 30;

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
pub struct ArrayDef(pub ffi::AVOptionArrayDef);
// SAFETY: as for `Table`.
unsafe impl Sync for ArrayDef {}
pub static ARRAY_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"1,2".as_ptr(),
    size_min: 0,
    size_max: 0,
    sep: 0,
});

const MAX: f64 = i32::MAX as f64;

/// The options of [`CLASS`], in `Ctx` order, then a named constant and the
/// terminator.
#[rustfmt::skip]
pub static OPTIONS: Table<23> = Table([
    opt(c"int".as_ptr(), c"an int".as_ptr(), offset_of!(Ctx, int), T::AV_OPT_TYPE_INT, int(7), -10.0, 100.0, c"mode".as_ptr()),
    opt(c"int64".as_ptr(), c"an int64".as_ptr(), offset_of!(Ctx, int64), T::AV_OPT_TYPE_INT64, int(-5_000_000_000), i64::MIN as f64, i64::MAX as f64, null()),
    opt(c"uint64".as_ptr(), c"a uint64".as_ptr(), offset_of!(Ctx, uint64), T::AV_OPT_TYPE_UINT64, int(42), 0.0, u64::MAX as f64, null()),
    opt(c"dbl".as_ptr(), c"a double".as_ptr(), offset_of!(Ctx, dbl), T::AV_OPT_TYPE_DOUBLE, dbl(2.5), -1e9, 1e9, null()),
    opt(c"flt".as_ptr(), c"a float".as_ptr(), offset_of!(Ctx, flt), T::AV_OPT_TYPE_FLOAT, dbl(0.25), 0.0, 1.0, null()),
    opt(c"string".as_ptr(), c"a string".as_ptr(), offset_of!(Ctx, string), T::AV_OPT_TYPE_STRING, str_(c"hello".as_ptr()), 0.0, 0.0, null()),
    opt(c"q".as_ptr(), c"a rational".as_ptr(), offset_of!(Ctx, q), T::AV_OPT_TYPE_RATIONAL, dbl(0.5), 0.0, MAX, null()),
    opt(c"boolean".as_ptr(), c"a bool".as_ptr(), offset_of!(Ctx, boolean), T::AV_OPT_TYPE_BOOL, int(1), -1.0, 1.0, null()),
    opt(c"flags".as_ptr(), c"some flags".as_ptr(), offset_of!(Ctx, flags), T::AV_OPT_TYPE_FLAGS, int(3), 0.0, MAX, c"fl".as_ptr()),
    opt(c"size".as_ptr(), c"an image size".as_ptr(), offset_of!(Ctx, size), T::AV_OPT_TYPE_IMAGE_SIZE, str_(c"640x480".as_ptr()), 0.0, MAX, null()),
    opt(c"rate".as_ptr(), c"a video rate".as_ptr(), offset_of!(Ctx, rate), T::AV_OPT_TYPE_VIDEO_RATE, str_(c"25".as_ptr()), 0.0, MAX, null()),
    opt(c"color".as_ptr(), c"a color".as_ptr(), offset_of!(Ctx, color), T::AV_OPT_TYPE_COLOR, str_(c"red".as_ptr()), 0.0, 0.0, null()),
    opt(c"duration".as_ptr(), c"a duration".as_ptr(), offset_of!(Ctx, duration), T::AV_OPT_TYPE_DURATION, int(1_500_000), 0.0, i64::MAX as f64, null()),
    opt(c"binary".as_ptr(), c"some bytes".as_ptr(), offset_of!(Ctx, binary), T::AV_OPT_TYPE_BINARY, str_(c"0a0b".as_ptr()), 0.0, 0.0, null()),
    grows(opt(c"dict".as_ptr(), c"a dictionary".as_ptr(), offset_of!(Ctx, dict), T::AV_OPT_TYPE_DICT, str_(c"k=v".as_ptr()), 0.0, 0.0, null())),
    grows(opt(c"array".as_ptr(), c"an int array".as_ptr(), offset_of!(Ctx, array), T(T::AV_OPT_TYPE_INT.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0), ffi::AVOption__bindgen_ty_1 { arr: &ARRAY_DEF.0 }, 0.0, 10.0, null())),
    opt(c"empty_array".as_ptr(), c"an array without a default".as_ptr(), offset_of!(Ctx, empty_array), T(T::AV_OPT_TYPE_INT.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0), ffi::AVOption__bindgen_ty_1 { arr: null() }, 0.0, 10.0, null()),
    opt(c"bare".as_ptr(), null(), offset_of!(Ctx, bare), T::AV_OPT_TYPE_INT, int(0), 0.0, 1.0, null()),
    opt(c"nostr".as_ptr(), c"a string without a default".as_ptr(), offset_of!(Ctx, nostr), T::AV_OPT_TYPE_STRING, str_(null()), 0.0, 0.0, null()),
    opt(c"bogus".as_ptr(), c"a type opt.h does not define".as_ptr(), offset_of!(Ctx, bogus), T(99), int(0), 0.0, 0.0, null()),
    opt(c"seven".as_ptr(), c"a named constant".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(7), 0.0, 0.0, c"mode".as_ptr()),
    opt(c"bit".as_ptr(), c"a flag constant".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"fl".as_ptr()),
    opt(null(), null(), 0, T(0), int(0), 0.0, 0.0, null()),
]);

/// The class of [`Ctx`], with [`OPTIONS`] and no `query_ranges` callback.
pub static CLASS: Class = Class(class(None));

/// `CLASS` as a fake object.
pub static FAKE: FakeObj = FakeObj(&CLASS.0);

/// [`CLASS`] with `query_ranges` set to `callback`.
pub const fn class(
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
        option: &OPTIONS.0[0],
        version: 0,
        log_level_offset_offset: 0,
        parent_log_context_offset: 0,
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
        get_category: None,
        query_ranges: callback,
        child_next: None,
        child_class_iterate: None,
        state_flags_offset: 0,
    }
}

/// The number of option names [`OPTIONS`] holds before its terminator.
pub const NAMED: usize = OPTIONS.0.len() - 1;

/// Keys a range query succeeds for with the default ranges.
pub const RANGED: [&core::ffi::CStr; 14] = [
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
pub const UNRANGED: [&core::ffi::CStr; 6] = [
    c"flags",
    c"binary",
    c"dict",
    c"array",
    c"empty_array",
    c"bogus",
];

/// Keys a range query rejects with `AVERROR(ENOMEM)`: unknown names, and named
/// constants, which a lookup without a unit skips.
pub const UNKNOWN: [&core::ffi::CStr; 4] = [c"missing", c"seven", c"bit", c""];

/// `AVERROR(EINVAL)`, which [`query_ranges`] fails with for `"fail"`.
pub const EINVAL: c_int = -22;

/// A `query_ranges` callback of [`CB_CLASS`], building its lists as a class
/// would, through the `av_malloc` family:
///
/// - `"size"`: 2 ranges of 3 components with `AV_OPT_MULTI_COMPONENT_RANGE`,
///   else of 1, each with an owned `str`;
/// - `"sparse"`: 3 ranges of 1 component, the middle one NULL;
/// - `"fail"`: `AVERROR(EINVAL)`, storing NULL;
/// - any other key: the default ranges.
///
/// # Safety
///
/// As `av_opt_query_ranges`' callback.
pub unsafe extern "C" fn query_ranges(
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
        // SAFETY: `r` is a fresh zeroed range, `array` holds `n` slots, and
        // `av_strdup` reads a literal.
        unsafe {
            (*r).str_ = ffi::av_strdup(c"range".as_ptr());
            (*r).value_min = i as f64;
            (*r).value_max = i as f64 + 10.0;
            (*r).component_min = -(i as f64);
            (*r).component_max = 255.0;
            (*r).is_range = (i % 2) as c_int;
            *array.add(i) = r;
        }
    }
    // SAFETY: `list` is a fresh zeroed list and `out` C's writable slot.
    unsafe {
        (*list).range = array;
        (*list).nb_ranges = nb_ranges;
        (*list).nb_components = nb_components;
        *out = list;
    }
    nb_components
}

/// [`CLASS`] with [`query_ranges`] as its callback.
pub static CB_CLASS: Class = Class(class(Some(query_ranges)));

/// `CB_CLASS` as a fake object.
pub static CB_FAKE: FakeObj = FakeObj(&CB_CLASS.0);

/// Keys [`query_ranges`] builds its own lists for.
pub const CB_KEYS: [&core::ffi::CStr; 3] = [c"size", c"sparse", c"fail"];

// Options wave 0, batch 3: a well-formed object for `ObjectRef` / `ObjectMut`.
// `CLASS` above has an option of a type opt.h does not define, outside their
// contract; `OBJ_CLASS` describes `Obj`, every field of a defined type, with a
// child of its own class reached through `child_next`.

/// The object [`OBJ_CLASS`] describes.
#[repr(C)]
pub struct Obj {
    pub class: *const ffi::AVClass,
    pub int: c_int,
    pub int64: i64,
    pub uint64: u64,
    pub uint: c_uint,
    pub dbl: f64,
    pub flt: f32,
    pub string: *mut c_char,
    pub q: ffi::AVRational,
    pub wide_q: ffi::AVRational,
    pub boolean: c_int,
    pub flags: c_int,
    pub big: i64,
    pub size: [c_int; 2],
    pub rate: ffi::AVRational,
    pub color: [u8; 4],
    pub duration: i64,
    pub binary: *mut u8,
    pub binary_len: c_int,
    pub dict: *mut c_void,
    pub array: *mut c_int,
    pub array_len: c_uint,
    pub strings: *mut *mut c_char,
    pub strings_len: c_uint,
    pub empty: *mut c_int,
    pub empty_len: c_uint,
    pub layout: ffi::AVChannelLayout,
    pub pix: c_int,
    pub sample: c_int,
    pub ro: c_int,
    pub nostr: *mut c_char,
    pub child: *mut Obj,
}

/// The definition of the `strings` option: `|`-separated, two by default.
pub static STRINGS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
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
pub static OBJ_OPTIONS: Table<40> = Table([
    opt(c"int".as_ptr(), c"an int".as_ptr(), offset_of!(Obj, int), T::AV_OPT_TYPE_INT, int(7), -10.0, 100.0, c"mode".as_ptr()),
    opt(c"int64".as_ptr(), c"an int64".as_ptr(), offset_of!(Obj, int64), T::AV_OPT_TYPE_INT64, int(-5_000_000_000), i64::MIN as f64, i64::MAX as f64, null()),
    opt(c"uint64".as_ptr(), c"a uint64".as_ptr(), offset_of!(Obj, uint64), T::AV_OPT_TYPE_UINT64, int(42), 0.0, u64::MAX as f64, null()),
    opt(c"uint".as_ptr(), c"a uint".as_ptr(), offset_of!(Obj, uint), T::AV_OPT_TYPE_UINT, int(9), 0.0, u32::MAX as f64, null()),
    opt(c"dbl".as_ptr(), c"a double".as_ptr(), offset_of!(Obj, dbl), T::AV_OPT_TYPE_DOUBLE, dbl(2.5), f64::MIN, f64::MAX, null()),
    opt(c"flt".as_ptr(), c"a float".as_ptr(), offset_of!(Obj, flt), T::AV_OPT_TYPE_FLOAT, dbl(0.25), 0.0, 1.0, null()),
    opt(c"string".as_ptr(), c"a string".as_ptr(), offset_of!(Obj, string), T::AV_OPT_TYPE_STRING, str_(c"hello".as_ptr()), 0.0, 0.0, null()),
    opt(c"q".as_ptr(), c"a rational".as_ptr(), offset_of!(Obj, q), T::AV_OPT_TYPE_RATIONAL, dbl(0.5), 0.0, INT_MAX, null()),
    opt(c"wide_q".as_ptr(), c"a rational of wide bounds".as_ptr(), offset_of!(Obj, wide_q), T::AV_OPT_TYPE_RATIONAL, dbl(0.25), -1e300, 1e300, null()),
    opt(c"boolean".as_ptr(), c"a bool".as_ptr(), offset_of!(Obj, boolean), T::AV_OPT_TYPE_BOOL, int(1), -1.0, 1.0, null()),
    opt(c"flags".as_ptr(), c"some flags".as_ptr(), offset_of!(Obj, flags), T::AV_OPT_TYPE_FLAGS, int(3), 0.0, u32::MAX as f64, c"fl".as_ptr()),
    opt(c"big".as_ptr(), c"64-bit flags".as_ptr(), offset_of!(Obj, big), T::AV_OPT_TYPE_INT64, int(0), i64::MIN as f64, i64::MAX as f64, c"big".as_ptr()),
    opt(c"size".as_ptr(), c"an image size".as_ptr(), offset_of!(Obj, size), T::AV_OPT_TYPE_IMAGE_SIZE, str_(c"640x480".as_ptr()), 0.0, INT_MAX, null()),
    opt(c"rate".as_ptr(), c"a video rate".as_ptr(), offset_of!(Obj, rate), T::AV_OPT_TYPE_VIDEO_RATE, str_(c"25".as_ptr()), 0.0, INT_MAX, null()),
    opt(c"color".as_ptr(), c"a color".as_ptr(), offset_of!(Obj, color), T::AV_OPT_TYPE_COLOR, str_(c"red".as_ptr()), 0.0, 0.0, null()),
    opt(c"duration".as_ptr(), c"a duration".as_ptr(), offset_of!(Obj, duration), T::AV_OPT_TYPE_DURATION, int(1_500_000), 0.0, i64::MAX as f64, null()),
    opt(c"binary".as_ptr(), c"some bytes".as_ptr(), offset_of!(Obj, binary), T::AV_OPT_TYPE_BINARY, str_(c"0a0b".as_ptr()), 0.0, 0.0, null()),
    opt(c"dict".as_ptr(), c"a dictionary".as_ptr(), offset_of!(Obj, dict), T::AV_OPT_TYPE_DICT, str_(c"k=v".as_ptr()), 0.0, 0.0, null()),
    opt(c"array".as_ptr(), c"an int array".as_ptr(), offset_of!(Obj, array), T(T::AV_OPT_TYPE_INT.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0), ffi::AVOption__bindgen_ty_1 { arr: &ARRAY_DEF.0 }, 0.0, 10.0, null()),
    opt(c"strings".as_ptr(), c"a string array".as_ptr(), offset_of!(Obj, strings), T(T::AV_OPT_TYPE_STRING.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0), ffi::AVOption__bindgen_ty_1 { arr: &STRINGS_DEF.0 }, 0.0, 0.0, null()),
    opt(c"empty".as_ptr(), c"an int array without a definition".as_ptr(), offset_of!(Obj, empty), T(T::AV_OPT_TYPE_INT.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0), ffi::AVOption__bindgen_ty_1 { arr: null() }, 0.0, 10.0, null()),
    opt(c"layout".as_ptr(), c"a channel layout".as_ptr(), offset_of!(Obj, layout), T::AV_OPT_TYPE_CHLAYOUT, str_(c"stereo".as_ptr()), 0.0, 0.0, null()),
    opt(c"pix".as_ptr(), c"a pixel format".as_ptr(), offset_of!(Obj, pix), T::AV_OPT_TYPE_PIXEL_FMT, int(0), -1.0, INT_MAX, null()),
    opt(c"sample".as_ptr(), c"a sample format".as_ptr(), offset_of!(Obj, sample), T::AV_OPT_TYPE_SAMPLE_FMT, int(1), -1.0, INT_MAX, null()),
    flagged(opt(c"ro".as_ptr(), c"read-only".as_ptr(), offset_of!(Obj, ro), T::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, null()), ffi::AV_OPT_FLAG_READONLY | ffi::AV_OPT_FLAG_EXPORT),
    flagged(opt(c"nostr".as_ptr(), c"a string without a default".as_ptr(), offset_of!(Obj, nostr), T::AV_OPT_TYPE_STRING, str_(null()), 0.0, 0.0, null()), ffi::AV_OPT_FLAG_ENCODING_PARAM),
    flagged(opt(c"cflags".as_ptr(), c"flags with child constants".as_ptr(), offset_of!(Obj, flags), T::AV_OPT_TYPE_FLAGS, int(3), 0.0, u32::MAX as f64, c"fl".as_ptr()), ffi::AV_OPT_FLAG_CHILD_CONSTS),
    flagged(opt(c"old".as_ptr(), c"use ro".as_ptr(), offset_of!(Obj, ro), T::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, null()), ffi::AV_OPT_FLAG_DEPRECATED),
    opt(c"seven".as_ptr(), c"a named constant".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(7), 0.0, 0.0, c"mode".as_ptr()),
    opt(c"one".as_ptr(), c"flag 1".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"bit".as_ptr(), c"flag 2".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"eight".as_ptr(), c"flag 8".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(8), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"huge".as_ptr(), c"a flag beyond int64_t as a double".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(i64::MAX), 0.0, 0.0, c"fl".as_ptr()),
    opt(c"low".as_ptr(), c"bit 0".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"big".as_ptr()),
    opt(c"high".as_ptr(), c"bit 40".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(1 << 40), 0.0, 0.0, c"big".as_ptr()),
    opt(c"mixed".as_ptr(), c"bits 0 and 40".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(1 | 1 << 40), 0.0, 0.0, c"big".as_ptr()),
    opt(c"notconst".as_ptr(), c"an int in unit big".as_ptr(), offset_of!(Obj, ro), T::AV_OPT_TYPE_INT, int(4), 0.0, 10.0, c"big".as_ptr()),
    opt(c"dbl_unit".as_ptr(), c"a double with a unit".as_ptr(), offset_of!(Obj, dbl), T::AV_OPT_TYPE_DOUBLE, dbl(2.5), f64::MIN, f64::MAX, c"dunit".as_ptr()),
    opt(c"dflag".as_ptr(), c"a constant of unit dunit".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(2), 0.0, 0.0, c"dunit".as_ptr()),
    opt(null(), null(), 0, T(0), int(0), 0.0, 0.0, null()),
]);

/// `child_next` of [`OBJ_CLASS`]: an object's `child`, once.
///
/// # Safety
///
/// `obj` must address a live [`Obj`].
pub unsafe extern "C" fn obj_child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
    if !prev.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: C passes the live `Obj` it found this class in.
    unsafe { (*obj.cast::<Obj>()).child.cast() }
}

/// The class of [`Obj`].
pub static OBJ_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-obj".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &OBJ_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: Some(obj_child_next),
    child_class_iterate: None,
    state_flags_offset: 0,
});

/// A heap [`Obj`] of [`OBJ_CLASS`], reached only through its raw pointer
/// (so that C's writes are never invalidated by a Rust reborrow), with an
/// optional child; dropping it frees what its options own, then it.
pub struct Owned(core::ptr::NonNull<Obj>);

impl Owned {
    /// A zeroed object, before `av_opt_set_defaults`.
    pub fn zeroed() -> Self {
        // SAFETY: every field of `Obj` is an integer, a float, a raw pointer
        // or a plain C struct of those, for which all-zero is valid.
        let obj: Box<Obj> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // SAFETY: `p` is the fresh allocation above.
        unsafe { (*p).class = &OBJ_CLASS.0 };
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// An object with its defaults set by C.
    pub fn new() -> Self {
        let o = Self::zeroed();
        // SAFETY: a live zeroed object of `OBJ_CLASS`.
        unsafe { ffi::av_opt_set_defaults(o.as_obj()) };
        o
    }

    /// An object with defaults and a child object with defaults.
    pub fn with_child() -> Self {
        let o = Self::new();
        let child = Self::new();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        core::mem::forget(child);
        o
    }

    /// A zeroed object with a zeroed child, before any default.
    pub fn zeroed_with_child() -> Self {
        let o = Self::zeroed();
        let child = Self::zeroed();
        // SAFETY: `o` is live; the child's allocation passes to it.
        unsafe { (*o.0.as_ptr()).child = child.0.as_ptr() };
        core::mem::forget(child);
        o
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub fn as_obj(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The object, for raw field reads.
    pub fn ptr(&self) -> *mut Obj {
        self.0.as_ptr()
    }

    /// The child object, if any.
    pub fn child(&self) -> *mut Obj {
        // SAFETY: `self` is live.
        unsafe { (*self.0.as_ptr()).child }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        let p = self.0.as_ptr();
        // SAFETY: `p` and its child are live objects this owner allocated,
        // whose options own what `av_opt_free` releases.
        unsafe {
            let child = (*p).child;
            if !child.is_null() {
                ffi::av_opt_free(child.cast());
                drop(Box::from_raw(child));
            }
            ffi::av_opt_free(p.cast());
            drop(Box::from_raw(p));
        }
    }
}
