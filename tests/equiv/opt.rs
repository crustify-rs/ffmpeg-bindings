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
