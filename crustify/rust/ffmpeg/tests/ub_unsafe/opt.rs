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

/// `set_options_string`, called within its `# Safety` contract: objects of
/// the test class (and a parent reaching one as its child), NULL and
/// classless objects, every option type, owned values replaced again and
/// again, and lists at the edges of the token syntax.
mod set_options_string {
    use core::ffi::{CStr, c_void};
    use core::ptr::{null, null_mut};
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::opt::set_options_string;

    use super::fixture::{CLASS, Class, Ctx, Table, class};

    /// `AVERROR_OPTION_NOT_FOUND`: `FFERRTAG(0xF8, 'O', 'P', 'T')`.
    const NOT_FOUND: i32 = -i32::from_le_bytes([0xF8, b'O', b'P', b'T']);
    const EINVAL: i32 = -22;

    /// An object of the test class that frees its option values on drop.
    struct Owned(Ctx);

    impl Owned {
        fn new() -> Self {
            Self(Ctx::new(&CLASS))
        }

        /// `set_options_string` with `=` and `:`.
        fn set(&mut self, opts: &CStr) -> Result<usize, i32> {
            // SAFETY: `self` is a live object of the well-formed test class,
            // whose bounds lie within each option type's range, its owned
            // fields hold NULL or values C stored, nothing else accesses it,
            // and no list here sets a NaN opacity.
            unsafe { set_options_string(self.0.as_obj(), opts, c"=", c":") }
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: the object's owned fields hold NULL or values C stored.
            unsafe { ffi::av_opt_free(self.0.as_obj()) }
        }
    }

    /// The string field's value.
    fn string(c: &Ctx) -> Option<&CStr> {
        // SAFETY: the field is NULL or a live string C stored.
        (!c.string.is_null()).then(|| unsafe { CStr::from_ptr(c.string) })
    }

    #[test]
    fn every_option_type() {
        let mut o = Owned::new();
        let all = c"int=5:int64=-3:uint64=18446744073709551615:dbl=-1e9:flt=1:string=s:q=7/2:\
            boolean=true:flags=bit:size=1x1:rate=pal:color=0xff00ff80:duration=0.000001:\
            binary=00ff:dict=k=v:array=0,10:empty_array=:bare=1:nostr=n";
        assert_eq!(o.set(all), Ok(19));
        let c = &o.0;
        assert_eq!((c.int, c.int64, c.uint64), (5, -3, u64::MAX));
        assert_eq!((c.dbl, c.flt), (-1e9, 1.0));
        assert_eq!(string(c), Some(c"s"));
        assert_eq!((c.q.num, c.q.den, c.boolean, c.flags), (7, 2, 1, 2));
        assert_eq!((c.size, c.rate.num, c.rate.den), ([1, 1], 25, 1));
        assert_eq!((c.color, c.duration), ([255, 0, 255, 128], 1));
        assert_eq!(c.binary_len, 2);
        assert!(!c.dict.is_null());
        assert_eq!(c.array_len, 2);
        // An empty array value stores no block.
        assert_eq!((c.empty_array_len, c.bare), (0, 1));
        assert!(!c.nostr.is_null());
    }

    /// Owned values replaced many times, with values of growing size, and
    /// cleared: ASan sees every block C frees and allocates, LSan any left.
    #[test]
    fn owned_values_replaced_and_cleared() {
        let mut o = Owned::new();
        for round in 0..64usize {
            let s = "x".repeat(round * 7);
            let hex = "ab".repeat(round);
            let arr = vec!["3"; round % 11].join(",");
            let opts = format!("string={s}:binary={hex}:dict=k{round}=v{s}:array={arr}:nostr={s}");
            let opts = CString::new(opts).unwrap();
            assert_eq!(o.set(&opts), Ok(5));
            assert_eq!(string(&o.0).unwrap().count_bytes(), round * 7);
            assert_eq!(o.0.binary_len as usize, round);
            assert_eq!(o.0.array_len as usize, round % 11);
        }
        assert_eq!(o.set(c"string=:binary=:array="), Ok(3));
        assert_eq!(string(&o.0), Some(c""));
        assert!(o.0.binary.is_null() && o.0.array.is_null());
    }

    /// Lists in exactly sized heap strings, so ASan sees any read past
    /// their NUL; the separators likewise.
    #[test]
    fn exactly_sized_lists() {
        let lists: [&[u8]; 14] = [
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
        ];
        for list in lists {
            let opts = CString::new(list).unwrap().into_boxed_c_str();
            let kv = CString::new("=").unwrap().into_boxed_c_str();
            let pairs = CString::new(":").unwrap().into_boxed_c_str();
            let mut o = Owned::new();
            // SAFETY: as in `Owned::set`.
            let ret = unsafe { set_options_string(o.0.as_obj(), &opts, &kv, &pairs) };
            assert!(
                ret.is_ok() || ret == Err(EINVAL) || ret == Err(NOT_FOUND) || ret == Err(-34),
                "{ret:?}"
            );
        }
        let long = CString::new(format!("string={}", "y".repeat(1 << 16)))
            .unwrap()
            .into_boxed_c_str();
        let mut o = Owned::new();
        assert_eq!(o.set(&long), Ok(1));
        assert_eq!(string(&o.0).unwrap().count_bytes(), 1 << 16);
    }

    #[test]
    fn failures_leave_earlier_pairs_set() {
        let mut o = Owned::new();
        assert_eq!(o.set(c"int=8:string=kept:missing=1:int=9"), Err(NOT_FOUND));
        assert_eq!((o.0.int, string(&o.0)), (8, Some(c"kept")));
        assert_eq!(o.set(c"string=again:int"), Err(EINVAL));
        assert_eq!(string(&o.0), Some(c"again"));
        assert_eq!(o.set(c"int=101"), Err(-34));
        assert_eq!(o.0.int, 8);
    }

    #[test]
    fn null_and_classless_objects() {
        for opts in [c"", c"int=1", c"a=b:c=d"] {
            // SAFETY: a NULL object is within the contract.
            let ret = unsafe { set_options_string(null_mut(), opts, c"=", c":") };
            assert_eq!(
                ret,
                if opts.is_empty() {
                    Ok(0)
                } else {
                    Err(NOT_FOUND)
                }
            );
            let mut ctx = Ctx::new(&CLASS);
            ctx.class = null();
            // SAFETY: an object with a NULL class is within the contract.
            let ret = unsafe { set_options_string(ctx.as_obj(), opts, c"=", c":") };
            assert_eq!(
                ret,
                if opts.is_empty() {
                    Ok(0)
                } else {
                    Err(NOT_FOUND)
                }
            );
            assert_eq!(ctx.int, 0);
        }
    }

    /// A parent with no options of its own, reaching a test-class object as
    /// its only child.
    #[repr(C)]
    struct Parent {
        class: *const ffi::AVClass,
        child: Ctx,
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

    /// The parent's `child_next`: its embedded child, once.
    unsafe extern "C" fn child_next(obj: *mut c_void, prev: *mut c_void) -> *mut c_void {
        if prev.is_null() {
            // SAFETY: C passes the live parent the class belongs to.
            unsafe { (&raw mut (*obj.cast::<Parent>()).child).cast() }
        } else {
            null_mut()
        }
    }

    static PARENT_CLASS: Class = Class(ffi::AVClass {
        class_name: c"crustify-opt-parent".as_ptr(),
        option: &PARENT_OPTIONS.0[0],
        child_next: Some(child_next),
        ..class(None)
    });

    #[test]
    fn children_are_searched() {
        let p = Box::into_raw(Box::new(Parent {
            class: &PARENT_CLASS.0,
            child: Ctx::new(&CLASS),
        }));
        // SAFETY: `p` is a live parent of a well-formed class whose child
        // meets the contract; the list is in range.
        let ret = unsafe { set_options_string(p.cast(), c"int=4:string=child", c"=", c":") };
        assert_eq!(ret, Ok(2));
        // SAFETY: `p` is live; C has returned.
        let child = unsafe { &raw mut (*p).child };
        // SAFETY: as above.
        assert_eq!(unsafe { (*child).int }, 4);
        // SAFETY: as above.
        assert_eq!(string(unsafe { &*child }), Some(c"child"));
        // SAFETY: the child's owned fields hold values C stored; `p` came
        // from `Box::into_raw` and nothing else holds it.
        unsafe {
            ffi::av_opt_free(child.cast());
            drop(Box::from_raw(p));
        }
    }

    /// Distinct objects set from many threads at once.
    #[test]
    fn distinct_objects_on_many_threads() {
        std::thread::scope(|s| {
            for t in 0..8 {
                s.spawn(move || {
                    let mut o = Owned::new();
                    for i in 0..50 {
                        let opts =
                            CString::new(format!("int={}:string=t{t}i{i}:dict=a={i}", i % 100))
                                .unwrap();
                        assert_eq!(o.set(&opts), Ok(3));
                    }
                    assert_eq!(o.0.int, 49);
                });
            }
        });
    }
}
