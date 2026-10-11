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
