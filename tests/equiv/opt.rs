//! Equivalence of `libavutil/opt.h`'s owned range lists with the raw C
//! queries, on independently owned lists of the shared test class.

use core::ffi::{CStr, c_int, c_void};
use core::mem::size_of;
use std::ffi::CString;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{OptionRanges, OptionRangesBox, OptionRangesRef};

#[path = "../common/opt.rs"]
mod fixture;
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

/// Reads the range at `p`, NULL included.
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

/// The Rust list behind `r`, read through the safe getters.
fn rust_list(r: OptionRangesRef<'_>) -> List {
    List {
        nb_ranges: r.range_count(),
        nb_components: r.component_count(),
        ranges: r.ranges().elems().map(range).collect(),
    }
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
