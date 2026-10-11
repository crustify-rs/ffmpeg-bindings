//! Equivalence of `libavutil/opt.h`'s owned range lists and their ranges
//! with the raw C queries and C's writes of the same fields, on
//! independently owned lists of the shared test classes (queried on a class,
//! as C's fake object, and on objects); and of option types with the values
//! C stores and reads.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;
use std::ffi::CString;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{
    ObjectRef, OptionRangeRef, OptionRangesBox, OptionRangesRef, RangeFlags,
};

#[path = "../common/opt.rs"]
pub(crate) mod fixture;
use fixture::{CB_FAKE, CB_KEYS, FAKE, FakeObj, Owned, RANGED, UNKNOWN, UNRANGED};

use crate::log::LogState;
use crate::log::class_fixture::with_class;

const FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
const NONE: RangeFlags = RangeFlags::NONE;
const MULTI: RangeFlags = RangeFlags::MULTI_COMPONENT;
const CHILDREN: RangeFlags = RangeFlags::CHILDREN;

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

/// What a query runs on: a fixture class, which C sees as the fake object
/// `fake`, or a live object of the well-formed `OBJ_CLASS`.
#[derive(Clone, Copy)]
enum On {
    Class(&'static FakeObj),
    Object(*mut c_void),
}

/// Runs the C query on `on`.
fn c_query(via: Via, on: On, key: &CStr, flags: RangeFlags) -> (c_int, CList) {
    let (obj, flags) = match on {
        On::Class(fake) => (fake.as_obj(), flags.bits() | FAKE_OBJ),
        On::Object(obj) => (obj, flags.bits()),
    };
    let mut out = CList(core::ptr::null_mut());
    // SAFETY: `out.0` is a writable slot; `obj` is a fake object of a
    // well-formed fixture class with `AV_OPT_SEARCH_FAKE_OBJ`, or an object.
    let ret = unsafe {
        match via {
            Via::Callback => ffi::av_opt_query_ranges(&mut out.0, obj, key.as_ptr(), flags),
            Via::Default => ffi::av_opt_query_ranges_default(&mut out.0, obj, key.as_ptr(), flags),
        }
    };
    (ret, out)
}

/// Runs the Rust query on `on`: on the class `with_class` reaches through
/// the fake object, or on the object's shared handle.
fn rust_query(via: Via, on: On, key: &CStr, flags: RangeFlags) -> Result<OptionRangesBox, i32> {
    match on {
        On::Class(fake) => {
            let _s = LogState::lock();
            // SAFETY: `fake` is a `const AVClass *` to a fixture class with a
            // name and no `item_name`, `get_category` or offsets, a context
            // `log_with_context` accepts.
            unsafe {
                with_class(fake.as_obj(), |c| match via {
                    Via::Callback => c.unwrap().option_ranges(key, flags),
                    Via::Default => c.unwrap().default_option_ranges(key, flags),
                })
            }
        }
        On::Object(obj) => {
            // SAFETY: each caller passes a live object of the well-formed
            // `OBJ_CLASS`, which nothing writes during the query.
            let obj = unsafe { ObjectRef::from_ptr(obj) }.unwrap();
            match via {
                Via::Callback => obj.option_ranges(key, flags),
                Via::Default => obj.default_option_ranges(key, flags),
            }
        }
    }
}

/// Runs both queries on `on` and asserts they agree; returns both lists.
#[track_caller]
fn both(via: Via, on: On, key: &CStr, flags: RangeFlags) -> (CList, Option<OptionRangesBox>) {
    let (ret, c) = c_query(via, on, key, flags);
    let rust = rust_query(via, on, key, flags);
    let ctx = format!("{via:?} {key:?} flags={:#x}", flags.bits());
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
fn every_key_and_flag_agrees_on_a_class() {
    for via in [Via::Callback, Via::Default] {
        for key in all_keys() {
            for flags in [NONE, MULTI, CHILDREN, MULTI | CHILDREN] {
                both(via, On::Class(&FAKE), key, flags);
            }
        }
    }
}

/// On objects of `OBJ_CLASS`, with and without a child to search.
#[test]
fn every_key_agrees_on_a_real_object() {
    let keys: Vec<&CStr> = fixture::OBJ_OPTIONS.0[..fixture::OBJ_OPTIONS.0.len() - 1]
        .iter()
        // SAFETY: each entry before the terminator names a literal.
        .map(|o| unsafe { CStr::from_ptr(o.name) })
        .chain(all_keys())
        .collect();
    for o in [Owned::new(), Owned::with_child()] {
        for via in [Via::Callback, Via::Default] {
            for key in &keys {
                for flags in [NONE, MULTI, CHILDREN, MULTI | CHILDREN] {
                    both(via, On::Object(o.as_obj()), key, flags);
                }
            }
        }
    }
}

#[test]
fn class_callback_lists_agree() {
    for key in all_keys() {
        for flags in [NONE, MULTI, CHILDREN] {
            let (c, rust) = both(Via::Callback, On::Class(&CB_FAKE), key, flags);
            // The default query bypasses the callback on the same class.
            both(Via::Default, On::Class(&CB_FAKE), key, flags);
            drop((c, rust));
        }
    }
    // The multi-component list really has three components of two ranges.
    let (_, rust) = both(Via::Callback, On::Class(&CB_FAKE), c"size", MULTI);
    let b = rust.unwrap();
    assert_eq!(
        (b.as_ref().range_count(), b.as_ref().component_count()),
        (2, 3)
    );
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
    let o = Owned::zeroed();
    let obj = On::Object(o.as_obj());
    // A list built before the limit drops stays valid and releasable.
    let (c_before, rust_before) = both(Via::Default, obj, c"int", NONE);

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
    both(Via::Default, obj, c"int", NONE);
    drop((c_before, rust_before));
}

/// Lists stay independent: each later query, and each drop, leaves every live
/// list of either side as it was.
#[test]
fn query_sequences_keep_lists_independent() {
    let o = Owned::new();
    let obj = On::Object(o.as_obj());
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
        let flags = if step % 2 == 0 { MULTI } else { NONE };
        let (c, rust) = both(via, obj, key, flags);
        let Some(rust) = rust else {
            // `bare` is a key of the older `CLASS` only.
            continue;
        };
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
    for key in [c"size", c"sparse", c"size"] {
        let (c, rust) = both(Via::Callback, On::Class(&CB_FAKE), key, MULTI);
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
    let cases: [(&'static FakeObj, &CStr, RangeFlags); 4] = [
        (&CB_FAKE, c"size", MULTI),
        (&CB_FAKE, c"sparse", NONE),
        (&FAKE, c"string", NONE),
        (&FAKE, c"size", MULTI),
    ];
    for (fake, key, flags) in cases {
        let (mut c, rust) = both(Via::Callback, On::Class(fake), key, flags);
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

/// Options wave 1, batch 0: the typed getters of `ObjectRef` against the raw
/// `av_opt_get_q`, `av_opt_get_video_rate`, `av_opt_get_pixel_fmt`,
/// `av_opt_get_sample_fmt`, `av_opt_get_chlayout`, `av_opt_get_dict_val` and
/// `av_opt_get_array`, on two independently owned objects given the same
/// writes, compared after every write; and `ClassRef::potential_children`
/// against `av_opt_child_class_iterate`.
mod getters {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::mem::size_of;
    use core::ptr;

    use ffmpeg::libavutil::channel_layout::ChannelLayoutVal;
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::ClassRef;
    use ffmpeg::libavutil::mem::HeapString;
    use ffmpeg::libavutil::opt::{ArrayElement, ObjectRef, OptionType, SearchFlags};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    use super::fixture::{ARR_CLASS, ARR_NAMES, ArrOwned, OBJ_CLASS, OBJ_OPTIONS, Owned};

    const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;

    /// Under BSan, C's number parses misreport (`av_strtod`'s end pointer):
    /// the object suites return early, as the wave-0 ones do.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    fn obj(p: *mut c_void) -> ObjectRef<'static> {
        // SAFETY: the fixture objects are well-formed and outlive every use
        // of the handle, during which only C's getters read them.
        unsafe { ObjectRef::from_ptr(p) }.unwrap()
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

    fn rational(q: ffi::AVRational) -> (i32, i32) {
        (q.num, q.den)
    }

    fn code<T>(r: &Result<T, i32>) -> i32 {
        r.as_ref().err().copied().unwrap_or(0)
    }

    /// `av_channel_layout_describe` of the layout at `l`.
    fn describe(l: *const ffi::AVChannelLayout) -> Vec<u8> {
        let mut buf = [0u8; 256];
        // SAFETY: `l` is a live layout and `buf` a writable buffer of its
        // length.
        let n = unsafe { ffi::av_channel_layout_describe(l, buf.as_mut_ptr().cast(), buf.len()) };
        assert!(n > 0 && (n as usize) < buf.len());
        buf[..n as usize - 1].to_vec()
    }

    fn rust_layout(l: &ChannelLayoutVal) -> Vec<u8> {
        const _: () = assert!(size_of::<ChannelLayoutVal>() == size_of::<ffi::AVChannelLayout>());
        // A `ChannelLayoutVal` is its `AVChannelLayout`, then a zero-sized
        // policy.
        describe(ptr::from_ref(l).cast())
    }

    /// `av_dict_get_string(m, '=', ';')`, or `None` for NULL.
    fn c_dict_string(m: *const ffi::AVDictionary) -> Option<Vec<u8>> {
        if m.is_null() {
            return None;
        }
        let mut buf: *mut c_char = ptr::null_mut();
        let (kv, pairs) = (b'=' as c_char, b';' as c_char);
        // SAFETY: a live dictionary and our slot.
        let ret = unsafe { ffi::av_dict_get_string(m, &mut buf, kv, pairs) };
        assert_eq!(ret, 0);
        // SAFETY: a fresh string we own, read then freed once.
        let s = unsafe { CStr::from_ptr(buf) }.to_bytes().to_vec();
        // SAFETY: as above.
        unsafe { ffi::av_free(buf.cast()) };
        Some(s)
    }

    fn rust_dict_string(d: &Dict) -> Option<Vec<u8>> {
        (!d.is_empty()).then(|| d.serialize(b'=', b';').unwrap().as_bytes().to_vec())
    }

    /// What each scalar getter reads of option `name` of `c` (raw C) and
    /// `r` (the wrappers): one line per getter.
    fn scalars(c: *mut c_void, r: ObjectRef<'_>, name: &CStr, children: bool) -> [Vec<String>; 2] {
        let (cf, rf) = flags_of(children);
        let n = name.as_ptr();
        let mut cs = Vec::new();
        let mut rs = Vec::new();

        let mut q = ffi::AVRational { num: 0, den: 0 };
        // SAFETY: a live well-formed object, a live name, a writable value.
        let ret = unsafe { ffi::av_opt_get_q(c, n, cf, &mut q) };
        cs.push(format!("q {ret} {:?}", (ret >= 0).then(|| rational(q))));
        let v = r.get_rational(name, rf);
        rs.push(format!(
            "q {} {:?}",
            code(&v),
            v.ok().map(|q| rational(q.into()))
        ));

        let mut q = ffi::AVRational { num: 0, den: 0 };
        // SAFETY: as above.
        let ret = unsafe { ffi::av_opt_get_video_rate(c, n, cf, &mut q) };
        let v = r.get_video_rate(name, rf);
        let is_rate = r.find_type(name, rf) == Some(OptionType::VIDEO_RATE);
        if is_rate {
            // C's defect: no reader for a video rate. The wrapper reads the
            // rate as `av_opt_get` formats it.
            assert_eq!(ret, EINVAL);
            let mut s = ptr::null_mut();
            // SAFETY: as above, with a writable string slot.
            assert_eq!(unsafe { ffi::av_opt_get(c, n, cf, &mut s) }, 0);
            // SAFETY: a fresh string we own, read then freed once.
            let text = unsafe { CStr::from_ptr(s.cast::<c_char>()) }
                .to_str()
                .unwrap()
                .to_owned();
            // SAFETY: as above.
            unsafe { ffi::av_free(s.cast()) };
            cs.push(format!("rate {text}"));
            let q: ffi::AVRational = v.unwrap().into();
            rs.push(format!("rate {}/{}", q.num, q.den));
        } else {
            cs.push(format!("rate {ret} {:?}", (ret >= 0).then(|| rational(q))));
            rs.push(format!(
                "rate {} {:?}",
                code(&v),
                v.ok().map(|q| rational(q.into()))
            ));
        }

        let mut p = ffi::AVPixelFormat(0);
        // SAFETY: as above.
        let ret = unsafe { ffi::av_opt_get_pixel_fmt(c, n, cf, &mut p) };
        cs.push(format!("pix {ret} {:?}", (ret >= 0).then_some(p.0)));
        let v = r.get_pixel_format(name, rf);
        rs.push(format!(
            "pix {} {:?}",
            code(&v),
            v.ok().map(|p| ffi::AVPixelFormat::from(p).0)
        ));

        let mut f = ffi::AVSampleFormat(0);
        // SAFETY: as above.
        let ret = unsafe { ffi::av_opt_get_sample_fmt(c, n, cf, &mut f) };
        cs.push(format!("sample {ret} {:?}", (ret >= 0).then_some(f.0)));
        let v = r.get_sample_format(name, rf);
        rs.push(format!(
            "sample {} {:?}",
            code(&v),
            v.ok().map(|f| ffi::AVSampleFormat::from(f).0)
        ));

        // SAFETY: an all-zero layout is the empty one C takes as output.
        let mut l: ffi::AVChannelLayout = unsafe { core::mem::zeroed() };
        // SAFETY: as above, with an initialised layout C may overwrite.
        let ret = unsafe { ffi::av_opt_get_chlayout(c, n, cf, &mut l) };
        cs.push(format!(
            "layout {ret} {:?}",
            (ret >= 0).then(|| describe(&l))
        ));
        // SAFETY: the copy C made (or the empty layout), disposed once.
        unsafe { ffi::av_channel_layout_uninit(&mut l) };
        let v = r.get_channel_layout(name, rf);
        rs.push(format!(
            "layout {} {:?}",
            code(&v),
            v.as_ref().ok().map(rust_layout)
        ));

        let mut d = ptr::null_mut();
        // SAFETY: as above, with a NULL dictionary slot.
        let ret = unsafe { ffi::av_opt_get_dict_val(c, n, cf, &mut d) };
        cs.push(format!(
            "dict {ret} {:?}",
            (ret >= 0).then(|| c_dict_string(d))
        ));
        // SAFETY: NULL or the copy C made, ours to free.
        unsafe { ffi::av_dict_free(&mut d) };
        let v = r.get_dict(name, rf);
        rs.push(format!(
            "dict {} {:?}",
            code(&v),
            v.as_ref().ok().map(rust_dict_string)
        ));
        [cs, rs]
    }

    /// The type of option `name`, as the wrappers find it.
    trait FindType {
        fn find_type(&self, name: &CStr, flags: SearchFlags) -> Option<OptionType>;
    }

    impl FindType for ObjectRef<'_> {
        fn find_type(&self, name: &CStr, flags: SearchFlags) -> Option<OptionType> {
            let me = *self;
            let search = |o: ObjectRef<'_>| {
                o.class()
                    .options()
                    .iter()
                    .find(|d| d.name() == Some(name) && d.option_type() != OptionType::CONST)
                    .map(|d| d.option_type())
            };
            if flags == SearchFlags::CHILDREN {
                for child in me.children() {
                    if let Some(t) = search(child) {
                        return Some(t);
                    }
                }
            }
            search(me)
        }
    }

    /// Writes applied to both objects, each followed by a comparison.
    const WRITES: [(&CStr, &CStr); 12] = [
        (c"q", c"3/7"),
        (c"rate", c"30000/1001"),
        (c"pix", c"rgb24"),
        (c"sample", c"fltp"),
        (c"layout", c"5.1(side)"),
        (c"layout", c"FL@a+FR@b"),
        (c"dict", c"a=1:A=2:b=3"),
        (c"dict", c""),
        (c"int", c"-5"),
        (c"dbl", c"1e300"),
        (c"dbl", c"-0.125"),
        (c"flt", c"0.3"),
    ];

    #[test]
    fn scalar_getters_agree_after_every_write() {
        if under_bsan() {
            return;
        }
        let (c, r) = (Owned::with_child(), Owned::with_child());
        let ro = obj(r.as_obj());
        let check = |step: &str| {
            for name in names() {
                for children in [false, true] {
                    let [a, b] = scalars(c.as_obj(), ro, name, children);
                    assert_eq!(a, b, "{name:?} children={children} after {step}");
                }
            }
        };
        check("defaults");
        for (i, (name, value)) in WRITES.iter().enumerate() {
            for (o, child) in [(&c, false), (&r, false), (&c, true), (&r, true)] {
                let target = if child { o.child().cast() } else { o.as_obj() };
                // SAFETY: a live object of the class (or its child), and
                // values C parses without undefined behaviour.
                let ret = unsafe { ffi::av_opt_set(target, name.as_ptr(), value.as_ptr(), 0) };
                assert_eq!(ret, 0, "{name:?}={value:?}");
            }
            check(&format!("write {i}"));
        }
    }

    /// One `av_opt_get_array` against `ObjectRef::get_array` for element type
    /// `T`, C writing `size` bytes per element of output type `ty`, read back
    /// by `c_show` (which releases what C allocated) and compared with
    /// `r_show` of each Rust element.
    #[allow(clippy::too_many_arguments)]
    fn array_case<T: ArrayElement>(
        c: *mut c_void,
        r: ObjectRef<'_>,
        name: &CStr,
        (start, count): (u32, u32),
        ty: ffi::AVOptionType,
        size: usize,
        c_show: impl Fn(*mut u8) -> String,
        r_show: impl Fn(&T) -> String,
    ) {
        let rust = r.get_array::<T>(name, SearchFlags::NONE, start, count);
        if code(&rust) == ERANGE {
            // Refused: C's `int64_t` conversion of one of the elements would
            // be undefined. Confirm with C's `double` read of them.
            let doubles = r
                .get_array::<f64>(name, SearchFlags::NONE, start, count)
                .unwrap();
            assert!(
                doubles
                    .iter()
                    .any(|d| !(-9.223_372_036_854_776e18..9.223_372_036_854_776e18).contains(d)),
                "{name:?} {start} {count}: {doubles:?}"
            );
            return;
        }
        // C writes only into a range that lies in the array, of at most four
        // elements here; zeroed, as C requires.
        let n = (count as usize).min(8);
        let mut buf = vec![0u64; (n * size).div_ceil(8)];
        let out = buf.as_mut_ptr().cast::<u8>();
        let n = name.as_ptr();
        // SAFETY: a live well-formed object, a live name, and a zeroed output
        // of `count` elements of `ty` whenever C writes (a range within the
        // array); the binary array is empty, so C reads no binary element.
        let ret = unsafe { ffi::av_opt_get_array(c, n, 0, start, count, ty, out.cast()) };
        let what = format!("{name:?} {start}+{count} as {}", ty.0);
        assert_eq!(code(&rust), ret, "{what}");
        if ret < 0 {
            return;
        }
        let cs: Vec<String> = (0..count as usize)
            .map(|i| {
                // SAFETY: C wrote `count` elements, each read once.
                c_show(unsafe { out.add(i * size) })
            })
            .collect();
        let rs: Vec<String> = rust.unwrap().iter().map(r_show).collect();
        assert_eq!(cs, rs, "{what}");
    }

    /// Every element type over every range of each array.
    fn arrays_agree(c: *mut c_void, r: ObjectRef<'_>, names: &[&CStr]) {
        use ffi::AVOptionType as T;
        for &name in names {
            let size = r.get_array_size(name, SearchFlags::NONE).unwrap_or(2) as u32;
            let ranges = [
                (0, 0),
                (0, 1),
                (0, size),
                (1, size.saturating_sub(1)),
                (size.saturating_sub(1), 1),
                (size, 0),
                (0, size + 1),
                (u32::MAX, 1),
            ];
            for range in ranges {
                macro_rules! plain {
                    ($t:ty, $ty:ident) => {
                        array_case::<$t>(
                            c,
                            r,
                            name,
                            range,
                            T::$ty,
                            size_of::<$t>(),
                            // SAFETY: an element C wrote, of this type.
                            |p| format!("{:?}", unsafe { p.cast::<$t>().read() }),
                            |v| format!("{v:?}"),
                        )
                    };
                }
                plain!(i32, AV_OPT_TYPE_INT);
                plain!(u32, AV_OPT_TYPE_UINT);
                plain!(i64, AV_OPT_TYPE_INT64);
                plain!(u64, AV_OPT_TYPE_UINT64);
                array_case::<f32>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_FLOAT,
                    4,
                    // SAFETY: an element C wrote, of this type.
                    |p| format!("{:#x}", unsafe { p.cast::<f32>().read() }.to_bits()),
                    |v| format!("{:#x}", v.to_bits()),
                );
                array_case::<f64>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_DOUBLE,
                    8,
                    // SAFETY: as above.
                    |p| format!("{:#x}", unsafe { p.cast::<f64>().read() }.to_bits()),
                    |v| format!("{:#x}", v.to_bits()),
                );
                array_case::<Rational>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_RATIONAL,
                    8,
                    |p| {
                        // SAFETY: an element C wrote, of this type.
                        let q = unsafe { p.cast::<ffi::AVRational>().read() };
                        format!("{:?}", rational(q))
                    },
                    |v| format!("{:?}", rational((*v).into())),
                );
                array_case::<PixelFormat>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_PIXEL_FMT,
                    4,
                    // SAFETY: as above.
                    |p| format!("{}", unsafe { p.cast::<c_int>().read() }),
                    |v| format!("{}", ffi::AVPixelFormat::from(*v).0),
                );
                array_case::<SampleFormat>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_SAMPLE_FMT,
                    4,
                    // SAFETY: as above.
                    |p| format!("{}", unsafe { p.cast::<c_int>().read() }),
                    |v| format!("{}", ffi::AVSampleFormat::from(*v).0),
                );
                array_case::<Option<HeapString>>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_STRING,
                    8,
                    |p| {
                        // SAFETY: a string element C wrote: NULL or one we own.
                        let s = unsafe { p.cast::<*mut c_char>().read() };
                        let text = (!s.is_null()).then(|| {
                            // SAFETY: as above, read then freed once.
                            unsafe { CStr::from_ptr(s) }.to_bytes().to_vec()
                        });
                        // SAFETY: as above.
                        unsafe { ffi::av_free(s.cast()) };
                        format!("{text:?}")
                    },
                    |v| format!("{:?}", v.as_ref().map(|s| s.as_bytes().to_vec())),
                );
                array_case::<Dict>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_DICT,
                    8,
                    |p| {
                        let slot = p.cast::<*mut ffi::AVDictionary>();
                        // SAFETY: a dictionary element C wrote, ours to free.
                        let text = c_dict_string(unsafe { slot.read() });
                        // SAFETY: as above.
                        unsafe { ffi::av_dict_free(slot) };
                        format!("{text:?}")
                    },
                    |v| format!("{:?}", rust_dict_string(v)),
                );
                array_case::<ChannelLayoutVal>(
                    c,
                    r,
                    name,
                    range,
                    T::AV_OPT_TYPE_CHLAYOUT,
                    size_of::<ffi::AVChannelLayout>(),
                    |p| {
                        let l = p.cast::<ffi::AVChannelLayout>();
                        let text = describe(l);
                        // SAFETY: a layout element C wrote, disposed once.
                        unsafe { ffi::av_channel_layout_uninit(l) };
                        format!("{text:?}")
                    },
                    |v| format!("{:?}", rust_layout(v)),
                );
            }
        }
    }

    #[test]
    fn array_reads_agree_for_every_type_and_range() {
        if under_bsan() {
            return;
        }
        let (c, r) = (ArrOwned::new(), ArrOwned::new());
        let names: Vec<&CStr> = ARR_NAMES.iter().copied().chain([c"missing"]).collect();
        arrays_agree(c.as_obj(), obj(r.as_obj()), &names);
        // `Obj`'s arrays, and its scalars, which are no arrays.
        let (c, r) = (Owned::new(), Owned::new());
        arrays_agree(
            c.as_obj(),
            obj(r.as_obj()),
            &[c"array", c"strings", c"empty", c"int", c"dict", c"layout"],
        );
    }

    /// Arrays rewritten through C's own setter on both objects, one array at
    /// a time, compared after each write.
    #[test]
    fn array_reads_agree_after_every_write() {
        if under_bsan() {
            return;
        }
        const WRITES: [(&CStr, &CStr); 10] = [
            (c"dbls", c"-0.5,2.5,-9e18,9.2e18"),
            (c"dbls", c""),
            (c"qs", c"-1/3,7/2"),
            (c"flts", c"1e-45,-2.5"),
            (c"strs", c"a\\,b,,c"),
            (c"dicts", c"x=1:X=2,,y=\\,"),
            (c"layouts", c"mono|FL@a+FR@b|ambisonic 1"),
            (c"pixs", c"nv12,gray"),
            (c"durs", c"2.5,1:00:00"),
            (c"i64s", c"-1,1,0"),
        ];
        let (c, r) = (ArrOwned::new(), ArrOwned::new());
        for (name, value) in WRITES {
            for o in [&c, &r] {
                // SAFETY: a live object of the class; values C parses without
                // undefined behaviour.
                let ret = unsafe { ffi::av_opt_set(o.as_obj(), name.as_ptr(), value.as_ptr(), 0) };
                assert_eq!(ret, 0, "{name:?}={value:?}");
            }
            arrays_agree(c.as_obj(), obj(r.as_obj()), &ARR_NAMES);
        }
    }

    /// `av_opt_child_class_iterate` from a NULL state until NULL.
    fn c_children(class: *const ffi::AVClass) -> Vec<*const ffi::AVClass> {
        let mut state = ptr::null_mut();
        let mut out = Vec::new();
        loop {
            // SAFETY: a live class and the state its callback alone advanced.
            let c = unsafe { ffi::av_opt_child_class_iterate(class, &mut state) };
            if c.is_null() {
                return out;
            }
            out.push(c);
        }
    }

    #[test]
    fn potential_children_agree() {
        let arr = ArrOwned::new();
        let plain = Owned::zeroed();
        for (o, raw) in [
            (arr.as_obj(), &raw const ARR_CLASS.0),
            (plain.as_obj(), &raw const OBJ_CLASS.0),
        ] {
            let c: Vec<&CStr> = c_children(raw)
                .into_iter()
                // SAFETY: each child is a live static class with a name.
                .map(|k| unsafe { CStr::from_ptr((*k).class_name) })
                .collect();
            let class: ClassRef<'_> = obj(o).class();
            let r: Vec<&CStr> = class
                .potential_children()
                .map(|k| k.name().unwrap())
                .collect();
            assert_eq!(c, r);
        }
        assert_eq!(c_children(&raw const ARR_CLASS.0).len(), 2);
    }
}

/// Options wave 1, batch 1: the typed setters, the array editors and the
/// dictionary appliers against the raw C calls, on independently owned
/// objects (with a child each) and values, comparing codes, every option as
/// C formats it, array sizes and leftover dictionaries after every step.
mod typed_setters {
    use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
    use core::ptr;

    use ffmpeg::libavutil::channel_layout::{ChannelLayout, ChannelLayoutVal};
    use ffmpeg::libavutil::dict::{Dict, DictionaryRef};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{ObjectMut, SearchFlags};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    use super::fixture::{ARRAYS_NAMES, Arrays, OBJ_OPTIONS, Owned, OwnedArrays};
    use ffi::AVOptionType as T;

    const ALLOW_NULL: c_int = ffi::AV_OPT_ALLOW_NULL as c_int;
    const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
    const REPLACE: c_int = ffi::AV_OPT_ARRAY_REPLACE as c_int;
    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;
    const ENOMEM: i32 = -12;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    fn flags_of(children: bool) -> (c_int, SearchFlags) {
        if children {
            (CHILDREN, SearchFlags::CHILDREN)
        } else {
            (0, SearchFlags::NONE)
        }
    }

    fn code(r: Result<(), i32>) -> i32 {
        r.err().unwrap_or(0)
    }

    /// `av_opt_get(obj, name, AV_OPT_ALLOW_NULL)` and, for arrays, the size.
    fn c_get(obj: *mut c_void, name: &CStr) -> (i32, Option<Vec<u8>>, i32, c_uint) {
        let mut out = ptr::null_mut();
        // SAFETY: a live well-formed object, a live name, a writable slot;
        // every binary length is consistent and binary arrays stay empty.
        let ret = unsafe { ffi::av_opt_get(obj, name.as_ptr(), ALLOW_NULL, &mut out) };
        let s = (!out.is_null()).then(|| {
            // SAFETY: a returned string is an owned C string, freed here.
            let s = unsafe { CStr::from_ptr(out.cast::<c_char>()) }
                .to_bytes()
                .to_vec();
            // SAFETY: as above.
            unsafe { ffi::av_free(out.cast()) };
            s
        });
        let mut n = 0;
        // SAFETY: as above, with a writable count.
        let size = unsafe { ffi::av_opt_get_array_size(obj, name.as_ptr(), 0, &mut n) };
        (ret, s, size, n)
    }

    /// What C reads of `objs`, option by option.
    fn observe(objs: &[*mut c_void], names: &[&CStr]) -> Vec<(i32, Option<Vec<u8>>, i32, c_uint)> {
        objs.iter()
            .flat_map(|&obj| names.iter().map(move |n| c_get(obj, n)))
            .collect()
    }

    /// The option names of `OBJ_CLASS`, and one that names nothing.
    fn obj_names() -> Vec<&'static CStr> {
        let mut v: Vec<&'static CStr> = OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) })
            .collect();
        v.push(c"missing");
        v
    }

    /// A dictionary's entries, in order, as C iterates them.
    fn c_entries(m: *const ffi::AVDictionary) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        let mut e = ptr::null();
        loop {
            // SAFETY: `m` is NULL or a live dictionary, `e` NULL or its
            // previous entry; nothing writes it during the walk.
            e = unsafe { ffi::av_dict_iterate(m, e) };
            if e.is_null() {
                return out;
            }
            // SAFETY: an entry of `m`, with live NUL-terminated strings.
            let (k, v) = unsafe { (CStr::from_ptr((*e).key), CStr::from_ptr((*e).value)) };
            out.push((k.to_bytes().to_vec(), v.to_bytes().to_vec()));
        }
    }

    /// `d`'s entries, read by C through its pointer.
    fn entries(d: &mut Dict) -> Vec<(Vec<u8>, Vec<u8>)> {
        let b = d.take();
        let seen = c_entries(b.as_ref().map_or(ptr::null(), |b| b.as_c_ptr()));
        *d = Dict::from(b);
        seen
    }

    /// A C dictionary of `pairs` (`AV_DICT_MULTIKEY`), NULL when empty.
    fn c_dict(pairs: &[(&CStr, &CStr)]) -> *mut ffi::AVDictionary {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: a live slot and strings; failures abort the test.
            let ret = unsafe {
                ffi::av_dict_set(
                    &mut m,
                    k.as_ptr(),
                    v.as_ptr(),
                    ffi::AV_DICT_MULTIKEY as c_int,
                )
            };
            assert!(ret >= 0);
        }
        m
    }

    fn c_dict_free(mut m: *mut ffi::AVDictionary) {
        // SAFETY: NULL or a dictionary this test built and owns.
        unsafe { ffi::av_dict_free(&mut m) };
    }

    /// A C layout parsed from `desc`, which the caller uninitialises.
    fn c_layout(desc: &CStr) -> ffi::AVChannelLayout {
        let mut l = ffi::AVChannelLayout::default();
        // SAFETY: a writable zeroed local and a live string.
        let ret = unsafe { ffi::av_channel_layout_from_string(&mut l, desc.as_ptr()) };
        assert_eq!(ret, 0, "{desc:?}");
        l
    }

    fn c_layout_uninit(mut l: ffi::AVChannelLayout) {
        // SAFETY: a layout `c_layout` built, owned here.
        unsafe { ffi::av_channel_layout_uninit(&mut l) };
    }

    type Pairs = &'static [(&'static CStr, &'static CStr)];

    /// The values of an array edit, of each element type.
    #[derive(Clone, Copy, Debug)]
    enum Vals {
        Str(&'static [&'static CStr]),
        OptStr(&'static [Option<&'static CStr>]),
        I32(&'static [i32]),
        I64(&'static [i64]),
        U32(&'static [u32]),
        U64(&'static [u64]),
        F32(&'static [f32]),
        F64(&'static [f64]),
        Q(&'static [(i32, i32)]),
        Pix(&'static [i32]),
        Sample(&'static [i32]),
        Size(&'static [(i32, i32)]),
        Color(&'static [[u8; 4]]),
        Layout(&'static [&'static CStr]),
        Dict(&'static [Option<Pairs>]),
    }

    /// One step of a sequence.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        Q(&'static CStr, (i32, i32), bool),
        Rate(&'static CStr, (i32, i32), bool),
        Pix(&'static CStr, i32, bool),
        Sample(&'static CStr, i32, bool),
        Layout(&'static CStr, &'static CStr, bool),
        DictVal(&'static CStr, Option<Pairs>, bool),
        Insert(&'static CStr, u32, Vals, bool),
        Replace(&'static CStr, u32, Vals, bool),
        Remove(&'static CStr, u32, u32, bool),
        FromDict(Pairs, Option<bool>),
    }

    /// `av_opt_set_array` of `vals` in C, building C copies of the values.
    fn c_array(obj: *mut c_void, name: &CStr, flags: c_int, start: u32, vals: Vals) -> i32 {
        macro_rules! call {
            ($ty:ident, $v:expr, $n:expr) => {
                // SAFETY: a live well-formed object and name; `$v` holds `$n`
                // values of the C type of `$ty`, live for the call; the
                // corpus holds only values C stores without undefined
                // behaviour.
                unsafe {
                    ffi::av_opt_set_array(
                        obj,
                        name.as_ptr(),
                        flags,
                        start,
                        $n as c_uint,
                        T::$ty,
                        $v,
                    )
                }
            };
        }
        match vals {
            Vals::Str(v) => {
                let p: Vec<*const c_char> = v.iter().map(|s| s.as_ptr()).collect();
                call!(AV_OPT_TYPE_STRING, p.as_ptr().cast(), p.len())
            }
            Vals::OptStr(v) => {
                let p: Vec<*const c_char> = v
                    .iter()
                    .map(|s| s.map_or(ptr::null(), CStr::as_ptr))
                    .collect();
                call!(AV_OPT_TYPE_STRING, p.as_ptr().cast(), p.len())
            }
            Vals::I32(v) => call!(AV_OPT_TYPE_INT, v.as_ptr().cast(), v.len()),
            Vals::I64(v) => call!(AV_OPT_TYPE_INT64, v.as_ptr().cast(), v.len()),
            Vals::U32(v) => call!(AV_OPT_TYPE_UINT, v.as_ptr().cast(), v.len()),
            Vals::U64(v) => call!(AV_OPT_TYPE_UINT64, v.as_ptr().cast(), v.len()),
            Vals::F32(v) => call!(AV_OPT_TYPE_FLOAT, v.as_ptr().cast(), v.len()),
            Vals::F64(v) => call!(AV_OPT_TYPE_DOUBLE, v.as_ptr().cast(), v.len()),
            Vals::Q(v) => {
                let p: Vec<ffi::AVRational> = v
                    .iter()
                    .map(|&(num, den)| ffi::AVRational { num, den })
                    .collect();
                call!(AV_OPT_TYPE_RATIONAL, p.as_ptr().cast(), p.len())
            }
            Vals::Pix(v) => call!(AV_OPT_TYPE_PIXEL_FMT, v.as_ptr().cast(), v.len()),
            Vals::Sample(v) => call!(AV_OPT_TYPE_SAMPLE_FMT, v.as_ptr().cast(), v.len()),
            Vals::Size(v) => {
                let p: Vec<[c_int; 2]> = v.iter().map(|&(w, h)| [w, h]).collect();
                call!(AV_OPT_TYPE_IMAGE_SIZE, p.as_ptr().cast(), p.len())
            }
            Vals::Color(v) => call!(AV_OPT_TYPE_COLOR, v.as_ptr().cast(), v.len()),
            Vals::Layout(v) => {
                let p: Vec<ffi::AVChannelLayout> = v.iter().map(|d| c_layout(d)).collect();
                let ret = call!(AV_OPT_TYPE_CHLAYOUT, p.as_ptr().cast(), p.len());
                p.into_iter().for_each(c_layout_uninit);
                ret
            }
            Vals::Dict(v) => {
                let p: Vec<*mut ffi::AVDictionary> = v
                    .iter()
                    .map(|d| d.map_or(ptr::null_mut(), c_dict))
                    .collect();
                let ret = call!(AV_OPT_TYPE_DICT, p.as_ptr().cast(), p.len());
                p.into_iter().for_each(c_dict_free);
                ret
            }
        }
    }

    /// The same edit through the wrapper, with Rust-owned values.
    fn rust_array(
        m: &mut ObjectMut<'_>,
        name: &CStr,
        replace: bool,
        start: u32,
        vals: Vals,
        f: SearchFlags,
    ) -> i32 {
        let start = start as usize;
        macro_rules! edit {
            ($v:expr) => {
                code(if replace {
                    m.replace_array(name, start, $v, f)
                } else {
                    m.insert_array(name, start, $v, f)
                })
            };
        }
        match vals {
            Vals::Str(v) => edit!(v),
            Vals::OptStr(v) => edit!(v),
            Vals::I32(v) => edit!(v),
            Vals::I64(v) => edit!(v),
            Vals::U32(v) => edit!(v),
            Vals::U64(v) => edit!(v),
            Vals::F32(v) => edit!(v),
            Vals::F64(v) => edit!(v),
            Vals::Q(v) => edit!(
                &v.iter()
                    .map(|&(n, d)| Rational::new(n, d))
                    .collect::<Vec<_>>()
            ),
            Vals::Pix(v) => edit!(
                &v.iter()
                    .map(|&p| PixelFormat::from_raw(ffi::AVPixelFormat(p)))
                    .collect::<Vec<_>>()
            ),
            Vals::Sample(v) => edit!(
                &v.iter()
                    .map(|&s| SampleFormat::from_raw(ffi::AVSampleFormat(s)))
                    .collect::<Vec<_>>()
            ),
            Vals::Size(v) => edit!(v),
            Vals::Color(v) => edit!(v),
            Vals::Layout(v) => {
                let owned: Vec<ChannelLayoutVal> =
                    v.iter().map(|d| ChannelLayout::parse(d).unwrap()).collect();
                edit!(&owned.iter().map(|l| l.as_ref()).collect::<Vec<_>>())
            }
            Vals::Dict(v) => {
                let owned: Vec<Dict> = v
                    .iter()
                    .map(|d| Dict::from_entries(d.unwrap_or(&[]).iter().copied()).unwrap())
                    .collect();
                edit!(
                    &owned
                        .iter()
                        .map(Dict::as_ref)
                        .collect::<Vec<Option<DictionaryRef<'_>>>>()
                )
            }
        }
    }

    fn c_apply(obj: *mut c_void, op: Op, leftover: &mut Vec<(Vec<u8>, Vec<u8>)>) -> i32 {
        let n = |name: &CStr| name.as_ptr();
        // SAFETY: each call gets a live, well-formed object, live strings and
        // values; the corpus holds only values C handles without undefined
        // behaviour.
        unsafe {
            match op {
                Op::Q(name, (num, den), c) => {
                    ffi::av_opt_set_q(obj, n(name), ffi::AVRational { num, den }, flags_of(c).0)
                }
                Op::Rate(name, (num, den), c) => ffi::av_opt_set_video_rate(
                    obj,
                    n(name),
                    ffi::AVRational { num, den },
                    flags_of(c).0,
                ),
                Op::Pix(name, p, c) => {
                    ffi::av_opt_set_pixel_fmt(obj, n(name), ffi::AVPixelFormat(p), flags_of(c).0)
                }
                Op::Sample(name, s, c) => {
                    ffi::av_opt_set_sample_fmt(obj, n(name), ffi::AVSampleFormat(s), flags_of(c).0)
                }
                Op::Layout(name, desc, c) => {
                    let l = c_layout(desc);
                    let ret = ffi::av_opt_set_chlayout(obj, n(name), &l, flags_of(c).0);
                    c_layout_uninit(l);
                    ret
                }
                Op::DictVal(name, pairs, c) => {
                    let d = pairs.map_or(ptr::null_mut(), c_dict);
                    let ret = ffi::av_opt_set_dict_val(obj, n(name), d, flags_of(c).0);
                    c_dict_free(d);
                    ret
                }
                Op::Insert(name, start, vals, c) => c_array(obj, name, flags_of(c).0, start, vals),
                Op::Replace(name, start, vals, c) => {
                    c_array(obj, name, flags_of(c).0 | REPLACE, start, vals)
                }
                Op::Remove(name, start, count, c) => ffi::av_opt_set_array(
                    obj,
                    n(name),
                    flags_of(c).0,
                    start,
                    count,
                    T::AV_OPT_TYPE_INT,
                    ptr::null(),
                ),
                Op::FromDict(pairs, flags) => {
                    let mut d = c_dict(pairs);
                    let ret = match flags {
                        None => ffi::av_opt_set_dict(obj, &mut d),
                        Some(c) => ffi::av_opt_set_dict2(obj, &mut d, flags_of(c).0),
                    };
                    *leftover = c_entries(d);
                    c_dict_free(d);
                    ret
                }
            }
        }
    }

    fn rust_apply(obj: *mut c_void, op: Op, leftover: &mut Vec<(Vec<u8>, Vec<u8>)>) -> i32 {
        // SAFETY: a live, well-formed object, accessed through this handle
        // only while it lives.
        let mut m = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
        match op {
            Op::Q(name, (num, den), c) => {
                code(m.set_rational(name, Rational::new(num, den), flags_of(c).1))
            }
            Op::Rate(name, (num, den), c) => {
                code(m.set_video_rate(name, Rational::new(num, den), flags_of(c).1))
            }
            Op::Pix(name, p, c) => code(m.set_pixel_format(
                name,
                PixelFormat::from_raw(ffi::AVPixelFormat(p)),
                flags_of(c).1,
            )),
            Op::Sample(name, s, c) => code(m.set_sample_format(
                name,
                SampleFormat::from_raw(ffi::AVSampleFormat(s)),
                flags_of(c).1,
            )),
            Op::Layout(name, desc, c) => {
                let l = ChannelLayout::parse(desc).unwrap();
                code(m.set_channel_layout(name, l.as_ref(), flags_of(c).1))
            }
            Op::DictVal(name, pairs, c) => {
                let d = Dict::from_entries(pairs.unwrap_or(&[]).iter().copied()).unwrap();
                code(m.set_dictionary(name, d.as_ref(), flags_of(c).1))
            }
            Op::Insert(name, start, vals, c) => {
                rust_array(&mut m, name, false, start, vals, flags_of(c).1)
            }
            Op::Replace(name, start, vals, c) => {
                rust_array(&mut m, name, true, start, vals, flags_of(c).1)
            }
            Op::Remove(name, start, count, c) => {
                code(m.remove_array(name, start as usize, count as usize, flags_of(c).1))
            }
            Op::FromDict(pairs, flags) => {
                let mut d = Dict::from_entries(pairs.iter().copied()).unwrap();
                let ret = code(match flags {
                    None => m.set_from_dict(&mut d),
                    Some(c) => m.set_from_dict_with(&mut d, flags_of(c).1),
                });
                *leftover = entries(&mut d);
                ret
            }
        }
    }

    /// The code the wrapper refuses an array edit with where C's would be
    /// undefined behaviour, by an independent model: an insertion into the
    /// binary array (C writes past each element), or a number converted to a
    /// rational or video rate element that is NaN, or within the option's
    /// bounds but beyond `int`.
    fn refused(op: Op) -> Option<i32> {
        let (name, vals) = match op {
            Op::Insert(n, _, v, _) | Op::Replace(n, _, v, _) => (n, v),
            // A scalar store over an array option, or a type mismatch message
            // that reads past `opt_type_desc` for it.
            Op::Q(n, ..)
            | Op::Rate(n, ..)
            | Op::Pix(n, ..)
            | Op::Sample(n, ..)
            | Op::Layout(n, ..)
            | Op::DictVal(n, ..)
                if [c"array", c"strings", c"empty"].contains(&n) =>
            {
                return Some(EINVAL);
            }
            _ => return None,
        };
        if name == c"bins" {
            return Some(EINVAL);
        }
        let (min, max) = match name.to_bytes() {
            b"qs" | b"rates" => (0.0, i32::MAX as f64),
            b"wide_qs" => (-1e300, 1e300),
            _ => return None,
        };
        let fits = |d: f64| d > -2_147_483_649.0 && d < 2_147_483_648.0;
        let bad = |d: f64| d.is_nan() || ((min..=max).contains(&d) && !fits(d));
        let any = match vals {
            Vals::I64(v) => v.iter().any(|&i| bad(i as f64)),
            Vals::F32(v) => v.iter().any(|&f| bad(f64::from(f))),
            Vals::F64(v) => v.iter().any(|&f| bad(f)),
            _ => false,
        };
        any.then_some(ERANGE)
    }

    /// Runs `ops` on a C-driven and a Rust-driven object pair, comparing
    /// codes, leftovers and what C reads of both after every step; `refused`
    /// steps are run on the Rust object only and must fail with the code
    /// given, leaving it as C observes it.
    fn run(
        c_objs: &[*mut c_void],
        r_objs: &[*mut c_void],
        names: &[&CStr],
        ops: &[(Op, Option<i32>)],
    ) {
        assert_eq!(observe(c_objs, names), observe(r_objs, names));
        for (i, &(op, refused)) in ops.iter().enumerate() {
            let step = format!("step {i}: {op:?}");
            let (mut c_left, mut r_left) = (Vec::new(), Vec::new());
            let before = observe(r_objs, names);
            let r = rust_apply(r_objs[0], op, &mut r_left);
            if let Some(err) = refused {
                assert_eq!(r, err, "{step}");
                assert_eq!(observe(r_objs, names), before, "{step}");
                continue;
            }
            assert_eq!(c_apply(c_objs[0], op, &mut c_left), r, "{step}");
            assert_eq!(r_left, c_left, "{step}");
            assert_eq!(observe(c_objs, names), observe(r_objs, names), "{step}");
        }
    }

    const STEREO_ONE: Pairs = &[(c"a", c"1"), (c"b", c"x y")];
    const ONE: Pairs = &[(c"k", c"v")];

    /// Scalar typed setters and dictionary appliers on `OBJ_CLASS`.
    fn scalar_ops() -> Vec<(Op, Option<i32>)> {
        let mut ops = Vec::new();
        for name in obj_names() {
            for c in [false, true] {
                ops.extend(
                    [
                        Op::Q(name, (3, 2), c),
                        Op::Q(name, (-7, 3), c),
                        Op::Q(name, (1, 0), c),
                        Op::Q(name, (0, 0), c),
                        Op::Q(name, (i32::MAX, -1), c),
                        Op::Rate(name, (30000, 1001), c),
                        Op::Rate(name, (25, 0), c),
                        Op::Pix(name, 3, c),
                        Op::Pix(name, -1, c),
                        Op::Pix(name, 99_999, c),
                        Op::Sample(name, 8, c),
                        Op::Sample(name, -2, c),
                        Op::Layout(name, c"5.1", c),
                        Op::Layout(name, c"FL@a+FR@b+LFE", c),
                        Op::DictVal(name, Some(STEREO_ONE), c),
                        Op::DictVal(name, None, c),
                    ]
                    .map(|op| (op, refused(op))),
                );
            }
        }
        // `av_opt_set_q` on an array stores over the array pointer, and the
        // other typed setters' type mismatch message on an array reads past
        // `opt_type_desc`: the generated steps on `array`, `strings` and
        // `empty` are refused (see `refused`).
        ops.extend([
            (
                Op::FromDict(
                    &[
                        (c"int", c"5"),
                        (c"string", c"s"),
                        (c"nosuch", c"1"),
                        (c"other", c"2"),
                    ],
                    None,
                ),
                None,
            ),
            (
                Op::FromDict(
                    &[(c"int", c"5"), (c"int", c"1000"), (c"nosuch", c"1")],
                    None,
                ),
                None,
            ),
            (
                Op::FromDict(
                    &[(c"flags", c"one+eight"), (c"layout", c"quad")],
                    Some(true),
                ),
                None,
            ),
            (Op::FromDict(&[(c"ro", c"1")], Some(false)), None),
            (Op::FromDict(&[], None), None),
            (
                Op::FromDict(&[(c"a", c"1"), (c"a", c"2")], Some(true)),
                None,
            ),
            (
                Op::FromDict(&[(c"dict", c"x=1:y=2"), (c"strings", c"p|q")], Some(false)),
                None,
            ),
            (
                Op::FromDict(&[(c"int", c"1"), (c"dbl", c"ld(1e300)")], None),
                Some(EINVAL),
            ),
        ]);
        ops
    }

    #[test]
    fn scalar_setters_and_dictionaries_agree() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class): the dictionary steps
        // parse numbers through C's own `av_opt_set`.
        if under_bsan() {
            return;
        }
        let (c, r) = (Owned::with_child(), Owned::with_child());
        let names = obj_names();
        run(
            &[c.as_obj(), c.child().cast()],
            &[r.as_obj(), r.child().cast()],
            &names,
            &scalar_ops(),
        );
    }

    /// `INT64_MAX` as a double, which C stores exactly.
    const I64_MAX_F: &[f64] = &[i64::MAX as f64];
    /// `UINT64_MAX` as a double, and a value between 2^63 and 2^64.
    const U64_HIGH_F: &[f64] = &[u64::MAX as f64, 1e19];

    /// Array edits of every value type on every array of `ARRAYS_CLASS`.
    fn array_ops() -> Vec<(Op, Option<i32>)> {
        use Vals::*;
        let all: [Vals; 15] = [
            Str(&[c"3", c"1/2", c"red", c"5.1"]),
            OptStr(&[None, Some(c"7")]),
            I32(&[5, -3]),
            I64(&[1 << 40, 9]),
            U32(&[7, u32::MAX]),
            U64(&[u64::MAX, 2]),
            F32(&[0.5, 0.25]),
            F64(&[2.5, -1e12]),
            Q(&[(1, 3), (5, 0)]),
            Pix(&[0, 3]),
            Sample(&[1, -1]),
            Size(&[(640, 480), (-1, 2)]),
            Color(&[[1, 2, 3, 4]]),
            Layout(&[c"stereo", c"FL@x+FR@y"]),
            Dict(&[Some(ONE), None, Some(STEREO_ONE)]),
        ];
        let mut ops = Vec::new();
        for name in ARRAYS_NAMES {
            if name == c"bins" {
                continue;
            }
            for (i, vals) in all.iter().enumerate() {
                let c = i % 3 == 0;
                ops.extend(
                    [
                        Op::Insert(name, 0, *vals, c),
                        Op::Replace(name, 1, *vals, c),
                        Op::Insert(name, 9, *vals, c),
                        Op::Remove(name, 1, 1, c),
                    ]
                    .map(|op| (op, refused(op))),
                );
            }
            ops.extend([
                (Op::Remove(name, 0, 2, false), None),
                (Op::Replace(name, 0, Vals::I32(&[]), true), None),
                (Op::Insert(name, 0, Vals::Str(&[]), false), None),
                (Op::Remove(name, 0, 100, false), None),
                (Op::Remove(name, 0, 0, true), None),
            ]);
        }
        ops.extend([
            // Undefined conversions in C, refused.
            (
                Op::Insert(c"qs", 0, Vals::F64(&[f64::NAN]), false),
                Some(ERANGE),
            ),
            (
                Op::Insert(c"wide_qs", 0, Vals::F64(&[0.5, 1e10]), false),
                Some(ERANGE),
            ),
            (
                Op::Replace(c"wide_qs", 0, Vals::I64(&[1 << 40]), true),
                Some(ERANGE),
            ),
            (
                Op::Insert(c"rates", 0, Vals::F32(&[f32::NAN]), false),
                Some(ERANGE),
            ),
            (
                Op::Insert(c"dbls", 0, Vals::Str(&[c"1", c"ld(1e300)"]), false),
                Some(EINVAL),
            ),
            (
                Op::Insert(c"colors", 0, Vals::Str(&[c"red@nan"]), false),
                Some(EINVAL),
            ),
            (
                Op::Insert(c"bins", 0, Vals::Str(&[c"0a"]), false),
                Some(EINVAL),
            ),
            (
                Op::Insert(c"bins", 0, Vals::OptStr(&[None]), false),
                Some(EINVAL),
            ),
            // `write_number`'s 64-bit extremes: `INT64_MAX` and `UINT64_MAX`
            // as doubles, and a `uint64_t` beyond the `int64_t` range.
            (Op::Insert(c"i64s", 0, Vals::F64(I64_MAX_F), false), None),
            (Op::Insert(c"durations", 0, Vals::F64(I64_MAX_F), false), None),
            (Op::Insert(c"u64s", 0, Vals::F64(U64_HIGH_F), false), None),
            // C's own failures.
            (
                Op::Insert(c"strs", 0, Vals::Str(&[c"x", c"y", c"z"]), false),
                None,
            ),
            (Op::Remove(c"strs", 0, 2, false), None),
            (
                Op::Insert(c"ints", 0, Vals::I32(&[1, 2, 3, 4, 5]), false),
                None,
            ),
            (Op::Insert(c"ints", 0, Vals::F64(&[1e9]), false), None),
            (
                Op::Insert(c"wide_qs", 0, Vals::F64(&[0.5, 2.5]), false),
                None,
            ),
            (
                Op::Insert(c"wide_qs", 0, Vals::I32(&[i32::MIN]), false),
                None,
            ),
            (
                Op::FromDict(
                    &[
                        (c"strs", c"p;q"),
                        (c"layouts", c"mono|quad"),
                        (c"nosuch", c"z"),
                    ],
                    Some(false),
                ),
                None,
            ),
            (
                Op::FromDict(&[(c"strs", c"p;q;r;s"), (c"ints", c"1")], Some(true)),
                None,
            ),
        ]);
        ops
    }

    #[test]
    fn array_edits_agree() {
        // BSan: as above, C's string parses reach the glibc end pointers.
        if under_bsan() {
            return;
        }
        let (c, r) = (OwnedArrays::with_child(), OwnedArrays::with_child());
        run(
            &[c.as_obj(), c.child().cast()],
            &[r.as_obj(), r.child().cast()],
            &ARRAYS_NAMES,
            &array_ops(),
        );
    }

    /// Random edits, without string parses, on zeroed objects, so that
    /// BorrowSanitizer runs them without C's evaluator (whose `av_strtod`
    /// end pointers it misreports).
    #[test]
    fn random_array_edits_agree() {
        let mut state = 0xfeed_face_0123_4567u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let pool: [Vals; 12] = [
            Vals::I32(&[5, -3, 0]),
            Vals::I64(&[1 << 40, 9]),
            Vals::U32(&[7]),
            Vals::U64(&[3, 4]),
            Vals::F32(&[0.5]),
            Vals::F64(&[2.5, 7.0]),
            Vals::Q(&[(1, 3), (2, 1)]),
            Vals::Pix(&[0]),
            Vals::Size(&[(1, 2), (3, 4)]),
            Vals::Color(&[[9, 9, 9, 9]]),
            Vals::Layout(&[c"mono", c"FL@x+FR@y"]),
            Vals::Dict(&[Some(ONE), None]),
        ];
        let names: Vec<&CStr> = ARRAYS_NAMES.into_iter().filter(|n| *n != c"bins").collect();
        let rounds = if under_bsan() { 40 } else { 600 };
        let ops: Vec<(Op, Option<i32>)> = (0..rounds)
            .map(|_| {
                let name = names[(next() % names.len() as u64) as usize];
                let (start, c) = ((next() % 4) as u32, next() % 2 == 0);
                let vals = pool[(next() % pool.len() as u64) as usize];
                let op = match next() % 3 {
                    0 => Op::Insert(name, start, vals, c),
                    1 => Op::Replace(name, start, vals, c),
                    _ => Op::Remove(name, start, (next() % 3) as u32, c),
                };
                (op, refused(op))
            })
            .collect();
        let (c, r) = (
            OwnedArrays::zeroed_with_child(),
            OwnedArrays::zeroed_with_child(),
        );
        run(
            &[c.as_obj(), c.child().cast()],
            &[r.as_obj(), r.child().cast()],
            &ARRAYS_NAMES,
            &ops,
        );
    }

    /// Allocation failures: C's own errors agree, and the array or option is
    /// left as C leaves it, except for the channel layout C leaves without a
    /// map (corrected; see the unit tests), which is compared once reset, and
    /// the array whose shrink failed, whose stale count C keeps (corrected;
    /// see `ub_unsafe/opt.rs::remove_array_shrink_failure`), which is
    /// compared once its count is repaired as the wrapper repairs it.
    #[test]
    fn allocation_failures_agree() {
        const KEY: &str = "CRUSTIFY_EQUIV_OPT_TYPED_ISOLATED";
        if under_bsan() {
            return;
        }
        if std::env::var_os(KEY).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "opt::typed_setters::allocation_failures_agree",
                    "--test-threads=1",
                ])
                .env(KEY, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }
        let set_limit = |limit: usize| {
            // SAFETY: `av_max_alloc` takes a size by value; nothing else runs
            // in this process.
            unsafe { ffi::av_max_alloc(limit) }
        };
        let names = ARRAYS_NAMES;
        for limit in [0, 1, 8, 16, 24, 48, 64, 128] {
            let (c, r) = (OwnedArrays::with_child(), OwnedArrays::with_child());
            let (co, ro) = (
                [c.as_obj(), c.child().cast()],
                [r.as_obj(), r.child().cast()],
            );
            // Steps whose values build without allocating.
            for op in [
                Op::Insert(c"strs", 1, Vals::Str(&[c"abcdefghijklmnop"]), false),
                Op::Insert(c"ints", 0, Vals::I32(&[1, 2, 3]), false),
                Op::Remove(c"ints", 0, 1, false),
                Op::Replace(c"layouts", 0, Vals::Str(&[c"FL@abc+FR@def"]), true),
            ] {
                let (mut cl, mut rl) = (Vec::new(), Vec::new());
                set_limit(limit);
                let cr = c_apply(co[0], op, &mut cl);
                let rr = rust_apply(ro[0], op, &mut rl);
                set_limit(i32::MAX as usize);
                assert_eq!(cr, rr, "{op:?} at {limit}");
                if let (Op::Remove(name, _, count, false), ENOMEM) = (op, cr) {
                    assert_eq!(name, c"ints");
                    // SAFETY: the live object's `ints` array, whose first
                    // `len - count` elements C left as the remaining ones.
                    unsafe { (*co[0].cast::<Arrays>()).ints.len -= count };
                }
                assert_eq!(
                    observe(&co, &names),
                    observe(&ro, &names),
                    "{op:?} at {limit}"
                );
            }
            // Layouts, dictionaries and applied dictionaries, built first.
            let c_layouts = [c_layout(c"FL@abc+FR@def+LFE@ghi")];
            let r_layouts = [ChannelLayout::parse(c"FL@abc+FR@def+LFE@ghi").unwrap()];
            let c_dicts = [c_dict(STEREO_ONE)];
            let r_dicts = [Dict::from_entries(STEREO_ONE.iter().copied()).unwrap()];
            let pairs: Pairs = &[(c"strs", c"x;y"), (c"nosuch", c"value")];
            let mut c_applied = c_dict(pairs);
            let mut r_applied = Dict::from_entries(pairs.iter().copied()).unwrap();
            // SAFETY: a live, well-formed object used through this handle only.
            let mut m = unsafe { ObjectMut::from_ptr(ro[0]) }.unwrap();
            set_limit(limit);
            // SAFETY: live objects, names and values of the C element types.
            let c_codes = unsafe {
                [
                    ffi::av_opt_set_array(
                        co[0],
                        c"layouts".as_ptr(),
                        REPLACE,
                        0,
                        1,
                        T::AV_OPT_TYPE_CHLAYOUT,
                        c_layouts.as_ptr().cast(),
                    ),
                    ffi::av_opt_set_array(
                        co[0],
                        c"dicts".as_ptr(),
                        CHILDREN,
                        0,
                        1,
                        T::AV_OPT_TYPE_DICT,
                        c_dicts.as_ptr().cast(),
                    ),
                    ffi::av_opt_set_dict(co[0], &mut c_applied),
                ]
            };
            let r_codes = [
                code(m.replace_array(c"layouts", 0, &[r_layouts[0].as_ref()], SearchFlags::NONE)),
                code(m.insert_array(c"dicts", 0, &[r_dicts[0].as_ref()], SearchFlags::CHILDREN)),
                code(m.set_from_dict(&mut r_applied)),
            ];
            set_limit(i32::MAX as usize);
            assert_eq!(c_codes, r_codes, "at {limit}");
            assert_eq!(c_entries(c_applied), entries(&mut r_applied), "at {limit}");
            assert_eq!(observe(&co, &names), observe(&ro, &names), "at {limit}");
            c_layouts.into_iter().for_each(c_layout_uninit);
            c_dicts.into_iter().for_each(c_dict_free);
            c_dict_free(c_applied);
        }
        // Scalar layouts: C's failed map copy leaves an unreadable layout,
        // which the reference resets as the wrapper does.
        for limit in [0, 16, 32] {
            let (c, r) = (Owned::new(), Owned::new());
            let l = c_layout(c"FL@abc+FR@def");
            let rl = ChannelLayout::parse(c"FL@abc+FR@def").unwrap();
            set_limit(limit);
            // SAFETY: live objects, name and layout.
            let cr = unsafe { ffi::av_opt_set_chlayout(c.as_obj(), c"layout".as_ptr(), &l, 0) };
            // SAFETY: a live, well-formed object used through this handle only.
            let mut m = unsafe { ObjectMut::from_ptr(r.as_obj()) }.unwrap();
            let rr = code(m.set_channel_layout(c"layout", rl.as_ref(), SearchFlags::NONE));
            set_limit(i32::MAX as usize);
            c_layout_uninit(l);
            assert_eq!(cr, rr, "at {limit}");
            if cr < 0 {
                // SAFETY: the live object's layout, which C left with a NULL
                // map and owns nothing.
                unsafe {
                    let p = &raw mut (*c.ptr()).layout;
                    assert!((*p).u.map.is_null());
                    p.write(ffi::AVChannelLayout::default());
                }
            }
            let names = obj_names();
            assert_eq!(
                observe(&[c.as_obj()], &names),
                observe(&[r.as_obj()], &names),
                "at {limit}"
            );
        }
    }
}

/// Options wave 1 review: a rational stored by `ObjectMut::insert_array`
/// converts as `av_opt_set_array` converts it, `llrint(1 / den) * num` into
/// an integer element, unlike `set_rational`'s rounded quotient into an
/// integer option: both pinned against C on independently owned objects.
mod rational_array_conversion {
    use core::ffi::{CStr, c_int, c_void};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{ObjectMut, SearchFlags};
    use ffmpeg::libavutil::rational::Rational;

    use super::fixture::OwnedArrays;

    const QS: [(i32, i32); 5] = [(3, 2), (7, 2), (-9, 4), (5, 1), (6, 1)];

    fn c_ints(obj: *mut c_void, name: &CStr) -> (i32, Vec<i64>) {
        let mut n = 0;
        // SAFETY: a live fixture object; `n` is a writable `unsigned`.
        let ret = unsafe { ffi::av_opt_get_array_size(obj, name.as_ptr(), 0, &mut n) };
        let mut out = vec![0i64; n as usize];
        let ty = ffi::AVOptionType::AV_OPT_TYPE_INT64;
        // SAFETY: `out` holds `n` zeroed `int64_t`s, the range lies in the
        // array, and integer elements convert to `int64_t` without UB.
        let got = unsafe {
            ffi::av_opt_get_array(obj, name.as_ptr(), 0, 0, n, ty, out.as_mut_ptr().cast())
        };
        (ret.min(got), out)
    }

    #[test]
    fn rationals_into_integer_arrays_and_options_convert_as_c() {
        let (c, r) = (
            OwnedArrays::zeroed_with_child(),
            OwnedArrays::zeroed_with_child(),
        );
        let raw: Vec<ffi::AVRational> = QS
            .iter()
            .map(|&(num, den)| ffi::AVRational { num, den })
            .collect();
        let qs: Vec<Rational> = QS
            .iter()
            .map(|&(num, den)| Rational::new(num, den))
            .collect();
        // SAFETY: a live fixture object reached through this handle only.
        let mut m = unsafe { ObjectMut::from_ptr(r.as_obj()) }.unwrap();
        for name in [c"ints", c"i64s"] {
            let ty = ffi::AVOptionType::AV_OPT_TYPE_RATIONAL;
            // SAFETY: a live fixture object and `QS.len()` `AVRational`s.
            let cret = unsafe {
                ffi::av_opt_set_array(
                    c.as_obj(),
                    name.as_ptr(),
                    0,
                    0,
                    QS.len() as u32,
                    ty,
                    raw.as_ptr().cast(),
                )
            };
            let rret = m.insert_array(name, 0, &qs, SearchFlags::NONE);
            assert_eq!((cret, rret), (0, Ok(())), "{name:?}");
            let cv = c_ints(c.as_obj(), name);
            let rv = m
                .as_ref()
                .get_array::<i64>(name, SearchFlags::NONE, 0, QS.len() as u32);
            assert_eq!(cv, (0, rv.clone().unwrap()), "{name:?}");
            // `llrint(1 / den) * num`: 1/2 and 1/4 round to 0.
            assert_eq!(rv, Ok(vec![0, 0, 0, 5, 6]), "{name:?}");
        }
        // The scalar setter rounds the quotient.
        for (k, &(num, den)) in QS.iter().enumerate() {
            let q = ffi::AVRational { num, den };
            // SAFETY: a live fixture object and a scalar `int` option.
            let cret = unsafe { ffi::av_opt_set_q(c.as_obj(), c"scalar".as_ptr(), q, 0) };
            let rret = m.set_rational(c"scalar", qs[k], SearchFlags::NONE);
            assert_eq!(rret.err().unwrap_or(0), cret, "{num}/{den}");
            let mut cv = 0i64;
            // SAFETY: as above; `cv` is writable.
            let got = unsafe { ffi::av_opt_get_int(c.as_obj(), c"scalar".as_ptr(), 0, &mut cv) };
            assert_eq!(got, 0 as c_int);
            assert_eq!(
                m.as_ref().get_int(c"scalar", SearchFlags::NONE),
                Ok(cv),
                "{num}/{den}"
            );
        }
        assert_eq!(m.as_ref().get_int(c"scalar", SearchFlags::NONE), Ok(6));
    }
}

/// Options wave 2, batch 0: option lookups on objects and on classes, the
/// walk of a class's options, the default check of one option and the
/// numeric evaluators, against the raw C on the same objects and classes
/// (C sees a class as the fake object `&class`): every option name with
/// every unit and option flag, defaults after each step of a set sequence,
/// and every option with every evaluator on values C evaluates without
/// undefined behaviour, flags accumulated across calls.
mod lookups {
    use core::ffi::{CStr, c_char, c_int, c_void};
    use core::ptr;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{EvalContext, ObjectMut, ObjectRef, OptionDefRef, SearchFlags};

    use super::fixture::{OBJ_CLASS, OBJ_OPTIONS, Obj, Owned};

    const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
    const FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
    const EINVAL: i32 = -22;

    /// What tells two table entries apart: the name, help and unit literals,
    /// the offset, the type and the flags.
    type Ident = (
        *const c_char,
        *const c_char,
        *const c_char,
        c_int,
        u32,
        c_int,
    );

    /// The entry at `o`, read raw; `None` for NULL.
    fn c_ident(o: *const ffi::AVOption) -> Option<Ident> {
        // SAFETY: NULL or an entry of a live static table.
        let o = unsafe { o.as_ref() }?;
        Some((o.name, o.help, o.unit, o.offset, o.type_.0, o.flags))
    }

    /// The entry behind `o`, through the safe getters.
    fn rust_ident(o: OptionDefRef<'_>) -> Ident {
        let p = |s: Option<&CStr>| s.map_or(ptr::null(), CStr::as_ptr);
        let ty = o.option_type().value();
        (
            p(o.name()),
            p(o.help()),
            p(o.unit()),
            o.offset(),
            ty,
            o.flags(),
        )
    }

    /// Every option name of `OBJ_CLASS`, constants included, and two that
    /// name nothing.
    fn names() -> Vec<&'static CStr> {
        let mut v: Vec<&'static CStr> = OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) })
            .collect();
        v.extend([c"missing", c""]);
        v
    }

    const UNITS: [Option<&CStr>; 6] = [
        None,
        Some(c"mode"),
        Some(c"fl"),
        Some(c"big"),
        Some(c"dunit"),
        Some(c"nosuch"),
    ];

    const OPT_FLAGS: [c_int; 5] = [
        0,
        ffi::AV_OPT_FLAG_READONLY as c_int,
        ffi::AV_OPT_FLAG_ENCODING_PARAM as c_int,
        ffi::AV_OPT_FLAG_CHILD_CONSTS as c_int,
        (ffi::AV_OPT_FLAG_READONLY | ffi::AV_OPT_FLAG_EXPORT) as c_int,
    ];

    /// The shared handle of `o`.
    fn shared(o: &Owned) -> ObjectRef<'_> {
        // SAFETY: a live object of the well-formed `OBJ_CLASS` (with its
        // child), which nothing writes while the handle lives.
        unsafe { ObjectRef::from_ptr(o.as_obj()) }.unwrap()
    }

    /// The exclusive handle of `o`.
    fn exclusive(o: &Owned) -> ObjectMut<'_> {
        // SAFETY: as for `shared`, used through no other handle meanwhile.
        unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
    }

    /// `OBJ_CLASS` as C's fake object: a pointer to its class pointer.
    struct Fake(*const ffi::AVClass);

    impl Fake {
        fn new() -> Self {
            Self(&OBJ_CLASS.0)
        }
        fn obj(&self) -> *mut c_void {
            (&raw const self.0).cast_mut().cast()
        }
    }

    /// An object with a child whose `int` differs from its own, so that a
    /// found target tells the two apart.
    fn parent_and_child() -> Owned {
        let o = Owned::with_child();
        // SAFETY: raw writes of the live child's field, no handle live.
        unsafe { (*o.child()).int = 9 };
        o
    }

    /// The `int` field C's target holds.
    fn int_at(target: *mut c_void) -> c_int {
        // SAFETY: every target here is a live `Obj`.
        unsafe { (*target.cast::<Obj>()).int }
    }

    #[test]
    fn finds_agree_on_objects_and_classes() {
        let fake = Fake::new();
        for o in [Owned::new(), parent_and_child()] {
            let obj = shared(&o);
            let class = obj.class();
            for name in names() {
                for unit in UNITS {
                    let unit_ptr = unit.map_or(ptr::null(), CStr::as_ptr);
                    for opt_flags in OPT_FLAGS {
                        for (sf, flags) in
                            [(0, SearchFlags::NONE), (CHILDREN, SearchFlags::CHILDREN)]
                        {
                            let ctx = format!("{name:?} {unit:?} {opt_flags:#x} {sf:#x}");
                            let (n, u) = (name.as_ptr(), unit_ptr);
                            // SAFETY: a live well-formed object, live strings.
                            let c = unsafe { ffi::av_opt_find(o.as_obj(), n, u, opt_flags, sf) };
                            let rust = obj.find(name, unit, opt_flags, flags);
                            assert_eq!(rust.map(rust_ident), c_ident(c), "{ctx}");

                            let mut target = ptr::null_mut();
                            // SAFETY: as above; `target` is a writable slot.
                            let c2 = unsafe {
                                ffi::av_opt_find2(o.as_obj(), n, u, opt_flags, sf, &mut target)
                            };
                            let rust2 = obj.find_with_target(name, unit, opt_flags, flags);
                            assert_eq!(rust2.map(|(o, _)| rust_ident(o)), c_ident(c2), "{ctx}");
                            if let Some((_, t)) = rust2 {
                                let int = t.get_int(c"int", SearchFlags::NONE).unwrap();
                                assert_eq!(int, i64::from(int_at(target)), "{ctx}");
                                assert!(t.class().ptr_eq(class));
                            }

                            // SAFETY: the fake object of the well-formed
                            // class, with `AV_OPT_SEARCH_FAKE_OBJ`.
                            let c3 = unsafe {
                                ffi::av_opt_find(fake.obj(), n, u, opt_flags, sf | FAKE_OBJ)
                            };
                            let rust3 = class.find(name, unit, opt_flags, flags);
                            assert_eq!(rust3.map(rust_ident), c_ident(c3), "{ctx}");
                        }
                    }
                }
            }
        }
    }

    /// A class search with children visits the classes
    /// `child_class_iterate` lists: `ARR_CLASS` lists `OBJ_CLASS`, then
    /// itself, so only names `OBJ_CLASS` has are looked up with children (C
    /// would recurse into `ARR_CLASS` again for any other).
    #[test]
    fn class_finds_search_potential_children_as_c() {
        let o = super::fixture::ArrOwned::new();
        // SAFETY: a live, defaulted object of the well-formed `ARR_CLASS`.
        let class = unsafe { ObjectRef::from_ptr(o.as_obj()) }.unwrap().class();
        let child = Owned::new();
        let child_class = shared(&child).class();
        let fake: *const ffi::AVClass = &super::fixture::ARR_CLASS.0;
        let fake_obj = (&raw const fake).cast_mut().cast::<c_void>();
        let mut found = 0;
        for name in names() {
            for unit in UNITS {
                for opt_flags in OPT_FLAGS {
                    if child_class
                        .find(name, unit, opt_flags, SearchFlags::NONE)
                        .is_none()
                    {
                        // C would go on into `ARR_CLASS` again.
                        continue;
                    }
                    found += 1;
                    let (n, u) = (name.as_ptr(), unit.map_or(ptr::null(), CStr::as_ptr));
                    let flags = CHILDREN | FAKE_OBJ;
                    // SAFETY: the fake object of a well-formed class; the
                    // first potential child has the option.
                    let c = unsafe { ffi::av_opt_find(fake_obj, n, u, opt_flags, flags) };
                    let rust = class.find(name, unit, opt_flags, SearchFlags::CHILDREN);
                    assert_eq!(rust.map(rust_ident), c_ident(c), "{name:?} {unit:?}");
                    assert!(rust.is_some());
                }
            }
        }
        assert!(found > 0);
    }

    #[test]
    fn options_walk_agrees_with_av_opt_next() {
        let o = Owned::new();
        let fake = Fake::new();
        let mut c_walk = Vec::new();
        let mut prev = ptr::null();
        loop {
            // SAFETY: the fake object of a well-formed class; `prev` is NULL
            // or the entry returned last.
            prev = unsafe { ffi::av_opt_next(fake.obj(), prev) };
            let Some(ident) = c_ident(prev) else { break };
            c_walk.push(ident);
        }
        let rust: Vec<_> = shared(&o).options().map(rust_ident).collect();
        assert_eq!(rust, c_walk);
        assert_eq!(rust.len(), OBJ_OPTIONS.0.len() - 1);
    }

    /// Option values set one step at a time, each a value C parses.
    const STEPS: [(&CStr, &CStr); 30] = [
        (c"int", c"8"),
        (c"int", c"7"),
        (c"string", c"x"),
        (c"string", c"hello"),
        (c"q", c"1/3"),
        (c"q", c"1/2"),
        (c"flags", c"+eight"),
        (c"size", c"hd720"),
        (c"rate", c"30"),
        (c"color", c"blue"),
        (c"binary", c"0a0c"),
        (c"binary", c"0a0b"),
        (c"dict", c"a=b"),
        (c"dict", c"k=v"),
        (c"array", c"3"),
        (c"strings", c"x"),
        (c"layout", c"mono"),
        (c"pix", c"rgb24"),
        (c"sample", c"s16"),
        (c"duration", c"2"),
        (c"uint", c"10"),
        (c"big", c"high"),
        (c"nostr", c"n"),
        (c"wide_q", c"3"),
        (c"flt", c"0.5"),
        (c"dbl", c"2.5"),
        (c"int64", c"0"),
        (c"boolean", c"false"),
        (c"layout", c"stereo"),
        (c"array", c"1,2"),
    ];

    /// `is_option_set_to_default` and `av_opt_is_set_to_default` on every
    /// entry of the parent and of its child, after each step.
    #[test]
    fn defaults_agree_after_every_step() {
        let o = parent_and_child();
        let check = |step: &str| {
            let obj = shared(&o);
            let child = obj.children().next().unwrap();
            for (i, e) in obj.options().enumerate() {
                let c_e = &raw const OBJ_OPTIONS.0[i];
                for (r, c_obj) in [(obj, o.as_obj()), (child, o.child().cast())] {
                    // SAFETY: a live object of `OBJ_CLASS` and an entry of its
                    // table; binary lengths stay consistent.
                    let c = unsafe { ffi::av_opt_is_set_to_default(c_obj, c_e) };
                    let rust = r.is_option_set_to_default(e);
                    assert_eq!(
                        rust.map(c_int::from),
                        if c < 0 { Err(c) } else { Ok(c) },
                        "{step} {i}"
                    );
                }
            }
        };
        check("defaults");
        for (name, val) in STEPS {
            exclusive(&o).set(name, val, SearchFlags::NONE).unwrap();
            check(&format!("{name:?}={val:?}"));
        }
        // An option of the child, found through the parent, checked on the
        // child, as C's `av_opt_is_set_to_default_by_name` does.
        let obj = shared(&o);
        let (int, target) = obj
            .find_with_target(c"int", None, 0, SearchFlags::CHILDREN)
            .unwrap();
        assert_eq!(target.is_option_set_to_default(int), Ok(false));
        assert_eq!(
            obj.is_set_to_default(c"int", SearchFlags::CHILDREN),
            Ok(false)
        );
    }

    /// Values the evaluators parse without undefined behaviour in C, for
    /// every type, constants of the fixture's units and errors included.
    const VALUES: [&CStr; 34] = [
        c"0",
        c"1",
        c"-1",
        c"7",
        c"seven",
        c"seven+1",
        c"default",
        c"max",
        c"min",
        c"none",
        c"all",
        c"1/3",
        c"3:2",
        c"30000/1001",
        c"2/0",
        c"0/0",
        c"1e3",
        c"1k",
        c"0x10",
        c"-1e300",
        c"1e300",
        c"4294967295",
        c"9223372036854775807",
        c"high",
        c"mixed",
        c"low",
        c"dflag",
        c"one",
        c"huge",
        c"0.25",
        c"abc",
        c"",
        c"1.5",
        c"-5e9",
    ];

    /// One evaluation's code and result bits (NaN compares by bits).
    type Eval = (i32, u64);

    /// Every evaluator on `val` as option `e` (C's `c_e`), in context `ctx`
    /// (C's `c_obj`): Rust's results, then C's.
    fn evaluate(
        e: OptionDefRef<'_>,
        c_e: *const ffi::AVOption,
        ctx: EvalContext<'_>,
        c_obj: *mut c_void,
        val: &CStr,
    ) -> (Vec<Eval>, Vec<Eval>) {
        fn code<T>(r: Result<T, i32>, bits: impl FnOnce(T) -> u64) -> Eval {
            match r {
                Ok(v) => (0, bits(v)),
                Err(e) => (e, 0),
            }
        }
        fn c_code(ret: c_int, bits: u64) -> Eval {
            if ret < 0 { (ret, 0) } else { (0, bits) }
        }
        let mut f = 3;
        let rust = vec![
            code(e.eval_flags(ctx, val, &mut f).map(|()| f), |v| {
                v as u32 as u64
            }),
            code(e.eval_int(ctx, val), |v| v as u32 as u64),
            code(e.eval_uint(ctx, val), u64::from),
            code(e.eval_int64(ctx, val), |v| v as u64),
            code(e.eval_float(ctx, val), |v| u64::from(v.to_bits())),
            code(e.eval_double(ctx, val), f64::to_bits),
            code(e.eval_rational(ctx, val), |q| {
                let q = q.into_raw();
                (q.num as u32 as u64) << 32 | q.den as u32 as u64
            }),
        ];
        let v = val.as_ptr();
        let (mut fl, mut i, mut u, mut i64_, mut fv, mut d) = (3, 0, 0, 0, 0.0, 0.0);
        let mut q = ffi::AVRational { num: 0, den: 0 };
        // SAFETY: a live object of `OBJ_CLASS`, or the class's fake object
        // for an option without child constants; an entry of its table; a
        // live value no refusal applies to; writable outputs.
        let c = unsafe {
            vec![
                c_code(
                    ffi::av_opt_eval_flags(c_obj, c_e, v, &mut fl),
                    fl as u32 as u64,
                ),
                c_code(ffi::av_opt_eval_int(c_obj, c_e, v, &mut i), i as u32 as u64),
                c_code(ffi::av_opt_eval_uint(c_obj, c_e, v, &mut u), u64::from(u)),
                c_code(
                    ffi::av_opt_eval_int64(c_obj, c_e, v, &mut i64_),
                    i64_ as u64,
                ),
                c_code(
                    ffi::av_opt_eval_float(c_obj, c_e, v, &mut fv),
                    u64::from(fv.to_bits()),
                ),
                c_code(ffi::av_opt_eval_double(c_obj, c_e, v, &mut d), d.to_bits()),
                c_code(
                    ffi::av_opt_eval_q(c_obj, c_e, v, &mut q),
                    (q.num as u32 as u64) << 32 | q.den as u32 as u64,
                ),
            ]
        };
        (rust, c)
    }

    /// Whether the evaluator of option `name`'s type refuses `val` before C
    /// reaches an unchecked conversion: a rational within wide bounds whose
    /// value is outside the `int` range, or a flag operand outside the
    /// `int64_t` range. C must not see these.
    fn refused(name: &CStr, val: &CStr) -> bool {
        let wide: [&CStr; 7] = [
            c"max",
            c"min",
            c"1e300",
            c"-1e300",
            c"4294967295",
            c"9223372036854775807",
            c"-5e9",
        ];
        match name.to_bytes() {
            b"wide_q" => wide.contains(&val),
            b"flags" | b"cflags" => val == c"-1e300",
            _ => false,
        }
    }

    #[test]
    fn evaluators_agree_on_objects_and_classes() {
        let o = parent_and_child();
        let obj = shared(&o);
        let fake = Fake::new();
        for (i, e) in obj.options().enumerate() {
            let c_e = &raw const OBJ_OPTIONS.0[i];
            let child_consts = e.flags() & ffi::AV_OPT_FLAG_CHILD_CONSTS as c_int != 0;
            for val in VALUES {
                let name = e.name().unwrap();
                if refused(name, val) {
                    let mut f = 0;
                    let r = match name.to_bytes() {
                        b"wide_q" => e.eval_rational(obj, val).map(drop),
                        _ => e.eval_flags(obj, val, &mut f),
                    };
                    assert_eq!(r, Err(-34), "{name:?} = {val:?}");
                    continue;
                }
                let (rust, c) = evaluate(e, c_e, obj.into(), o.as_obj(), val);
                assert_eq!(rust, c, "object: {name:?} = {val:?}");
                if child_consts {
                    // C would hand the fake object to `child_next`.
                    let mut f = 0;
                    assert_eq!(e.eval_flags(obj.class(), val, &mut f), Err(EINVAL));
                    continue;
                }
                let (rust, c) = evaluate(e, c_e, obj.class().into(), fake.obj(), val);
                assert_eq!(rust, c, "class: {name:?} = {val:?}");
            }
        }
    }

    /// Flag tokens applied one call after another to one value on each side,
    /// compared after every step, failing tokens' partial writes included.
    #[test]
    fn flag_sequences_agree_after_every_step() {
        let o = parent_and_child();
        let obj = shared(&o);
        let fake = Fake::new();
        let steps = [
            c"+one",
            c"+bit",
            c"-one",
            c"eight",
            c"+one+bit-eight",
            c"+nosuch",
            c"+eight+abc",
            c"all",
            c"-all",
            c"+1.5",
            c"0x3",
            c"-1",
            c"+one",
            c"huge",
            c"none",
            c"+4294967295",
            c"one-",
            c"+",
            c"",
        ];
        for (cflags, c_obj, ctx) in [
            (c"flags", o.as_obj(), EvalContext::from(obj)),
            (c"cflags", o.as_obj(), EvalContext::from(obj)),
            (c"flags", fake.obj(), EvalContext::from(obj.class())),
        ] {
            let e = obj.find(cflags, None, 0, SearchFlags::NONE).unwrap();
            let i = obj
                .options()
                .position(|x| x.name() == Some(cflags))
                .unwrap();
            let c_e = &raw const OBJ_OPTIONS.0[i];
            let (mut rust, mut c) = (3, 3);
            for val in steps {
                let r = e.eval_flags(ctx, val, &mut rust);
                // SAFETY: as in `evaluate`.
                let ret = unsafe { ffi::av_opt_eval_flags(c_obj, c_e, val.as_ptr(), &mut c) };
                assert_eq!(r.err().unwrap_or(0), ret, "{cflags:?} {val:?}");
                assert_eq!(rust, c, "{cflags:?} {val:?}");
            }
        }
    }

    /// Flags values longer than C's 255-byte token buffer, which C cuts into
    /// tokens it evaluates one by one: the values C evaluates safely agree
    /// with C after every call on one flags value per side; a value with a
    /// cut token whose evaluation C converts unchecked is refused alone
    /// (options wave-2 review,
    /// `ub_safe/eval-flags-token-split-hides-evaluator-call`).
    #[test]
    fn long_flag_values_are_cut_and_refused_as_tokens() {
        // BSan misreads C's reads through glibc `strtod` / `strtol` end
        // pointers (`eval.c:120`, `parseutils.c:166`) in these evaluations
        // and the fixture's defaults, as in the other object suites.
        if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            return;
        }
        /// `head`, padded with `fill` to `pad` bytes, then `tail`.
        fn long(head: &str, fill: u8, pad: usize, tail: &str) -> std::ffi::CString {
            let mut v = head.as_bytes().to_vec();
            v.resize(pad, fill);
            v.extend(tail.as_bytes());
            std::ffi::CString::new(v).unwrap()
        }
        let o = parent_and_child();
        let obj = shared(&o);
        let fake = Fake::new();
        let agreed = [
            long("0*", b'0', 255, "1"),
            long("0*", b'0', 255, "ld(1)"),
            long("+0*", b'0', 256, "one"),
            long("-0*", b'0', 256, "+bit"),
            long("", b'0', 254, "1eight"),
            long("", b' ', 255, "one"),
            long("0*", b'0', 255, "abc"),
            long("0*", b'0', 300, "+one"),
            long("eight", b' ', 255, "-eight"),
        ];
        let refused = [
            long("0*", b'0', 255, "ld(1e300)"),
            long("+0*", b'0', 256, "ld(-1e300)"),
            long("0*", b'0', 255, "print(1,1e300)"),
            long("0*", b'0', 255, "taylor(1,2,1e300)"),
        ];
        for (cflags, c_obj, ctx) in [
            (c"flags", o.as_obj(), EvalContext::from(obj)),
            (c"cflags", o.as_obj(), EvalContext::from(obj)),
            (c"flags", fake.obj(), EvalContext::from(obj.class())),
        ] {
            let e = obj.find(cflags, None, 0, SearchFlags::NONE).unwrap();
            let i = obj
                .options()
                .position(|x| x.name() == Some(cflags))
                .unwrap();
            let c_e = &raw const OBJ_OPTIONS.0[i];
            let (mut rust, mut c) = (3, 3);
            for val in &agreed {
                let r = e.eval_flags(ctx, val, &mut rust);
                // SAFETY: as in `evaluate`.
                let ret = unsafe { ffi::av_opt_eval_flags(c_obj, c_e, val.as_ptr(), &mut c) };
                assert_eq!(r.err().unwrap_or(0), ret, "{cflags:?} {val:?}");
                assert_eq!(rust, c, "{cflags:?} {val:?}");
            }
            for val in &refused {
                let before = rust;
                assert_eq!(e.eval_flags(ctx, val, &mut rust), Err(EINVAL), "{val:?}");
                assert_eq!(rust, before);
            }
        }
    }

    /// A dictionary option whose default C cannot parse (a key with no
    /// value): both default checks fail with C's `av_dict_parse_string`
    /// error, before and after a value is set.
    #[test]
    fn unparsable_dictionary_defaults_fail_as_c() {
        use super::fixture::{Class, Table};

        #[repr(C)]
        struct DictObj {
            class: *const ffi::AVClass,
            dict: *mut ffi::AVDictionary,
        }

        static OPTIONS: Table<2> = Table([
            ffi::AVOption {
                name: c"dict".as_ptr(),
                help: ptr::null(),
                offset: core::mem::offset_of!(DictObj, dict) as c_int,
                type_: ffi::AVOptionType::AV_OPT_TYPE_DICT,
                default_val: ffi::AVOption__bindgen_ty_1 {
                    str_: c"nokey".as_ptr(),
                },
                min: 0.0,
                max: 0.0,
                flags: 0,
                unit: ptr::null(),
            },
            ffi::AVOption {
                name: ptr::null(),
                help: ptr::null(),
                offset: 0,
                type_: ffi::AVOptionType(0),
                default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
                min: 0.0,
                max: 0.0,
                flags: 0,
                unit: ptr::null(),
            },
        ]);
        static CLASS: Class = Class(ffi::AVClass {
            class_name: c"crustify-opt-bad-dict".as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            option: &OPTIONS.0[0],
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

        let o = Box::into_raw(Box::new(DictObj {
            class: &CLASS.0,
            dict: ptr::null_mut(),
        }));
        let obj = o.cast::<c_void>();
        let c_o = &raw const OPTIONS.0[0];
        for step in [None, Some(c"k=v")] {
            {
                // SAFETY: a live object of the well-formed class above (its
                // default fails to parse, which is no undefined behaviour),
                // used through this handle only while it lives.
                let mut handle = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
                if let Some(val) = step {
                    handle.set(c"dict", val, SearchFlags::NONE).unwrap();
                }
            }
            // SAFETY: as above, shared while C reads too.
            let handle = unsafe { ObjectRef::from_ptr(obj) }.unwrap();
            let o_rust = handle.find(c"dict", None, 0, SearchFlags::NONE).unwrap();
            // SAFETY: the live object and an entry of its class's table.
            let c = unsafe { ffi::av_opt_is_set_to_default(obj, c_o) };
            // SAFETY: as above.
            let c_named = unsafe { ffi::av_opt_is_set_to_default_by_name(obj, c"dict".as_ptr(), 0) };
            assert_eq!(c, EINVAL);
            assert_eq!(handle.is_option_set_to_default(o_rust), Err(c));
            assert_eq!(handle.is_set_to_default(c"dict", SearchFlags::NONE), Err(c_named));
        }
        {
            // SAFETY: as above.
            let mut handle = unsafe { ObjectMut::from_ptr(obj) }.unwrap();
            handle.free_options();
        }
        // SAFETY: the box allocated above, its dictionary freed.
        drop(unsafe { Box::from_raw(o) });
    }
}
