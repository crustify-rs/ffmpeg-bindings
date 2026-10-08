//! UB tests for `libavutil/opt.h`'s unsafe range queries, each called within
//! its `# Safety` contract, and for the safe getters of the lists they return.

use core::ffi::{CStr, c_int};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::{OptionRanges, OptionRangesBox, OptionRangesRef};

#[path = "../common/opt.rs"]
mod fixture;
use fixture::{CB_FAKE, CLASS, Ctx, EINVAL, FAKE, FakeObj, RANGED, UNKNOWN, UNRANGED};

const FAKE_OBJ: c_int = ffi::AV_OPT_SEARCH_FAKE_OBJ as c_int;
const MULTI: c_int = ffi::AV_OPT_MULTI_COMPONENT_RANGE as c_int;
const CHILDREN: c_int = ffi::AV_OPT_SEARCH_CHILDREN as c_int;
const ENOMEM: i32 = -12;
const ENOSYS: i32 = -38;

/// Reads every count, element, bound and string of `r`, through the safe
/// getters and each element's raw fields; returns the element count.
fn read_all(r: OptionRangesRef<'_>) -> usize {
    let (n, c) = (r.range_count(), r.component_count());
    let all = r.ranges();
    assert_eq!(all.len(), n * c);
    for comp in 0..c {
        for i in 0..n {
            let p = r.get(i, comp).unwrap();
            assert_eq!(Some(p), all.elem(comp * n + i));
            if p.is_null() {
                continue;
            }
            // SAFETY: a non-NULL element addresses a range the live list owns.
            let e = unsafe { p.read() };
            assert!(e.value_min <= e.value_max);
            if !e.str_.is_null() {
                // SAFETY: a range's non-NULL `str` is an owned C string.
                assert!(!unsafe { CStr::from_ptr(e.str_) }.to_bytes().is_empty());
            }
        }
    }
    assert!(r.get(n, 0).is_none() && r.get(0, c).is_none());
    assert!(all.elem(n * c).is_none());
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
        assert!(sparse.as_ref().get(1, 0).unwrap().is_null());
        // Drop out of creation order.
        drop(one);
        drop(b);
        drop(sparse);
    }

    #[test]
    fn views_and_copies_outlive_handles_but_not_the_owner() {
        let b = query(&CB_FAKE, c"size", MULTI).unwrap();
        // A view taken through a temporary handle keeps the owner's borrow.
        let view = b.as_ref().ranges();
        let copies: Vec<_> = view.elems().collect();
        let again = b.as_ref().ranges();
        assert_eq!(copies, again.elems().collect::<Vec<_>>());
        // Sub-runs stay within the run.
        let (head, tail) = view.split_at(2).unwrap();
        assert_eq!((head.len(), tail.len()), (2, 4));
        assert!(view.split_at(7).is_none() && view.slice(5..7).is_none());
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
        assert!(back.as_ref().get(1, 0).unwrap().is_null());

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
