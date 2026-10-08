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
