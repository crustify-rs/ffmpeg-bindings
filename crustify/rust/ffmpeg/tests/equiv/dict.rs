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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// Options wave 1, batch 0: the readers and writers of [`Dict`] and
/// `DictionaryRef` against `av_dict_get`, `av_dict_iterate`, `av_dict_count`,
/// `av_dict_get_string`, `av_dict_set`, `av_dict_set_int`,
/// `av_dict_parse_string` and `av_dict_copy` on independently owned
/// dictionaries, compared after every step of random write sequences.
mod readers_and_writers {
    use core::ffi::{CStr, c_char, c_int};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::dict::{Dict, Flags};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapString;

    use super::{CDict, Rng, observe, observe_dict};

    /// The write flags, as Rust and C spell them.
    const FLAGS: [(Flags, u32); 6] = [
        (Flags::MATCH_CASE, ffi::AV_DICT_MATCH_CASE),
        (Flags::IGNORE_SUFFIX, ffi::AV_DICT_IGNORE_SUFFIX),
        (Flags::DONT_OVERWRITE, ffi::AV_DICT_DONT_OVERWRITE),
        (Flags::APPEND, ffi::AV_DICT_APPEND),
        (Flags::MULTIKEY, ffi::AV_DICT_MULTIKEY),
        (Flags::DEDUP, ffi::AV_DICT_DEDUP),
    ];

    /// Keys that match one another in every way the flags tell apart: by
    /// case, as prefixes, and the empty key.
    const KEYS: [&CStr; 8] = [
        c"", c"a", c"A", c"ab", c"AB", c"title", c"TiTlE", c"k=v;x\\",
    ];

    fn flags(rng: &mut Rng) -> (Flags, c_int) {
        let mut f = (Flags::NONE, 0);
        for (rust, c) in FLAGS {
            if rng.next().is_multiple_of(3) {
                f = (f.0 | rust, f.1 | c as c_int);
            }
        }
        f
    }

    fn key(rng: &mut Rng) -> &'static CStr {
        KEYS[(rng.next() % KEYS.len() as u64) as usize]
    }

    fn value(rng: &mut Rng) -> CString {
        let n = (rng.next() % 6) as usize;
        CString::new(format!("v{}", "x".repeat(n))).unwrap()
    }

    fn heap(s: &CStr) -> HeapString {
        // SAFETY: `av_strdup` reads a live C string.
        let p = unsafe { ffi::av_strdup(s.as_ptr()) };
        // SAFETY: a fresh `av_malloc`-family string we own.
        unsafe { HeapString::from_raw(p) }.unwrap()
    }

    fn code(r: Result<(), i32>) -> c_int {
        r.err().unwrap_or(0)
    }

    /// One write, applied to `d` and to `c` with the flags C spells; their
    /// codes.
    fn write(rng: &mut Rng, d: &mut Dict, c: &mut CDict) -> (c_int, c_int, String) {
        let (rf, cf) = flags(rng);
        match rng.next() % 7 {
            0 | 1 => {
                let (k, v) = (key(rng), value(rng));
                let what = format!("set {k:?} {v:?} {cf:#x}");
                (code(d.set(k, &v, rf)), c.set(k, Some(&v), cf), what)
            }
            2 => {
                let k = key(rng);
                (
                    code(d.remove(k, rf)),
                    c.set(k, None, cf),
                    format!("remove {k:?} {cf:#x}"),
                )
            }
            3 => {
                let (k, v) = (key(rng), value(rng));
                let ret = code(d.set_owned(heap(k), heap(&v), rf));
                // C's own `DONT_STRDUP` write of strings it owns.
                let both = (ffi::AV_DICT_DONT_STRDUP_KEY | ffi::AV_DICT_DONT_STRDUP_VAL) as c_int;
                // SAFETY: two fresh `av_strdup` strings handed over to C, and
                // our own slot.
                let c_ret = unsafe {
                    ffi::av_dict_set(
                        &mut c.0,
                        ffi::av_strdup(k.as_ptr()),
                        ffi::av_strdup(v.as_ptr()),
                        cf | both,
                    )
                };
                (ret, c_ret, format!("set_owned {k:?} {v:?} {cf:#x}"))
            }
            4 => {
                let k = key(rng);
                let v = rng.next() as i64 >> (rng.next() % 64);
                // SAFETY: our own slot and a live key.
                let c_ret = unsafe { ffi::av_dict_set_int(&mut c.0, k.as_ptr(), v, cf) };
                (
                    code(d.set_int(k, v, rf)),
                    c_ret,
                    format!("set_int {k:?} {v} {cf:#x}"),
                )
            }
            5 => {
                const STRINGS: [&CStr; 6] = [
                    c"a=1:b=2",
                    c"title='x:y':A=\\=",
                    c" k = v ; x=y",
                    c"=v",
                    c"k",
                    c"",
                ];
                let s = STRINGS[(rng.next() % STRINGS.len() as u64) as usize];
                let (kv, pairs) = if rng.next().is_multiple_of(2) {
                    (c"=", c":")
                } else {
                    (c"=:", c";")
                };
                // SAFETY: our own slot and live strings.
                let c_ret = unsafe {
                    ffi::av_dict_parse_string(&mut c.0, s.as_ptr(), kv.as_ptr(), pairs.as_ptr(), cf)
                };
                let what = format!("parse_pairs {s:?} {kv:?} {pairs:?} {cf:#x}");
                (code(d.parse_pairs(s, kv, pairs, rf)), c_ret, what)
            }
            _ => {
                let n = (rng.next() % 4) as usize;
                let src: Vec<(CString, CString)> =
                    (0..n).map(|_| (key(rng).to_owned(), value(rng))).collect();
                let rust_src = Dict::from_entries(src.iter().map(|(k, v)| (k, v))).unwrap();
                let c_src = CDict::build(&src, ffi::AV_DICT_MULTIKEY as c_int);
                // SAFETY: our own slot and a live (or NULL) source.
                let c_ret = unsafe { ffi::av_dict_copy(&mut c.0, c_src.0, cf) };
                let what = format!("copy_from {src:?} {cf:#x}");
                (code(d.copy_from(rust_src.as_ref(), rf)), c_ret, what)
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn write_sequences_agree_after_every_step() {
        let mut rng = Rng(0x0dd_b1a5_e5ca_1ab1);
        for _ in 0..200 {
            let mut d = Dict::new();
            let mut c = CDict(ptr::null_mut());
            let mut log = Vec::new();
            for _ in 0..(rng.next() % 30) {
                let (rust, c_ret, what) = write(&mut rng, &mut d, &mut c);
                log.push(what);
                assert_eq!(rust, c_ret, "{log:?}");
                assert_eq!(observe_dict(&mut d), observe(c.0), "{log:?}");
                assert_eq!(d.is_empty(), c.0.is_null(), "{log:?}");
            }
        }
    }

    /// The entries of C's `av_dict_get(m, key, prev, flags)` walk.
    fn c_matches(m: *const ffi::AVDictionary, key: &CStr, flags: c_int) -> Vec<(CString, CString)> {
        let mut out = Vec::new();
        let mut e: *const ffi::AVDictionaryEntry = ptr::null();
        loop {
            // SAFETY: `m` is NULL or live, `e` NULL or its previous match.
            e = unsafe { ffi::av_dict_get(m, key.as_ptr(), e, flags) };
            if e.is_null() {
                return out;
            }
            // SAFETY: a live entry of `m`.
            let (k, v) = unsafe { (CStr::from_ptr((*e).key), CStr::from_ptr((*e).value)) };
            out.push((k.to_owned(), v.to_owned()));
        }
    }

    /// `av_dict_get_string(m, kv, pairs)`: the code and the string.
    fn c_string(m: *const ffi::AVDictionary, kv: u8, pairs: u8) -> (c_int, Option<CString>) {
        let mut buf: *mut c_char = ptr::null_mut();
        // SAFETY: `m` NULL or live, `buf` our slot.
        let ret = unsafe { ffi::av_dict_get_string(m, &mut buf, kv as c_char, pairs as c_char) };
        if ret < 0 {
            return (ret, None);
        }
        // SAFETY: a fresh string we own, read then freed once.
        let s = unsafe { CStr::from_ptr(buf) }.to_owned();
        // SAFETY: as above.
        unsafe { ffi::av_free(buf.cast()) };
        (ret, Some(s))
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn readers_agree_on_every_key_flag_and_separator() {
        let mut rng = Rng(0x005e_ed0f_d1c7);
        let seps = [b'=', b';', b':', b',', b'\\', 0, b' '];
        for round in 0..60 {
            let n = (round % 9) as usize;
            let pairs: Vec<(CString, CString)> = (0..n)
                .map(|_| (key(&mut rng).to_owned(), value(&mut rng)))
                .collect();
            let d = Dict::from_entries(pairs.iter().map(|(k, v)| (k, v))).unwrap();
            let c = CDict::build(&pairs, ffi::AV_DICT_MULTIKEY as c_int);
            let owned = |e: ffmpeg::libavutil::dict::DictionaryEntryRef<'_>| {
                (e.key().to_owned(), e.value().to_owned())
            };
            // Through the owner and, when not empty, the shared handle.
            let iter: Vec<_> = d.iter().map(owned).collect();
            assert_eq!(iter, observe(c.0).entries);
            assert_eq!(d.len(), observe(c.0).count as usize);
            assert_eq!(d.iter().len(), d.len());
            if let Some(r) = d.as_ref() {
                assert_eq!(r.iter().map(owned).collect::<Vec<_>>(), iter);
                assert_eq!(r.into_iter().count(), r.len());
                assert!(!r.is_empty());
            }
            for key in KEYS.iter().copied().chain([c"t", c"TITLEX", c"zzz"]) {
                for bits in 0..4u32 {
                    let rf = [
                        Flags::NONE,
                        Flags::MATCH_CASE,
                        Flags::IGNORE_SUFFIX,
                        Flags::MATCH_CASE | Flags::IGNORE_SUFFIX,
                    ][bits as usize];
                    let cf = bits as c_int;
                    let all = c_matches(c.0, key, cf);
                    assert_eq!(d.get_all(key, rf).map(owned).collect::<Vec<_>>(), all);
                    assert_eq!(d.get(key, rf).map(owned), all.first().cloned());
                    if let Some(r) = d.as_ref() {
                        assert_eq!(r.get_all(key, rf).map(owned).collect::<Vec<_>>(), all);
                        assert_eq!(r.get(key, rf).map(owned), all.first().cloned());
                    }
                }
            }
            for kv in seps {
                for p in seps {
                    let (ret, s) = c_string(c.0, kv, p);
                    let rust = d.serialize(kv, p);
                    assert_eq!(rust.as_ref().err().copied().unwrap_or(0), ret, "{kv} {p}");
                    assert_eq!(rust.ok().map(|h| h.as_c_str().to_owned()), s);
                    if let Some(r) = d.as_ref() {
                        assert_eq!(
                            r.serialize(kv, p).ok().map(|h| h.as_c_str().to_owned()),
                            c_string(c.0, kv, p).1
                        );
                    }
                }
            }
        }
    }

    /// A serialized dictionary parses back into the same entries, in C and in
    /// Rust, for keys and values holding the separators.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn serialized_strings_parse_back_as_in_c() {
        let pairs: Vec<(CString, CString)> =
            [("a", "1"), ("k=v", "x;y"), ("b\\", " sp "), ("t", "")]
                .iter()
                .map(|&(k, v)| (CString::new(k).unwrap(), CString::new(v).unwrap()))
                .collect();
        let d = Dict::from_entries(pairs.iter().map(|(k, v)| (k, v))).unwrap();
        let s = d.serialize(b'=', b';').unwrap();
        let mut back = Dict::new();
        let mut c = CDict(ptr::null_mut());
        let ret = back.parse_pairs(s.as_c_str(), c"=", c";", Flags::NONE);
        // SAFETY: our own slot and live strings.
        let c_ret = unsafe {
            ffi::av_dict_parse_string(&mut c.0, s.as_ptr(), c"=".as_ptr(), c";".as_ptr(), 0)
        };
        assert_eq!(code(ret), c_ret);
        assert_eq!(observe_dict(&mut back), observe(c.0));
    }
}
