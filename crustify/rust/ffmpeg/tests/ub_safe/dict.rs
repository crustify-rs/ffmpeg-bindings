//! UB tests for `libavutil/dict.h`'s dictionary owners and handles, through
//! the safe API only: empty and populated [`Dict`]s, [`DictionaryBox`]es,
//! their copies, conversions and handles, dropped in every order and moved
//! and shared across threads.

use core::ffi::CStr;
use std::ffi::CString;
use std::thread;

use ffmpeg::libavutil::dict::{
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
fn owners_move_between_threads() {
    let d = dict(&pairs(30, 9));
    let b = dict(&pairs(7, 2)).take().unwrap();
    let (d, b) = thread::spawn(move || (d.clone(), b.clone()))
        .join()
        .unwrap();
    thread::spawn(move || drop((d, b))).join().unwrap();
}

#[test]
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
