//! Equivalence of `libavutil/dict.h`'s owners with C: [`Dict::from_entries`]
//! against `av_dict_set` inserts, and the deep copy of [`Dict`] and
//! [`DictionaryBox`] against `av_dict_copy`, observed through C's readers
//! after every step.
//!
//! The copies match `av_dict_copy` with `AV_DICT_MULTIKEY`, which keeps every
//! entry. C's default copy (no flags) merges keys equal case-insensitively, so
//! it matches only for dictionaries without such duplicates
//! (`clone_matches_the_default_copy_without_duplicate_keys`).

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use ffmpeg::libavutil::dict::{Dict, DictionaryBox};
use ffmpeg::libavutil::ffi;

const MULTIKEY: c_int = ffi::AV_DICT_MULTIKEY as c_int;

/// A dictionary C owns, freed by `av_dict_free`.
struct CDict(*mut ffi::AVDictionary);

impl CDict {
    /// C's dictionary of `pairs`, each inserted by `av_dict_set` with `flags`.
    fn build(pairs: &[(CString, CString)], flags: c_int) -> Self {
        let mut d = Self(ptr::null_mut());
        for (k, v) in pairs {
            assert!(d.set(k, Some(v), flags) >= 0);
        }
        d
    }

    fn set(&mut self, key: &CStr, value: Option<&CStr>, flags: c_int) -> c_int {
        let value = value.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: our own slot; the strings are live for the call.
        unsafe { ffi::av_dict_set(&mut self.0, key.as_ptr(), value, flags) }
    }

    /// `av_dict_copy` of this dictionary into a NULL destination.
    fn copy(&self, flags: c_int) -> (c_int, Self) {
        let mut copy = Self(ptr::null_mut());
        // SAFETY: a fresh destination slot and a live (or NULL) source.
        let ret = unsafe { ffi::av_dict_copy(&mut copy.0, self.0, flags) };
        (ret, copy)
    }
}

impl Drop for CDict {
    fn drop(&mut self) {
        // SAFETY: our own slot, holding NULL or the dictionary C built.
        unsafe { ffi::av_dict_free(&mut self.0) }
    }
}

/// What C observes of the dictionary at `m`: `av_dict_count`, every entry
/// in `av_dict_iterate` order, and `av_dict_get_string`.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    count: c_int,
    entries: Vec<(CString, CString)>,
    string: CString,
}

fn observe(m: *const ffi::AVDictionary) -> Observed {
    // SAFETY: `m` is NULL or a live dictionary.
    let count = unsafe { ffi::av_dict_count(m) };
    let mut entries = Vec::new();
    let mut e = ptr::null();
    loop {
        // SAFETY: `m` is NULL or a live dictionary, `e` NULL or its previous
        // entry; nothing writes the dictionary during the walk.
        e = unsafe { ffi::av_dict_iterate(m, e) };
        if e.is_null() {
            break;
        }
        // SAFETY: `e` is an entry of `m`, whose strings are live and
        // NUL-terminated.
        let (k, v) = unsafe { (CStr::from_ptr((*e).key), CStr::from_ptr((*e).value)) };
        entries.push((k.to_owned(), v.to_owned()));
    }
    let mut buf: *mut c_char = ptr::null_mut();
    // SAFETY: `m` as above; `buf` is our out-slot.
    let ret = unsafe { ffi::av_dict_get_string(m, &mut buf, b'=' as c_char, b';' as c_char) };
    assert!(ret >= 0);
    // SAFETY: on success `buf` is an `av_malloc` NUL-terminated string.
    let string = unsafe { CStr::from_ptr(buf) }.to_owned();
    // SAFETY: `buf` is ours to free, once.
    unsafe { ffi::av_free(buf.cast()) };
    Observed {
        count,
        entries,
        string,
    }
}

/// What C observes of `d`, which is moved out and back to reach its pointer.
fn observe_dict(d: &mut Dict) -> Observed {
    let b = d.take();
    let seen = observe(b.as_ref().map_or(ptr::null(), |b| b.as_c_ptr()));
    *d = Dict::from(b);
    seen
}

/// Applies C's `av_dict_set` to the dictionary `d` owns, through its raw
/// pointer, as a C consumer handed the dictionary would.
fn c_set(d: &mut Dict, key: &CStr, value: Option<&CStr>, flags: c_int) -> c_int {
    let mut raw = CDict(
        Option::<DictionaryBox>::from(core::mem::take(d))
            .map_or(ptr::null_mut(), DictionaryBox::into_c),
    );
    let ret = raw.set(key, value, flags);
    // SAFETY: NULL or the dictionary C left in the slot, which `d` takes
    // back from the `CDict` (whose drop then sees NULL).
    *d = Dict::from(unsafe {
        DictionaryBox::from_c(core::mem::replace(&mut raw.0, ptr::null_mut()))
    });
    ret
}

/// A deterministic xorshift stream.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Keys from a small alphabet in both cases (so that C's case-insensitive
/// matching has duplicates to merge), the empty string included.
fn pairs(rng: &mut Rng, n: usize) -> Vec<(CString, CString)> {
    const KEYS: [&str; 7] = ["", "a", "A", "title", "TITLE", "k=v", "x;y\\z"];
    (0..n)
        .map(|i| {
            let k = KEYS[(rng.next() % KEYS.len() as u64) as usize];
            let v = format!("{i}:{}", "v".repeat((rng.next() % 40) as usize));
            (CString::new(k).unwrap(), CString::new(v).unwrap())
        })
        .collect()
}

#[test]
fn empty_dictionary_is_c_null() {
    let mut d = Dict::new();
    let c = CDict(ptr::null_mut());
    assert_eq!(observe_dict(&mut d), observe(c.0));
    let mut none = Dict::from_entries::<&CStr, &CStr>([]).unwrap();
    assert!(none.take().is_none());
    let (ret, c_copy) = c.copy(MULTIKEY);
    assert_eq!((ret, c_copy.0), (0, ptr::null_mut()));
    let mut copy = d.try_clone().unwrap();
    assert!(copy.as_ref().is_none());
    assert_eq!(observe_dict(&mut copy), observe(c_copy.0));
}

#[test]
fn from_entries_matches_c_multikey_inserts() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for n in [1, 2, 3, 7, 16, 64] {
        for _ in 0..8 {
            let p = pairs(&mut rng, n);
            for len in 1..=p.len() {
                let mut d = Dict::from_entries(p[..len].iter().map(|(k, v)| (k, v))).unwrap();
                let c = CDict::build(&p[..len], MULTIKEY);
                let seen = observe_dict(&mut d);
                assert_eq!(seen, observe(c.0), "{:?}", &p[..len]);
                assert_eq!(seen.entries, p[..len]);
            }
        }
    }
}

#[test]
fn clone_matches_c_multikey_copy_step_by_step() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for n in [1, 2, 5, 33] {
        let p = pairs(&mut rng, n);
        let mut original = Dict::from_entries(p.iter().map(|(k, v)| (k, v))).unwrap();
        let mut c_original = CDict::build(&p, MULTIKEY);

        // A copy of each.
        let mut copy = original.clone();
        let (ret, c_copy) = c_original.copy(MULTIKEY);
        assert_eq!(ret, 0);
        assert_eq!(observe_dict(&mut copy), observe(c_copy.0));
        assert_eq!(observe_dict(&mut original), observe(c_original.0));

        // Writes to the originals leave the copies alone.
        for (key, value) in [(c"a", Some(c"new")), (c"title", None), (c"", Some(c"e"))] {
            assert_eq!(
                c_set(&mut original, key, value, 0),
                c_original.set(key, value, 0)
            );
            assert_eq!(observe_dict(&mut original), observe(c_original.0));
            assert_eq!(observe_dict(&mut copy), observe(c_copy.0));
        }
        let mut rest = Some(original.try_clone().unwrap());
        let (_, c_rest) = c_original.copy(MULTIKEY);
        assert_eq!(observe_dict(rest.as_mut().unwrap()), observe(c_rest.0));

        // Copies outlive their sources, and copy on.
        // The box's clone too.
        drop((original, c_original));
        let boxed = copy.take().unwrap();
        let mut again = Dict::from(boxed.clone());
        let (_, c_again) = c_copy.copy(MULTIKEY);
        drop((boxed, c_copy));
        assert_eq!(observe_dict(&mut again), observe(c_again.0));
        drop(rest.take());
    }
}

#[test]
fn clone_matches_the_default_copy_without_duplicate_keys() {
    let unique: Vec<_> = [("a", "1"), ("title", "2"), ("", "3"), ("x;y\\z", "4=")]
        .iter()
        .map(|&(k, v)| (CString::new(k).unwrap(), CString::new(v).unwrap()))
        .collect();
    let d = Dict::from_entries(unique.iter().map(|(k, v)| (k, v))).unwrap();
    let (ret, c_copy) = CDict::build(&unique, 0).copy(0);
    assert_eq!(ret, 0);
    assert_eq!(observe_dict(&mut d.clone()), observe(c_copy.0));

    // With `a` and `A`, the default copy keeps one entry where the clone
    // keeps both, as the MULTIKEY copy does.
    let dup: Vec<_> = [("a", "1"), ("A", "2")]
        .iter()
        .map(|&(k, v)| (CString::new(k).unwrap(), CString::new(v).unwrap()))
        .collect();
    let d = Dict::from_entries(dup.iter().map(|(k, v)| (k, v))).unwrap();
    let c = CDict::build(&dup, MULTIKEY);
    let (_, merged) = c.copy(0);
    let (_, kept) = c.copy(MULTIKEY);
    assert_eq!(observe(merged.0).count, 1);
    let mut clone = d.clone();
    assert_eq!(observe_dict(&mut clone), observe(kept.0));
    assert_eq!(observe_dict(&mut clone).count, 2);
}
