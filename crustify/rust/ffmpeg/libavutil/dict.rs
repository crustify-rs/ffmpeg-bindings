//! Safe wrappers over `libavutil/dict.h`.
//!
//! [`Dictionary`] is the layout type of `AVDictionary`, a list of key/value
//! string pairs whose body is private to `dict.c`: Rust, like every public C
//! consumer, reaches it only through a pointer, and NULL is the empty
//! dictionary wherever C takes one. C has no constructor for it: `av_dict_set`
//! allocates a dictionary when it inserts into a NULL `AVDictionary *`, and
//! frees it (writing NULL back) when its last entry is deleted, so a non-NULL
//! dictionary always holds at least one entry, and a routine that inserts or
//! deletes needs the owner's slot rather than the object.
//!
//! Two owners follow from that:
//!
//! - [`Dict`], an owned dictionary that may be empty: C's `AVDictionary *`
//!   variable, NULL when empty. It is built empty by [`Dict::new`] or from
//!   pairs by [`Dict::from_entries`], and is what a routine that inserts or
//!   deletes acts on, since it may allocate or free the dictionary.
//! - [`DictionaryBox`], the sole owner of a non-empty dictionary, released
//!   with every entry by `av_dict_free` and deep-copied, duplicate keys
//!   included, by `av_dict_copy`. A [`Dict`] holds one when it is not empty.
//!
//! Both hand out a [`DictionaryRef`] for the C readers. [`DictionaryMut`] has
//! no setters: every writer of a dictionary can free it, so writes go through
//! [`Dict`].
//!
//! [`DictionaryEntry`] is one `AVDictionaryEntry`, which exists only inside a
//! dictionary's entry array. It is only borrowed, as a [`DictionaryEntryRef`]
//! whose key and value borrow from the dictionary for the handle's lifetime;
//! the handle's lifetime is that of a shared borrow of the dictionary, since
//! any insertion or deletion moves or frees the entries. C forbids callers to
//! change an entry, so [`DictionaryEntryMut`] has no setters either.

use core::ffi::{CStr, c_int};
use core::fmt;
use core::ptr::{self, NonNull, addr_of};

use ffibox::{CBox, CDrop, CDupClone, define_ctype};

use super::ffi;

/// `AV_DICT_MULTIKEY`: insert without looking for an equal key, so that
/// every pair lands, in order, duplicates included.
const MULTIKEY: c_int = ffi::AV_DICT_MULTIKEY as c_int;

define_ctype!(
    /// Wraps: AVDictionary
    ///
    /// A dictionary of key/value strings, opaque outside `dict.c`. Owned as a
    /// [`Dict`] (possibly empty) or a [`DictionaryBox`] (never empty), and
    /// read through a [`DictionaryRef`].
    Dictionary,
    DictionaryRef,
    DictionaryMut,
    ffi::AVDictionary
);

// SAFETY: a dictionary is `av_malloc` storage (the struct, its entry array
// and every key and value) with no lock, refcount, thread-local state or
// registration anywhere, and `av_dict_free` releases it through `av_free`,
// which any thread may call. Moving the owner or an exclusive handle to
// another thread therefore moves plain heap data; `DictionaryMut` has no
// setters.
unsafe impl Send for Dictionary {}
// SAFETY: as above.
unsafe impl Send for DictionaryMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes. The only C
// routines a `DictionaryRef` may reach are those taking a `const
// AVDictionary *` (`av_dict_count`, `av_dict_iterate`, `av_dict_get`,
// `av_dict_get_string`, and `av_dict_copy`'s source, which the clone uses),
// and they only read; the struct has no interior mutability, so concurrent
// shared access reads unchanging bytes.
unsafe impl Sync for Dictionary {}
// SAFETY: as for `Sync for Dictionary`: copies of a shared handle only read.
unsafe impl Send for DictionaryRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for DictionaryRef<'_> {}
// SAFETY: `&DictionaryMut` reaches the dictionary only through `as_ref`, a
// shared handle, and the exclusive handle has no setters either.
unsafe impl Sync for DictionaryMut<'_> {}

/// `av_dict_free` and `av_dict_copy`, as the policy of an owned
/// [`Dictionary`].
///
/// Thread-agnostic: it releases through `av_free`, which any thread may call,
/// so a [`DictionaryBox`] is `Send` and `Sync`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DictionaryFree;

/// The sole owner of a non-empty dictionary, released with every key and
/// value by `av_dict_free`.
///
/// `Clone` is a deep copy through `av_dict_copy` that keeps every entry in
/// order, duplicate keys included (`AV_DICT_MULTIKEY`), so the copy iterates
/// exactly as the original does; `try_clone` returns `None` when C runs out
/// of memory, where `clone` aborts.
pub type DictionaryBox = CBox<Dictionary, DictionaryFree>;

/// Wraps: av_dict_free
/// Handwritten
///
/// `impl_cdrop!` would need a raw-pointer adapter only to pass the
/// dictionary's slot; the trait method holds the slot itself.
// SAFETY: `av_dict_free` releases a dictionary `av_dict_set` (directly or
// through `av_dict_copy`) allocated: each entry's key and value, the entry
// array, then the struct, exactly once. The only writer a safe path reaches
// is `av_dict_set` on a `Dict`'s slot, which keeps it such a dictionary.
unsafe impl CDrop<Dictionary> for DictionaryFree {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<Dictionary>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVDictionary>();
        // SAFETY: the caller hands over a live dictionary whose claim it
        // owns; `av_dict_free` releases it once and NULLs only our local
        // slot.
        unsafe { ffi::av_dict_free(&mut slot) }
    }
}

/// Wraps: av_dict_copy
/// Handwritten
///
/// The copy needs a fresh destination slot, the `AV_DICT_MULTIKEY` flag (with
/// no flags C overwrites a key equal to an earlier one, case-insensitively,
/// and the copy loses entries) and the release of a partial copy on failure,
/// which `impl_cdupclone!` cannot express.
// SAFETY: `av_dict_copy` into a NULL destination allocates a new dictionary
// through `av_dict_set` and inserts a fresh `av_strdup` of every key and
// value of the source, which it only reads: the copy shares nothing with the
// source and `c_drop` releases it. On failure the partial copy is freed here
// and `None` returned.
unsafe impl CDupClone<Dictionary> for DictionaryFree {
    unsafe fn c_dup(&self, ptr: NonNull<Dictionary>) -> Option<NonNull<Dictionary>> {
        let mut copy = ptr::null_mut();
        let src = ptr.as_ptr().cast::<ffi::AVDictionary>();
        // SAFETY: `src` is a live dictionary, per the caller, distinct from
        // the local destination slot `av_dict_copy` writes.
        let ret = unsafe { ffi::av_dict_copy(&mut copy, src, MULTIKEY) };
        if ret < 0 {
            // SAFETY: `copy` is NULL or the partial copy C allocated, which
            // nothing else references.
            unsafe { ffi::av_dict_free(&mut copy) };
            return None;
        }
        // A source always holds an entry, so a successful copy is non-NULL.
        NonNull::new(copy.cast())
    }
}

/// An owned dictionary, empty or not: C's `AVDictionary *` variable, which
/// holds NULL when the dictionary is empty.
///
/// Built empty by [`new`](Self::new) (or `Default`), or from key/value pairs
/// by [`from_entries`](Self::from_entries); dropping it frees every entry.
/// `Clone` deep-copies the entries, duplicate keys included, as
/// [`DictionaryBox`]'s does, and aborts where [`try_clone`](Self::try_clone)
/// returns `None`.
///
/// The handles borrow it, so it cannot drop or change while one lives.
#[derive(Clone, Debug, Default)]
pub struct Dict(Option<DictionaryBox>);

impl Dict {
    /// The empty dictionary, C's NULL `AVDictionary *`.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self(None)
    }

    /// A dictionary of `entries`, in order and duplicates included, each key
    /// and value copied; `None` when C runs out of memory, after freeing
    /// what it built.
    ///
    /// No pair is merged with another: iterating the result yields `entries`
    /// as given, as C does for pairs inserted with `AV_DICT_MULTIKEY`. No
    /// entries is the empty dictionary.
    pub fn from_entries<K, V>(entries: impl IntoIterator<Item = (K, V)>) -> Option<Self>
    where
        K: AsRef<CStr>,
        V: AsRef<CStr>,
    {
        let mut dict = Self::new();
        for (key, value) in entries {
            if dict.append(key.as_ref(), value.as_ref()) < 0 {
                return None;
            }
        }
        Some(dict)
    }

    /// Shared handle to the dictionary; `None` when it is empty.
    #[inline]
    #[must_use]
    pub fn as_ref(&self) -> Option<DictionaryRef<'_>> {
        self.0.as_ref().map(CBox::as_ref)
    }

    /// Exclusive handle to the dictionary; `None` when it is empty.
    #[inline]
    #[must_use]
    pub fn as_mut(&mut self) -> Option<DictionaryMut<'_>> {
        self.0.as_mut().map(CBox::as_mut)
    }

    /// Takes the dictionary out, leaving this one empty; `None` when it is
    /// empty.
    #[inline]
    pub fn take(&mut self) -> Option<DictionaryBox> {
        self.0.take()
    }

    /// A deep copy of the dictionary, duplicate keys included; `None` when C
    /// runs out of memory. An empty dictionary copies without allocating.
    #[must_use]
    pub fn try_clone(&self) -> Option<Self> {
        match &self.0 {
            None => Some(Self::new()),
            Some(dict) => dict.try_clone().map(|copy| Self(Some(copy))),
        }
    }

    /// Appends `key` and `value`, copied, with `AV_DICT_MULTIKEY`: C's
    /// `av_dict_set` on this dictionary's slot, which allocates the
    /// dictionary when it is empty. Returns C's status.
    fn append(&mut self, key: &CStr, value: &CStr) -> c_int {
        let mut slot = self.0.take().map_or(ptr::null_mut(), CBox::into_c);
        let (key, value) = (key.as_ptr(), value.as_ptr());
        // SAFETY: `slot` is NULL or the dictionary this owner held; `key`
        // and `value` are NUL-terminated strings live for the call.
        let ret = unsafe { ffi::av_dict_set(&mut slot, key, value, MULTIKEY) };
        // SAFETY: `av_dict_set` leaves NULL or the dictionary it allocated
        // or grew in the slot, referenced by nothing else, which this owner
        // takes back.
        self.0 = unsafe { DictionaryBox::from_c(slot) };
        ret
    }
}

impl From<DictionaryBox> for Dict {
    #[inline]
    fn from(dict: DictionaryBox) -> Self {
        Self(Some(dict))
    }
}

impl From<Option<DictionaryBox>> for Dict {
    #[inline]
    fn from(dict: Option<DictionaryBox>) -> Self {
        Self(dict)
    }
}

impl From<Dict> for Option<DictionaryBox> {
    #[inline]
    fn from(dict: Dict) -> Self {
        dict.0
    }
}

define_ctype!(
    /// Wraps: AVDictionaryEntry
    ///
    /// One key/value pair of a dictionary, borrowed from it as a
    /// [`DictionaryEntryRef`]. Both strings are owned by the dictionary,
    /// never NULL, and live until the next insertion or deletion, which the
    /// handle's borrow of the dictionary excludes.
    DictionaryEntry,
    DictionaryEntryRef,
    DictionaryEntryMut,
    ffi::AVDictionaryEntry
);

// SAFETY: an entry is two pointers to `av_malloc` strings owned by its
// dictionary, with no thread affinity (see `Send for Dictionary`), and
// `DictionaryEntryMut` has no setters.
unsafe impl Send for DictionaryEntry {}
// SAFETY: as above.
unsafe impl Send for DictionaryEntryMut<'_> {}

// SAFETY: the getters only read, nothing in an entry is interior-mutable,
// and C writes an entry only inside `av_dict_set`, which needs the
// dictionary's slot and so excludes every handle borrowed from it.
unsafe impl Sync for DictionaryEntry {}
// SAFETY: as for `Sync for DictionaryEntry`: copies of a shared handle only
// read.
unsafe impl Send for DictionaryEntryRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for DictionaryEntryRef<'_> {}
// SAFETY: `&DictionaryEntryMut` reaches the entry only through `as_ref`, a
// shared handle, and the exclusive handle has no setters either.
unsafe impl Sync for DictionaryEntryMut<'_> {}

impl<'a> DictionaryEntryRef<'a> {
    /// Field: AVDictionaryEntry.key
    ///
    /// The entry's key, as it was inserted (lookups compare it
    /// case-insensitively unless asked not to).
    ///
    /// # Panics
    ///
    /// If the key is NULL, which `av_dict_set` never stores.
    pub fn key(&self) -> &'a CStr {
        // SAFETY: a read of a projected field through the shared handle.
        let key = unsafe { addr_of!((*self.as_ptr()).key).read() };
        let key = NonNull::new(key).expect("a dictionary entry has a key");
        // SAFETY: the key of an entry of a dictionary, a NUL-terminated
        // string the handle's borrow keeps live and unwritten for `'a`.
        unsafe { CStr::from_ptr(key.as_ptr()) }
    }

    /// Field: AVDictionaryEntry.value
    ///
    /// The entry's value.
    ///
    /// # Panics
    ///
    /// If the value is NULL, which `av_dict_set` never stores.
    pub fn value(&self) -> &'a CStr {
        // SAFETY: a read of a projected field through the shared handle.
        let value = unsafe { addr_of!((*self.as_ptr()).value).read() };
        let value = NonNull::new(value).expect("a dictionary entry has a value");
        // SAFETY: as for `key`: C writes a value only under `av_dict_set`,
        // which the handle's borrow of the dictionary excludes.
        unsafe { CStr::from_ptr(value.as_ptr()) }
    }
}

impl fmt::Debug for DictionaryEntryRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DictionaryEntry")
            .field("key", &self.key())
            .field("value", &self.value())
            .finish()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use core::ffi::c_char;
    use std::ffi::CString;
    use std::panic::catch_unwind;

    /// The entries of the dictionary at `m` (NULL: none), read by C's
    /// `av_dict_iterate`.
    fn c_entries(m: *const ffi::AVDictionary) -> Vec<(CString, CString)> {
        let mut out = Vec::new();
        let mut e = ptr::null();
        loop {
            // SAFETY: `m` is NULL or a live dictionary, and `e` NULL or its
            // previous entry, which nothing has invalidated.
            e = unsafe { ffi::av_dict_iterate(m, e) };
            // SAFETY: a non-NULL `e` is an entry of `m`, live for the loop.
            let Some(entry) = (unsafe { DictionaryEntryRef::from_ptr(e.cast_mut()) }) else {
                return out;
            };
            out.push((entry.key().to_owned(), entry.value().to_owned()));
        }
    }

    /// The entries of `d`, read by C.
    fn entries(d: &Dict) -> Vec<(CString, CString)> {
        c_entries(d.as_ref().map_or(ptr::null(), |r| r.as_ptr().cast()))
    }

    fn owned(pairs: &[(&CStr, &CStr)]) -> Vec<(CString, CString)> {
        pairs
            .iter()
            .map(|&(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    /// Adopts a dictionary C built from `pairs` with `flags`.
    fn adopt_c(pairs: &[(&CStr, &CStr)], flags: c_int) -> DictionaryBox {
        let mut m = ptr::null_mut();
        for (k, v) in pairs {
            // SAFETY: `m` is our own slot; the strings are live for the call.
            assert!(unsafe { ffi::av_dict_set(&mut m, k.as_ptr(), v.as_ptr(), flags) } >= 0);
        }
        // SAFETY: `m` is a dictionary C allocated, referenced by nothing
        // else, which the box takes over.
        unsafe { DictionaryBox::from_c(m) }.unwrap()
    }

    const PAIRS: [(&CStr, &CStr); 4] = [(c"a", c"1"), (c"B", c"2"), (c"A", c"3"), (c"", c"")];

    #[test]
    fn entry_getters_read_both_strings() {
        let mut raw = ffi::AVDictionaryEntry {
            key: c"title".as_ptr().cast_mut(),
            value: c"x y".as_ptr().cast_mut(),
        };
        // SAFETY: `raw` is a live local whose strings are literals.
        let e = unsafe { DictionaryEntryRef::from_ptr(&mut raw) }.unwrap();
        assert_eq!((e.key(), e.value()), (c"title", c"x y"));
        assert_eq!(
            format!("{e:?}"),
            r#"DictionaryEntry { key: "title", value: "x y" }"#
        );
    }

    #[test]
    fn entry_getters_refuse_a_null_string() {
        let mut raw = ffi::AVDictionaryEntry {
            key: ptr::null_mut(),
            value: ptr::null_mut(),
        };
        // SAFETY: `raw` is a live local.
        let e = unsafe { DictionaryEntryRef::from_ptr(&mut raw) }.unwrap();
        assert!(catch_unwind(|| e.key()).is_err());
        assert!(catch_unwind(|| e.value()).is_err());
    }

    #[test]
    fn adopted_dictionary_clones_deeply_and_frees() {
        let original = adopt_c(&PAIRS, MULTIKEY);
        let copy = original.clone();
        assert_ne!(copy.as_c_ptr(), original.as_c_ptr());
        assert_eq!(c_entries(copy.as_c_ptr()), owned(&PAIRS));
        // The copy's strings are its own.
        let first = |b: &DictionaryBox| {
            // SAFETY: `b` is a live non-empty dictionary.
            let e = unsafe { ffi::av_dict_iterate(b.as_c_ptr(), ptr::null()) };
            // SAFETY: `e` is its first entry, live while `b` is.
            unsafe { (*e).key }
        };
        assert_ne!(first(&copy), first(&original));
        drop(original);
        assert_eq!(
            c_entries(copy.try_clone().unwrap().as_c_ptr()),
            owned(&PAIRS)
        );
    }

    #[test]
    fn clone_keeps_keys_c_would_merge() {
        // Without AV_DICT_MULTIKEY, C keeps one of `a` and `A`.
        let merged = adopt_c(&PAIRS, 0);
        assert_eq!(c_entries(merged.as_c_ptr()).len(), 3);
        let d = Dict::from_entries(PAIRS).unwrap();
        assert_eq!(entries(&d.clone()), owned(&PAIRS));
        assert_eq!(entries(&d.try_clone().unwrap()), owned(&PAIRS));
    }

    #[test]
    fn from_entries_keeps_every_pair_in_order() {
        assert!(
            Dict::from_entries::<&CStr, &CStr>([])
                .unwrap()
                .as_ref()
                .is_none()
        );
        let d = Dict::from_entries(PAIRS).unwrap();
        assert_eq!(entries(&d), owned(&PAIRS));
        let strings = Dict::from_entries([(CString::new("k").unwrap(), c"v")]).unwrap();
        assert_eq!(entries(&strings), owned(&[(c"k", c"v")]));
    }

    /// C's `av_dict_set` on `d`'s slot, as a writer of the dictionary
    /// would call it.
    fn c_set(d: &mut Dict, key: *const c_char, value: *const c_char) -> c_int {
        let mut slot = d.take().map_or(ptr::null_mut(), CBox::into_c);
        // SAFETY: our own slot; `key` and `value` are NULL or literals.
        let ret = unsafe { ffi::av_dict_set(&mut slot, key, value, 0) };
        // SAFETY: NULL or the dictionary C left in the slot, unshared.
        *d = Dict::from(unsafe { DictionaryBox::from_c(slot) });
        ret
    }

    #[test]
    fn append_allocates_and_deleting_the_last_entry_empties() {
        let mut d = Dict::new();
        assert_eq!(d.append(c"k", c"v"), 0);
        assert_eq!(d.append(c"K", c"w"), 0);
        assert_eq!(entries(&d), owned(&[(c"k", c"v"), (c"K", c"w")]));
        // Deleting frees the dictionary with its last entry, NULLing the
        // slot, which the owner then holds as empty.
        assert_eq!(c_set(&mut d, c"k".as_ptr(), ptr::null()), 0);
        assert_eq!(entries(&d), owned(&[(c"K", c"w")]));
        assert_eq!(c_set(&mut d, c"k".as_ptr(), ptr::null()), 0);
        assert!(d.as_ref().is_none() && d.as_mut().is_none());
        // A failed call on an empty dictionary leaves it empty.
        assert!(c_set(&mut d, ptr::null(), c"v".as_ptr()) < 0);
        assert!(d.as_ref().is_none());
        assert_eq!(d.append(c"", c""), 0);
        assert_eq!(entries(&d), owned(&[(c"", c"")]));
    }

    #[test]
    fn owners_convert_without_copying() {
        let mut d = Dict::from_entries(PAIRS).unwrap();
        let ptr = d.as_mut().unwrap().as_mut_ptr();
        let b = d.take().unwrap();
        assert!(d.as_ref().is_none() && d.take().is_none());
        assert_eq!(b.as_c_ptr(), ptr);
        let d = Dict::from(b);
        let b: Option<DictionaryBox> = d.into();
        assert_eq!(b.as_ref().map(|b| b.as_c_ptr()), Some(ptr));
        assert!(Option::<DictionaryBox>::from(Dict::from(None::<DictionaryBox>)).is_none());
        assert!(Dict::default().try_clone().unwrap().as_ref().is_none());
    }

    /// Runs the test `name` alone in a child process, where it may lower
    /// the process-wide allocation limit; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_UNIT_DICT_ISOLATED";
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

    /// C's allocation failures part-way through, under an `av_max_alloc`
    /// limit the last value exceeds: the partial dictionaries are freed (as
    /// LeakSanitizer checks) and the source is untouched.
    #[test]
    fn allocation_failures_free_partial_dictionaries() {
        if !isolated("libavutil::dict::unit_tests::allocation_failures_free_partial_dictionaries") {
            return;
        }
        let long = CString::new(vec![b'x'; 1000]).unwrap();
        let pairs = [(c"a", c"1"), (c"b", c"2"), (c"c", long.as_c_str())];
        let d = Dict::from_entries(pairs).unwrap();
        let b = adopt_c(&pairs, MULTIKEY);
        super::super::mem::set_max_alloc(500);
        assert!(Dict::from_entries(pairs).is_none());
        assert!(d.try_clone().is_none());
        assert!(b.try_clone().is_none());
        // Only the long value fails: shorter dictionaries still copy.
        let short = Dict::from_entries(pairs[..2].iter().copied()).unwrap();
        // The third slot of the entry array (48 bytes) fails, after the
        // dictionary and two entries were allocated.
        super::super::mem::set_max_alloc(40);
        let three = [(c"a", c"1"), (c"b", c"2"), (c"c", c"3")];
        assert!(Dict::from_entries(three).is_none());
        assert_eq!(entries(&short.try_clone().unwrap()), owned(&pairs[..2]));
        // The 16-byte dictionary itself fails, after its strings.
        super::super::mem::set_max_alloc(8);
        assert!(Dict::from_entries([(c"", c"")]).is_none());
        assert!(short.try_clone().is_none());
        super::super::mem::set_max_alloc(i32::MAX as usize);
        assert_eq!(entries(&d.try_clone().unwrap()), owned(&pairs));
        assert_eq!(c_entries(b.try_clone().unwrap().as_c_ptr()), owned(&pairs));
    }
}
