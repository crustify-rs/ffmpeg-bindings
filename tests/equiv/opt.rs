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

/// `av_set_options_string`: C and Rust set options of two objects of the
/// test class from the same lists, and every option field compares equal
/// after each call.
mod set_options_string {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::set_options_string;

    use super::fixture::{CLASS, Ctx};

    /// What C can observe of every option field of a [`Ctx`]; floats by
    /// their bits, so that NaN compares.
    #[derive(Debug, PartialEq)]
    struct Fields {
        int: c_int,
        int64: i64,
        uint64: u64,
        dbl: u64,
        flt: u32,
        string: Option<CString>,
        q: (c_int, c_int),
        boolean: c_int,
        flags: c_int,
        size: [c_int; 2],
        rate: (c_int, c_int),
        color: [u8; 4],
        duration: i64,
        binary: Option<Vec<u8>>,
        dict: Option<Vec<(CString, CString)>>,
        array: Option<Vec<c_int>>,
        empty_array: Option<Vec<c_int>>,
        bare: c_int,
        nostr: Option<CString>,
        bogus: c_int,
    }

    fn string(p: *const c_char) -> Option<CString> {
        // SAFETY: per the caller, `p` is NULL or a string field's live
        // NUL-terminated value.
        (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_owned())
    }

    fn array<T: Copy>(p: *const T, len: usize) -> Option<Vec<T>> {
        // SAFETY: per the caller, `p` is NULL or a field's live block of
        // `len` elements.
        (!p.is_null()).then(|| unsafe { core::slice::from_raw_parts(p, len) }.to_vec())
    }

    fn dict(d: *mut c_void) -> Option<Vec<(CString, CString)>> {
        if d.is_null() {
            return None;
        }
        let mut entries = Vec::new();
        let mut e: *const ffi::AVDictionaryEntry = ptr::null();
        loop {
            // SAFETY: `d` is the live dictionary C stored in the field, and
            // `e` NULL or its previous entry; the empty key with
            // `AV_DICT_IGNORE_SUFFIX` matches every entry in turn.
            e = unsafe {
                ffi::av_dict_get(
                    d.cast(),
                    c"".as_ptr(),
                    e,
                    ffi::AV_DICT_IGNORE_SUFFIX as c_int,
                )
            };
            if e.is_null() {
                return Some(entries);
            }
            // SAFETY: an entry of a live dictionary holds two live strings.
            let (k, v) = unsafe { ((*e).key, (*e).value) };
            entries.push((string(k).unwrap(), string(v).unwrap()));
        }
    }

    fn fields(c: &Ctx) -> Fields {
        Fields {
            int: c.int,
            int64: c.int64,
            uint64: c.uint64,
            dbl: c.dbl.to_bits(),
            flt: c.flt.to_bits(),
            string: string(c.string),
            q: (c.q.num, c.q.den),
            boolean: c.boolean,
            flags: c.flags,
            size: c.size,
            rate: (c.rate.num, c.rate.den),
            color: c.color,
            duration: c.duration,
            binary: array(c.binary, c.binary_len as usize),
            dict: dict(c.dict),
            array: array(c.array, c.array_len as usize),
            empty_array: array(c.empty_array, c.empty_array_len as usize),
            bare: c.bare,
            nostr: string(c.nostr),
            bogus: c.bogus,
        }
    }

    /// An object of the test class that frees its option values on drop.
    struct Owned(Ctx);

    impl Owned {
        fn new() -> Self {
            Self(Ctx::new(&CLASS))
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: the object's owned fields hold NULL or values C stored.
            unsafe { ffi::av_opt_free(self.0.as_obj()) }
        }
    }

    fn c_set(ctx: *mut c_void, opts: &CStr, kv: &CStr, pairs: &CStr) -> Result<usize, i32> {
        // SAFETY: as for `set_options_string`, which every caller honours.
        let ret =
            unsafe { ffi::av_set_options_string(ctx, opts.as_ptr(), kv.as_ptr(), pairs.as_ptr()) };
        if ret < 0 { Err(ret) } else { Ok(ret as usize) }
    }

    /// `AVERROR_OPTION_NOT_FOUND`: `FFERRTAG(0xF8, 'O', 'P', 'T')`.
    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);

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
        (c"empty_array=7,8:bare=1:nostr=x", c"=", c":"),
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
        (c"bogus=1", c"=", c":"),
        (c"seven=1", c"=", c":"),
        (c"int=seven", c"=", c":"),
        (c"flags=+bit-bit", c"=", c":"),
        (c"color=red@0x80:size=320x240:rate=ntsc", c"=", c":"),
        (c"color=nocolor", c"=", c":"),
        (c"duration=-1:01:02.5", c"=", c":"),
        (c"string=\\\\back\\\\slash", c"=", c":"),
    ];

    #[test]
    fn sequences_agree_after_every_call() {
        let (mut c, mut r) = (Owned::new(), Owned::new());
        for &(opts, kv, pairs) in STEPS {
            let expected = c_set(c.0.as_obj(), opts, kv, pairs);
            // SAFETY: `r` is a live object of the test class whose owned
            // fields hold NULL or values C stored; every value is in range
            // and no color has a NaN opacity.
            let got = unsafe { set_options_string(r.0.as_obj(), opts, kv, pairs) };
            assert_eq!(got, expected, "{opts:?}");
            assert_eq!(fields(&r.0), fields(&c.0), "{opts:?}");
        }
        // Then the defaults, set over the values, and the list once more.
        for o in [&mut c, &mut r] {
            // SAFETY: as above.
            unsafe { ffi::av_opt_set_defaults2(o.0.as_obj(), 0, 0) };
        }
        assert_eq!(fields(&r.0), fields(&c.0));
        for &(opts, kv, pairs) in STEPS.iter().rev() {
            let expected = c_set(c.0.as_obj(), opts, kv, pairs);
            // SAFETY: as above.
            let got = unsafe { set_options_string(r.0.as_obj(), opts, kv, pairs) };
            assert_eq!(got, expected, "{opts:?}");
            assert_eq!(fields(&r.0), fields(&c.0), "{opts:?}");
        }
    }

    /// C's image-size setter copies its uninitialised result into the field
    /// when the size does not parse, so only the error and the other fields
    /// compare after one.
    #[test]
    fn failed_image_sizes_agree_but_for_the_size() {
        let (mut c, mut r) = (Owned::new(), Owned::new());
        for opts in [
            c"int=5:size=0x0",
            c"size=640x480:size=x",
            c"size=-1x1",
            c"size=hd720",
        ] {
            let expected = c_set(c.0.as_obj(), opts, c"=", c":");
            // SAFETY: as in `sequences_agree_after_every_call`.
            let got = unsafe { set_options_string(r.0.as_obj(), opts, c"=", c":") };
            assert_eq!(got, expected, "{opts:?}");
            let (mut cf, mut rf) = (fields(&c.0), fields(&r.0));
            if expected.is_err() {
                (cf.size, rf.size) = ([0; 2], [0; 2]);
            }
            assert_eq!(rf, cf, "{opts:?}");
        }
        assert_eq!((r.0.size, c.0.size), ([1280, 720], [1280, 720]));
    }

    #[test]
    fn errors_agree() {
        let cases: &[(&CStr, &CStr, &CStr, Result<usize, i32>)] = &[
            (c"int=5", c"=", c":", Ok(1)),
            (c"int=5:int=6", c"=", c":", Ok(2)),
            (c"", c"=", c":", Ok(0)),
            (c"missing=1", c"=", c":", Err(NOT_FOUND)),
            (c"int", c"=", c":", Err(-22)),
            (c"=5", c"=", c":", Err(-22)),
            (c"int=5", c"", c":", Err(-22)),
            (c"int=1000", c"=", c":", Err(-34)),
        ];
        for &(opts, kv, pairs, expected) in cases {
            let (mut c, mut r) = (Owned::new(), Owned::new());
            assert_eq!(c_set(c.0.as_obj(), opts, kv, pairs), expected, "{opts:?}");
            // SAFETY: as in `sequences_agree_after_every_call`.
            let got = unsafe { set_options_string(r.0.as_obj(), opts, kv, pairs) };
            assert_eq!(got, expected, "{opts:?}");
            assert_eq!(fields(&r.0), fields(&c.0), "{opts:?}");
        }
    }

    /// `av_get_token` allocates each key as long as the rest of the list,
    /// and C fails with `AVERROR(ENOMEM)` when it cannot; both sides agree at
    /// every `av_max_alloc` limit around that size. The limit is
    /// process-global, so this lowers it only in a child process running
    /// nothing else, and passes when the child does.
    #[test]
    fn allocation_failure_at_the_av_max_alloc_limit_agrees() {
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
            let (mut c, mut r) = (Owned::new(), Owned::new());
            // SAFETY: `av_max_alloc` takes a size by value; nothing else
            // runs in this process.
            unsafe { ffi::av_max_alloc(limit) };
            let expected = c_set(c.0.as_obj(), opts, c"=", c":");
            // SAFETY: as in `sequences_agree_after_every_call`.
            let got = unsafe { set_options_string(r.0.as_obj(), opts, c"=", c":") };
            // SAFETY: as above; restores the default limit.
            unsafe { ffi::av_max_alloc(i32::MAX as usize) };
            assert_eq!(got, expected, "limit={limit}");
            if limit <= len {
                // The first key's allocation fails; later ones (the value,
                // `av_opt_set`'s expression parser) may fail at larger
                // limits too.
                assert_eq!(expected, Err(-12), "limit={limit}");
            }
            assert_eq!(fields(&r.0), fields(&c.0), "limit={limit}");
            if limit == 4096 {
                assert_eq!(got, Ok(2));
            }
        }
    }

    /// With a NULL object, or one whose class is NULL, every key is not
    /// found, and an empty list sets nothing without reading the object.
    #[test]
    fn classless_objects_agree() {
        let mut classless = Ctx::new(&CLASS);
        classless.class = ptr::null();
        for obj in [ptr::null_mut(), classless.as_obj()] {
            for opts in [c"", c"int=5", c"a=b:c=d", c"int"] {
                let expected = c_set(obj, opts, c"=", c":");
                // SAFETY: a NULL object, or one with a NULL class, is within
                // the contract.
                let got = unsafe { set_options_string(obj, opts, c"=", c":") };
                assert_eq!(got, expected, "{opts:?}");
            }
        }
        assert_eq!(classless.int, 0);
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
