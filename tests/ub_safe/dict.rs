//! UB tests for `libavutil/dict.h`'s dictionary owners and handles, through
//! the safe API only: empty and populated [`Dict`]s, [`DictionaryBox`]es,
//! their copies, conversions and handles, dropped in every order and moved
//! and shared across threads.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

use core::ffi::CStr;
use std::ffi::CString;
use std::thread;

use ffmpeg_bindings::libavutil::libavutil::dict::{
    Dict, DictionaryBox, DictionaryEntryMut, DictionaryEntryRef, DictionaryFree, DictionaryMut,
    DictionaryRef,
};

fn pairs(n: usize, len: usize) -> Vec<(CString, CString)> {
    (0..n)
        .map(|i| {
            let k = CString::new(format!("key{}", i % 7)).unwrap();
            let v = CString::new(vec![b'a' + (i % 26) as u8; len]).unwrap();
            (k, v)
        })
        .collect()
}

fn dict(p: &[(CString, CString)]) -> Dict {
    Dict::from_entries(p.iter().map(|(k, v)| (k, v))).unwrap()
}

#[test]
fn owners_and_handles_are_thread_safe() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Dict>();
    send_sync::<DictionaryBox>();
    send_sync::<DictionaryFree>();
    send_sync::<DictionaryRef<'_>>();
    send_sync::<DictionaryMut<'_>>();
    send_sync::<DictionaryEntryRef<'_>>();
    send_sync::<DictionaryEntryMut<'_>>();
}

#[test]
fn empty_dictionary_has_no_object() {
    let mut d = Dict::new();
    assert!(d.as_ref().is_none() && d.as_mut().is_none());
    assert!(d.take().is_none());
    let copy = d.clone();
    assert!(copy.try_clone().unwrap().as_ref().is_none());
    assert!(Dict::default().as_ref().is_none());
    assert!(
        Dict::from_entries::<&CStr, &CStr>([])
            .unwrap()
            .as_ref()
            .is_none()
    );
    assert!(Option::<DictionaryBox>::from(Dict::from(None)).is_none());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_entry_shape_builds_copies_and_frees() {
    for (n, len) in [(1, 0), (1, 1), (3, 17), (64, 5), (2000, 2), (8, 4096)] {
        let d = dict(&pairs(n, len));
        assert!(d.as_ref().is_some());
        let c = d.clone();
        let t = c.try_clone().unwrap();
        drop((d, c, t));
    }
    // Empty keys and values, and keys equal case-insensitively, all stay.
    let d = Dict::from_entries([(c"", c""), (c"a", c"1"), (c"A", c"2"), (c"a", c"")]).unwrap();
    drop(d.clone());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn copies_outlive_their_sources() {
    let p = pairs(40, 33);
    let first = dict(&p);
    let second = first.clone();
    drop(first);
    let third = second.try_clone().unwrap();
    drop(second);
    let mut boxed = third.clone().take().unwrap();
    drop(third);
    let reboxed = boxed.clone();
    let _ = boxed.as_mut();
    drop(boxed);
    let _ = reboxed.as_ref();
    drop(reboxed.try_clone().unwrap());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn conversions_move_the_dictionary_without_copying() {
    let mut d = dict(&pairs(5, 3));
    let b = d.take().unwrap();
    assert!(d.as_ref().is_none());
    d = Dict::from(b);
    assert!(d.as_mut().is_some());
    let b: Option<DictionaryBox> = d.into();
    let d = Dict::from(b);
    let mut e = d;
    let _ = e.as_ref();
    drop(e.take());
    drop(e);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn handles_borrow_for_their_owner() {
    let mut d = dict(&pairs(3, 3));
    {
        let r = d.as_ref().unwrap();
        let r2 = r;
        let copy = d.try_clone().unwrap();
        let _ = (r, r2, copy.as_ref());
    }
    let mut m = d.as_mut().unwrap();
    let _ = m.as_ref();
    let _ = m.as_mut();
    drop(d);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn owners_move_between_threads() {
    let d = dict(&pairs(30, 9));
    let b = dict(&pairs(7, 2)).take().unwrap();
    let (d, b) = thread::spawn(move || (d.clone(), b.clone()))
        .join()
        .unwrap();
    thread::spawn(move || drop((d, b))).join().unwrap();
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn shared_dictionaries_copy_concurrently() {
    let d = dict(&pairs(50, 12));
    let b = dict(&pairs(20, 3)).take().unwrap();
    thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..50 {
                    let copy = d.try_clone().unwrap();
                    let boxed = b.clone();
                    let _ = (d.as_ref(), b.as_ref(), copy.as_ref());
                    drop((copy, boxed));
                }
            });
        }
        let r = d.as_ref().unwrap();
        s.spawn(move || {
            let _ = r;
        });
    });
}

/// Options wave 1, batch 0: the readers and writers through the safe API:
/// every write path and flag on empty, small and large dictionaries, owned
/// strings handed over on every path (`LeakSanitizer` sees any C leak or double
/// free), entries and iterators read up to each write the borrow checker
/// lets through, and readers sharing one dictionary across threads.
mod readers_and_writers {
    use core::ffi::CStr;
    use std::ffi::CString;
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::dict::{
        Dict, DictionaryEntryRef, Flags, Iter, Matches,
    };
    use ffmpeg_bindings::libavutil::libavutil::mem::{HeapString, string_from_c_str};

    const ALL: [Flags; 6] = [
        Flags::MATCH_CASE,
        Flags::IGNORE_SUFFIX,
        Flags::DONT_OVERWRITE,
        Flags::APPEND,
        Flags::MULTIKEY,
        Flags::DEDUP,
    ];

    /// Every combination of the flags.
    fn combinations() -> impl Iterator<Item = Flags> {
        (0..1u32 << ALL.len()).map(|bits| {
            ALL.iter()
                .enumerate()
                .filter(|(i, _)| bits & 1 << i != 0)
                .fold(Flags::NONE, |f, (_, &g)| f | g)
        })
    }

    fn heap(s: &CStr) -> HeapString {
        string_from_c_str(s).unwrap()
    }

    /// Reads every entry, key and value, and the serialized string.
    fn read_all(d: &Dict) -> usize {
        let mut bytes = 0;
        for e in d {
            bytes += e.key().count_bytes() + e.value().count_bytes();
        }
        assert_eq!(d.iter().len(), d.len());
        if let Ok(s) = d.serialize(b'=', b';') {
            bytes += s.len();
        }
        bytes
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_write_and_flag_on_every_size() {
        let long = CString::new(vec![b'q'; 4096]).unwrap();
        let keys = [c"", c"a", c"A", c"ab", long.as_c_str()];
        for flags in combinations() {
            let mut d = Dict::new();
            for (i, &k) in keys.iter().enumerate() {
                let v = if i % 2 == 0 { long.as_c_str() } else { c"v" };
                d.set(k, v, flags).unwrap();
                d.set_owned(heap(k), heap(v), flags).unwrap();
                d.set_int(k, i64::MIN + i as i64, flags).unwrap();
                read_all(&d);
            }
            let _ = d.parse_pairs(c"a=1:b='x:y':=bad", c"=", c":", flags);
            let src = d.clone();
            d.copy_from(src.as_ref(), flags).unwrap();
            d.copy_from(None, flags).unwrap();
            read_all(&d);
            for &k in &keys {
                d.remove(k, flags).unwrap();
            }
            read_all(&d);
            // Remove everything that is left, case-insensitively by prefix.
            while !d.is_empty() {
                d.remove(c"", Flags::IGNORE_SUFFIX).unwrap();
            }
            assert!(d.as_ref().is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn large_dictionaries_grow_and_shrink() {
        let mut d = Dict::new();
        for i in 0..3000 {
            let k = CString::new(format!("key{i}")).unwrap();
            d.set_int(&k, i, Flags::NONE).unwrap();
        }
        assert_eq!(d.len(), 3000);
        for i in (0..3000).step_by(2) {
            let k = CString::new(format!("KEY{i}")).unwrap();
            d.remove(&k, Flags::NONE).unwrap();
        }
        assert_eq!(d.len(), 1500);
        assert_eq!(d.get_all(c"key1", Flags::IGNORE_SUFFIX).count(), 556);
        read_all(&d);
        let copy = d.clone();
        d.copy_from(copy.as_ref(), Flags::MULTIKEY).unwrap();
        assert_eq!(d.len(), 3000);
        drop(copy);
        read_all(&d);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn malformed_and_separator_heavy_strings_parse_without_overreading() {
        for s in [
            c"",
            c"=",
            c"==",
            c":",
            c"a",
            c"a=",
            c"=b",
            c"a=b:",
            c"a=b::c=d",
            c"'",
            c"a='",
            c"\\",
            c"a=\\",
            c"a=b\\",
            c"'a'='b':'c",
            c"  a  =  b  ",
            c"a=b;c=d",
        ] {
            for (kv, pairs) in [(c"=", c":"), (c"", c""), (c"=:", c":="), (c"'", c"\\")] {
                let mut d = Dict::new();
                let _ = d.parse_pairs(s, kv, pairs, Flags::NONE);
                read_all(&d);
            }
        }
        for kv in [0u8, b'\\', b'=', b';', b'\'', 0xFF] {
            for pairs in [0u8, b'\\', b'=', b';', b'\'', 0xFF] {
                let d = Dict::from_entries([(c"a=;\\'", c" x "), (c"", c"")]).unwrap();
                let _ = d.serialize(kv, pairs);
                let _ = Dict::new().serialize(kv, pairs);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn entries_and_iterators_live_until_the_next_write() {
        let mut d = Dict::from_entries([(c"a", c"1"), (c"b", c"2"), (c"A", c"3")]).unwrap();
        let held: Vec<DictionaryEntryRef<'_>> = d.get_all(c"a", Flags::NONE).collect();
        let mut it = d.iter();
        let first = it.next().unwrap();
        let r = d.as_ref().unwrap();
        let from_ref = r.get(c"b", Flags::NONE).unwrap();
        // Every borrow is read after the others were taken.
        assert_eq!(held.len(), 2);
        assert_eq!(first.key(), c"a");
        assert_eq!(from_ref.value(), c"2");
        assert_eq!(it.count(), 2);
        assert!(held.iter().all(|e| !e.value().is_empty()));
        // Then a write, after which new borrows read the new entries.
        d.set(c"b", c"22", Flags::NONE).unwrap();
        assert_eq!(d.get(c"B", Flags::NONE).unwrap().value(), c"22");
        let taken = d.take().unwrap();
        let r = taken.as_ref();
        let e = r.get(c"a", Flags::MATCH_CASE).unwrap();
        drop(d);
        assert_eq!(e.value(), c"1");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn readers_share_one_dictionary_across_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Flags>();
        send_sync::<Iter<'_>>();
        send_sync::<Matches<'_, '_>>();
        let pairs: Vec<(CString, CString)> = (0..200)
            .map(|i| {
                let k = CString::new(format!("k{}", i % 37)).unwrap();
                (k, CString::new(format!("value {i}")).unwrap())
            })
            .collect();
        let d = Dict::from_entries(pairs.iter().map(|(k, v)| (k, v))).unwrap();
        thread::scope(|s| {
            for t in 0..8 {
                let d = &d;
                s.spawn(move || {
                    for _ in 0..20 {
                        read_all(d);
                        let k = CString::new(format!("K{t}")).unwrap();
                        let n = d.get_all(&k, Flags::NONE).count();
                        assert!(n > 0);
                        assert!(d.get(&k, Flags::MATCH_CASE).is_none());
                    }
                });
            }
            // Iterators and entries move to other threads with the borrow.
            let it = d.iter();
            let m = d.get_all(c"k1", Flags::IGNORE_SUFFIX);
            let e = d.get(c"k2", Flags::NONE).unwrap();
            s.spawn(move || (it.count(), m.count(), e.value().count_bytes()));
        });
        // Owners with every entry written on another thread drop here.
        let mut moved = thread::spawn(move || {
            let mut d = d;
            d.set(c"late", c"x", Flags::MULTIKEY).unwrap();
            d
        })
        .join()
        .unwrap();
        moved.remove(c"late", Flags::NONE).unwrap();
        read_all(&moved);
    }
}
