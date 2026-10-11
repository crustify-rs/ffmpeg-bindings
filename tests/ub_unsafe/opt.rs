//! UB tests for `libavutil/opt.h`'s unsafe range queries, each called within
//! its `# Safety` contract, and for the safe getters and setters of the lists
//! and ranges they return.

use core::ffi::{CStr, c_int};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{OptionRangeRef, OptionRanges, OptionRangesBox, OptionRangesRef};

#[path = "../common/opt.rs"]
pub(crate) mod fixture;
use fixture::{CB_FAKE, CLASS, Ctx, EINVAL, FAKE, FakeObj, RANGED, UNKNOWN, UNRANGED};

const FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
const MULTI: c_int = ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int;
const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
const ENOMEM: i32 = -12;
const ENOSYS: i32 = -38;

/// Every field of `e`, through the safe getters.
fn fields(e: OptionRangeRef<'_>) -> (Option<&CStr>, [f64; 4], bool) {
    let bounds = [
        e.value_min(),
        e.value_max(),
        e.component_min(),
        e.component_max(),
    ];
    (e.label(), bounds, e.is_range())
}

/// Reads every count, element, bound and string of `r`, through the safe
/// getters; returns the element count.
fn read_all(r: OptionRangesRef<'_>) -> usize {
    let (n, c) = (r.range_count(), r.component_count());
    let all: Vec<_> = r.ranges().collect();
    assert_eq!(all.len(), n * c);
    assert_eq!(r.ranges().len(), n * c);
    for comp in 0..c {
        for i in 0..n {
            let e = r.get(i, comp);
            assert_eq!(e.is_none(), all[comp * n + i].is_none());
            let Some(e) = e else { continue };
            assert_eq!(fields(e), fields(all[comp * n + i].unwrap()));
            assert!(e.value_min() <= e.value_max());
            if let Some(label) = e.label() {
                assert!(!label.to_bytes().is_empty());
            }
        }
    }
    assert!(r.get(n, 0).is_none() && r.get(0, c).is_none());
    assert!(r.ranges().nth(n * c).is_none());
    n * c
}

/// `OptionRanges::query`, within its contract: a live object (or fake object)
/// of a well-formed class with a non-NULL class pointer.
mod query {
    use super::*;

    fn query(obj: &'static FakeObj, key: &CStr, flags: c_int) -> Result<OptionRangesBox, i32> {
        // SAFETY: `obj` is a `const AVClass *` to a well-formed fixture class,
        // and `flags` carries `AV_OPT_SEARCH_FAKE_OBJ`.
        unsafe { OptionRanges::query(obj.as_obj(), key, flags | FAKE_OBJ) }
    }

    #[test]
    fn every_ranged_key_on_a_fake_object() {
        for key in RANGED {
            for flags in [0, MULTI, CHILDREN] {
                let b = query(&FAKE, key, flags).unwrap();
                assert_eq!(read_all(b.as_ref()), 1);
            }
        }
    }

    #[test]
    fn rejected_keys_own_nothing() {
        for key in UNRANGED {
            assert_eq!(query(&FAKE, key, 0).err(), Some(ENOSYS), "{key:?}");
        }
        for key in UNKNOWN {
            assert_eq!(query(&FAKE, key, MULTI).err(), Some(ENOMEM), "{key:?}");
        }
        assert_eq!(query(&CB_FAKE, c"fail", MULTI).err(), Some(EINVAL));
    }

    #[test]
    fn real_object_with_children_search() {
        let mut ctx = Ctx::new(&CLASS);
        for key in RANGED {
            for flags in [0, MULTI, CHILDREN] {
                // SAFETY: `ctx` is a live object of the well-formed `CLASS`,
                // which has no children to iterate.
                let b = unsafe { OptionRanges::query(ctx.as_obj(), key, flags) }.unwrap();
                read_all(b.as_ref());
            }
        }
    }

    #[test]
    fn callback_lists_free_every_owned_range_and_string() {
        // Two ranges of three components, each range owning its `str`;
        // LeakSanitizer reports any range or string the drop misses.
        let b = query(&CB_FAKE, c"size", MULTI).unwrap();
        assert_eq!(read_all(b.as_ref()), 6);
        // Without the multi-component flag, one component.
        let one = query(&CB_FAKE, c"size", 0).unwrap();
        assert_eq!(read_all(one.as_ref()), 2);
        // A NULL element reads as NULL and is skipped by the release.
        let sparse = query(&CB_FAKE, c"sparse", 0).unwrap();
        assert_eq!(read_all(sparse.as_ref()), 3);
        assert!(sparse.as_ref().get(1, 0).is_none());
        assert!(sparse.as_ref().ranges().nth(1).unwrap().is_none());
        // Drop out of creation order.
        drop(one);
        drop(b);
        drop(sparse);
    }

    #[test]
    fn views_and_copies_outlive_handles_but_not_the_owner() {
        let b = query(&CB_FAKE, c"size", MULTI).unwrap();
        // An iterator taken through a temporary handle keeps the owner's
        // borrow, and so do the range handles and strings it yields.
        let view = b.as_ref().ranges();
        let handles: Vec<_> = view.map(Option::unwrap).collect();
        let labels: Vec<_> = handles.iter().map(|e| e.label()).collect();
        let again: Vec<_> = b.as_ref().ranges().map(|e| fields(e.unwrap())).collect();
        assert_eq!(
            handles.iter().map(|&e| fields(e)).collect::<Vec<_>>(),
            again
        );
        assert!(labels.iter().all(|l| *l == Some(c"range")));
        // Iterating from both ends meets in the middle and stays within the
        // run.
        let mut it = view;
        assert_eq!(it.nth(1).map(|e| e.unwrap().value_min()), Some(1.0));
        assert_eq!(it.nth_back(1).map(|e| e.unwrap().value_min()), Some(4.0));
        assert_eq!(it.len(), 2);
        assert_eq!(it.by_ref().count(), 2);
        assert!(it.next().is_none() && it.next_back().is_none());
        drop(b);
    }

    /// A list and its handles cross threads: one is read by several threads
    /// at once through copies of a shared handle, moved to another thread and
    /// dropped there, and queried on many threads concurrently (`Send` /
    /// `Sync` on `OptionRanges`; ThreadSanitizer reports any race).
    #[test]
    fn lists_are_read_moved_and_dropped_across_threads() {
        let b = query(&CB_FAKE, c"size", MULTI).unwrap();
        let shared: &OptionRangesBox = &b;
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    let r = shared.as_ref();
                    for _ in 0..32 {
                        assert_eq!(read_all(r), 6);
                        assert_eq!(read_all(shared.as_ref()), 6);
                    }
                });
            }
            let r: OptionRangesRef<'_> = b.as_ref();
            assert_eq!(s.spawn(move || read_all(r)).join().unwrap(), 6);
        });
        // Dropped on a thread other than the one that queried it.
        std::thread::spawn(move || drop(b)).join().unwrap();

        // Built on one thread, read on another, then returned to the first.
        let back = std::thread::spawn(|| {
            let sparse = query(&CB_FAKE, c"sparse", 0).unwrap();
            assert_eq!(read_all(sparse.as_ref()), 3);
            sparse
        })
        .join()
        .unwrap();
        assert!(back.as_ref().get(1, 0).is_none());

        // Concurrent queries of one shared class, each list owned by its
        // thread.
        let totals: Vec<usize> = std::thread::scope(|s| {
            let workers: Vec<_> = (0..8)
                .map(|i| {
                    s.spawn(move || {
                        let key = [c"size", c"sparse", c"int", c"string"][i % 4];
                        let b = query(&CB_FAKE, key, if i % 2 == 0 { MULTI } else { 0 }).unwrap();
                        read_all(b.as_ref())
                    })
                })
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        assert_eq!(totals, [6, 3, 1, 1, 6, 3, 1, 1]);
    }

    #[test]
    fn many_lists_live_at_once() {
        let lists: Vec<_> = (0..64)
            .map(|i| {
                let key = [c"size", c"sparse", c"int", c"string"][i % 4];
                query(&CB_FAKE, key, if i % 2 == 0 { MULTI } else { 0 }).unwrap()
            })
            .collect();
        let total: usize = lists.iter().map(|b| read_all(b.as_ref())).sum();
        assert_eq!(total, 16 * (6 + 3 + 1 + 1));
    }
}

/// `OptionRanges::query_default`, within its contract, which also admits a
/// NULL class pointer and skips the class callback.
mod query_default {
    use super::*;

    fn query_default(
        obj: &'static FakeObj,
        key: &CStr,
        flags: c_int,
    ) -> Result<OptionRangesBox, i32> {
        // SAFETY: `obj` is a `const AVClass *` to a well-formed fixture class,
        // or NULL, and `flags` carries `AV_OPT_SEARCH_FAKE_OBJ`.
        unsafe { OptionRanges::query_default(obj.as_obj(), key, flags | FAKE_OBJ) }
    }

    #[test]
    fn every_key_one_range_of_one_component() {
        for key in RANGED {
            for flags in [0, MULTI, CHILDREN] {
                let b = query_default(&FAKE, key, flags).unwrap();
                let r = b.as_ref();
                assert_eq!(
                    (r.range_count(), r.component_count(), read_all(r)),
                    (1, 1, 1)
                );
            }
        }
        for key in UNRANGED {
            assert_eq!(query_default(&FAKE, key, 0).err(), Some(ENOSYS));
        }
        for key in UNKNOWN {
            assert_eq!(query_default(&FAKE, key, 0).err(), Some(ENOMEM));
        }
    }

    #[test]
    fn callback_is_bypassed() {
        // `"size"` is an image size: the default list has one component even
        // when the class callback would give three.
        let b = query_default(&CB_FAKE, c"size", MULTI).unwrap();
        assert_eq!(b.as_ref().component_count(), 1);
        // The callback's own keys are unknown options to the default query.
        assert_eq!(query_default(&CB_FAKE, c"fail", 0).err(), Some(ENOMEM));
    }

    #[test]
    fn null_class_fails_without_a_list() {
        static NULL_CLASS: FakeObj = FakeObj(core::ptr::null());
        assert_eq!(query_default(&NULL_CLASS, c"int", 0).err(), Some(ENOMEM));

        let mut ctx = Ctx::new(&CLASS);
        ctx.class = core::ptr::null();
        // SAFETY: `ctx` is a live object whose class pointer is NULL, which
        // `query_default` admits.
        let r = unsafe { OptionRanges::query_default(ctx.as_obj(), c"int", 0) };
        assert_eq!(r.err(), Some(ENOMEM));
    }

    #[test]
    fn null_object_fails_without_a_list() {
        for flags in [0, FAKE_OBJ, MULTI | CHILDREN, FAKE_OBJ | CHILDREN] {
            // SAFETY: `query_default` admits a NULL `obj`, with any flags.
            let r = unsafe { OptionRanges::query_default(core::ptr::null_mut(), c"int", flags) };
            assert_eq!(r.err(), Some(ENOMEM), "flags={flags:#x}");
        }
    }

    #[test]
    fn real_object() {
        let mut ctx = Ctx::new(&CLASS);
        for key in RANGED {
            // SAFETY: `ctx` is a live object of the well-formed `CLASS`.
            let b = unsafe { OptionRanges::query_default(ctx.as_obj(), key, MULTI) }.unwrap();
            read_all(b.as_ref());
        }
    }
}

/// The range handles of lists the queries return, within their contracts:
/// rewriting bounds, replacing, taking and clearing owned strings, across
/// threads and drops.
mod range_handles {
    use super::*;
    use ffmpeg::libavutil::mem::{HeapString, string_from_c_str};

    fn query(obj: &'static FakeObj, key: &CStr, flags: c_int) -> OptionRangesBox {
        // SAFETY: `obj` is a `const AVClass *` to a well-formed fixture class,
        // and `flags` carries `AV_OPT_SEARCH_FAKE_OBJ`.
        unsafe { OptionRanges::query(obj.as_obj(), key, flags | FAKE_OBJ) }.unwrap()
    }

    fn heap(s: &CStr) -> Option<HeapString> {
        Some(string_from_c_str(s).unwrap())
    }

    /// Every element of a callback list gets a new label and bounds; the
    /// drop then releases the new strings (and the replaced ones were
    /// released at once): LeakSanitizer reports a miss, ASan a double free.
    #[test]
    fn labels_replaced_on_every_element_are_released_once() {
        let mut b = query(&CB_FAKE, c"size", MULTI);
        let mut m = b.as_mut();
        for comp in 0..3 {
            for i in 0..2 {
                let mut e = m.get_mut(i, comp).unwrap();
                e.set_label(heap(c"replaced"));
                e.set_value_min(-1.0);
                e.set_value_max(f64::from(u8::MAX));
                e.set_is_range(comp == 1);
            }
        }
        let r = b.as_ref();
        assert_eq!(read_all(r), 6);
        for e in r.ranges().map(Option::unwrap) {
            assert_eq!(e.label(), Some(c"replaced"));
            assert_eq!((e.value_min(), e.value_max()), (-1.0, 255.0));
        }
        drop(b);
    }

    /// A label taken out of a list outlives it; the list then frees none.
    #[test]
    fn taken_labels_outlive_the_list() {
        let mut b = query(&CB_FAKE, c"size", 0);
        let mut taken = Vec::new();
        for i in 0..2 {
            taken.push(b.as_mut().get_mut(i, 0).unwrap().take_label().unwrap());
            assert!(b.as_mut().get_mut(i, 0).unwrap().take_label().is_none());
        }
        assert!(b.as_ref().ranges().all(|e| e.unwrap().label().is_none()));
        drop(b);
        assert!(taken.iter().all(|s| s.as_c_str() == c"range"));
        // A taken label moves to another list, which frees it in turn.
        let mut other = query(&FAKE, c"int", 0);
        let mut m = other.as_mut();
        let mut e = m.get_mut(0, 0).unwrap();
        e.set_label(taken.pop());
        assert_eq!(e.as_ref().label(), Some(c"range"));
        drop(other);
        drop(taken);
    }

    /// Default lists have NULL labels; setting, clearing and resetting one
    /// on a list that started without strings keeps the release balanced.
    #[test]
    fn default_list_labels_set_cleared_and_reset() {
        for key in RANGED {
            let mut b = query(&FAKE, key, MULTI);
            let mut m = b.as_mut();
            let mut e = m.get_mut(0, 0).unwrap();
            assert_eq!(e.as_ref().label(), None);
            e.set_label(None);
            e.set_label(heap(key));
            e.set_label(None);
            e.set_label(heap(c""));
            assert_eq!(e.as_ref().label(), Some(c""));
            assert!(m.get_mut(1, 0).is_none() && m.get_mut(0, 1).is_none());
        }
    }

    /// NULL elements of a sparse callback list have no exclusive handle, and
    /// rewriting their neighbours leaves them NULL.
    #[test]
    fn null_elements_stay_null_around_writes() {
        let mut b = query(&CB_FAKE, c"sparse", 0);
        assert!(b.as_mut().get_mut(1, 0).is_none());
        for i in [0, 2] {
            let mut m = b.as_mut();
            let mut e = m.get_mut(i, 0).unwrap();
            e.set_label(heap(c"kept"));
            e.set_component_min(1.0);
            e.set_component_max(2.0);
        }
        let holes: Vec<bool> = b.as_ref().ranges().map(|e| e.is_none()).collect();
        assert_eq!(holes, [false, true, false]);
        assert_eq!(read_all(b.as_ref()), 3);
    }

    /// A list is rewritten on one thread through its exclusive handle, read
    /// by several at once through copies of range handles, then rewritten
    /// and dropped on others (`Send` / `Sync` on `OptionRange` and its
    /// handles; ThreadSanitizer reports any race).
    #[test]
    fn ranges_are_written_read_and_moved_across_threads() {
        let mut b = query(&CB_FAKE, c"size", MULTI);
        let mut b = std::thread::spawn(move || {
            b.as_mut().get_mut(1, 2).unwrap().set_label(heap(c"sent"));
            b
        })
        .join()
        .unwrap();
        let r = b.as_ref();
        let e = r.get(1, 2).unwrap();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    for _ in 0..32 {
                        assert_eq!(e.label(), Some(c"sent"));
                        assert_eq!(read_all(r), 6);
                    }
                });
            }
            let iter = r.ranges();
            assert_eq!(s.spawn(move || iter.count()).join().unwrap(), 6);
        });
        // An exclusive range handle moves to a scoped thread and writes there.
        let mut m = b.as_mut();
        let e = m.get_mut(0, 0).unwrap();
        std::thread::scope(|s| {
            s.spawn(move || {
                let mut e = e;
                e.set_label(heap(c"scoped"));
                e.set_value_max(9.0);
            });
        });
        assert_eq!(b.as_ref().get(0, 0).unwrap().label(), Some(c"scoped"));
        std::thread::spawn(move || drop(b)).join().unwrap();
    }
}

/// `ObjectMut::set_options_string` on handles `ObjectMut::from_ptr` adopts
/// within its contract (the options wave-0 batch-1 review made the former
/// free `unsafe fn set_options_string(ctx: *mut c_void, ..)` this safe
/// method): objects of the well-formed `OBJ_CLASS` (and a parent reaching
/// one as its child), every option type, owned values replaced again and
/// again, lists at the edges of the token syntax, and the refusals of
/// values whose parse is undefined in C.
mod set_options_string {
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr::{null, null_mut};
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::{BareObject, ObjectMut};

    use super::fixture::{OBJ_CLASS, Obj, Owned, Table};

    /// `AVERROR_OPTION_NOT_FOUND`: `FFERRTAG(0xF8, 'O', 'P', 'T')`.
    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);
    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// The exclusive handle of `o`.
    fn exclusive(o: &Owned) -> ObjectMut<'_> {
        // SAFETY: a live object of the well-formed `OBJ_CLASS`, accessed
        // through no other handle while this one lives.
        unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
    }

    /// `set_options_string` with `=` and `:`.
    fn set(o: &Owned, opts: &CStr) -> Result<usize, i32> {
        exclusive(o).set_options_string(opts, c"=", c":")
    }

    /// The object's fields, read raw while no handle is live.
    fn obj(o: &Owned) -> &Obj {
        // SAFETY: the live object, which nothing writes while the borrow
        // lasts (each caller drops it before the next call).
        unsafe { &*o.ptr() }
    }

    /// The string field's value.
    fn string(c: &Obj) -> Option<&CStr> {
        // SAFETY: the field is NULL or a live string C stored.
        (!c.string.is_null()).then(|| unsafe { CStr::from_ptr(c.string) })
    }

    #[test]
    fn every_option_type() {
        if under_bsan() {
            return;
        }
        let o = Owned::new();
        let all = c"int=5:int64=-3:uint64=18446744073709551615:uint=4:dbl=-1e9:flt=1:\
            string=s:q=7/2:wide_q=-2:boolean=true:flags=bit:big=high:size=1x1:rate=pal:\
            color=0xff00ff80:duration=0.000001:binary=00ff:dict=k=v:array=0,10:strings=x|y:\
            empty=:layout=mono:pix=gray:sample=s16:nostr=n";
        assert_eq!(set(&o, all), Ok(25));
        let c = obj(&o);
        assert_eq!((c.int, c.int64, c.uint64, c.uint), (5, -3, u64::MAX, 4));
        assert_eq!((c.dbl, c.flt), (-1e9, 1.0));
        assert_eq!(string(c), Some(c"s"));
        assert_eq!(
            (c.q.num, c.q.den, c.wide_q.num, c.wide_q.den),
            (7, 2, -2, 1)
        );
        assert_eq!((c.boolean, c.flags, c.big), (1, 2, 1 << 40));
        assert_eq!((c.size, c.rate.num, c.rate.den), ([1, 1], 25, 1));
        assert_eq!((c.color, c.duration), ([255, 0, 255, 128], 1));
        assert_eq!(c.binary_len, 2);
        assert!(!c.dict.is_null());
        assert_eq!((c.array_len, c.strings_len), (2, 2));
        // An empty array value stores no block.
        assert_eq!(c.empty_len, 0);
        assert!(c.empty.is_null() && !c.nostr.is_null());
        assert_eq!(c.layout.nb_channels, 1);
    }

    /// Owned values replaced many times, with values of growing size, and
    /// cleared: ASan sees every block C frees and allocates, LSan any left.
    #[test]
    fn owned_values_replaced_and_cleared() {
        if under_bsan() {
            return;
        }
        let o = Owned::new();
        for round in 0..64usize {
            let s = "x".repeat(round * 7);
            let hex = "ab".repeat(round);
            let arr = vec!["3"; round % 11].join(",");
            let opts = format!(
                "string={s}:binary={hex}:dict=k{round}=v{s}:array={arr}:nostr={s}:strings={s}|{s}"
            );
            let opts = CString::new(opts).unwrap();
            assert_eq!(set(&o, &opts), Ok(6));
            let c = obj(&o);
            assert_eq!(string(c).unwrap().count_bytes(), round * 7);
            assert_eq!(c.binary_len as usize, round);
            assert_eq!(c.array_len as usize, round % 11);
        }
        assert_eq!(set(&o, c"string=:binary=:array=:strings="), Ok(4));
        let c = obj(&o);
        assert_eq!(string(c), Some(c""));
        assert!(c.binary.is_null() && c.array.is_null() && c.strings.is_null());
    }

    /// Lists in exactly sized heap strings, so ASan sees any read past
    /// their NUL; the separators likewise.
    #[test]
    fn exactly_sized_lists() {
        if under_bsan() {
            return;
        }
        let lists: [&[u8]; 16] = [
            b"",
            b"i",
            b"=",
            b":",
            b"int=",
            b"int=1:",
            b"int=1::",
            b"int='1",
            b"int=1\\",
            b"int\\",
            b"'int'=1",
            b"  int  =  2  ",
            b"string='a'b'c':int=3",
            b"string=\\'",
            b"flags=+1e30",
            b"color=red@nan",
        ];
        for list in lists {
            let opts = CString::new(list).unwrap().into_boxed_c_str();
            let kv = CString::new("=").unwrap().into_boxed_c_str();
            let pairs = CString::new(":").unwrap().into_boxed_c_str();
            let o = Owned::new();
            let ret = exclusive(&o).set_options_string(&opts, &kv, &pairs);
            assert!(
                ret.is_ok() || [EINVAL, NOT_FOUND, ERANGE].contains(&ret.unwrap_err()),
                "{ret:?}"
            );
        }
        let long = CString::new(format!("string={}", "y".repeat(1 << 16)))
            .unwrap()
            .into_boxed_c_str();
        let o = Owned::new();
        assert_eq!(set(&o, &long), Ok(1));
        assert_eq!(string(obj(&o)).unwrap().count_bytes(), 1 << 16);
    }

    #[test]
    fn failures_leave_earlier_pairs_set() {
        if under_bsan() {
            return;
        }
        let o = Owned::new();
        assert_eq!(
            set(&o, c"int=8:string=kept:missing=1:int=9"),
            Err(NOT_FOUND)
        );
        assert_eq!((obj(&o).int, string(obj(&o))), (8, Some(c"kept")));
        assert_eq!(set(&o, c"string=again:int"), Err(EINVAL));
        assert_eq!(string(obj(&o)), Some(c"again"));
        assert_eq!(set(&o, c"int=101"), Err(ERANGE));
        assert_eq!(obj(&o).int, 8);
    }

    /// Each of these reaches undefined behaviour in C (an unchecked
    /// conversion of a flag operand or rational, a NaN opacity, an ambisonic
    /// order C overflows on, an evaluator index out of range) and is refused
    /// before C runs, with no pair set, wherever it sits in the list and
    /// whichever object of the tree it names.
    #[test]
    fn refused_values_set_nothing() {
        if under_bsan() {
            return;
        }
        let o = Owned::with_child();
        for (list, err) in [
            (c"flags=+1e30", ERANGE),
            (c"int=3:flags=-huge", ERANGE),
            (c"wide_q=1e10:int=3", ERANGE),
            (c"int=3:color=red@nan", EINVAL),
            (c"layout=ambisonic 2147483647", EINVAL),
            (c"int=ld(1e300)", EINVAL),
            (c"array=1,gcd(1e300,1)", EINVAL),
            (c"flags=+time(0)", EINVAL),
            (c"wide_q=time(0)*1e10", EINVAL),
            // C stops at the unknown key, but the refusal comes first.
            (c"missing=1:flags=+1e30", ERANGE),
        ] {
            assert_eq!(set(&o, list), Err(err), "{list:?}");
            let c = obj(&o);
            assert_eq!((c.int, c.flags, c.wide_q.num), (7, 3, 1), "{list:?}");
            // SAFETY: a raw read of the live child, while no handle is live.
            assert_eq!(unsafe { (*o.child()).int }, 7, "{list:?}");
        }
    }

    /// A bare object has no options: every key is not found, and an empty
    /// list sets nothing.
    #[test]
    fn bare_objects_find_no_key() {
        static CLASS: Class = Class::new(c"crustify-opt-bare-list", ClassCategory::NA);
        let mut bare = BareObject::new(CLASS.as_ref()).unwrap();
        for (opts, expected) in [
            (c"", Ok(0)),
            (c"int=1", Err(NOT_FOUND)),
            (c"a=b:c=d", Err(NOT_FOUND)),
            (c"int", Err(EINVAL)),
        ] {
            let got = bare.as_object_mut().set_options_string(opts, c"=", c":");
            assert_eq!(got, expected, "{opts:?}");
        }
    }

    /// A parent with no options of its own, reaching an `OBJ_CLASS` object
    /// as its only child.
    #[repr(C)]
    struct Parent {
        class: *const ffi::AVClass,
        child: *mut Obj,
    }

    static PARENT_OPTIONS: Table<1> = Table([ffi::AVOption {
        name: null(),
        help: null(),
        offset: 0,
        type_: ffi::AVOptionType(0),
        default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
        min: 0.0,
        max: 0.0,
        flags: 0,
        unit: null(),
    }]);

    /// The parent's `child_next`: its child, once.
    unsafe extern "C" fn child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
        if prev.is_null() {
            // SAFETY: C passes the live parent the class belongs to.
            unsafe { (*obj.cast::<Parent>()).child.cast() }
        } else {
            null_mut()
        }
    }

    static PARENT_CLASS: super::fixture::Class = super::fixture::Class(ffi::AVClass {
        class_name: c"crustify-opt-parent".as_ptr(),
        item_name: Some(ffi::av_default_item_name),
        option: &PARENT_OPTIONS.0[0],
        version: 0,
        log_level_offset_offset: 0,
        parent_log_context_offset: 0,
        category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
        get_category: None,
        query_ranges: None,
        child_next: Some(child_next),
        child_class_iterate: None,
        state_flags_offset: 0,
    });

    #[test]
    fn children_are_searched() {
        if under_bsan() {
            return;
        }
        let child = Owned::new();
        let mut parent = Parent {
            class: &PARENT_CLASS.0,
            child: child.ptr(),
        };
        // SAFETY: a live parent of a well-formed class whose only child is a
        // live `OBJ_CLASS` object it reaches; nothing else accesses either
        // while the handle lives.
        let mut m = unsafe { ObjectMut::from_ptr((&raw mut parent).cast()) }.unwrap();
        assert_eq!(
            m.set_options_string(c"int=4:string=child", c"=", c":"),
            Ok(2)
        );
        assert_eq!(
            m.set_options_string(c"int=4:flags=+1e30", c"=", c":"),
            Err(ERANGE)
        );
        let c = obj(&child);
        assert_eq!((c.int, string(c)), (4, Some(c"child")));
        let s: *const c_char = c.string;
        assert!(!s.is_null());
    }

    /// Distinct objects set from many threads at once.
    #[test]
    fn distinct_objects_on_many_threads() {
        if under_bsan() {
            return;
        }
        std::thread::scope(|s| {
            for t in 0..8 {
                s.spawn(move || {
                    let o = Owned::new();
                    for i in 0..50 {
                        let opts =
                            CString::new(format!("int={}:string=t{t}i{i}:dict=a={i}", i % 100))
                                .unwrap();
                        assert_eq!(set(&o, &opts), Ok(3));
                    }
                    assert_eq!(obj(&o).int, 49);
                });
            }
        });
    }

    #[test]
    fn class_is_the_fixture_class() {
        let o = Owned::new();
        assert!(core::ptr::eq(obj(&o).class, &OBJ_CLASS.0));
    }
}

/// Options wave 0, batch 3: `ObjectRef::from_ptr` and `ObjectMut::from_ptr`,
/// each called on objects that satisfy their contract (the fixture's
/// well-formed `OBJ_CLASS` objects, NULL, a NULL class), then every safe
/// method on the adopted handles, with the values that reach undefined
/// behaviour in C.
mod objects {
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::{BareObject, ObjectMut, ObjectRef, SearchFlags, SerializeFlags};

    use super::fixture::{OBJ_OPTIONS, Obj, Owned};

    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// The names of every `OBJ_CLASS` option, constants included, and two
    /// that name nothing.
    fn names() -> Vec<&'static CStr> {
        let mut v: Vec<&'static CStr> = OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) })
            .collect();
        v.extend([c"missing", c""]);
        v
    }

    /// The shared handle of `o`.
    fn shared(o: &Owned) -> ObjectRef<'_> {
        // SAFETY: a live, defaulted object of the well-formed `OBJ_CLASS`,
        // written by nothing while the handle lives.
        unsafe { ObjectRef::from_ptr(o.as_obj()) }.unwrap()
    }

    /// The exclusive handle of `o`.
    fn exclusive(o: &Owned) -> ObjectMut<'_> {
        // SAFETY: as above, accessed through no other handle while this one
        // lives.
        unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap()
    }

    /// Calls every getter on every name, with both search flags.
    fn read_all(r: ObjectRef<'_>) {
        for name in names() {
            for flags in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                let _ = r.get(name, flags).map(|s| s.as_bytes().len());
                let _ = r.get_nullable(name, flags);
                let _ = r.get_int(name, flags);
                let _ = r.get_double(name, flags);
                let _ = r.get_image_size(name, flags);
                let _ = r.get_array_size(name, flags);
                let _ = r.is_set_to_default(name, flags);
            }
            for flag in [c"one", c"bit", c"high", c"mixed", c"dflag", c"missing"] {
                let _ = r.flag_is_set(name, flag);
            }
        }
        for flags in [
            SerializeFlags::NONE,
            SerializeFlags::SKIP_DEFAULTS,
            SerializeFlags::SEARCH_CHILDREN | SerializeFlags::OPT_FLAGS_EXACT,
        ] {
            let _ = r.serialize(0, flags, b'=', b':');
        }
        let _ = r.children().count();
    }

    #[test]
    fn object_ref_from_ptr() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        // SAFETY: NULL is accepted.
        assert!(unsafe { ObjectRef::from_ptr(ptr::null_mut()) }.is_none());
        let mut no_class: *const c_void = ptr::null();
        // SAFETY: a live object whose class pointer is NULL.
        assert!(unsafe { ObjectRef::from_ptr((&raw mut no_class).cast()) }.is_none());
        let o = Owned::with_child();
        read_all(shared(&o));
        // Handles copy, and the children are objects of their own.
        let r = shared(&o);
        let copy = r;
        let child = copy.children().next().unwrap();
        assert!(child.class().ptr_eq(r.class()));
        read_all(child);
        // Values at the edges of what C reads, written raw while no handle
        // is live.
        // SAFETY: raw writes of the live object's fields, each a valid value
        // of its type.
        unsafe {
            let p = o.ptr();
            (*p).q = ffmpeg::libavutil::ffi::AVRational { num: 1, den: 0 };
            (*p).wide_q = ffmpeg::libavutil::ffi::AVRational { num: 0, den: 0 };
            (*p).dbl = f64::NAN;
            (*p).flt = f32::INFINITY;
            (*p).duration = i64::MIN;
            (*p).big = i64::MIN;
            (*p).uint64 = u64::MAX;
            (*p).flags = -1;
        }
        read_all(shared(&o));
        // SAFETY: as above.
        unsafe { (*o.ptr()).dbl = 1e300 };
        read_all(shared(&o));
        let r = shared(&o);
        assert_eq!(r.get_int(c"dbl", SearchFlags::NONE), Err(ERANGE));
        assert_eq!(r.get_int(c"q", SearchFlags::NONE), Err(ERANGE));
        assert_eq!(r.get_int(c"wide_q", SearchFlags::NONE), Err(ERANGE));
        assert!(!r.flag_is_set(c"dbl_unit", c"dflag"));
        // A zeroed object, before any default: NULL strings, empty arrays.
        let z = Owned::zeroed();
        read_all(shared(&z));
        static LOG: Class = Class::new(c"crustify-opt-show", ClassCategory::NA);
        let log = BareObject::new(LOG.as_ref()).unwrap();
        r.show(None, -1, 0);
        r.show(
            Some(log.as_object()),
            ffmpeg::libavutil::ffi::AV_OPT_FLAG_ENCODING_PARAM as i32,
            0,
        );
        r.show(Some(child), 0, -1);
    }

    #[test]
    fn object_mut_from_ptr() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        // SAFETY: NULL is accepted.
        assert!(unsafe { ObjectMut::from_ptr(ptr::null_mut()) }.is_none());
        let mut no_class: *const c_void = ptr::null();
        // SAFETY: a live object whose class pointer is NULL.
        assert!(unsafe { ObjectMut::from_ptr((&raw mut no_class).cast()) }.is_none());

        let o = Owned::with_child();
        let mut m = exclusive(&o);
        let none = SearchFlags::NONE;
        // Each of these reaches undefined behaviour in C (UBSan
        // float-cast-overflow in opt.c or eval.c or parseutils.c, a scalar
        // store over an array pointer, an out-of-bounds type name read) and
        // is refused.
        for (name, val, err) in [
            (c"flags", c"+1e30", ERANGE),
            (c"flags", c"-huge", ERANGE),
            (c"int", c"ld(1e300)", EINVAL),
            (c"rate", c"ld(1e300)", EINVAL),
            (c"wide_q", c"1e10", ERANGE),
            (c"color", c"red@nan", EINVAL),
            (c"layout", c"ambisonic 2147483647", EINVAL),
            (c"array", c"1,gcd(1e300,1)", EINVAL),
        ] {
            assert_eq!(m.set(name, val, none), Err(err), "{name:?} = {val:?}");
            assert_eq!(
                m.set(name, val, SearchFlags::CHILDREN),
                Err(err),
                "{name:?} = {val:?}"
            );
        }
        assert_eq!(m.set_double(c"wide_q", f64::NAN, none), Err(ERANGE));
        assert_eq!(m.set_double(c"wide_q", 1e10, none), Err(ERANGE));
        assert_eq!(m.set_int(c"wide_q", 1 << 40, none), Err(ERANGE));
        assert_eq!(m.set_int(c"array", 5, none), Err(EINVAL));
        assert_eq!(m.set_double(c"strings", 5.0, none), Err(EINVAL));
        assert_eq!(m.set_bin(c"array", &[1], none), Err(EINVAL));
        assert_eq!(m.set_image_size(c"strings", 1, 1, none), Err(EINVAL));
        assert_eq!(
            m.set_from_string(c"int=1:color=red@nan", &[], c"=", c":"),
            Err(EINVAL)
        );

        // Everything else, on every option.
        let values: [&CStr; 14] = [
            c"1",
            c"-1",
            c"0x10",
            c"1k",
            c"max",
            c"+one-bit",
            c"auto",
            c"none",
            c"stereo",
            c"10x20",
            c"0a0B",
            c"k=v:x=y",
            c"1|2\\|3",
            c"",
        ];
        for name in names() {
            for val in values {
                let _ = m.set(name, val, none);
            }
            let _ = m.set_int(name, i64::MAX, none);
            let _ = m.set_int(name, i64::MIN, none);
            let _ = m.set_double(name, f64::INFINITY, none);
            let _ = m.set_double(name, -0.5, none);
            let _ = m.set_bin(name, &[0xAB; 7], none);
            let _ = m.set_bin(name, &[], none);
            let _ = m.set_image_size(name, 3, 4, none);
            let _ = m.set_image_size(name, -3, 4, none);
        }
        read_all(m.as_ref());
        // C's own `av_opt_free` leaves a stale binary length behind; the
        // readers that follow would dereference the NULL buffer.
        m.set_bin(c"binary", &[1, 2, 3], none).unwrap();
        m.free_options();
        read_all(m.as_ref());
        m.free_options();
        m.set_defaults();
        m.set_defaults_masked(ffmpeg::libavutil::ffi::AV_OPT_FLAG_ENCODING_PARAM as i32, 0);
        assert_eq!(
            m.set_from_string(c"7:+eight", &[c"int", c"flags"], c"=", c":"),
            Ok(2)
        );
        assert!(m.set_from_string(c"x", &[], c"=", c":").is_err());

        // Children, each in turn, and through the parent.
        {
            let mut cursor = m.children_mut();
            let mut child = cursor.next_child().unwrap();
            child.set(c"string", c"child", none).unwrap();
            child.as_mut().free_options();
            assert!(cursor.next_child().is_none());
            assert!(cursor.next_child().is_none());
        }
        m.set(c"int", c"9", SearchFlags::CHILDREN).unwrap();
        // SAFETY: a raw read of the live child, while only `m` is live.
        assert_eq!(unsafe { (*o.child()).int }, 9);

        // Copies between objects of the class, both ways, and into a
        // zeroed one.
        let other = Owned::new();
        exclusive(&other).set(c"string", c"other", none).unwrap();
        m.copy_from(shared(&other)).unwrap();
        exclusive(&other).copy_from(m.as_ref()).unwrap();
        let z = Owned::zeroed();
        exclusive(&z).copy_from(m.as_ref()).unwrap();
        static BARE: Class = Class::new(c"crustify-opt-bare", ClassCategory::NA);
        let bare = BareObject::new(BARE.as_ref()).unwrap();
        assert_eq!(m.copy_from(bare.as_object()), Err(EINVAL));
        read_all(shared(&z));
    }

    /// Random values from tokens near every refusal rule, set on every
    /// option: each is refused, fails in C, or sets a value, without a
    /// sanitizer report; the object stays readable and freeable.
    #[test]
    fn random_values() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        const TOKENS: [&str; 30] = [
            "+",
            "-",
            "1e30",
            "1e300",
            "nan",
            "inf",
            "huge",
            "one",
            "max",
            "min",
            "default",
            "all",
            "ld(",
            "gcd(",
            ")",
            ",",
            "1",
            "0x7fffffff",
            "2147483648",
            "@",
            "red",
            "ambisonic ",
            "2147483647",
            "/",
            ":",
            "|",
            "\\",
            "=",
            "x",
            "-9223372036854775809",
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let o = Owned::with_child();
        let names = names();
        let mut m = exclusive(&o);
        for i in 0..20_000 {
            let n = 1 + next() % 5;
            let v: String = (0..n)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect();
            let v = CString::new(v).unwrap();
            let name = names[(next() % names.len() as u64) as usize];
            let flags = if i % 2 == 0 {
                SearchFlags::NONE
            } else {
                SearchFlags::CHILDREN
            };
            let _ = m.set(name, &v, flags);
            if i % 997 == 0 {
                read_all(m.as_ref());
            }
        }
        m.free_options();
    }

    /// The paths that need no C string parse, on zeroed objects (no
    /// defaults, which C parses): typed setters, failed color lookups and
    /// their restore, frees with the length reset, copies, children and
    /// formatted reads, each through a fresh reborrow of the handle. The
    /// object tests BSan runs.
    #[test]
    fn typed_paths_across_reborrows() {
        let o = Owned::zeroed_with_child();
        let other = Owned::zeroed();
        let none = SearchFlags::NONE;
        let mut m = exclusive(&o);
        for round in 0..3 {
            let mut r = m.as_mut();
            r.set_int(c"int", 5 + round, none).unwrap();
            r.set_double(c"dbl", 0.5, none).unwrap();
            r.set_bin(c"binary", &[1, 2, 3], none).unwrap();
            r.set_image_size(c"size", 4, 6, SearchFlags::CHILDREN)
                .unwrap();
            assert!(r.set(c"color", c"nocolor", none).is_err());
            assert_eq!(r.set_int(c"array", 1, none), Err(EINVAL));
            assert_eq!(r.set_bin(c"strings", &[1], none), Err(EINVAL));
            if let Some(mut child) = r.children_mut().next_child() {
                child.set_int(c"int", 9, none).unwrap();
                child.free_options();
            }
            let s = r.as_ref();
            assert_eq!(s.get(c"binary", none).unwrap().as_bytes(), b"010203");
            assert_eq!(s.get_int(c"int", none), Ok(5 + round));
            assert_eq!(s.get_image_size(c"size", SearchFlags::CHILDREN), Ok((4, 6)));
            assert_eq!(s.children().count(), 1);
            exclusive(&other).copy_from(s).unwrap();
            r.free_options();
            assert_eq!(r.as_ref().get(c"binary", none).unwrap().as_bytes(), b"");
        }
        m.copy_from(shared(&other)).unwrap();
        // SAFETY: a raw read of the live object, while only `m` is live.
        assert_eq!(unsafe { (*o.ptr()).binary_len }, 3);
    }

    /// The fixture's `Obj` layout is what the table's offsets describe.
    #[test]
    fn fixture_strings_are_c_strings() {
        let o = Owned::new();
        // SAFETY: a raw read of the defaulted object's string.
        let s = unsafe { (*o.ptr()).string };
        // SAFETY: a defaulted string option is an owned C string.
        assert_eq!(unsafe { CStr::from_ptr(s.cast::<c_char>()) }, c"hello");
        let _: *mut Obj = o.ptr();
    }
}

/// Options wave 0, batch-1 review: `ObjectRef::from_ptr` adopting logging
/// contexts (structs whose first member is their class pointer) for the safe
/// API that logs through one: `parse_color_with_context`, whose raw
/// `*mut c_void` context the review replaced with an `ObjectRef`,
/// `ObjectRef::show` and `eval::parse_and_eval`. A class whose `item_name`
/// reads its object's payload is valid for such a context, but not for a
/// `BareObject`, which has no payload.
mod log_contexts {
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr::{null, null_mut};
    use std::ffi::CString;

    use ffmpeg::libavutil::eval::parse_and_eval;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{BareObject, ObjectRef};
    use ffmpeg::libavutil::parseutils::parse_color_with_context;

    use super::fixture::Class;

    const EINVAL: i32 = -22;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// A logging context: its class pointer, then a payload its class's
    /// callbacks read.
    #[repr(C)]
    struct Ctx {
        class: *const ffi::AVClass,
        payload: [u64; 2],
    }

    impl Ctx {
        fn new(class: &'static Class) -> Self {
            Self {
                class: &class.0,
                payload: [1, 2],
            }
        }
    }

    unsafe extern "C" fn item_name(ctx: *mut c_void) -> *const c_char {
        // SAFETY: C passes the live context the class belongs to.
        let payload = unsafe { (*ctx.cast::<Ctx>()).payload };
        assert_eq!(payload, [1, 2]);
        c"crustify-log-ctx".as_ptr()
    }

    unsafe extern "C" fn get_category(ctx: *mut c_void) -> ffi::AVClassCategory {
        // SAFETY: as for `item_name`.
        let payload = unsafe { (*ctx.cast::<Ctx>()).payload };
        assert_eq!(payload, [1, 2]);
        ffi::AVClassCategory::AV_CLASS_CATEGORY_FILTER
    }

    /// A class without options, children or offsets, with `item_name` and
    /// `get_category` as given.
    const fn class(
        item_name: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
        get_category: Option<unsafe extern "C" fn(*mut c_void) -> ffi::AVClassCategory>,
    ) -> Class {
        Class(ffi::AVClass {
            class_name: c"crustify-log".as_ptr(),
            item_name,
            option: null(),
            version: ((ffi::LIBAVUTIL_VERSION_MAJOR << 16)
                | (ffi::LIBAVUTIL_VERSION_MINOR << 8)
                | ffi::LIBAVUTIL_VERSION_MICRO) as i32,
            log_level_offset_offset: 0,
            parent_log_context_offset: 0,
            category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
            get_category,
            query_ranges: None,
            child_next: None,
            child_class_iterate: None,
            state_flags_offset: 0,
        })
    }

    static READS_NAME: Class = class(Some(item_name), None);
    static READS_CATEGORY: Class = class(Some(ffi::av_default_item_name), Some(get_category));
    static DEFAULT_NAME: Class = class(Some(ffi::av_default_item_name), None);
    static NO_NAME: Class = class(None, None);

    /// The handle of `ctx`.
    fn adopt(ctx: &mut Ctx) -> ObjectRef<'_> {
        // SAFETY: a live context of a well-formed class without options,
        // whose callbacks accept it, written by nothing while the handle
        // lives.
        unsafe { ObjectRef::from_ptr((&raw mut *ctx).cast()) }.unwrap()
    }

    const COLORS: [&CStr; 14] = [
        c"red",
        c"AliceBlue@0.5",
        c"#ff000080",
        c"0x00ff00@0x40",
        c"random",
        c"bikeshed@1",
        c"nosuchcolor",
        c"#12345",
        c"red@2",
        c"red@nan",
        c"#",
        c"0x",
        c"",
        c"@",
    ];

    /// Every color, whole and at every prefix length (plus a few past it),
    /// logging failures through `log_ctx`.
    fn sweep(log_ctx: Option<ObjectRef<'_>>) {
        for color in COLORS {
            let exact = CString::from(color).into_boxed_c_str();
            let lens = (0..=color.count_bytes() + 2)
                .map(Some)
                .chain([None, Some(usize::MAX)]);
            for len in lens {
                if let Err(e) = parse_color_with_context(&exact, len, log_ctx) {
                    assert_eq!(e, EINVAL, "{color:?} {len:?}");
                }
            }
        }
    }

    #[test]
    fn null_and_classless_contexts_are_no_context() {
        // SAFETY: NULL is accepted.
        assert!(unsafe { ObjectRef::from_ptr(null_mut()) }.is_none());
        let mut classless = Ctx::new(&READS_NAME);
        classless.class = null();
        // SAFETY: a live object whose class pointer is NULL.
        assert!(unsafe { ObjectRef::from_ptr((&raw mut classless).cast()) }.is_none());
        sweep(None);
    }

    #[test]
    fn classed_contexts() {
        for class in [&READS_NAME, &READS_CATEGORY, &DEFAULT_NAME, &NO_NAME] {
            let mut local = Ctx::new(class);
            sweep(Some(adopt(&mut local)));
            let r = adopt(&mut local);
            r.show(Some(r), -1, 0);
            // A boxed context, freed after the calls.
            let mut boxed = Box::new(Ctx::new(class));
            sweep(Some(adopt(&mut boxed)));
            if !under_bsan() {
                // The parse error is logged by an "Eval" context whose
                // parent is the log context.
                let r = adopt(&mut boxed);
                assert_eq!(
                    parse_and_eval(c"nosuch", &[], &[], &[], 0, Some(r)),
                    Err(EINVAL)
                );
            }
            drop(boxed);
        }
    }

    /// Long colors: C copies at most 127 bytes after the marker, so the
    /// alpha specifier is cut off or missed, never read past.
    #[test]
    fn long_colors() {
        let mut ctx = Ctx::new(&READS_NAME);
        for n in [120, 126, 127, 128, 129, 1000] {
            for (prefix, suffix) in [("", "@nan"), ("#", "@0.5"), ("0x", "@0x80"), ("red", "")] {
                let s = format!("{prefix}{}{suffix}", "f".repeat(n));
                let s = CString::new(s).unwrap().into_boxed_c_str();
                for len in [None, Some(n), Some(n + 3), Some(usize::MAX)] {
                    let ret = parse_color_with_context(&s, len, Some(adopt(&mut ctx)));
                    assert_eq!(ret, Err(EINVAL), "{n} {prefix:?} {suffix:?} {len:?}");
                }
            }
        }
    }

    /// Logging calls a class's `item_name` and `get_category` on the object
    /// it logs with; a bare object is only a class pointer, so its class
    /// must not have callbacks that read more (options wave 0, batch-1
    /// review: `BareObject::new` accepted them, and logging read past the
    /// object).
    #[test]
    fn bare_objects_refuse_classes_whose_callbacks_read_the_object() {
        for (class, bare) in [
            (&READS_NAME, false),
            (&READS_CATEGORY, false),
            (&DEFAULT_NAME, true),
            (&NO_NAME, true),
        ] {
            let mut ctx = Ctx::new(class);
            let r = adopt(&mut ctx);
            let object = BareObject::new(r.class());
            assert_eq!(object.is_some(), bare);
            if let Some(object) = object {
                r.show(Some(object.as_object()), -1, 0);
                object.as_object().show(Some(object.as_object()), -1, 0);
            }
        }
    }
}

/// Options wave 0, batch-1 review: values `ObjectMut::set` previews before
/// C evaluates them again, for C's unchecked `(int64_t)` conversion of a
/// `+` / `-` flag operand (`opt.c:508`) and `(int)` conversion of a wide
/// rational (`opt.c:343`). The preview must see the value C will: an operand
/// that reads the clock may change between the two evaluations, and an
/// array option's `default` is the bits of its definition's address in C.
/// Each reproducer in `crustify/reviews/20261005/libavutil/options/wave-0/
/// batch-1/ub_unsafe/` is one of these scenarios.
mod previewed_values {
    use core::ffi::{c_int, c_uint, c_void};
    use core::mem::offset_of;
    use core::ptr::{null, null_mut};
    use std::ffi::CString;
    use std::time::{SystemTime, UNIX_EPOCH};

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{ObjectMut, SearchFlags};

    use super::fixture::{ArrayDef, Class, Owned, Table};

    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// An object with a flags array and a wide rational array, both with a
    /// definition (no default, `,` separator).
    #[repr(C)]
    struct Arrays {
        class: *const ffi::AVClass,
        flags: *mut c_uint,
        nb_flags: c_uint,
        q: *mut ffi::AVRational,
        nb_q: c_uint,
    }

    static DEF: ArrayDef = ArrayDef(ffi::AVOptionArrayDef {
        def: null(),
        size_min: 0,
        size_max: 0,
        sep: 0,
    });

    const fn array(
        name: &'static core::ffi::CStr,
        offset: usize,
        base: ffi::AVOptionType,
        min: f64,
        max: f64,
    ) -> ffi::AVOption {
        ffi::AVOption {
            name: name.as_ptr(),
            help: null(),
            offset: offset as c_int,
            type_: ffi::AVOptionType(base.0 | ffi::AVOptionType::AV_OPT_TYPE_FLAG_ARRAY.0),
            default_val: ffi::AVOption__bindgen_ty_1 { arr: &DEF.0 },
            min,
            max,
            flags: 0,
            unit: null(),
        }
    }

    static OPTIONS: Table<3> = Table([
        array(
            c"flags",
            offset_of!(Arrays, flags),
            ffi::AVOptionType::AV_OPT_TYPE_FLAGS,
            0.0,
            u32::MAX as f64,
        ),
        array(
            c"q",
            offset_of!(Arrays, q),
            ffi::AVOptionType::AV_OPT_TYPE_RATIONAL,
            -1e300,
            1e300,
        ),
        ffi::AVOption {
            name: null(),
            help: null(),
            offset: 0,
            type_: ffi::AVOptionType(0),
            default_val: ffi::AVOption__bindgen_ty_1 { i64_: 0 },
            min: 0.0,
            max: 0.0,
            flags: 0,
            unit: null(),
        },
    ]);

    static CLASS: Class = Class(ffi::AVClass {
        class_name: c"crustify-opt-arrays".as_ptr(),
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

    /// `DEFAULT_NUMVAL` of an array option is its definition's address,
    /// non-zero: `not(not(default))` is 1 in C, and the preview agrees.
    #[test]
    fn array_defaults_preview_as_c_evaluates_them() {
        if under_bsan() {
            return;
        }
        let o = Box::into_raw(Box::new(Arrays {
            class: &CLASS.0,
            flags: null_mut(),
            nb_flags: 0,
            q: null_mut(),
            nb_q: 0,
        }));
        {
            // SAFETY: a live object of the well-formed class above (two
            // empty arrays of the types their options store), reached
            // through this handle only.
            let mut m = unsafe { ObjectMut::from_ptr(o.cast::<c_void>()) }.unwrap();
            let none = SearchFlags::NONE;
            assert_eq!(
                m.set(c"flags", c"+1e30*not(not(default))", none),
                Err(ERANGE)
            );
            assert_eq!(
                m.set(c"flags", c"1,-1e30*not(not(default))", none),
                Err(ERANGE)
            );
            assert_eq!(m.set(c"q", c"1e10*not(not(default))", none), Err(ERANGE));
            // In range, C stores what the preview saw.
            m.set(c"flags", c"+not(not(default))+4,8", none).unwrap();
            m.set(c"q", c"not(not(default))*7,1/3", none).unwrap();
            assert_eq!(
                m.as_ref().get(c"flags", none).unwrap().as_bytes(),
                b"0x00000005,0x00000008"
            );
            assert_eq!(m.as_ref().get(c"q", none).unwrap().as_bytes(), b"7/1,1/3");
            m.free_options();
        }
        // SAFETY: `o` came from `Box::into_raw`, no handle is live, and
        // `free_options` released both arrays.
        drop(unsafe { Box::from_raw(o) });
    }

    /// Operands that read the clock are refused, whatever its reading; the
    /// loop straddles the check and C's evaluation as the reproducer does,
    /// and must leave no sanitizer report (UBSan recovers: the suite's log
    /// is grepped for `runtime error:`).
    #[test]
    fn clock_reading_operands_are_refused() {
        if under_bsan() {
            return;
        }
        let o = Owned::new();
        // SAFETY: a live, defaulted object of the well-formed `OBJ_CLASS`,
        // reached through this handle only.
        let mut m = unsafe { ObjectMut::from_ptr(o.as_obj()) }.unwrap();
        let none = SearchFlags::NONE;
        for (name, val) in [
            (c"flags", c"+time(0)"),
            (c"flags", c"-ti me ( 0 )"),
            (c"flags", c"one+if(gt(time(0),0),1e30,1)"),
            (c"cflags", c"+time(0)*0"),
            (c"wide_q", c"time(0)*1e10"),
            (c"wide_q", c"if(lt(time(0),0),1,1e10)"),
        ] {
            assert_eq!(m.set(name, val, none), Err(EINVAL), "{name:?} = {val:?}");
        }
        // A rational of `int` bounds and a scalar number need no preview:
        // C's range check comes first, and the conversions are defined.
        m.set(c"q", c"time(0)*0+1", none).unwrap();
        m.set(c"dbl", c"time(0)", none).unwrap();
        for attempt in 0..2_000u32 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            let t = now + f64::from(attempt % 500) * 1e-6;
            let val = CString::new(format!("+if(gte(time(0),{t:.6}),1e30,1)")).unwrap();
            assert_eq!(m.set(c"flags", &val, none), Err(EINVAL));
        }
    }
}

/// Options wave 1, batch 1: the typed setters, array editors and dictionary
/// appliers on objects adopted with `ObjectMut::from_ptr` within its
/// contract (a live object of a well-formed class, accessed through the one
/// handle): the unsafe adoption is the only way to an object with options,
/// so these paths of its contract are exercised here, as wave 0's setters
/// are. Every setter on every option of both fixture classes, values of
/// every element type at every array position, the values C would turn into
/// undefined behaviour, Rust-owned layouts and dictionaries reborrowed and
/// dropped after C copied them, dictionaries handed to C and back, and
/// distinct objects on many threads; every option is read back after each
/// round, and the objects are freed by C.
mod typed_setters {
    use core::ffi::{CStr, c_void};
    use std::ffi::CString;

    use ffmpeg::libavutil::channel_layout::{ChannelCustom, ChannelLayout};
    use ffmpeg::libavutil::dict::Dict;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::{ObjectMut, ObjectRef, SearchFlags};
    use ffmpeg::libavutil::pixfmt::PixelFormat;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::samplefmt::SampleFormat;

    use super::fixture::{ARRAYS_NAMES, OBJ_OPTIONS, Owned, OwnedArrays};

    const EINVAL: i32 = -22;
    const ERANGE: i32 = -34;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// The exclusive handle of the object at `obj`.
    fn exclusive<'a>(obj: *mut c_void) -> ObjectMut<'a> {
        // SAFETY: every caller passes a live fixture object of a well-formed
        // class, accessed through no other handle while this one lives.
        unsafe { ObjectMut::from_ptr(obj) }.unwrap()
    }

    /// Reads every option of `names` through the getters, children included.
    fn read_all(r: ObjectRef<'_>, names: &[&CStr]) {
        for &n in names {
            for f in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                let _ = r.get(n, f);
                let _ = r.get_nullable(n, f);
                let _ = r.get_array_size(n, f);
                let _ = r.is_set_to_default(n, f);
            }
        }
    }

    fn obj_names() -> Vec<&'static CStr> {
        OBJ_OPTIONS.0[..OBJ_OPTIONS.0.len() - 1]
            .iter()
            // SAFETY: every name of the static table is a literal.
            .map(|o| unsafe { CStr::from_ptr(o.name) })
            .chain([c"missing"])
            .collect()
    }

    /// A custom layout whose first name fills its field without a NUL.
    fn unterminated() -> ffmpeg::libavutil::channel_layout::ChannelLayoutVal {
        let mut l = ChannelLayout::custom(1).unwrap();
        let mut map = l.as_mut().into_custom_map_mut().unwrap();
        map.get_mut(0)
            .unwrap()
            .set(ChannelCustom::from_raw(ffi::AVChannelCustom {
                id: ffi::AVChannel::AV_CHAN_FRONT_LEFT,
                name: [b'n' as core::ffi::c_char; 16],
                opaque: core::ptr::null_mut(),
            }));
        l
    }

    #[test]
    fn every_typed_setter_on_every_option() {
        let o = Owned::with_child();
        let names = obj_names();
        let layouts = [
            ChannelLayout::parse(c"5.1").unwrap(),
            ChannelLayout::parse(c"FL@front-left+FR").unwrap(),
            ChannelLayout::unspecified(3),
            ChannelLayout::parse(c"ambisonic 1").unwrap(),
        ];
        let bad = unterminated();
        let dict = Dict::from_entries([(c"k", c"v"), (c"K", c"w")]).unwrap();
        let mut m = exclusive(o.as_obj());
        for round in 0..2 {
            for &name in &names {
                for f in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                    for q in [(1, 2), (1, 0), (0, 0), (i32::MIN, -1), (i32::MAX, i32::MIN)] {
                        let _ = m.set_rational(name, Rational::new(q.0, q.1), f);
                        let _ = m.set_video_rate(name, Rational::new(q.0, q.1), f);
                    }
                    for p in [-2, -1, 0, 3, 1000, i32::MAX, i32::MIN] {
                        let _ = m.set_pixel_format(
                            name,
                            PixelFormat::from_raw(ffi::AVPixelFormat(p)),
                            f,
                        );
                        let _ = m.set_sample_format(
                            name,
                            SampleFormat::from_raw(ffi::AVSampleFormat(p)),
                            f,
                        );
                    }
                    for l in &layouts {
                        let _ = m.set_channel_layout(name, l.as_ref(), f);
                    }
                    assert!(m.set_channel_layout(name, bad.as_ref(), f).is_err());
                    let _ = m.set_dictionary(name, dict.as_ref(), f);
                    if round == 1 {
                        let _ = m.set_dictionary(name, None, f);
                    }
                }
            }
            read_all(m.as_ref(), &names);
        }
        // Array options refuse every typed scalar store.
        for name in [c"array", c"strings", c"empty"] {
            let f = SearchFlags::NONE;
            assert_eq!(m.set_rational(name, Rational::new(1, 2), f), Err(EINVAL));
            assert_eq!(m.set_video_rate(name, Rational::new(1, 2), f), Err(EINVAL));
            assert_eq!(m.set_pixel_format(name, PixelFormat::NONE, f), Err(EINVAL));
            assert_eq!(
                m.set_sample_format(name, SampleFormat::NONE, f),
                Err(EINVAL)
            );
            assert_eq!(
                m.set_channel_layout(name, layouts[0].as_ref(), f),
                Err(EINVAL)
            );
            assert_eq!(m.set_dictionary(name, None, f), Err(EINVAL));
        }
        m.free_options();
        read_all(m.as_ref(), &names);
    }

    /// Exactly sized heap copies of `strs`, and borrows of them.
    fn exact(strs: &[&[u8]]) -> Vec<Box<CStr>> {
        strs.iter()
            .map(|s| CString::new(*s).unwrap().into_boxed_c_str())
            .collect()
    }

    #[test]
    fn every_array_edit_of_every_value_type() {
        let o = OwnedArrays::with_child();
        let strings = exact(&[
            b"3", b"1/2", b"red", b"5.1", b"a=1:b=2", b"640x480", b"", b"1:02.5",
        ]);
        let strs: Vec<&CStr> = strings.iter().map(|s| &**s).collect();
        let opt_strs: Vec<Option<&CStr>> = strs.iter().map(|&s| Some(s)).chain([None]).collect();
        let layouts = [
            ChannelLayout::parse(c"stereo").unwrap(),
            ChannelLayout::parse(c"FL@x+FR@y").unwrap(),
        ];
        let layout_refs: Vec<_> = layouts.iter().map(|l| l.as_ref()).collect();
        let dicts = [Dict::from_entries([(c"a", c"1")]).unwrap(), Dict::new()];
        let dict_refs: Vec<_> = dicts.iter().map(Dict::as_ref).collect();
        let qs = [
            Rational::new(1, 3),
            Rational::new(1, 0),
            Rational::new(i32::MIN, -1),
        ];
        let pix = [
            PixelFormat::NONE,
            PixelFormat::from_raw(ffi::AVPixelFormat(3)),
        ];
        let samples = [
            SampleFormat::S16,
            SampleFormat::from_raw(ffi::AVSampleFormat(-7)),
        ];
        let mut m = exclusive(o.as_obj());
        let rounds = if under_bsan() { 1 } else { 3 };
        for _ in 0..rounds {
            for name in ARRAYS_NAMES {
                for f in [SearchFlags::NONE, SearchFlags::CHILDREN] {
                    for start in [0, 1, 3, 7, u32::MAX as usize, usize::MAX] {
                        macro_rules! both {
                            ($v:expr) => {{
                                let _ = m.insert_array(name, start, $v, f);
                                let _ = m.replace_array(name, start, $v, f);
                            }};
                        }
                        // String elements parse through C's evaluator, whose
                        // `av_strtod` BSan misreports (see `objects`).
                        if !under_bsan() {
                            both!(&strs[..]);
                            both!(&opt_strs[..]);
                        }
                        both!(&[5i32, -3, i32::MIN]);
                        both!(&[1i64 << 40, 9]);
                        both!(&[7u32, u32::MAX]);
                        both!(&[u64::MAX]);
                        both!(&[0.5f32, f32::INFINITY]);
                        both!(&[2.5f64, -1e300]);
                        both!(&qs);
                        both!(&pix);
                        both!(&samples);
                        both!(&[(640, 480), (-1, i32::MAX)]);
                        both!(&[[1u8, 2, 3, 4]]);
                        both!(&layout_refs[..]);
                        both!(&dict_refs[..]);
                        let _ = m.remove_array(name, start, 1, f);
                        let _ = m.remove_array(name, 0, start, f);
                    }
                }
            }
            read_all(m.as_ref(), &ARRAYS_NAMES);
        }
        // Values C would convert with undefined behaviour, and the binary
        // array C overruns, are refused.
        let f = SearchFlags::NONE;
        assert_eq!(m.insert_array(c"qs", 0, &[f64::NAN], f), Err(ERANGE));
        assert_eq!(m.insert_array(c"rates", 0, &[f32::NAN], f), Err(ERANGE));
        assert_eq!(m.insert_array(c"wide_qs", 0, &[1e10], f), Err(ERANGE));
        assert_eq!(m.replace_array(c"wide_qs", 0, &[i64::MAX], f), Err(ERANGE));
        assert_eq!(m.insert_array(c"bins", 0, &[c"0a0b"], f), Err(EINVAL));
        assert_eq!(m.insert_array(c"bins", 0, &[None::<&CStr>], f), Err(EINVAL));
        assert_eq!(
            m.insert_array(c"layouts", 0, &[unterminated().as_ref()], f),
            Err(EINVAL)
        );
        if !under_bsan() {
            assert_eq!(m.insert_array(c"dbls", 0, &[c"ld(1e300)"], f), Err(EINVAL));
            assert_eq!(m.insert_array(c"colors", 0, &[c"red@nan"], f), Err(EINVAL));
            assert_eq!(m.set(c"bins", c"0a,0b", f), Err(EINVAL));
        }
        // SAFETY: a raw read of the live object, while only `m` is live.
        assert_eq!(unsafe { (*o.ptr()).bins.len }, 0);
        m.free_options();
        read_all(m.as_ref(), &ARRAYS_NAMES);
    }

    /// C copies each borrowed layout and dictionary: the Rust values are
    /// reborrowed, rewritten and dropped afterwards without touching what
    /// the object holds, which is read and freed by C. On zeroed objects,
    /// with no string parses, so that BorrowSanitizer runs it without C's
    /// evaluator (whose `av_strtod` end pointers it misreports).
    #[test]
    fn borrowed_values_across_reborrows() {
        let o = OwnedArrays::zeroed_with_child();
        let s = Owned::zeroed_with_child();
        for round in 0..3 {
            let mut layouts = [
                ChannelLayout::parse(c"FL@abc+FR@def+LFE").unwrap(),
                ChannelLayout::parse(c"7.1").unwrap(),
            ];
            let mut dicts = [
                Dict::from_entries([(c"key", c"value")]).unwrap(),
                Dict::new(),
            ];
            {
                let mut m = exclusive(o.as_obj());
                let refs: Vec<_> = layouts.iter().map(|l| l.as_ref()).collect();
                m.insert_array(c"layouts", 0, &refs, SearchFlags::NONE)
                    .unwrap();
                m.replace_array(c"layouts", 1, &refs[..1], SearchFlags::NONE)
                    .unwrap();
                m.insert_array(c"layouts", 0, &refs, SearchFlags::CHILDREN)
                    .unwrap();
                let drefs: Vec<_> = dicts.iter().map(Dict::as_ref).collect();
                m.insert_array(c"dicts", 0, &drefs, SearchFlags::NONE)
                    .unwrap();
                let mut scalar = exclusive(s.as_obj());
                scalar
                    .set_channel_layout(c"layout", layouts[0].as_ref(), SearchFlags::CHILDREN)
                    .unwrap();
                scalar
                    .set_dictionary(c"dict", dicts[0].as_ref(), SearchFlags::NONE)
                    .unwrap();
            }
            // The Rust values change and drop; the objects keep their copies.
            layouts[0].as_mut().set_native(1 << 2);
            dicts[0] = Dict::new();
            drop(layouts);
            drop(dicts);
            let m = exclusive(o.as_obj());
            let got = m.as_ref().get(c"layouts", SearchFlags::NONE).unwrap();
            assert!(
                got.as_bytes()
                    .starts_with(b"3 channels (FL@abc+FR@def+LFE)|"),
                "{round}"
            );
            let scalar = exclusive(s.as_obj());
            let got = scalar
                .as_ref()
                .get(c"layout", SearchFlags::CHILDREN)
                .unwrap();
            assert_eq!(got.as_bytes(), b"3 channels (FL@abc+FR@def+LFE)");
            assert_eq!(
                scalar
                    .as_ref()
                    .get(c"dict", SearchFlags::NONE)
                    .unwrap()
                    .as_bytes(),
                b"key=value"
            );
            let mut m = m;
            m.remove_array(c"layouts", 0, 1, SearchFlags::NONE).unwrap();
        }
    }

    /// Dictionaries handed to C and taken back: the leftovers are owned
    /// again, a failure leaves the dictionary as it was, and every block is
    /// freed once (LeakSanitizer).
    #[test]
    fn dictionaries_handed_over_and_back() {
        if under_bsan() {
            return;
        }
        let o = Owned::with_child();
        let mut m = exclusive(o.as_obj());
        for round in 0..20 {
            let mut d = Dict::from_entries([
                (c"int", c"5"),
                (c"string", c"text"),
                (c"nosuch", c"1"),
                (c"layout", c"quad"),
                (c"strings", c"x|y"),
                (c"dict", c"a=1"),
            ])
            .unwrap();
            if round % 2 == 0 {
                m.set_from_dict(&mut d).unwrap();
            } else {
                m.set_from_dict_with(&mut d, SearchFlags::CHILDREN).unwrap();
            }
            assert!(d.as_ref().is_some());
            // C's own failure (out of range) and a refused value.
            let mut bad = Dict::from_entries([(c"int", c"1000"), (c"other", c"x")]).unwrap();
            assert_eq!(m.set_from_dict(&mut bad), Err(ERANGE));
            let mut refused = Dict::from_entries([(c"dbl", c"ld(1e300)")]).unwrap();
            assert_eq!(m.set_from_dict(&mut refused), Err(EINVAL));
            drop((d, bad, refused));
            read_all(m.as_ref(), &obj_names());
        }
    }

    #[test]
    fn distinct_objects_on_many_threads() {
        std::thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    let o = OwnedArrays::with_child();
                    let layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
                    let dict = Dict::from_entries([(c"k", c"v")]).unwrap();
                    let mut m = exclusive(o.as_obj());
                    for i in 0..50 {
                        let f = if (i + t) % 2 == 0 {
                            SearchFlags::NONE
                        } else {
                            SearchFlags::CHILDREN
                        };
                        let _ = m.insert_array(c"layouts", i % 3, &[layout.as_ref()], f);
                        let _ = m.insert_array(c"dicts", 0, &[dict.as_ref()], f);
                        let _ = m.replace_array(c"qs", 0, &[Rational::new(i as i32, 7)], f);
                        let _ = m.remove_array(c"layouts", 0, 1, f);
                    }
                    read_all(m.as_ref(), &ARRAYS_NAMES);
                });
            }
        });
    }
}
