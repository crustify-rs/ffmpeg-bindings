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

// Options wave 1, batch 0: an array of each option type `av_opt_get_array`
// reads, for `ObjectRef::get_array`, besides `Obj`'s `int` and `string`
// arrays. The binary array has no default: C parses a binary element into
// a pointer and a length, which an array element has no room for.

/// The object [`ARR_CLASS`] describes: each array is a pointer followed by
/// its element count.
#[repr(C)]
pub struct ArrObj {
    pub class: *const ffi::AVClass,
    pub ints: *mut c_int,
    pub ints_len: c_uint,
    pub uints: *mut c_uint,
    pub uints_len: c_uint,
    pub i64s: *mut i64,
    pub i64s_len: c_uint,
    pub u64s: *mut u64,
    pub u64s_len: c_uint,
    pub dbls: *mut f64,
    pub dbls_len: c_uint,
    pub flts: *mut f32,
    pub flts_len: c_uint,
    pub strs: *mut *mut c_char,
    pub strs_len: c_uint,
    pub qs: *mut ffi::AVRational,
    pub qs_len: c_uint,
    pub rates: *mut ffi::AVRational,
    pub rates_len: c_uint,
    pub bools: *mut c_int,
    pub bools_len: c_uint,
    pub sizes: *mut [c_int; 2],
    pub sizes_len: c_uint,
    pub colors: *mut [u8; 4],
    pub colors_len: c_uint,
    pub durs: *mut i64,
    pub durs_len: c_uint,
    pub pixs: *mut c_int,
    pub pixs_len: c_uint,
    pub samples: *mut c_int,
    pub samples_len: c_uint,
    pub dicts: *mut *mut c_void,
    pub dicts_len: c_uint,
    pub layouts: *mut ffi::AVChannelLayout,
    pub layouts_len: c_uint,
    pub bins: *mut *mut u8,
    pub bins_len: c_uint,
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

pub static INTS_DEF: ArrayDef = arr_def(c"1,-2,2147483647,-2147483648".as_ptr(), 0);
pub static UINTS_DEF: ArrayDef = arr_def(c"0,4294967295,9".as_ptr(), 0);
pub static I64S_DEF: ArrayDef = arr_def(c"-5000000000,123456789012".as_ptr(), 0);
pub static U64S_DEF: ArrayDef = arr_def(c"42,7".as_ptr(), 0);
pub static DBLS_DEF: ArrayDef = arr_def(c"1.5,-2,1e300,0.1".as_ptr(), 0);
pub static FLTS_DEF: ArrayDef = arr_def(c"0.25,-3e38,1".as_ptr(), 0);
pub static STRS_DEF: ArrayDef = arr_def(c"x,,y\\,z".as_ptr(), 0);
pub static QS_DEF: ArrayDef = arr_def(c"1/3,5,-7/2".as_ptr(), 0);
pub static RATES_DEF: ArrayDef = arr_def(c"25,30000/1001".as_ptr(), 0);
pub static BOOLS_DEF: ArrayDef = arr_def(c"true,false,auto".as_ptr(), 0);
pub static SIZES_DEF: ArrayDef = arr_def(c"640x480,hd720".as_ptr(), 0);
pub static COLORS_DEF: ArrayDef = arr_def(c"red,0x00ff0080".as_ptr(), 0);
pub static DURS_DEF: ArrayDef = arr_def(c"1.5,00:01:00".as_ptr(), 0);
pub static PIXS_DEF: ArrayDef = arr_def(c"yuv420p,rgb24,none".as_ptr(), 0);
pub static SAMPLES_DEF: ArrayDef = arr_def(c"s16,fltp".as_ptr(), 0);
pub static DICTS_DEF: ArrayDef = arr_def(c"a=1:b=2:A=3,k=v".as_ptr(), 0);
pub static LAYOUTS_DEF: ArrayDef = arr_def(c"stereo|5.1(side)|FL+FR".as_ptr(), b'|');

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
        Some(d) => &d.0 as *const ffi::AVOptionArrayDef,
        None => null(),
    };
    opt(
        name,
        c"an array".as_ptr(),
        offset,
        T(ty.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0),
        ffi::AVOption__bindgen_ty_1 { arr: def },
        min,
        max,
        null(),
    )
}

/// The options of [`ARR_CLASS`], in `ArrObj` order, then the terminator.
#[rustfmt::skip]
pub static ARR_OPTIONS: Table<19> = Table([
    arr(c"ints".as_ptr(), offset_of!(ArrObj, ints), T::AV_OPT_TYPE_INT, Some(&INTS_DEF), i32::MIN as f64, INT_MAX),
    arr(c"uints".as_ptr(), offset_of!(ArrObj, uints), T::AV_OPT_TYPE_UINT, Some(&UINTS_DEF), 0.0, u32::MAX as f64),
    arr(c"i64s".as_ptr(), offset_of!(ArrObj, i64s), T::AV_OPT_TYPE_INT64, Some(&I64S_DEF), -1e18, 1e18),
    arr(c"u64s".as_ptr(), offset_of!(ArrObj, u64s), T::AV_OPT_TYPE_UINT64, Some(&U64S_DEF), 0.0, 1e18),
    arr(c"dbls".as_ptr(), offset_of!(ArrObj, dbls), T::AV_OPT_TYPE_DOUBLE, Some(&DBLS_DEF), f64::MIN, f64::MAX),
    arr(c"flts".as_ptr(), offset_of!(ArrObj, flts), T::AV_OPT_TYPE_FLOAT, Some(&FLTS_DEF), -f32::MAX as f64, f32::MAX as f64),
    arr(c"strs".as_ptr(), offset_of!(ArrObj, strs), T::AV_OPT_TYPE_STRING, Some(&STRS_DEF), 0.0, 0.0),
    arr(c"qs".as_ptr(), offset_of!(ArrObj, qs), T::AV_OPT_TYPE_RATIONAL, Some(&QS_DEF), -1e9, 1e9),
    arr(c"rates".as_ptr(), offset_of!(ArrObj, rates), T::AV_OPT_TYPE_VIDEO_RATE, Some(&RATES_DEF), 0.0, INT_MAX),
    arr(c"bools".as_ptr(), offset_of!(ArrObj, bools), T::AV_OPT_TYPE_BOOL, Some(&BOOLS_DEF), -1.0, 1.0),
    arr(c"sizes".as_ptr(), offset_of!(ArrObj, sizes), T::AV_OPT_TYPE_IMAGE_SIZE, Some(&SIZES_DEF), 0.0, INT_MAX),
    arr(c"colors".as_ptr(), offset_of!(ArrObj, colors), T::AV_OPT_TYPE_COLOR, Some(&COLORS_DEF), 0.0, 0.0),
    arr(c"durs".as_ptr(), offset_of!(ArrObj, durs), T::AV_OPT_TYPE_DURATION, Some(&DURS_DEF), 0.0, 1e18),
    arr(c"pixs".as_ptr(), offset_of!(ArrObj, pixs), T::AV_OPT_TYPE_PIXEL_FMT, Some(&PIXS_DEF), -1.0, INT_MAX),
    arr(c"samples".as_ptr(), offset_of!(ArrObj, samples), T::AV_OPT_TYPE_SAMPLE_FMT, Some(&SAMPLES_DEF), -1.0, INT_MAX),
    arr(c"dicts".as_ptr(), offset_of!(ArrObj, dicts), T::AV_OPT_TYPE_DICT, Some(&DICTS_DEF), 0.0, 0.0),
    arr(c"layouts".as_ptr(), offset_of!(ArrObj, layouts), T::AV_OPT_TYPE_CHLAYOUT, Some(&LAYOUTS_DEF), 0.0, 0.0),
    arr(c"bins".as_ptr(), offset_of!(ArrObj, bins), T::AV_OPT_TYPE_BINARY, None, 0.0, 0.0),
    opt(null(), null(), 0, T(0), int(0), 0.0, 0.0, null()),
]);

/// The names of [`ARR_OPTIONS`], in order.
pub const ARR_NAMES: [&core::ffi::CStr; 18] = [
    c"ints", c"uints", c"i64s", c"u64s", c"dbls", c"flts", c"strs", c"qs", c"rates", c"bools",
    c"sizes", c"colors", c"durs", c"pixs", c"samples", c"dicts", c"layouts", c"bins",
];

/// `child_class_iterate` of [`ARR_CLASS`]: [`OBJ_CLASS`], then
/// [`ARR_CLASS`] itself, then nothing.
///
/// # Safety
///
/// `iter` must address a state this function alone advanced from NULL.
pub unsafe extern "C" fn arr_child_class_iterate(iter: *mut *mut c_void) -> *const ffi::AVClass {
    // SAFETY: C passes its writable state slot.
    let state = unsafe { &mut *iter };
    let i = *state as usize;
    *state = (i + 1) as *mut c_void;
    match i {
        0 => &OBJ_CLASS.0,
        1 => &ARR_CLASS.0,
        _ => null(),
    }
}

/// The class of [`ArrObj`], whose potential children are listed by
/// [`arr_child_class_iterate`].
pub static ARR_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-arrays".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &ARR_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: None,
    child_class_iterate: Some(arr_child_class_iterate),
    state_flags_offset: 0,
});

/// A heap [`ArrObj`] of [`ARR_CLASS`] with its defaults set by C, reached
/// only through its raw pointer; dropping it frees its arrays, then it.
pub struct ArrOwned(core::ptr::NonNull<ArrObj>);

impl ArrOwned {
    pub fn new() -> Self {
        // SAFETY: every field of `ArrObj` is a raw pointer or an integer, for
        // which all-zero is valid.
        let obj: Box<ArrObj> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // SAFETY: `p` is the fresh allocation above; C fills each array from
        // its definition.
        unsafe {
            (*p).class = &ARR_CLASS.0;
            ffi::av_opt_set_defaults(p.cast());
        }
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// The object as the `void *` the `av_opt_*` routines take.
    pub fn as_obj(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// The object, for raw field access.
    pub fn ptr(&self) -> *mut ArrObj {
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
        // SAFETY: `p` is a live object this owner allocated, whose arrays
        // `av_opt_free` releases with their elements.
        unsafe {
            ffi::av_opt_free(p.cast());
            drop(Box::from_raw(p));
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
pub struct Arr<T> {
    pub elems: *mut T,
    pub len: c_uint,
}

/// The object [`ARRAYS_CLASS`] describes.
#[repr(C)]
pub struct Arrays {
    pub class: *const ffi::AVClass,
    pub ints: Arr<c_int>,
    pub i64s: Arr<i64>,
    pub uints: Arr<c_uint>,
    pub u64s: Arr<u64>,
    pub dbls: Arr<f64>,
    pub flts: Arr<f32>,
    pub qs: Arr<ffi::AVRational>,
    pub wide_qs: Arr<ffi::AVRational>,
    pub rates: Arr<ffi::AVRational>,
    pub flags: Arr<c_uint>,
    pub bools: Arr<c_int>,
    pub durations: Arr<i64>,
    pub sizes: Arr<[c_int; 2]>,
    pub colors: Arr<[u8; 4]>,
    pub pix: Arr<c_int>,
    pub samples: Arr<c_int>,
    pub layouts: Arr<ffi::AVChannelLayout>,
    pub dicts: Arr<*mut c_void>,
    pub strs: Arr<*mut c_char>,
    pub bins: Arr<*mut u8>,
    pub ro: Arr<c_int>,
    pub scalar: c_int,
    pub child: *mut Arrays,
}

/// `strs`: `;`-separated, one to three elements, two by default.
pub static EDIT_STRS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"a;b".as_ptr(),
    size_min: 1,
    size_max: 3,
    sep: b';' as c_char,
});

/// `layouts`: `|`-separated (layout names hold `,` and `+`), two by default.
pub static EDIT_LAYOUTS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
    def: c"stereo|mono".as_ptr(),
    size_min: 0,
    size_max: 0,
    sep: b'|' as c_char,
});

/// `ints`: two by default, at most six.
pub static EDIT_INTS_DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
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
        T(ty.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0),
        ffi::AVOption__bindgen_ty_1 { arr: def },
        min,
        max,
        null(),
    )
}

/// The options of [`ARRAYS_CLASS`], in `Arrays` order, then a flag constant
/// and the terminator.
#[rustfmt::skip]
pub static ARRAYS_OPTIONS: Table<25> = Table([
    edit_arr(c"ints", offset_of!(Arrays, ints), T::AV_OPT_TYPE_INT, &EDIT_INTS_DEF.0, -10.0, 100.0),
    edit_arr(c"i64s", offset_of!(Arrays, i64s), T::AV_OPT_TYPE_INT64, null(), i64::MIN as f64, i64::MAX as f64),
    edit_arr(c"uints", offset_of!(Arrays, uints), T::AV_OPT_TYPE_UINT, null(), 0.0, u32::MAX as f64),
    edit_arr(c"u64s", offset_of!(Arrays, u64s), T::AV_OPT_TYPE_UINT64, null(), 0.0, u64::MAX as f64),
    edit_arr(c"dbls", offset_of!(Arrays, dbls), T::AV_OPT_TYPE_DOUBLE, null(), -1e9, 1e9),
    edit_arr(c"flts", offset_of!(Arrays, flts), T::AV_OPT_TYPE_FLOAT, null(), 0.0, 1.0),
    edit_arr(c"qs", offset_of!(Arrays, qs), T::AV_OPT_TYPE_RATIONAL, null(), 0.0, INT_MAX),
    edit_arr(c"wide_qs", offset_of!(Arrays, wide_qs), T::AV_OPT_TYPE_RATIONAL, null(), -1e300, 1e300),
    edit_arr(c"rates", offset_of!(Arrays, rates), T::AV_OPT_TYPE_VIDEO_RATE, null(), 0.0, INT_MAX),
    ffi::AVOption { unit: c"afl".as_ptr(), ..edit_arr(c"flags", offset_of!(Arrays, flags), T::AV_OPT_TYPE_FLAGS, null(), 0.0, u32::MAX as f64) },
    edit_arr(c"bools", offset_of!(Arrays, bools), T::AV_OPT_TYPE_BOOL, null(), -1.0, 1.0),
    edit_arr(c"durations", offset_of!(Arrays, durations), T::AV_OPT_TYPE_DURATION, null(), 0.0, i64::MAX as f64),
    edit_arr(c"sizes", offset_of!(Arrays, sizes), T::AV_OPT_TYPE_IMAGE_SIZE, null(), 0.0, INT_MAX),
    edit_arr(c"colors", offset_of!(Arrays, colors), T::AV_OPT_TYPE_COLOR, null(), 0.0, 0.0),
    edit_arr(c"pix", offset_of!(Arrays, pix), T::AV_OPT_TYPE_PIXEL_FMT, null(), -1.0, INT_MAX),
    edit_arr(c"samples", offset_of!(Arrays, samples), T::AV_OPT_TYPE_SAMPLE_FMT, null(), -1.0, INT_MAX),
    edit_arr(c"layouts", offset_of!(Arrays, layouts), T::AV_OPT_TYPE_CHLAYOUT, &EDIT_LAYOUTS_DEF.0, 0.0, 0.0),
    edit_arr(c"dicts", offset_of!(Arrays, dicts), T::AV_OPT_TYPE_DICT, null(), 0.0, 0.0),
    edit_arr(c"strs", offset_of!(Arrays, strs), T::AV_OPT_TYPE_STRING, &EDIT_STRS_DEF.0, 0.0, 0.0),
    edit_arr(c"bins", offset_of!(Arrays, bins), T::AV_OPT_TYPE_BINARY, null(), 0.0, 0.0),
    flagged(edit_arr(c"ro", offset_of!(Arrays, ro), T::AV_OPT_TYPE_INT, null(), 0.0, 10.0), ffi::AV_OPT_FLAG_READONLY),
    opt(c"scalar".as_ptr(), c"not an array".as_ptr(), offset_of!(Arrays, scalar), T::AV_OPT_TYPE_INT, int(1), 0.0, 10.0, null()),
    opt(c"one".as_ptr(), c"flag 1".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(1), 0.0, 0.0, c"afl".as_ptr()),
    opt(c"four".as_ptr(), c"flag 4".as_ptr(), 0, T::AV_OPT_TYPE_CONST, int(4), 0.0, 0.0, c"afl".as_ptr()),
    opt(null(), null(), 0, T(0), int(0), 0.0, 0.0, null()),
]);

/// `child_next` of [`ARRAYS_CLASS`]: an object's `child`, once.
///
/// # Safety
///
/// `obj` must address a live [`Arrays`].
pub unsafe extern "C" fn arrays_child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
    if !prev.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: C passes the live `Arrays` it found this class in.
    unsafe { (*obj.cast::<Arrays>()).child.cast() }
}

/// The class of [`Arrays`].
pub static ARRAYS_CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-opt-edits".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: &ARRAYS_OPTIONS.0[0],
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: Some(arrays_child_next),
    child_class_iterate: None,
    state_flags_offset: 0,
});

/// Every option name of [`ARRAYS_CLASS`] that holds a value, and one that
/// names nothing.
pub const ARRAYS_NAMES: [&core::ffi::CStr; 23] = [
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
pub struct OwnedArrays(core::ptr::NonNull<Arrays>);

impl OwnedArrays {
    /// A zeroed object, before `av_opt_set_defaults`.
    pub fn zeroed() -> Self {
        // SAFETY: every field of `Arrays` is an integer, a raw pointer or a
        // struct of those, for which all-zero is valid.
        let obj: Box<Arrays> = Box::new(unsafe { core::mem::zeroed() });
        let p = Box::into_raw(obj);
        // SAFETY: `p` is the fresh allocation above.
        unsafe { (*p).class = &ARRAYS_CLASS.0 };
        Self(core::ptr::NonNull::new(p).unwrap())
    }

    /// An object with its defaults set by C.
    pub fn new() -> Self {
        let o = Self::zeroed();
        // SAFETY: a live zeroed object of `ARRAYS_CLASS`.
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

    /// A zeroed object with a zeroed child, before any default: no option
    /// value is parsed, so C's evaluator never runs (BorrowSanitizer
    /// misreports its `av_strtod` end pointers).
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
    pub fn ptr(&self) -> *mut Arrays {
        self.0.as_ptr()
    }

    /// The child object, if any.
    pub fn child(&self) -> *mut Arrays {
        // SAFETY: `self` is live.
        unsafe { (*self.0.as_ptr()).child }
    }
}

impl Drop for OwnedArrays {
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
