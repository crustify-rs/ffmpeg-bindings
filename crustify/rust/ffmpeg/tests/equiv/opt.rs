//! Equivalence of `libavutil/opt.h`'s owned range lists and their ranges
//! with the raw C queries and C's writes of the same fields, on
//! independently owned lists of the shared test class; and of option types
//! with the values C stores and reads.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;
use std::ffi::CString;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{OptionRangeRef, OptionRanges, OptionRangesBox, OptionRangesRef};

#[path = "../common/opt.rs"]
pub(crate) mod fixture;
use fixture::{CB_FAKE, CB_KEYS, CLASS, Ctx, FAKE, RANGED, UNKNOWN, UNRANGED};

const FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
const MULTI: c_int = ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int;
const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;

/// Every key of the test class, including those a query rejects.
fn all_keys() -> impl Iterator<Item = &'static CStr> {
    RANGED
        .into_iter()
        .chain(UNRANGED)
        .chain(UNKNOWN)
        .chain(CB_KEYS)
}

/// What C can observe of one range.
#[derive(Debug, PartialEq)]
struct Range {
    str_: Option<CString>,
    value: (f64, f64),
    component: (f64, f64),
    is_range: c_int,
}

/// What C can observe of a list: its counts and every range, in order.
#[derive(Debug, PartialEq)]
struct List {
    nb_ranges: usize,
    nb_components: usize,
    ranges: Vec<Option<Range>>,
}

/// Reads the range at `p` through its raw fields, NULL included.
fn range(p: *const ffi::AVOptionRange) -> Option<Range> {
    if p.is_null() {
        return None;
    }
    // SAFETY: per each caller, `p` is an element of a live list, which owns
    // the range and its NUL-terminated (or NULL) `str`.
    unsafe {
        let r = p.read();
        Some(Range {
            str_: (!r.str_.is_null()).then(|| CStr::from_ptr(r.str_).to_owned()),
            value: (r.value_min, r.value_max),
            component: (r.component_min, r.component_max),
            is_range: r.is_range,
        })
    }
}

/// The C list at `p`, read through its raw fields.
fn c_list(p: *const ffi::AVOptionRanges) -> List {
    // SAFETY: `p` is a live list C returned, owned by the caller.
    let l = unsafe { p.read() };
    let n = (l.nb_ranges * l.nb_components) as usize;
    List {
        nb_ranges: l.nb_ranges as usize,
        nb_components: l.nb_components as usize,
        // SAFETY: a list's `range` array holds `nb_ranges * nb_components`
        // elements.
        ranges: (0..n).map(|i| range(unsafe { *l.range.add(i) })).collect(),
    }
}

/// The range behind `e`, read through the safe getters. `is_range` reads as
/// a `bool`, so a list C built with another non-zero flag compares unequal.
fn rust_range(e: OptionRangeRef<'_>) -> Range {
    Range {
        str_: e.label().map(CStr::to_owned),
        value: (e.value_min(), e.value_max()),
        component: (e.component_min(), e.component_max()),
        is_range: c_int::from(e.is_range()),
    }
}

/// The Rust list behind `r`, read through the safe getters.
fn rust_list(r: OptionRangesRef<'_>) -> List {
    let list = List {
        nb_ranges: r.range_count(),
        nb_components: r.component_count(),
        ranges: r.ranges().map(|e| e.map(rust_range)).collect(),
    };
    // Indexing reads the same ranges as iterating.
    for c in 0..list.nb_components {
        for i in 0..list.nb_ranges {
            let e = r.get(i, c).map(rust_range);
            assert_eq!(e.as_ref(), list.ranges[c * list.nb_ranges + i].as_ref());
        }
    }
    list
}

/// A C-owned list, freed with `av_opt_freep_ranges` on drop.
struct CList(*mut ffi::AVOptionRanges);

impl Drop for CList {
    fn drop(&mut self) {
        // SAFETY: `self.0` is NULL or a list C returned, owned here.
        unsafe { ffi::av_opt_freep_ranges(&mut self.0) };
        assert!(self.0.is_null(), "av_opt_freep_ranges NULLs the slot");
    }
}

#[derive(Clone, Copy, Debug)]
enum Via {
    Callback,
    Default,
}

/// Runs the C query on `obj`.
fn c_query(via: Via, obj: *mut c_void, key: &CStr, flags: c_int) -> (c_int, CList) {
    let mut out = CList(core::ptr::null_mut());
    // SAFETY: `out.0` is a writable slot; each caller passes an object of a
    // well-formed fixture class, matching `flags`.
    let ret = unsafe {
        match via {
            Via::Callback => ffi::av_opt_query_ranges(&mut out.0, obj, key.as_ptr(), flags),
            Via::Default => ffi::av_opt_query_ranges_default(&mut out.0, obj, key.as_ptr(), flags),
        }
    };
    (ret, out)
}

/// Runs the Rust query on `obj`.
fn rust_query(
    via: Via,
    obj: *mut c_void,
    key: &CStr,
    flags: c_int,
) -> Result<OptionRangesBox, i32> {
    // SAFETY: as for `c_query`.
    unsafe {
        match via {
            Via::Callback => OptionRanges::query(obj, key, flags),
            Via::Default => OptionRanges::query_default(obj, key, flags),
        }
    }
}

/// Runs both queries on `obj` and asserts they agree; returns both lists.
#[track_caller]
fn both(via: Via, obj: *mut c_void, key: &CStr, flags: c_int) -> (CList, Option<OptionRangesBox>) {
    let (ret, c) = c_query(via, obj, key, flags);
    let rust = rust_query(via, obj, key, flags);
    let ctx = format!("{via:?} {key:?} flags={flags:#x}");
    match &rust {
        Err(e) => {
            assert!(
                ret < 0,
                "{ctx}: C succeeded with {ret}, Rust failed with {e}"
            );
            assert_eq!(*e, ret, "{ctx}");
            assert!(c.0.is_null(), "{ctx}: C failure stored a list");
        }
        Ok(b) => {
            assert!(ret >= 0, "{ctx}: Rust succeeded, C failed with {ret}");
            // C returns the component count it stored.
            assert_eq!(b.as_ref().component_count(), ret as usize, "{ctx}");
            assert_eq!(rust_list(b.as_ref()), c_list(c.0), "{ctx}");
        }
    }
    (c, rust.ok())
}

#[test]
fn every_key_and_flag_agrees_on_a_fake_object() {
    for via in [Via::Callback, Via::Default] {
        for key in all_keys() {
            for flags in [FAKE_OBJ, FAKE_OBJ | MULTI, FAKE_OBJ | CHILDREN] {
                both(via, FAKE.as_obj(), key, flags);
            }
        }
    }
}

#[test]
fn every_key_agrees_on_a_real_object() {
    let mut ctx = Ctx::new(&CLASS);
    for via in [Via::Callback, Via::Default] {
        for key in all_keys() {
            for flags in [0, MULTI, CHILDREN] {
                both(via, ctx.as_obj(), key, flags);
            }
        }
    }
}

#[test]
fn class_callback_lists_agree() {
    for key in all_keys() {
        for flags in [FAKE_OBJ, FAKE_OBJ | MULTI] {
            let (c, rust) = both(Via::Callback, CB_FAKE.as_obj(), key, flags);
            // The default query bypasses the callback on the same class.
            both(Via::Default, CB_FAKE.as_obj(), key, flags);
            drop((c, rust));
        }
    }
    // The multi-component list really has three components of two ranges.
    let (_, rust) = both(Via::Callback, CB_FAKE.as_obj(), c"size", FAKE_OBJ | MULTI);
    let b = rust.unwrap();
    assert_eq!(
        (b.as_ref().range_count(), b.as_ref().component_count()),
        (2, 3)
    );
}

#[test]
fn null_class_agrees_with_the_default_query() {
    let mut ctx = Ctx::new(&CLASS);
    ctx.class = core::ptr::null();
    for key in [c"int", c"missing"] {
        both(Via::Default, ctx.as_obj(), key, 0);
    }
    let fake = fixture::FakeObj(core::ptr::null());
    let obj = (&raw const fake).cast_mut().cast();
    both(Via::Default, obj, c"int", FAKE_OBJ);
}

/// The default query rejects a NULL object before reading it, with or
/// without the fake-object flag.
#[test]
fn null_object_agrees_with_the_default_query() {
    for key in [c"int", c"missing"] {
        for flags in [0, FAKE_OBJ, MULTI, CHILDREN | FAKE_OBJ] {
            both(Via::Default, core::ptr::null_mut(), key, flags);
        }
    }
}

/// Set in the child process an isolated test re-runs itself in.
const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

/// The default query allocates the list, its one-slot array and its range
/// before looking the option up, and fails with `AVERROR(ENOMEM)` when any of
/// them does; both sides agree at every `av_max_alloc` limit around their
/// sizes. The limit is process-global, so this lowers it only in a child
/// process running nothing else, and passes when the child does.
#[test]
fn allocation_failure_at_the_av_max_alloc_limit_agrees() {
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "opt::allocation_failure_at_the_av_max_alloc_limit_agrees",
            ])
            .args(["--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        return;
    }

    let list = size_of::<ffi::AVOptionRanges>();
    let slot = size_of::<*mut ffi::AVOptionRange>();
    let range = size_of::<ffi::AVOptionRange>();
    let largest = list.max(slot).max(range);
    let mut ctx = Ctx::new(&CLASS);
    let obj = ctx.as_obj();
    // A list built before the limit drops stays valid and releasable.
    let (c_before, rust_before) = both(Via::Default, obj, c"int", 0);

    for limit in [
        0,
        list - 1,
        list,
        slot,
        range - 1,
        largest - 1,
        largest,
        largest + 1,
    ] {
        // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in
        // this process.
        unsafe { ffi::av_max_alloc(limit) };
        for via in [Via::Callback, Via::Default] {
            for key in [c"int", c"size", c"missing", c"flags"] {
                let (c, rust) = both(via, obj, key, MULTI);
                let expect_ok = limit >= largest && RANGED.contains(&key);
                assert_eq!(rust.is_some(), expect_ok, "{via:?} {key:?} limit={limit}");
                assert_eq!(c.0.is_null(), !expect_ok, "{via:?} {key:?} limit={limit}");
            }
        }
    }
    // SAFETY: as above; restores the default limit.
    unsafe { ffi::av_max_alloc(i32::MAX as usize) };
    both(Via::Default, obj, c"int", 0);
    drop((c_before, rust_before));
}

/// Lists stay independent: each later query, and each drop, leaves every live
/// list of either side as it was.
#[test]
fn query_sequences_keep_lists_independent() {
    let mut ctx = Ctx::new(&CLASS);
    let obj = ctx.as_obj();
    let mut live: Vec<(List, CList, OptionRangesBox)> = Vec::new();
    let check = |live: &[(List, CList, OptionRangesBox)]| {
        for (expected, c, rust) in live {
            assert_eq!(&c_list(c.0), expected);
            assert_eq!(&rust_list(rust.as_ref()), expected);
        }
    };
    for (step, key) in RANGED.iter().cycle().take(40).enumerate() {
        let via = if step % 3 == 0 {
            Via::Default
        } else {
            Via::Callback
        };
        let flags = if step % 2 == 0 { MULTI } else { 0 };
        let (c, rust) = both(via, obj, key, flags);
        let rust = rust.unwrap();
        live.push((c_list(c.0), c, rust));
        check(&live);
        // Drop from the front, the back and the middle in turn, each side
        // apart from the other.
        if step % 4 == 3 {
            let (_, c, rust) = live.remove(step % live.len());
            drop(rust);
            check(&live);
            drop(c);
            check(&live);
        }
    }
    // Then the callback's owned strings, mixed in.
    let cb = CB_FAKE.as_obj();
    for key in [c"size", c"sparse", c"size"] {
        let (c, rust) = both(Via::Callback, cb, key, FAKE_OBJ | MULTI);
        live.push((c_list(c.0), c, rust.unwrap()));
        check(&live);
    }
    while let Some((_, c, rust)) = live.pop() {
        drop(c);
        check(&live);
        drop(rust);
    }
}

/// One write to a range, applied by Rust through `OptionRangeMut` and by C
/// as C code writes the fields.
#[derive(Clone, Copy, Debug)]
enum Write {
    Label(Option<&'static CStr>),
    Take,
    ValueMin(f64),
    ValueMax(f64),
    ComponentMin(f64),
    ComponentMax(f64),
    IsRange(bool),
}

/// Applies `w` to range `(i, comp)` of the C list `c` as C code would: a
/// string is replaced by `av_freep` then `av_strdup`, and taken by moving
/// it out and NULLing the field. Returns the taken string, if any.
fn c_write(c: &CList, i: usize, comp: usize, w: Write) -> Option<Option<CString>> {
    // SAFETY: `c.0` is a live list C returned, owned by `c`.
    let l = unsafe { c.0.read() };
    let slot = comp * l.nb_ranges as usize + i;
    // SAFETY: `slot` is within the list's `nb_ranges * nb_components`
    // elements, per each caller.
    let r = unsafe { *l.range.add(slot) };
    assert!(!r.is_null());
    // SAFETY: `r` is a live range the list owns; its `str` is NULL or an
    // `av_malloc`-family string, freed and replaced as `av_opt_freep_ranges`
    // expects.
    unsafe {
        match w {
            Write::Label(s) => {
                let mut old = (*r).str_.cast_mut().cast::<c_void>();
                ffi::av_freep((&raw mut old).cast());
                (*r).str_ = s.map_or(core::ptr::null(), |s| {
                    ffi::av_strdup(s.as_ptr()).cast_const()
                });
            }
            Write::Take => {
                let old = core::mem::replace(&mut (*r).str_, core::ptr::null());
                let taken = (!old.is_null()).then(|| CStr::from_ptr(old).to_owned());
                ffi::av_free(old.cast_mut().cast());
                return Some(taken);
            }
            Write::ValueMin(v) => (*r).value_min = v,
            Write::ValueMax(v) => (*r).value_max = v,
            Write::ComponentMin(v) => (*r).component_min = v,
            Write::ComponentMax(v) => (*r).component_max = v,
            Write::IsRange(b) => (*r).is_range = c_int::from(b),
        }
    }
    None
}

/// Applies `w` to range `(i, comp)` of the Rust list `b` through the safe
/// setters. Returns the taken string, if any.
fn rust_write(b: &mut OptionRangesBox, i: usize, comp: usize, w: Write) -> Option<Option<CString>> {
    let mut m = b.as_mut();
    let mut e = m.get_mut(i, comp).unwrap();
    match w {
        Write::Label(s) => {
            e.set_label(s.map(|s| ffmpeg::libavutil::mem::string_from_c_str(s).unwrap()))
        }
        Write::Take => return Some(e.take_label().map(|s| s.as_c_str().to_owned())),
        Write::ValueMin(v) => e.set_value_min(v),
        Write::ValueMax(v) => e.set_value_max(v),
        Write::ComponentMin(v) => e.set_component_min(v),
        Write::ComponentMax(v) => e.set_component_max(v),
        Write::IsRange(v) => e.set_is_range(v),
    }
    None
}

/// The writes of [`writes_agree_after_every_step`], cycled over the
/// elements.
const WRITES: [Write; 14] = [
    Write::Label(Some(c"a")),
    Write::ValueMin(-3.5),
    Write::Label(Some(c"replaced")),
    Write::IsRange(false),
    Write::ComponentMax(f64::MAX),
    Write::Take,
    Write::Take,
    Write::Label(None),
    Write::Label(Some(c"")),
    Write::ValueMax(f64::INFINITY),
    Write::ComponentMin(-0.0),
    Write::IsRange(true),
    Write::Label(None),
    Write::Label(Some(c"last")),
];

/// The same writes to a C list and a Rust list of the same query leave both
/// equal after every step, including the strings each `take` hands back;
/// the drops then release each side's strings once (LeakSanitizer, ASan).
#[test]
fn writes_agree_after_every_step() {
    let cases: [(&fixture::FakeObj, &CStr, c_int); 4] = [
        (&CB_FAKE, c"size", FAKE_OBJ | MULTI),
        (&CB_FAKE, c"sparse", FAKE_OBJ),
        (&FAKE, c"string", FAKE_OBJ),
        (&FAKE, c"size", FAKE_OBJ | MULTI),
    ];
    for (obj, key, flags) in cases {
        let (mut c, rust) = both(Via::Callback, obj.as_obj(), key, flags);
        let mut rust = rust.unwrap();
        let shape = c_list(c.0);
        let live: Vec<(usize, usize)> = (0..shape.nb_components)
            .flat_map(|comp| (0..shape.nb_ranges).map(move |i| (i, comp)))
            .filter(|&(i, comp)| shape.ranges[comp * shape.nb_ranges + i].is_some())
            .collect();
        for (step, w) in WRITES.iter().cycle().take(3 * WRITES.len()).enumerate() {
            let (i, comp) = live[step % live.len()];
            let ctx = format!("{key:?} step {step} {w:?} at ({i}, {comp})");
            let taken = (c_write(&c, i, comp, *w), rust_write(&mut rust, i, comp, *w));
            assert_eq!(taken.0, taken.1, "{ctx}");
            assert_eq!(rust_list(rust.as_ref()), c_list(c.0), "{ctx}");
        }
        // NULL elements have no exclusive handle and stay NULL.
        for (k, e) in shape.ranges.iter().enumerate() {
            if e.is_none() {
                let (i, comp) = (k % shape.nb_ranges, k / shape.nb_ranges);
                assert!(rust.as_mut().get_mut(i, comp).is_none());
            }
        }
        drop(rust);
        drop(core::mem::replace(&mut c, CList(core::ptr::null_mut())));
    }
}

/// Option types as C stores and reads them: through every class's option
/// table, and through `av_opt_get_array_size`, which accepts an option
/// exactly when its type carries the array flag.
mod option_type {
    use core::ffi::{CStr, c_int, c_uint, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::ClassRef;
    use ffmpeg::libavutil::opt::OptionType;

    use super::fixture::{CLASS, Ctx, FAKE, OPTIONS};
    use crate::log::LogState;
    use crate::log::class_fixture::with_class;

    /// `AVERROR(EINVAL)`.
    const EINVAL: c_int = -22;

    /// The type C stores for each option `av_opt_next` walks for `obj`.
    fn c_types(obj: *mut c_void) -> Vec<(*const core::ffi::c_char, ffi::AVOptionType)> {
        let mut out = Vec::new();
        let mut prev = ptr::null();
        loop {
            // SAFETY: `obj` is a `const AVClass *` slot to a well-formed
            // class; `prev` is NULL or an entry of its table.
            prev = unsafe { ffi::av_opt_next(obj, prev) };
            if prev.is_null() {
                return out;
            }
            // SAFETY: a live table entry, read by value.
            let o = unsafe { prev.read() };
            out.push((o.name, o.type_));
        }
    }

    /// Asserts each option of `c` has the type, base and array flag C
    /// stores, in `av_opt_next` order; returns the types read.
    fn compare(c: ClassRef<'_>, obj: *mut c_void) -> Vec<OptionType> {
        let rust: Vec<_> = c
            .options()
            .iter()
            .map(|o| (o.name().map_or(ptr::null(), CStr::as_ptr), o.option_type()))
            .collect();
        let c_side = c_types(obj);
        assert_eq!(rust.len(), c_side.len());
        for ((name, ty), (c_name, raw)) in rust.iter().zip(&c_side) {
            assert_eq!(name, c_name);
            assert_eq!(ty.value(), raw.0);
            assert_eq!(ffi::AVOptionType::from(*ty), *raw);
            // libavutil's `TYPE_BASE` and its array test.
            let flag = ffi::AVOptionType::AV_OPT_TYPE_FLAG_ARRAY.0;
            assert_eq!(ty.base().value(), raw.0 & !flag);
            assert_eq!(ty.is_array(), raw.0 & flag != 0);
        }
        rust.into_iter().map(|(_, ty)| ty).collect()
    }

    /// A `const AVClass *` slot: the fake object C's option readers take.
    struct Fake(*const ffi::AVClass);

    impl Fake {
        fn obj(&mut self) -> *mut c_void {
            (&raw mut self.0).cast()
        }
    }

    #[test]
    fn fixture_types_read_as_c_stores_them() {
        let _s = LogState::lock();
        let mut fake = Fake(FAKE.0);
        let obj = fake.obj();
        // SAFETY: `obj` is a context whose class is the static fixture class.
        let types = unsafe { with_class(obj, |c| compare(c.unwrap(), obj)) };
        assert_eq!(types.len(), OPTIONS.0.len() - 1);
        assert!(types.contains(&OptionType::INT.array()));
        assert!(types.contains(&OptionType::from_value(99)));
    }

    #[test]
    fn iamf_types_read_as_c_stores_them() {
        let _s = LogState::lock();
        let getters: [unsafe extern "C" fn() -> *const ffi::AVClass; 3] = [
            ffi::av_iamf_audio_element_get_class,
            ffi::av_iamf_mix_presentation_get_class,
            ffi::av_iamf_param_definition_get_class,
        ];
        let mut seen = Vec::new();
        for get in getters {
            // SAFETY: each returns a pointer to a static class.
            let mut fake = Fake(unsafe { get() });
            let obj = fake.obj();
            // SAFETY: `obj` is a context whose class is a static FFmpeg
            // class with no offsets.
            seen.extend(unsafe { with_class(obj, |c| compare(c.unwrap(), obj)) });
        }
        // FFmpeg's own tables name only known base types.
        assert!(!seen.is_empty());
        assert!(seen.iter().all(|t| t.name().is_some() && !t.is_array()));
    }

    /// `av_opt_get_array_size` succeeds for exactly the options whose type
    /// `is_array`, and rejects the others with `AVERROR(EINVAL)`.
    #[test]
    fn array_flag_agrees_with_av_opt_get_array_size() {
        let _s = LogState::lock();
        let mut ctx = Ctx::new(&CLASS);
        let obj = ctx.as_obj();
        // SAFETY: `obj` is a zeroed object of the well-formed `CLASS`; its
        // array options stay NULL with a zero count.
        let types: Vec<(CString, OptionType)> = unsafe {
            with_class(obj, |c| {
                let options = c.unwrap().options();
                let named = options
                    .iter()
                    .map(|o| (o.name().unwrap().to_owned(), o.option_type()));
                named.collect()
            })
        };
        let mut arrays = 0;
        for (name, ty) in types {
            if ty == OptionType::CONST {
                continue;
            }
            let mut n: c_uint = 7;
            // SAFETY: `obj` is the live object, `name` a live string and `n`
            // a writable count.
            let ret = unsafe { ffi::av_opt_get_array_size(obj, name.as_ptr(), 0, &mut n) };
            if ty.is_array() {
                assert_eq!((ret, n), (0, 0), "{name:?} {ty:?}");
                arrays += 1;
            } else {
                assert_eq!((ret, n), (EINVAL, 7), "{name:?} {ty:?}");
            }
        }
        assert_eq!(arrays, 2);
    }
}

/// `av_set_options_string` against `ObjectMut::set_options_string`: C and
/// Rust set options of two independently owned `OBJ_CLASS` objects (each
/// with a child, which C searches first) from the same lists, and every
/// option of both objects and children reads the same after each call.
/// (The options wave-0 batch-1 review replaced the raw free function the
/// earlier version of this module compared, on the test class whose
/// undefined option type `ObjectMut`'s contract excludes.)
mod set_options_string {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::{BareObject, ObjectMut};

    use super::fixture::{OBJ_OPTIONS, Owned};

    const ALLOW_NULL: c_int = ffi::AV_OPT_ALLOW_NULL as c_int;

    /// `AVERROR_OPTION_NOT_FOUND`: `FFERRTAG(0xF8, 'O', 'P', 'T')`.
    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// What C reads of `o` and its child: every option but the named
    /// constants, formatted by `av_opt_get`; an image size or color that a
    /// failed parse may have left uninitialised (`mask`: C copies its parse
    /// buffer into the field either way) reads as `None`.
    fn observe(o: &Owned, mask: bool) -> Vec<(i32, Option<Vec<u8>>)> {
        let names = OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            .filter(|o| o.type_ != ffi::AVOptionType::AV_OPT_TYPE_CONST)
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) });
        let mut seen = Vec::new();
        for obj in [o.as_obj(), o.child().cast()] {
            for name in names.clone() {
                if mask && (name == c"size" || name == c"color") {
                    seen.push((0, None));
                    continue;
                }
                let mut out = ptr::null_mut();
                // SAFETY: a live, well-formed object, a live name and a
                // writable slot.
                let ret = unsafe { ffi::av_opt_get(obj, name.as_ptr(), ALLOW_NULL, &mut out) };
                let s = (!out.is_null()).then(|| {
                    // SAFETY: a returned string is an owned C string, freed
                    // here.
                    let s = unsafe { CStr::from_ptr(out.cast::<c_char>()) }
                        .to_bytes()
                        .to_vec();
                    // SAFETY: as above.
                    unsafe { ffi::av_free(out.cast()) };
                    s
                });
                seen.push((ret, s));
            }
        }
        seen
    }

    /// After a failing list, the image size and color C may have filled
    /// from its uninitialised parse buffers (in the object or the child),
    /// reset on both sides so that later steps compare.
    fn reset_clobbered(objects: [&Owned; 2]) {
        for o in objects {
            for p in [o.ptr(), o.child()] {
                if !p.is_null() {
                    // SAFETY: raw writes of a live object's plain fields,
                    // while no handle is live.
                    unsafe {
                        (*p).size = [0; 2];
                        (*p).color = [0; 4];
                    }
                }
            }
        }
    }

    fn c_set(obj: *mut c_void, opts: &CStr, kv: &CStr, pairs: &CStr) -> Result<usize, i32> {
        // SAFETY: a live, well-formed object (or NULL-free bare one), live
        // strings, and lists whose values C sets without undefined
        // behaviour.
        let ret =
            unsafe { ffi::av_set_options_string(obj, opts.as_ptr(), kv.as_ptr(), pairs.as_ptr()) };
        if ret < 0 { Err(ret) } else { Ok(ret as usize) }
    }

    fn rust_set(o: &Owned, opts: &CStr, kv: &CStr, pairs: &CStr) -> Result<usize, i32> {
        // SAFETY: a live, well-formed object, accessed through this handle
        // only while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap();
        m.set_options_string(opts, kv, pairs)
    }

    /// Each step: the list and its separators.
    const STEPS: &[(&CStr, &CStr, &CStr)] = &[
        (c"", c"=", c":"),
        (c"int=5", c"=", c":"),
        (c"int=6:int64=-3:uint64=7:dbl=1.5:flt=0.5", c"=", c":"),
        (
            c"string=abc:q=1/3:boolean=0:flags=bit:size=hd720",
            c"=",
            c":",
        ),
        (
            c"rate=30000/1001:color=blue@0.5:duration=2.5:binary=0102",
            c"=",
            c":",
        ),
        (c"dict=a\\=1\\:b\\=2:array=1,2,3", c"=", c":"),
        (c"string=replaced:binary=:dict=c=3:array=4", c"=", c":"),
        (c"empty=7,8:uint=3:nostr=x", c"=", c":"),
        (c" int = 9 ; string = ' spaced : quoted ' ", c"=", c";"),
        (c"int:10,string:\\'esc\\'", c":", c","),
        (c"int=11 dbl@2.25", c"=@", c" "),
        (c"int=12,,dbl=3", c"=", c","),
        (c"int=13:", c"=", c":"),
        (c"int=14", c"=", c""),
        (c"int=15", c"", c":"),
        (c"=15", c"=", c":"),
        (c"int", c"=", c":"),
        (c"int=16:missing=1:int=17", c"=", c":"),
        (c"int=1000", c"=", c":"),
        (c"int=17:boolean=maybe", c"=", c":"),
        (c"ro=1", c"=", c":"),
        (c"seven=1", c"=", c":"),
        (c"int=seven", c"=", c":"),
        (c"flags=+bit-bit", c"=", c":"),
        (c"cflags=+one+eight", c"=", c":"),
        (c"big=high+low:wide_q=1e9", c"=", c":"),
        (c"color=red@0x80:size=320x240:rate=ntsc", c"=", c":"),
        (c"color=nocolor", c"=", c":"),
        (
            c"strings=a|b\\\\|c:layout=5.1:pix=yuv420p:sample=fltp",
            c"=",
            c":",
        ),
        (c"duration=-1:01:02.5", c"=", c":"),
        (c"string=\\\\back\\\\slash", c"=", c":"),
    ];

    #[test]
    fn sequences_agree_after_every_call() {
        if under_bsan() {
            return;
        }
        let (c, r) = (Owned::with_child(), Owned::with_child());
        for &(opts, kv, pairs) in STEPS {
            let expected = c_set(c.as_obj(), opts, kv, pairs);
            let got = rust_set(&r, opts, kv, pairs);
            assert_eq!(got, expected, "{opts:?}");
            let failed = expected.is_err();
            assert_eq!(observe(&r, failed), observe(&c, failed), "{opts:?}");
            if failed {
                reset_clobbered([&c, &r]);
            }
        }
        // Then the defaults, set over the values, and the lists once more.
        for o in [&c, &r] {
            // SAFETY: a live, well-formed object.
            unsafe { ffi::av_opt_set_defaults(o.as_obj()) };
        }
        assert_eq!(observe(&r, false), observe(&c, false));
        for &(opts, kv, pairs) in STEPS.iter().rev() {
            let expected = c_set(c.as_obj(), opts, kv, pairs);
            let got = rust_set(&r, opts, kv, pairs);
            assert_eq!(got, expected, "{opts:?}");
            let failed = expected.is_err();
            assert_eq!(observe(&r, failed), observe(&c, failed), "{opts:?}");
            if failed {
                reset_clobbered([&c, &r]);
            }
        }
    }

    /// C's image-size and color setters copy their uninitialised result into
    /// the field when the value does not parse, so only the error and the
    /// other options compare after one.
    #[test]
    fn failed_image_sizes_agree_but_for_the_size() {
        if under_bsan() {
            return;
        }
        let (c, r) = (Owned::new(), Owned::new());
        for opts in [
            c"int=5:size=0x0",
            c"size=640x480:size=x",
            c"size=-1x1",
            c"size=hd720",
        ] {
            let expected = c_set(c.as_obj(), opts, c"=", c":");
            let got = rust_set(&r, opts, c"=", c":");
            assert_eq!(got, expected, "{opts:?}");
            let failed = expected.is_err();
            assert_eq!(observe(&r, failed), observe(&c, failed), "{opts:?}");
        }
        // SAFETY: raw reads of the live objects, while no handle is live.
        let sizes = unsafe { ((*r.ptr()).size, (*c.ptr()).size) };
        assert_eq!(sizes, ([1280, 720], [1280, 720]));
    }

    #[test]
    fn errors_agree() {
        if under_bsan() {
            return;
        }
        let cases: &[(&CStr, &CStr, &CStr, Result<usize, i32>)] = &[
            (c"int=5", c"=", c":", Ok(1)),
            (c"int=5:int=6", c"=", c":", Ok(2)),
            (c"", c"=", c":", Ok(0)),
            (c"missing=1", c"=", c":", Err(NOT_FOUND)),
            (c"int", c"=", c":", Err(-22)),
            (c"=5", c"=", c":", Err(-22)),
            (c"int=5", c"", c":", Err(-22)),
            (c"int=1000", c"=", c":", Err(-34)),
            (c"ro=1", c"=", c":", Err(-22)),
        ];
        for &(opts, kv, pairs, expected) in cases {
            let (c, r) = (Owned::with_child(), Owned::with_child());
            assert_eq!(c_set(c.as_obj(), opts, kv, pairs), expected, "{opts:?}");
            assert_eq!(rust_set(&r, opts, kv, pairs), expected, "{opts:?}");
            assert_eq!(observe(&r, false), observe(&c, false), "{opts:?}");
        }
    }

    /// `av_get_token` allocates each key as long as the rest of the list,
    /// and C fails with `AVERROR(ENOMEM)` when it cannot; both sides agree at
    /// every `av_max_alloc` limit around that size. The limit is
    /// process-global, so this lowers it only in a child process running
    /// nothing else, and passes when the child does.
    #[test]
    fn allocation_failure_at_the_av_max_alloc_limit_agrees() {
        if under_bsan() {
            return;
        }
        if std::env::var_os(super::ISOLATED).is_none() {
            let name =
                "opt::set_options_string::allocation_failure_at_the_av_max_alloc_limit_agrees";
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--test-threads=1", "--nocapture"])
                .env(super::ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }
        let opts = c"int=5:string=abc";
        let len = opts.count_bytes();
        for limit in [0, 1, 4, len - 1, len, len + 1, 64, 4096] {
            let (c, r) = (Owned::new(), Owned::new());
            // SAFETY: `av_max_alloc` takes a size by value; nothing else
            // runs in this process.
            unsafe { ffi::av_max_alloc(limit) };
            let expected = c_set(c.as_obj(), opts, c"=", c":");
            let got = rust_set(&r, opts, c"=", c":");
            // SAFETY: as above; restores the default limit.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            assert_eq!(got, expected, "limit={limit}");
            if limit <= len {
                // The first key's allocation fails; later ones (the value,
                // `av_opt_set`'s expression parser) may fail at larger
                // limits too.
                assert_eq!(expected, Err(-12), "limit={limit}");
            }
            assert_eq!(observe(&r, false), observe(&c, false), "limit={limit}");
            if limit == 4096 {
                assert_eq!(got, Ok(2));
            }
        }
    }

    /// On an object without options every key is not found, and an empty
    /// list sets nothing.
    #[test]
    fn bare_objects_agree() {
        static CLASS: Class = Class::new(c"crustify-opt-bare-list", ClassCategory::NA);
        let mut bare = BareObject::new(CLASS.as_ref()).unwrap();
        let mut c_bare = CLASS.as_c_ptr();
        for opts in [c"", c"int=5", c"a=b:c=d", c"int"] {
            let expected = c_set((&raw mut c_bare).cast(), opts, c"=", c":");
            let got = bare.as_object_mut().set_options_string(opts, c"=", c":");
            assert_eq!(got, expected, "{opts:?}");
        }
    }
}

/// Equivalence of `OptionArrayDef`, built and edited through the safe API,
/// with an independently owned raw `AVOptionArrayDef` given the same values:
/// both are read by C's `av_opt_set_defaults` as an array option's
/// definition, and by the handle's getters against the raw fields, after
/// every step of one edit sequence.
///
/// The option holds strings, which C splits and unescapes without its
/// expression evaluator: number elements would reach `av_strtod`, whose
/// glibc-written end pointer BSan misreads (see `eval.rs`'s tests).
mod array_def {
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::mem::offset_of;
    use core::num::NonZeroU32;
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{OptionArrayDef, OptionArrayDefMut};

    use ffi::AVOptionType as T;

    /// An object of the one-option class `defaults` builds.
    #[repr(C)]
    struct Obj {
        class: *const ffi::AVClass,
        elems: *mut *mut c_char,
        len: c_uint,
    }

    /// The current `LIBAVUTIL_VERSION_INT`.
    const VERSION: c_int = (ffi::LIBAVUTIL_VERSION_MAJOR << 16
        | ffi::LIBAVUTIL_VERSION_MINOR << 8
        | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

    /// The elements `av_opt_set_defaults` gives a string array option defined
    /// by `def`, on a fresh object, then freed by `av_opt_free`.
    ///
    /// # Safety
    ///
    /// `def` must be NULL or address a live definition, unwritten during the
    /// call, whose `def` is NULL or a live NUL-terminated string and whose
    /// separator `av_opt_set_defaults` accepts.
    unsafe fn defaults(def: *const ffi::AVOptionArrayDef) -> Vec<CString> {
        let table = [
            ffi::AVOption {
                name: c"a".as_ptr(),
                help: ptr::null(),
                offset: offset_of!(Obj, elems) as c_int,
                type_: T(T::AV_OPT_TYPE_STRING.0 | T::AV_OPT_TYPE_FLAG_ARRAY.0),
                default_val: ffi::AVOption__bindgen_ty_1 { arr: def },
                min: 0.0,
                max: 0.0,
                flags: 0,
                unit: ptr::null(),
            },
            ffi::AVOption::default(),
        ];
        let class = ffi::AVClass {
            class_name: c"equiv-array-def".as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            option: table.as_ptr(),
            version: VERSION,
            ..Default::default()
        };
        let mut obj = Obj {
            class: &class,
            elems: ptr::null_mut(),
            len: 0,
        };
        let o = (&raw mut obj).cast::<c_void>();
        // SAFETY: `obj` is led by a valid class whose terminated table and
        // definition outlive the calls; `av_opt_set_defaults2` writes the
        // array of `len` strings, and `av_opt_free` releases them.
        unsafe {
            ffi::av_opt_set_defaults2(o, 0, 0);
            let elems = if obj.elems.is_null() {
                Vec::new()
            } else {
                core::slice::from_raw_parts(obj.elems, obj.len as usize)
                    .iter()
                    .map(|&s| CStr::from_ptr(s).to_owned())
                    .collect()
            };
            ffi::av_opt_free(o);
            assert!(obj.elems.is_null() && obj.len == 0);
            elems
        }
    }

    /// One edit, applied to both definitions.
    #[derive(Clone, Copy, Debug)]
    enum Edit {
        Def(Option<&'static CStr>),
        Min(u32),
        Max(u32),
        Sep(u8),
    }

    fn apply(m: &mut OptionArrayDefMut<'_>, raw: &mut ffi::AVOptionArrayDef, edit: Edit) {
        match edit {
            Edit::Def(def) => {
                m.set_def(def);
                raw.def = def.map_or(ptr::null(), CStr::as_ptr);
            }
            Edit::Min(n) => {
                m.set_size_min(n);
                raw.size_min = n;
            }
            Edit::Max(n) => {
                m.set_size_max(NonZeroU32::new(n));
                raw.size_max = n;
            }
            Edit::Sep(b) => {
                m.set_separator(b).unwrap();
                raw.sep = b as c_char;
            }
        }
    }

    #[test]
    fn edits_read_alike_by_c_and_by_the_getters() {
        use Edit::*;
        let steps: [(Edit, &[&str]); 25] = [
            (Def(None), &[]),
            (Def(Some(c"a,b,c")), &["a", "b", "c"]),
            (Sep(b','), &["a", "b", "c"]),
            (Max(3), &["a", "b", "c"]),
            // Too many elements: the array stays unset.
            (Max(2), &[]),
            (Max(0), &["a", "b", "c"]),
            (Sep(b'|'), &["a,b,c"]),
            (Def(Some(c"d|e")), &["d", "e"]),
            (Min(2), &["d", "e"]),
            // Too few.
            (Min(3), &[]),
            (Def(Some(c"d|e|f")), &["d", "e", "f"]),
            (Def(None), &[]),
            (Def(Some(c"")), &[]),
            (Min(0), &[]),
            (Sep(b' '), &[]),
            (Def(Some(c"7 8 9 10")), &["7", "8", "9", "10"]),
            (Sep(b';'), &["7 8 9 10"]),
            // An escaped separator or backslash stays in its element.
            (Def(Some(cr"1\;2;3\\;4")), &["1;2", r"3\", "4"]),
            // Empty elements, but none after a trailing separator.
            (Def(Some(c";;x;")), &["", "", "x"]),
            (Sep(0), &[";;x;"]),
            (Def(Some(c"11,12")), &["11", "12"]),
            (Def(Some(cr"a\,b,\q")), &["a,b", "q"]),
            (Max(u32::MAX), &["a,b", "q"]),
            (Min(u32::MAX), &[]),
            (Min(1), &["a,b", "q"]),
        ];
        let mut d = OptionArrayDef::new();
        let mut raw = ffi::AVOptionArrayDef {
            def: ptr::null(),
            size_min: 0,
            size_max: 0,
            sep: 0,
        };
        for (i, (edit, expected)) in steps.into_iter().enumerate() {
            apply(&mut d.as_mut(), &mut raw, edit);

            let r = d.as_ref();
            // SAFETY: `raw.def` is NULL or a literal.
            let raw_def = (!raw.def.is_null()).then(|| unsafe { CStr::from_ptr(raw.def) });
            assert_eq!(r.def(), raw_def, "step {i}");
            assert_eq!(r.size_min(), raw.size_min, "step {i}");
            assert_eq!(
                r.size_max().map_or(0, NonZeroU32::get),
                raw.size_max,
                "step {i}"
            );
            let sep = if raw.sep == 0 { b',' } else { raw.sep as u8 };
            assert_eq!(r.separator(), sep, "step {i}");

            // The pointer is taken after the edit, from the shared borrow C
            // reads through; `OptionArrayDef` is transparent over the C type.
            let rust_ptr = ptr::from_ref(&d).cast::<ffi::AVOptionArrayDef>();
            // SAFETY: both definitions are live and unwritten during the
            // calls, their strings are literals, and every separator is one
            // `set_separator` accepted.
            let (c, rust) = unsafe { (defaults(&raw const raw), defaults(rust_ptr)) };
            assert_eq!(c, rust, "step {i}: {edit:?}");
            let expected: Vec<CString> =
                expected.iter().map(|e| CString::new(*e).unwrap()).collect();
            assert_eq!(rust, expected, "step {i}: {edit:?}");
        }
    }

    #[test]
    fn a_missing_definition_reads_as_the_new_one() {
        let d = OptionArrayDef::new();
        // SAFETY: NULL, and a live definition with no default.
        let (none, new) = unsafe { (defaults(ptr::null()), defaults(ptr::from_ref(&d).cast())) };
        assert_eq!(none, new);
        assert!(new.is_empty());
    }
}

/// Options wave 0, batch 3: the object API against the raw `av_opt_*` calls,
/// on two independently owned objects of the fixture's `OBJ_CLASS` (each
/// with a child), one driven through C and one through `ObjectMut`, compared
/// after every step of fixed and random sequences. Values the wrappers refuse
/// (C's undefined behaviour) are not in the corpus; their refusal is checked
/// in `ub_unsafe` and the unit tests.
mod objects {
    use core::cell::RefCell;
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{self, Callback, Record};
    use ffmpeg::libavutil::opt::{
        ObjectMut, ObjectRef, SearchFlags, SerializeFlags, parse_key_value,
    };

    use super::fixture::{OBJ_OPTIONS, Owned};
    use crate::log::LogState;

    const ALLOW_NULL: c_int = ffi::AV_OPT_ALLOW_NULL as c_int;
    const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
    const ERANGE: i32 = -34;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// Every option name of `OBJ_CLASS`, constants included, and one that
    /// names nothing.
    fn names() -> Vec<&'static CStr> {
        let mut v: Vec<&'static CStr> = OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) })
            .collect();
        v.push(c"missing");
        v
    }

    fn flags_of(children: bool) -> (c_int, SearchFlags) {
        if children {
            (CHILDREN, SearchFlags::CHILDREN)
        } else {
            (0, SearchFlags::NONE)
        }
    }

    /// `av_opt_get(obj, name, flags)`: its code and string.
    fn c_get(obj: *mut c_void, name: &CStr, flags: c_int) -> (i32, Option<Vec<u8>>) {
        let mut out = ptr::null_mut();
        // SAFETY: a live well-formed object, a live name, a writable slot;
        // callers pass `AV_OPT_ALLOW_NULL` or keep binary lengths consistent.
        let ret = unsafe { ffi::av_opt_get(obj, name.as_ptr(), flags, &mut out) };
        let s = (!out.is_null()).then(|| {
            // SAFETY: a returned string is an owned C string, freed here.
            let s = unsafe { CStr::from_ptr(out.cast::<c_char>()) }
                .to_bytes()
                .to_vec();
            // SAFETY: as above.
            unsafe { ffi::av_free(out.cast()) };
            s
        });
        (ret, s)
    }

    /// What C reads of an object and its child: every option formatted.
    fn observe(o: &Owned) -> Vec<(i32, Option<Vec<u8>>)> {
        let mut objs = vec![o.as_obj()];
        if !o.child().is_null() {
            objs.push(o.child().cast());
        }
        objs.into_iter()
            .flat_map(|obj| names().into_iter().map(move |n| c_get(obj, n, ALLOW_NULL)))
            .collect()
    }

    /// One step of a sequence.
    #[derive(Clone, Debug)]
    enum Op {
        Set(&'static CStr, &'static CStr, bool),
        Int(&'static CStr, i64, bool),
        Double(&'static CStr, f64, bool),
        Bin(&'static CStr, &'static [u8], bool),
        Size(&'static CStr, i32, i32, bool),
        Defaults,
        Masked(i32, i32),
        FromString(&'static CStr, &'static [&'static CStr]),
        Free,
        CopyFrom,
    }

    /// Values C sets or rejects without undefined behaviour.
    const SETS: [(&CStr, &CStr); 66] = [
        (c"int", c"5"),
        (c"int", c"seven"),
        (c"int", c"max"),
        (c"int", c"1000"),
        (c"int", c"x"),
        (c"int", c"2*default+min"),
        (c"int64", c"-1e18"),
        (c"int64", c"1k"),
        (c"uint64", c"18446744073709551615"),
        (c"uint64", c"-1"),
        (c"uint64", c"1e19"),
        (c"uint", c"4294967295"),
        (c"dbl", c"1e300"),
        (c"dbl", c"-0.0"),
        (c"dbl", c"PI"),
        (c"flt", c"0.75"),
        (c"flt", c"2"),
        (c"string", c"text"),
        (c"string", c""),
        (c"q", c"3/2"),
        (c"q", c"30000:1001"),
        (c"q", c"0.333"),
        (c"q", c"1e-9"),
        (c"q", c"1:0"),
        (c"wide_q", c"1e9"),
        (c"wide_q", c"-7/3"),
        (c"boolean", c"auto"),
        (c"boolean", c"off"),
        (c"boolean", c"2"),
        (c"flags", c"one+eight"),
        (c"flags", c"+bit"),
        (c"flags", c"-one"),
        (c"flags", c"0x10"),
        (c"flags", c"+nosuch"),
        (c"big", c"high"),
        (c"big", c"low"),
        (c"size", c"hd720"),
        (c"size", c"10x20"),
        (c"size", c"bad"),
        (c"rate", c"ntsc"),
        (c"rate", c"24000/1001"),
        (c"rate", c"-1"),
        (c"color", c"blue@0.5"),
        (c"color", c"#ff00ff80"),
        (c"color", c"nocolor"),
        (c"duration", c"1:02.5"),
        (c"duration", c"-3ms"),
        (c"binary", c"deadBEEF"),
        (c"binary", c"abc"),
        (c"dict", c"a=1:b=2"),
        (c"dict", c"bad"),
        (c"array", c"1,2,3"),
        (c"array", c"11"),
        (c"array", c""),
        (c"strings", c"x|y\\|z"),
        (c"strings", c"1|2|3|4|5"),
        (c"layout", c"5.1"),
        (c"layout", c"bogus"),
        (c"pix", c"yuv420p"),
        (c"pix", c"3"),
        (c"pix", c"99999"),
        (c"sample", c"fltp"),
        (c"sample", c"2"),
        (c"sample", c"none"),
        (c"ro", c"1"),
        (c"missing", c"1"),
    ];

    const SHORTHAND: [&CStr; 2] = [c"int", c"flags"];

    /// A random step.
    fn random_op(next: &mut impl FnMut() -> u64) -> Op {
        let names = names();
        let name = names[(next() % names.len() as u64) as usize];
        let children = next().is_multiple_of(3);
        match next() % 12 {
            0..=4 => {
                let (n, v) = SETS[(next() % SETS.len() as u64) as usize];
                Op::Set(n, v, children)
            }
            5 => Op::Int(
                name,
                [0, 1, -1, 7, i64::MAX, i64::MIN, 1 << 40][(next() % 7) as usize],
                children,
            ),
            6 => Op::Double(
                name,
                [0.5, -2.0, 1e300, f64::INFINITY, 3.0, -1e10][(next() % 6) as usize],
                children,
            ),
            7 => Op::Bin(
                name,
                [&[][..], &[1, 2, 3], &[0xFF; 9]][(next() % 3) as usize],
                children,
            ),
            8 => Op::Size(name, (next() % 5) as i32 - 1, 2, children),
            9 => [
                Op::Defaults,
                Op::Masked(ffi::AV_OPT_FLAG_ENCODING_PARAM as i32, 0),
                Op::Free,
            ][(next() % 3) as usize]
                .clone(),
            10 => Op::FromString(
                [
                    c"int=3:flags=+eight",
                    c"9:one",
                    c"string=a\\:b:size=1x2",
                    c"bad",
                    c"int=x",
                ][(next() % 5) as usize],
                &SHORTHAND,
            ),
            _ => Op::CopyFrom,
        }
    }

    /// `av_opt_free`, with the binary length reset the wrapper adds (C keeps
    /// it, after which its readers dereference the NULL buffer).
    fn c_free(obj: *mut super::fixture::Obj) {
        // SAFETY: a live object whose options own what C frees.
        unsafe {
            ffi::av_opt_free(obj.cast());
            (*obj).binary_len = 0;
        }
    }

    /// The code the wrapper refuses `op` with, where C's call would be
    /// undefined behaviour: a scalar store over an array, a type name read
    /// out of bounds, an unchecked `int` conversion of a wide rational.
    fn refused(op: &Op) -> Option<i32> {
        let array = |n: &CStr| [c"array", c"strings", c"empty"].contains(&n);
        let fits_int = |d: f64| d > -2_147_483_649.0 && d < 2_147_483_648.0;
        match *op {
            Op::Int(n, _, _) | Op::Double(n, _, _) | Op::Bin(n, _, _) | Op::Size(n, _, _, _)
                if array(n) =>
            {
                Some(-22)
            }
            Op::Int(n, v, _) if n == c"wide_q" && !fits_int(v as f64) => Some(ERANGE),
            Op::Double(n, v, _)
                if n == c"wide_q" && (-1e300..=1e300).contains(&v) && !fits_int(v) =>
            {
                Some(ERANGE)
            }
            _ => None,
        }
    }

    /// The color or image size `name` (of the object or, with `children`,
    /// its child) as raw bytes, for the reference to put back after a failed
    /// parse as the wrapper does (C stores its uninitialised parse buffer).
    fn c_clobbered(o: &Owned, name: &CStr, children: bool) -> Option<(*mut u8, [u8; 8])> {
        let p = if children { o.child() } else { o.ptr() };
        // SAFETY: field projections of the live object.
        let field = unsafe {
            match name.to_bytes() {
                b"color" => (&raw mut (*p).color).cast::<u8>(),
                b"size" => (&raw mut (*p).size).cast::<u8>(),
                _ => return None,
            }
        };
        let mut bytes = [0; 8];
        // SAFETY: the field holds at least 4 bytes; `size` holds 8.
        unsafe {
            ptr::copy_nonoverlapping(
                field,
                bytes.as_mut_ptr(),
                if name == c"color" { 4 } else { 8 },
            )
        };
        Some((field, bytes))
    }

    fn c_apply(o: &Owned, src: &Owned, op: &Op) -> i32 {
        if let Op::Set(n, v, c) = *op {
            let kept = c_clobbered(o, n, c && !o.child().is_null());
            // SAFETY: a live, well-formed object; corpus values only.
            let ret = unsafe { ffi::av_opt_set(o.as_obj(), n.as_ptr(), v.as_ptr(), flags_of(c).0) };
            if let (true, Some((field, bytes))) = (ret < 0, kept) {
                // SAFETY: as in `c_clobbered`.
                unsafe {
                    ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        field,
                        if n == c"color" { 4 } else { 8 },
                    )
                };
            }
            return ret;
        }
        let obj = o.as_obj();
        // SAFETY: each call gets a live, well-formed object and live strings
        // and buffers; the corpus holds only values C handles without
        // undefined behaviour.
        unsafe {
            match *op {
                Op::Set(n, v, c) => ffi::av_opt_set(obj, n.as_ptr(), v.as_ptr(), flags_of(c).0),
                Op::Int(n, v, c) => ffi::av_opt_set_int(obj, n.as_ptr(), v, flags_of(c).0),
                Op::Double(n, v, c) => ffi::av_opt_set_double(obj, n.as_ptr(), v, flags_of(c).0),
                Op::Bin(n, v, c) => ffi::av_opt_set_bin(
                    obj,
                    n.as_ptr(),
                    v.as_ptr(),
                    v.len() as c_int,
                    flags_of(c).0,
                ),
                Op::Size(n, w, h, c) => {
                    ffi::av_opt_set_image_size(obj, n.as_ptr(), w, h, flags_of(c).0)
                }
                Op::Defaults => {
                    ffi::av_opt_set_defaults(obj);
                    0
                }
                Op::Masked(m, f) => {
                    ffi::av_opt_set_defaults2(obj, m, f);
                    0
                }
                Op::FromString(s, sh) => {
                    let sh: Vec<*const c_char> =
                        sh.iter().map(|s| s.as_ptr()).chain([ptr::null()]).collect();
                    ffi::av_opt_set_from_string(
                        obj,
                        s.as_ptr(),
                        sh.as_ptr(),
                        c"=".as_ptr(),
                        c":".as_ptr(),
                    )
                }
                Op::Free => {
                    c_free(o.ptr());
                    if !o.child().is_null() {
                        c_free(o.child());
                    }
                    0
                }
                Op::CopyFrom => ffi::av_opt_copy(obj, src.as_obj()),
            }
        }
    }

    fn rust_apply(o: &Owned, src: &Owned, op: &Op) -> i32 {
        // SAFETY: a live, well-formed object, accessed through this handle
        // only while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap();
        let code = |r: Result<(), i32>| r.err().unwrap_or(0);
        match *op {
            Op::Set(n, v, c) => code(m.set(n, v, flags_of(c).1)),
            Op::Int(n, v, c) => code(m.set_int(n, v, flags_of(c).1)),
            Op::Double(n, v, c) => code(m.set_double(n, v, flags_of(c).1)),
            Op::Bin(n, v, c) => code(m.set_bin(n, v, flags_of(c).1)),
            Op::Size(n, w, h, c) => code(m.set_image_size(n, w, h, flags_of(c).1)),
            Op::Defaults => {
                m.set_defaults();
                0
            }
            Op::Masked(mask, f) => {
                m.set_defaults_masked(mask, f);
                0
            }
            Op::FromString(s, sh) => match m.set_from_string(s, sh, c"=", c":") {
                Ok(n) => n as i32,
                Err(e) => e,
            },
            Op::Free => {
                m.free_options();
                if let Some(mut child) = m.children_mut().next_child() {
                    child.free_options();
                }
                0
            }
            Op::CopyFrom => {
                // SAFETY: the source is a live, well-formed object nothing
                // writes during the call.
                let src = unsafe { ObjectRef::from_ptr(src.as_obj()) }.unwrap();
                code(m.copy_from(src))
            }
        }
    }

    /// Whether C's `av_opt_get_int` of `name` is defined: the wrapper refuses
    /// the conversions it is not.
    fn c_int_defined(obj: *mut c_void, name: &CStr, flags: c_int) -> bool {
        // Only floats, doubles and rationals convert; the other types read
        // their integer as is.
        if ![c"dbl", c"flt", c"q", c"wide_q", c"dbl_unit"].contains(&name) {
            return true;
        }
        let mut d = 0.0;
        // SAFETY: a live object and name, a writable double.
        let ret = unsafe { ffi::av_opt_get_double(obj, name.as_ptr(), flags, &mut d) };
        ret < 0 || (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&d)
    }

    /// Compares every getter of both objects, after a step.
    #[track_caller]
    fn compare_getters(c: &Owned, r: &Owned, step: &str) {
        // SAFETY: a live, well-formed object nothing writes while it lives.
        let h = unsafe { ObjectRef::from_ptr(r.as_obj()) }.unwrap();
        let obj = c.as_obj();
        for name in names() {
            for children in [false, true] {
                let (cf, rf) = flags_of(children);
                let at = format!("{step}: {name:?} {children}");
                let (ret, s) = c_get(obj, name, cf);
                let got = h.get(name, rf);
                assert_eq!(got.as_ref().err().copied().unwrap_or(0), ret, "get {at}");
                assert_eq!(got.ok().map(|s| s.as_bytes().to_vec()), s, "get {at}");
                let (ret, s) = c_get(obj, name, cf | ALLOW_NULL);
                let got = h.get_nullable(name, rf);
                assert_eq!(
                    got.as_ref().err().copied().unwrap_or(0),
                    ret,
                    "get_nullable {at}"
                );
                assert_eq!(
                    got.ok().flatten().map(|s| s.as_bytes().to_vec()),
                    s,
                    "get_nullable {at}"
                );

                let mut i = 0;
                if c_int_defined(obj, name, cf) {
                    // SAFETY: the conversion is defined, as checked.
                    let ret = unsafe { ffi::av_opt_get_int(obj, name.as_ptr(), cf, &mut i) };
                    assert_eq!(
                        h.get_int(name, rf),
                        if ret < 0 { Err(ret) } else { Ok(i) },
                        "get_int {at}"
                    );
                } else {
                    assert_eq!(h.get_int(name, rf), Err(ERANGE), "get_int {at}");
                }
                let mut d = 0.0;
                // SAFETY: a live object and name, writable outputs.
                let ret = unsafe { ffi::av_opt_get_double(obj, name.as_ptr(), cf, &mut d) };
                let got = h.get_double(name, rf);
                assert_eq!(got.err().unwrap_or(0), ret, "get_double {at}");
                if let Ok(v) = got {
                    assert!(
                        v.to_bits() == d.to_bits() || (v.is_nan() && d.is_nan()),
                        "get_double {at}"
                    );
                }
                let (mut w, mut hh) = (0, 0);
                let n = name.as_ptr();
                // SAFETY: as above.
                let ret = unsafe { ffi::av_opt_get_image_size(obj, n, cf, &mut w, &mut hh) };
                assert_eq!(
                    h.get_image_size(name, rf),
                    if ret < 0 { Err(ret) } else { Ok((w, hh)) },
                    "size {at}"
                );
                let mut n: c_uint = 0;
                // SAFETY: as above.
                let ret = unsafe { ffi::av_opt_get_array_size(obj, name.as_ptr(), cf, &mut n) };
                assert_eq!(
                    h.get_array_size(name, rf),
                    if ret < 0 { Err(ret) } else { Ok(n as usize) },
                    "array size {at}"
                );
                // SAFETY: as above.
                let ret = unsafe { ffi::av_opt_is_set_to_default_by_name(obj, name.as_ptr(), cf) };
                assert_eq!(
                    h.is_set_to_default(name, rf),
                    if ret < 0 { Err(ret) } else { Ok(ret > 0) },
                    "default {at}"
                );
            }
            for flag in [
                c"one", c"bit", c"eight", c"low", c"dflag", c"seven", c"missing",
            ] {
                if !c_int_defined(obj, name, 0) {
                    continue;
                }
                // SAFETY: as above; the field's integer read is defined.
                let ret = unsafe { ffi::av_opt_flag_is_set(obj, name.as_ptr(), flag.as_ptr()) };
                assert_eq!(
                    h.flag_is_set(name, flag),
                    ret != 0,
                    "{step}: flag {name:?} {flag:?}"
                );
            }
        }
        for (opt_flags, flags) in [
            (0, SerializeFlags::NONE),
            (0, SerializeFlags::SKIP_DEFAULTS),
            (ffi::AV_OPT_FLAG_ENCODING_PARAM as i32, SerializeFlags::NONE),
            (
                ffi::AV_OPT_FLAG_ENCODING_PARAM as i32,
                SerializeFlags::OPT_FLAGS_EXACT,
            ),
            (
                0,
                SerializeFlags::SEARCH_CHILDREN | SerializeFlags::SKIP_DEFAULTS,
            ),
        ] {
            for (kv, pairs) in [(b'=', b':'), (b'#', b';'), (b':', b':')] {
                let mut out = ptr::null_mut();
                // SAFETY: a live object, a writable slot.
                let ret = unsafe {
                    ffi::av_opt_serialize(
                        obj,
                        opt_flags,
                        flags.bits(),
                        &mut out,
                        kv as c_char,
                        pairs as c_char,
                    )
                };
                let c_str = (!out.is_null()).then(|| {
                    // SAFETY: an owned C string, freed here.
                    let s = unsafe { CStr::from_ptr(out) }.to_bytes().to_vec();
                    // SAFETY: as above.
                    unsafe { ffi::av_free(out.cast()) };
                    s
                });
                let got = h.serialize(opt_flags, flags, kv, pairs);
                assert_eq!(
                    got.as_ref().err().copied().unwrap_or(0),
                    ret.min(0),
                    "{step}: serialize"
                );
                assert_eq!(
                    got.ok().map(|s| s.as_bytes().to_vec()),
                    c_str,
                    "{step}: serialize"
                );
            }
        }
        let (cc, rc) = (c.child().is_null(), h.children().count());
        assert_eq!(rc, usize::from(!cc));
    }

    /// Runs `ops` on a C-driven and a Rust-driven object, comparing codes,
    /// states and getters after every step.
    fn run(ops: &[Op], getters_every: usize) {
        let (c, r) = (Owned::with_child(), Owned::with_child());
        let (c_src, r_src) = (Owned::new(), Owned::new());
        for src in [&c_src, &r_src] {
            // SAFETY: live objects and strings, values C handles.
            unsafe {
                ffi::av_opt_set(src.as_obj(), c"string".as_ptr(), c"copied".as_ptr(), 0);
                ffi::av_opt_set(src.as_obj(), c"strings".as_ptr(), c"p|q".as_ptr(), 0);
                ffi::av_opt_set_bin(src.as_obj(), c"binary".as_ptr(), [9u8, 8].as_ptr(), 2, 0);
            }
        }
        assert_eq!(observe(&c), observe(&r));
        for (i, op) in ops.iter().enumerate() {
            let step = format!("step {i}: {op:?}");
            if let Some(err) = refused(op) {
                assert_eq!(rust_apply(&r, &r_src, op), err, "{step}");
                continue;
            }
            assert_eq!(
                c_apply(&c, &c_src, op),
                rust_apply(&r, &r_src, op),
                "{step}"
            );
            assert_eq!(observe(&c), observe(&r), "{step}");
            // SAFETY: raw reads of the live objects' binary lengths.
            unsafe { assert_eq!((*c.ptr()).binary_len, (*r.ptr()).binary_len, "{step}") };
            if i % getters_every == 0 {
                compare_getters(&c, &r, &step);
            }
        }
        compare_getters(&c, &r, "end");
    }

    #[test]
    fn every_corpus_value_sets_as_in_c() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        let mut ops: Vec<Op> = SETS.iter().map(|&(n, v)| Op::Set(n, v, false)).collect();
        ops.extend(SETS.iter().map(|&(n, v)| Op::Set(n, v, true)));
        run(&ops, 7);
    }

    #[test]
    fn typed_setters_defaults_free_and_copies_agree() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        let mut ops = Vec::new();
        for name in names() {
            for children in [false, true] {
                ops.extend([
                    Op::Int(name, 3, children),
                    Op::Int(name, i64::MIN, children),
                    Op::Double(name, 0.5, children),
                    Op::Double(name, f64::INFINITY, children),
                    Op::Bin(name, &[1, 2, 3], children),
                    Op::Bin(name, &[], children),
                    Op::Size(name, 4, 5, children),
                    Op::Size(name, -4, 5, children),
                ]);
            }
        }
        ops.extend([
            Op::Free,
            Op::Free,
            Op::Masked(ffi::AV_OPT_FLAG_ENCODING_PARAM as i32, 0),
            Op::Defaults,
            Op::CopyFrom,
            Op::FromString(c"int=3:flags=+eight", &SHORTHAND),
            Op::FromString(c"9:one:string=s", &SHORTHAND),
            Op::FromString(c"9:string=s:one", &SHORTHAND),
            Op::FromString(c"", &SHORTHAND),
            Op::Free,
            Op::CopyFrom,
        ]);
        run(&ops, 5);
    }

    #[test]
    fn random_sequences_agree() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        let mut state = 0xdead_beef_cafe_f00du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4 {
            let ops: Vec<Op> = (0..300).map(|_| random_op(&mut next)).collect();
            run(&ops, 50);
        }
    }

    #[test]
    fn children_are_listed_as_c_lists_them() {
        let o = Owned::with_child();
        // SAFETY: a live object and a NULL cursor.
        let first = unsafe { ffi::av_opt_child_next(o.as_obj(), ptr::null_mut()) };
        // SAFETY: as above, with C's last child as the cursor.
        let second = unsafe { ffi::av_opt_child_next(o.as_obj(), first) };
        // SAFETY: a live, well-formed object nothing writes.
        let h = unsafe { ObjectRef::from_ptr(o.as_obj()) }.unwrap();
        let children: Vec<_> = h.children().collect();
        assert_eq!(children.len(), 1);
        assert!(second.is_null());
        assert_eq!(first, o.child().cast());
        assert!(children[0].class().ptr_eq(h.class()));
        // The child's own options are what C reads at the child.
        assert_eq!(
            children[0]
                .get(c"int", SearchFlags::NONE)
                .unwrap()
                .as_bytes(),
            c_get(first, c"int", 0).1.unwrap()
        );
        let lone = Owned::new();
        // SAFETY: as above.
        let h = unsafe { ObjectRef::from_ptr(lone.as_obj()) }.unwrap();
        assert_eq!(h.children().count(), 0);
    }

    #[test]
    fn key_values_parse_as_in_c() {
        for opts in [
            c"a=1:b=2",
            c" k = v ",
            c"k='quoted:value':x",
            c"k=a\\:b",
            c"novalue",
            c"=v",
            c"",
            c":x=1",
            c"k#v;q",
            c"key.with/odd-chars_=1",
        ] {
            for (kv, pairs) in [(c"=", c":"), (c"=#", c":;"), (c"", c"")] {
                for implicit in [false, true] {
                    let (mut rest, mut key, mut val) =
                        (opts.as_ptr(), ptr::null_mut(), ptr::null_mut());
                    let flags = if implicit {
                        ffi::AV_OPT_FLAG_IMPLICIT_KEY.0
                    } else {
                        0
                    };
                    // SAFETY: writable slots and live strings.
                    let ret = unsafe {
                        ffi::av_opt_get_key_value(
                            &mut rest,
                            kv.as_ptr(),
                            pairs.as_ptr(),
                            flags,
                            &mut key,
                            &mut val,
                        )
                    };
                    let take = |p: *mut c_char| {
                        (!p.is_null()).then(|| {
                            // SAFETY: an owned C string, freed here.
                            let s = unsafe { CStr::from_ptr(p) }.to_owned();
                            // SAFETY: as above.
                            unsafe { ffi::av_free(p.cast()) };
                            s
                        })
                    };
                    let (key, val) = (take(key), take(val));
                    let at = format!("{opts:?} {kv:?} {pairs:?} {implicit}");
                    match parse_key_value(opts, kv, pairs, implicit) {
                        Ok(p) => {
                            assert_eq!(ret, 0, "{at}");
                            assert_eq!(p.key.map(|k| k.as_c_str().to_owned()), key, "{at}");
                            assert_eq!(Some(p.value.as_c_str().to_owned()), val, "{at}");
                            assert_eq!(p.rest.as_ptr(), rest, "{at}");
                        }
                        Err(e) => assert_eq!(e, ret, "{at}"),
                    }
                }
            }
        }
    }

    type Line = (c_int, Option<String>, String);

    thread_local! {
        static LINES: RefCell<Option<Vec<Line>>> = const { RefCell::new(None) };
    }

    /// `line` with every context address (` @ 0x...`) masked.
    fn mask_addresses(line: &str) -> String {
        let mut out = String::new();
        let mut rest = line;
        while let Some(at) = rest.find(" @ 0x") {
            out.push_str(&rest[..at]);
            out.push_str(" @ ADDR");
            rest = rest[at + 5..].trim_start_matches(|c: char| c.is_ascii_hexdigit());
        }
        out.push_str(rest);
        out
    }

    /// Captures this thread's messages while [`captured`] runs.
    struct Capture;

    impl Callback for Capture {
        fn log(record: Record<'_>) {
            LINES.with(|l| {
                if let Some(lines) = l.borrow_mut().as_mut() {
                    let mut prefix = true;
                    let line = record.to_line(&mut prefix).unwrap().into_string().unwrap();
                    let class = record
                        .class()
                        .and_then(|c| c.name())
                        .map(|n| n.to_string_lossy().into_owned());
                    lines.push((record.level().raw(), class, mask_addresses(&line)));
                }
            });
        }
    }

    /// The lines `f` logs on this thread. The caller holds the [`LogState`]
    /// lock.
    fn captured(f: impl FnOnce()) -> Vec<Line> {
        LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
        log::set_callback::<Capture>();
        f();
        log::set_default_callback();
        LINES.with(|l| l.borrow_mut().take().unwrap())
    }

    /// Whether this is the isolated run of test `name` (a child process with
    /// one thread, which installs the process-wide log callback alone); the
    /// parent spawns it and checks its status.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_EQUIV_OPT_OBJECTS_ISOLATED";
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

    /// Every allocating step at small `av_max_alloc` limits. Not the
    /// defaults: when C's evaluator cannot allocate, `av_parse_video_rate`
    /// fails after `sscanf` stored only the numerator of its uninitialised
    /// result, which `av_opt_set_defaults` stores whole (documented on
    /// `ObjectMut::set_defaults`).
    #[test]
    fn allocation_failures_agree() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        if !isolated("opt::objects::allocation_failures_agree") {
            return;
        }
        let set_limit = |limit: usize| {
            // SAFETY: `av_max_alloc` takes a size by value; nothing else runs
            // in this process.
            unsafe { ffi::av_max_alloc(limit) }
        };
        for limit in [0, 1, 2, 4, 6, 8, 16, 64] {
            let (c, r) = (Owned::with_child(), Owned::with_child());
            let (c_src, r_src) = (Owned::new(), Owned::new());
            set_limit(limit);
            for op in [
                Op::CopyFrom,
                Op::FromString(c"string=abcdefgh:int=1", &[]),
                Op::FromString(c"abcdefgh", &SHORTHAND),
                Op::Set(c"string", c"abcdefghijkl", false),
                Op::Set(c"dict", c"key=value", false),
                Op::Bin(c"binary", &[1; 9], false),
            ] {
                assert_eq!(
                    c_apply(&c, &c_src, &op),
                    rust_apply(&r, &r_src, &op),
                    "{op:?} at {limit}"
                );
                set_limit(i32::MAX as usize);
                assert_eq!(observe(&c), observe(&r), "{op:?} at {limit}");
                set_limit(limit);
            }
            let h = r.as_obj();
            // SAFETY: a live, well-formed object nothing writes.
            let h = unsafe { ObjectRef::from_ptr(h) }.unwrap();
            for name in [c"string", c"binary", c"dict", c"int", c"array"] {
                let (ret, s) = c_get(c.as_obj(), name, 0);
                let got = h.get(name, SearchFlags::NONE);
                assert_eq!(
                    got.as_ref().err().copied().unwrap_or(0),
                    ret,
                    "{name:?} at {limit}"
                );
                assert_eq!(
                    got.ok().map(|s| s.as_bytes().to_vec()),
                    s,
                    "{name:?} at {limit}"
                );
            }
            let mut out = ptr::null_mut();
            // SAFETY: a live object, a writable slot.
            let ret = unsafe {
                ffi::av_opt_serialize(c.as_obj(), 0, 0, &mut out, b'=' as c_char, b':' as c_char)
            };
            if !out.is_null() {
                // SAFETY: an owned C string.
                unsafe { ffi::av_free(out.cast()) };
            }
            assert_eq!(
                h.serialize(0, SerializeFlags::NONE, b'=', b':')
                    .err()
                    .unwrap_or(0),
                ret.min(0)
            );
            for opts in [c"key=value", c"k=v"] {
                let (mut rest, mut key, mut val) =
                    (opts.as_ptr(), ptr::null_mut(), ptr::null_mut());
                // SAFETY: writable slots and live strings.
                let ret = unsafe {
                    ffi::av_opt_get_key_value(
                        &mut rest,
                        c"=".as_ptr(),
                        c":".as_ptr(),
                        0,
                        &mut key,
                        &mut val,
                    )
                };
                // SAFETY: NULL or owned strings.
                unsafe {
                    ffi::av_free(key.cast());
                    ffi::av_free(val.cast());
                }
                assert_eq!(
                    parse_key_value(opts, c"=", c":", false).err().unwrap_or(0),
                    ret,
                    "{opts:?} at {limit}"
                );
            }
            set_limit(i32::MAX as usize);
        }
    }

    /// Options wave 0, batch-1 review: `av_opt_show2` lists only options
    /// whose flags meet `req_flags`, and `OBJ_CLASS`'s are mostly flagless,
    /// so its listing never reached most of C's default and range printers
    /// (`log_default`, `log_value`, `log_int_value`, `get_opt_const_name`).
    /// This lists a copy of the table with every option flagged, plus
    /// aliases whose bounds and defaults are the named extremes those
    /// printers spell out, from C and through `ObjectRef::show`, and compares
    /// the lines.
    #[test]
    fn listings_of_every_type_and_extreme_log_as_c() {
        if under_bsan() {
            return;
        }
        if !isolated("opt::objects::listings_of_every_type_and_extreme_log_as_c") {
            return;
        }
        use super::fixture::{OBJ_CLASS, Obj};
        use core::mem::offset_of;
        type T = ffi::AVOptionType;
        let enc = ffi::AV_OPT_FLAG_ENCODING_PARAM as c_int;
        let end = OBJ_OPTIONS.0.len() - 1;
        let mut table: Vec<ffi::AVOption> = OBJ_OPTIONS.0[..end]
            .iter()
            .map(|o| ffi::AVOption {
                flags: o.flags | enc,
                ..*o
            })
            .collect();
        let alias = |name: &'static CStr,
                     offset: usize,
                     ty: T,
                     i: Option<i64>,
                     d: f64,
                     min: f64,
                     max: f64,
                     unit: Option<&'static CStr>| ffi::AVOption {
            name: name.as_ptr(),
            help: c"an extreme".as_ptr(),
            offset: offset as c_int,
            type_: ty,
            default_val: match i {
                Some(i) => ffi::AVOption__bindgen_ty_1 { i64_: i },
                None => ffi::AVOption__bindgen_ty_1 { dbl: d },
            },
            min,
            max,
            flags: enc,
            unit: unit.map_or(ptr::null(), CStr::as_ptr),
        };
        let (int, int64, uint) = (
            offset_of!(Obj, int),
            offset_of!(Obj, int64),
            offset_of!(Obj, uint),
        );
        let (flt, dbl) = (offset_of!(Obj, flt), offset_of!(Obj, dbl));
        let (imin, imax) = (i32::MIN as f64, i32::MAX as f64);
        let (fmax, fmin) = (f32::MAX as f64, f32::MIN_POSITIVE as f64);
        table.extend([
            alias(
                c"imax",
                int,
                T::AV_OPT_TYPE_INT,
                Some(i32::MAX.into()),
                0.0,
                imin,
                imax,
                None,
            ),
            alias(
                c"imin",
                int,
                T::AV_OPT_TYPE_INT,
                Some(i32::MIN.into()),
                0.0,
                imin,
                imax,
                None,
            ),
            alias(
                c"named",
                int,
                T::AV_OPT_TYPE_INT,
                Some(7),
                0.0,
                -10.0,
                100.0,
                Some(c"mode"),
            ),
            alias(
                c"u32",
                uint,
                T::AV_OPT_TYPE_UINT,
                Some(u32::MAX.into()),
                0.0,
                0.0,
                u32::MAX as f64,
                None,
            ),
            alias(
                c"i64max",
                int64,
                T::AV_OPT_TYPE_INT64,
                Some(i64::MAX),
                0.0,
                i64::MIN as f64,
                i64::MAX as f64,
                None,
            ),
            alias(
                c"i64min",
                int64,
                T::AV_OPT_TYPE_INT64,
                Some(i64::MIN),
                0.0,
                i64::MIN as f64,
                i64::MAX as f64,
                None,
            ),
            alias(
                c"fmax",
                flt,
                T::AV_OPT_TYPE_FLOAT,
                None,
                fmax,
                -fmax,
                fmax,
                None,
            ),
            alias(
                c"fmin",
                flt,
                T::AV_OPT_TYPE_FLOAT,
                None,
                fmin,
                -fmin,
                fmin,
                None,
            ),
            alias(
                c"dmin",
                dbl,
                T::AV_OPT_TYPE_DOUBLE,
                None,
                f64::MIN_POSITIVE,
                -f64::MIN_POSITIVE,
                f64::MIN_POSITIVE,
                None,
            ),
            alias(
                c"dmax",
                dbl,
                T::AV_OPT_TYPE_DOUBLE,
                None,
                -f64::MAX,
                f64::MIN,
                f64::MAX,
                None,
            ),
            alias(
                c"dint",
                dbl,
                T::AV_OPT_TYPE_DOUBLE,
                None,
                imax,
                imin,
                u32::MAX as f64,
                None,
            ),
            alias(
                c"dlow",
                dbl,
                T::AV_OPT_TYPE_DOUBLE,
                None,
                imin,
                i64::MIN as f64,
                0.5,
                None,
            ),
            alias(
                c"hexflags",
                offset_of!(Obj, flags),
                T::AV_OPT_TYPE_FLAGS,
                Some(0x40),
                0.0,
                0.0,
                u32::MAX as f64,
                Some(c"dunit"),
            ),
            alias(
                c"auto",
                offset_of!(Obj, boolean),
                T::AV_OPT_TYPE_BOOL,
                Some(-1),
                0.0,
                -1.0,
                1.0,
                None,
            ),
            alias(
                c"forever",
                offset_of!(Obj, duration),
                T::AV_OPT_TYPE_DURATION,
                Some(i64::MAX),
                0.0,
                0.0,
                i64::MAX as f64,
                None,
            ),
            alias(
                c"long",
                offset_of!(Obj, duration),
                T::AV_OPT_TYPE_DURATION,
                Some(3_723_000_001),
                0.0,
                0.0,
                i64::MAX as f64,
                None,
            ),
            alias(
                c"negative",
                offset_of!(Obj, duration),
                T::AV_OPT_TYPE_DURATION,
                Some(-61_500_000),
                0.0,
                i64::MIN as f64,
                0.0,
                None,
            ),
            alias(
                c"nopix",
                offset_of!(Obj, pix),
                T::AV_OPT_TYPE_PIXEL_FMT,
                Some(-1),
                0.0,
                -1.0,
                imax,
                None,
            ),
            alias(
                c"nosample",
                offset_of!(Obj, sample),
                T::AV_OPT_TYPE_SAMPLE_FMT,
                Some(-1),
                0.0,
                -1.0,
                imax,
                None,
            ),
            alias(
                c"third",
                offset_of!(Obj, q),
                T::AV_OPT_TYPE_RATIONAL,
                None,
                1.0 / 3.0,
                0.0,
                imax,
                None,
            ),
        ]);
        table.push(OBJ_OPTIONS.0[end]);
        let mut class = OBJ_CLASS.0;
        class.option = table.as_ptr();
        let (c, r) = (Owned::with_child(), Owned::with_child());
        for o in [&c, &r] {
            // SAFETY: `o` is live and owned; its class pointer is swapped for
            // a local class over the same fields (every alias names a field
            // of its type), live until it is restored below.
            unsafe { (*o.ptr()).class = &class };
        }
        let _state = LogState::lock();
        for (req, rej) in [
            (enc, 0),
            (-1, ffi::AV_OPT_FLAG_READONLY as c_int),
            (enc, enc),
        ] {
            for log_ctx in [false, true] {
                let c_lines = captured(|| {
                    let ctx = if log_ctx {
                        c.child().cast()
                    } else {
                        ptr::null_mut()
                    };
                    // SAFETY: live objects of well-formed classes.
                    unsafe { ffi::av_opt_show2(c.as_obj(), ctx, req, rej) };
                });
                let r_lines = captured(|| {
                    // SAFETY: a live, well-formed object nothing writes.
                    let h = unsafe { ObjectRef::from_ptr(r.as_obj()) }.unwrap();
                    let ctx = log_ctx.then(|| h.children().next().unwrap());
                    h.show(ctx, req, rej);
                });
                assert_eq!(c_lines, r_lines, "{req} {rej} {log_ctx}");
                if rej != enc {
                    let text: String = c_lines.iter().map(|l| format!("{l:?}")).collect();
                    for spelled in [
                        "INT_MAX",
                        "INT_MIN",
                        "UINT32_MAX",
                        "I64_MAX",
                        "I64_MIN",
                        "FLT_MAX",
                        "FLT_MIN",
                        "DBL_MIN",
                        "-DBL_MAX",
                        "seven",
                        "auto",
                        "none",
                        "1/3",
                    ] {
                        assert!(text.contains(spelled), "{spelled} in {text}");
                    }
                }
            }
        }
        for o in [&c, &r] {
            // SAFETY: as above, restoring the static class.
            unsafe { (*o.ptr()).class = &OBJ_CLASS.0 };
        }
    }

    #[test]
    fn listings_and_errors_log_as_c() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        if !isolated("opt::objects::listings_and_errors_log_as_c") {
            return;
        }
        let _state = LogState::lock();
        let (c, r) = (Owned::with_child(), Owned::with_child());
        for (req, rej) in [
            (0, 0),
            (-1, 0),
            (ffi::AV_OPT_FLAG_ENCODING_PARAM as i32, 0),
            (-1, ffi::AV_OPT_FLAG_READONLY as i32),
        ] {
            for log_ctx in [None, Some(true)] {
                let c_lines = captured(|| {
                    let ctx = log_ctx.map_or(ptr::null_mut(), |_| c.child().cast());
                    // SAFETY: live objects.
                    unsafe { ffi::av_opt_show2(c.as_obj(), ctx, req, rej) };
                });
                let r_lines = captured(|| {
                    // SAFETY: a live, well-formed object nothing writes.
                    let h = unsafe { ObjectRef::from_ptr(r.as_obj()) }.unwrap();
                    let ctx = log_ctx.map(|_| h.children().next().unwrap());
                    h.show(ctx, req, rej);
                });
                assert!(!c_lines.is_empty());
                assert_eq!(c_lines, r_lines);
            }
        }
        for op in [
            Op::Set(c"int", c"1000", false),
            Op::Set(c"int", c"x", false),
            Op::Set(c"size", c"bad", false),
            Op::Set(c"ro", c"1", false),
            Op::Bin(c"int", &[1], false),
            Op::Size(c"size", -1, 1, false),
            Op::Double(c"int", 1e300, false),
            Op::FromString(c"x", &[]),
            Op::FromString(c"missing=1", &[]),
            Op::Set(c"old", c"1", false),
        ] {
            let (c_src, r_src) = (Owned::new(), Owned::new());
            let c_lines = captured(|| {
                c_apply(&c, &c_src, &op);
            });
            let r_lines = captured(|| {
                rust_apply(&r, &r_src, &op);
            });
            assert_eq!(c_lines, r_lines, "{op:?}");
        }
        let c_lines = captured(|| {
            let mut out = ptr::null_mut();
            // SAFETY: a live object, a writable slot; invalid separators are
            // rejected before C reads further.
            unsafe { ffi::av_opt_serialize(c.as_obj(), 0, 0, &mut out, 0, b':' as c_char) };
        });
        // SAFETY: as above.
        let h = unsafe { ObjectRef::from_ptr(r.as_obj()) }.unwrap();
        let r_lines = captured(|| {
            let _ = h.serialize(0, SerializeFlags::NONE, 0, b':');
        });
        assert_eq!(c_lines, r_lines);
        let _ = CString::new("").unwrap();
    }
}
