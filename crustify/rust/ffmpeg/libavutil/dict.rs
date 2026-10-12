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
//!
//! The readers (`av_dict_count`, `av_dict_iterate`, `av_dict_get`,
//! `av_dict_get_string`) are methods of [`DictionaryRef`], and of [`Dict`]
//! for a dictionary that may be empty: [`len`](DictionaryRef::len),
//! [`iter`](DictionaryRef::iter), [`get`](DictionaryRef::get) and
//! [`get_all`](DictionaryRef::get_all) with lookup [`Flags`], and
//! [`serialize`](DictionaryRef::serialize). The writers (`av_dict_set`,
//! `av_dict_set_int`, `av_dict_parse_string`, `av_dict_copy`) are methods of
//! [`Dict`], since each may allocate or free the dictionary: [`Dict::set`],
//! [`Dict::remove`], [`Dict::set_owned`] (C's `AV_DICT_DONT_STRDUP_*`,
//! which hand owned [`HeapString`]s over), [`Dict::set_int`],
//! [`Dict::parse_pairs`] and [`Dict::copy_from`].

use core::ffi::{CStr, c_char, c_int};
use core::fmt;
use core::iter::FusedIterator;
use core::ops::{BitOr, BitOrAssign};
use core::ptr::{self, NonNull, addr_of};

use ffibox::{CBox, CDrop, CDupClone, define_ctype};

use super::ffi;
use super::mem::HeapString;
use super::opt::AVERROR_BUG;

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
        let (key, value) = (key.as_ptr(), value.as_ptr());
        // SAFETY: `key` and `value` are NUL-terminated strings live for the
        // call, and the slot is this owner's (see `with_slot`).
        self.with_slot(|slot| unsafe { ffi::av_dict_set(slot, key, value, MULTIKEY) })
    }

    /// Runs `write`, a C writer of this dictionary's slot, on a slot holding
    /// the dictionary (NULL when empty), then takes back what the writer left
    /// there, and returns its status.
    ///
    /// `write` must leave the slot NULL or holding a dictionary it alone
    /// references that `av_dict_free` releases and that holds an entry, as
    /// every `av_dict_*` writer does (`av_dict_set` frees the dictionary with
    /// its last entry, NULLing the slot).
    fn with_slot(&mut self, write: impl FnOnce(*mut *mut ffi::AVDictionary) -> c_int) -> c_int {
        let mut slot = self.0.take().map_or(ptr::null_mut(), CBox::into_c);
        let ret = write(&mut slot);
        // SAFETY: per `write`'s obligation, the slot holds NULL or a
        // non-empty dictionary nothing else references, which this owner
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

/// The `AV_DICT_*` flags of a lookup or a write.
///
/// A lookup ([`DictionaryRef::get`], and the search [`Dict::set`] makes for
/// the entry to replace) reads [`MATCH_CASE`](Self::MATCH_CASE) and
/// [`IGNORE_SUFFIX`](Self::IGNORE_SUFFIX); a write reads every flag. C's
/// `AV_DICT_DONT_STRDUP_KEY` and `AV_DICT_DONT_STRDUP_VAL` are not flags
/// here: [`Dict::set_owned`] is the write that hands strings over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(c_int);

impl Flags {
    /// Keys match case-insensitively (ASCII), in full; a write replaces the
    /// first matching entry.
    pub const NONE: Self = Self(0);
    /// `AV_DICT_MATCH_CASE`: keys match only with equal case.
    pub const MATCH_CASE: Self = Self(ffi::AV_DICT_MATCH_CASE as c_int);
    /// `AV_DICT_IGNORE_SUFFIX`: a key matches every key it is a prefix of; the
    /// empty key matches every entry.
    pub const IGNORE_SUFFIX: Self = Self(ffi::AV_DICT_IGNORE_SUFFIX as c_int);
    /// `AV_DICT_DONT_OVERWRITE`: a write leaves a matching entry (and the
    /// dictionary) unchanged.
    pub const DONT_OVERWRITE: Self = Self(ffi::AV_DICT_DONT_OVERWRITE as c_int);
    /// `AV_DICT_APPEND`: a write appends the value to a matching entry's,
    /// without a separator, rather than replacing it.
    pub const APPEND: Self = Self(ffi::AV_DICT_APPEND as c_int);
    /// `AV_DICT_MULTIKEY`: a write appends a new entry without looking for a
    /// matching one, so keys may repeat; a removal then removes nothing.
    pub const MULTIKEY: Self = Self(MULTIKEY);
    /// `AV_DICT_DEDUP`: with [`MULTIKEY`](Self::MULTIKEY), a write of a pair
    /// that is already present (an equal value under a matching key) changes
    /// nothing.
    pub const DEDUP: Self = Self(ffi::AV_DICT_DEDUP as c_int);

    /// The C flag bits.
    pub const fn bits(self) -> c_int {
        self.0
    }
}

impl BitOr for Flags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Flags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// The writers: each may allocate the dictionary or, deleting its last
/// entry, free it, so they act on the owner.
impl Dict {
    /// Wraps: av_dict_set
    ///
    /// Stores a copy of `value` under a copy of `key`. Without
    /// [`Flags::MULTIKEY`] it replaces the first entry whose key matches
    /// `key` (per `flags`): C deletes that entry, moving the last entry into
    /// its place, and appends the new pair, spelled as `key`, at the end
    /// (with [`Flags::APPEND`], with `value` appended to the old value; with
    /// [`Flags::DONT_OVERWRITE`], C leaves the entry as it is). With no
    /// match, or with [`Flags::MULTIKEY`], it appends a new entry.
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOMEM)` when C runs out of memory, which leaves the
    /// entries as they were.
    pub fn set(&mut self, key: &CStr, value: &CStr, flags: Flags) -> Result<(), i32> {
        let (key, value) = (key.as_ptr(), value.as_ptr());
        // SAFETY: `key` and `value` are live C strings C copies, and no
        // `AV_DICT_DONT_STRDUP_*` flag reaches it; the slot is ours.
        let ret = self.with_slot(|slot| unsafe { ffi::av_dict_set(slot, key, value, flags.0) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_dict_set
    ///
    /// Deletes the first entry whose key matches `key` (per `flags`), whose
    /// place the last entry then takes; deleting the last entry empties the
    /// dictionary. Nothing happens without a match, or with
    /// [`Flags::DONT_OVERWRITE`] or [`Flags::MULTIKEY`].
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOMEM)` when C cannot copy `key`, which leaves the entries
    /// as they were.
    pub fn remove(&mut self, key: &CStr, flags: Flags) -> Result<(), i32> {
        let (key, flags) = (key.as_ptr(), flags.0);
        // SAFETY: as for `set`, with a NULL value, which deletes.
        let ret = self.with_slot(|slot| unsafe { ffi::av_dict_set(slot, key, ptr::null(), flags) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_dict_set
    ///
    /// As [`set`](Self::set), storing `key` and `value` themselves rather
    /// than copies (`AV_DICT_DONT_STRDUP_KEY | AV_DICT_DONT_STRDUP_VAL`):
    /// the dictionary takes both over, and C frees what it does not store
    /// (both strings when [`Flags::DONT_OVERWRITE`] or [`Flags::DEDUP`] keep
    /// the entries, `value` once [`Flags::APPEND`] has appended it, and both
    /// on failure).
    ///
    /// # Errors
    ///
    /// As [`set`](Self::set).
    pub fn set_owned(
        &mut self,
        key: HeapString,
        value: HeapString,
        flags: Flags,
    ) -> Result<(), i32> {
        let flags =
            flags.0 | (ffi::AV_DICT_DONT_STRDUP_KEY | ffi::AV_DICT_DONT_STRDUP_VAL) as c_int;
        let (key, value) = (key.into_raw(), value.into_raw());
        // SAFETY: `key` and `value` are `av_malloc`-family strings we owned,
        // which `av_dict_set` takes over with the `DONT_STRDUP` flags on
        // every path (storing them or releasing them with `av_free`); the
        // slot is ours.
        let ret = self.with_slot(|slot| unsafe { ffi::av_dict_set(slot, key, value, flags) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_dict_set_int
    ///
    /// As [`set`](Self::set), storing `value` in decimal.
    ///
    /// # Errors
    ///
    /// As [`set`](Self::set).
    pub fn set_int(&mut self, key: &CStr, value: i64, flags: Flags) -> Result<(), i32> {
        let key = key.as_ptr();
        // SAFETY: as for `set`; C formats `value` into its own buffer.
        let ret = self.with_slot(|slot| unsafe { ffi::av_dict_set_int(slot, key, value, flags.0) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_dict_parse_string
    ///
    /// Stores, as [`set`](Self::set) with `flags`, each `key=value` pair of
    /// `s`, in order: a key is ended by a byte of `key_val_sep`, a value by
    /// a byte of `pairs_sep`, and either may quote with `'...'` or escape a
    /// byte with `\`, as `av_get_token` reads them; white space around them
    /// is dropped.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` at the first pair with an empty key or value, or no
    /// key separator, and `AVERROR(ENOMEM)`; the pairs before it stay stored.
    pub fn parse_pairs(
        &mut self,
        s: &CStr,
        key_val_sep: &CStr,
        pairs_sep: &CStr,
        flags: Flags,
    ) -> Result<(), i32> {
        let (s, kv, pairs) = (s.as_ptr(), key_val_sep.as_ptr(), pairs_sep.as_ptr());
        let flags = flags.0;
        // SAFETY: the strings are live C strings C only reads, and C drops
        // the `DONT_STRDUP` flags, which `Flags` cannot carry anyway; the
        // slot is ours.
        let parse = |slot| unsafe { ffi::av_dict_parse_string(slot, s, kv, pairs, flags) };
        let ret = self.with_slot(parse);
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Wraps: av_dict_copy
    ///
    /// Stores, as [`set`](Self::set) with `flags`, every entry of `src`, in
    /// order (none when `src` is `None`, an empty dictionary). With
    /// [`Flags::NONE`], entries of `src` whose keys match an earlier one
    /// replace it, so the copy can hold fewer entries than `src`;
    /// [`Flags::MULTIKEY`] keeps them all, as `Clone` does.
    ///
    /// # Errors
    ///
    /// `AVERROR(ENOMEM)`; the entries copied before it stay stored.
    pub fn copy_from(&mut self, src: Option<DictionaryRef<'_>>, flags: Flags) -> Result<(), i32> {
        let src = src.map_or(ptr::null(), |d| d.as_ptr());
        // SAFETY: `src` is NULL or a live dictionary the shared borrow keeps
        // unwritten, never this one (which is borrowed exclusively), and C
        // only reads it; the slot is ours.
        let ret = self.with_slot(|slot| unsafe { ffi::av_dict_copy(slot, src, flags.0) });
        if ret < 0 { Err(ret) } else { Ok(()) }
    }
}

/// The readers, for a dictionary that may be empty: as [`DictionaryRef`]'s,
/// on C's NULL dictionary when it is.
impl Dict {
    /// The number of entries, as [`DictionaryRef::len`]; 0 when empty.
    pub fn len(&self) -> usize {
        count(self.as_ref())
    }

    /// Whether the dictionary has no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// The entries in order, as [`DictionaryRef::iter`].
    pub fn iter(&self) -> Iter<'_> {
        Iter::new(self.as_ref())
    }

    /// The first entry whose key matches `key`, as [`DictionaryRef::get`].
    pub fn get(&self, key: &CStr, flags: Flags) -> Option<DictionaryEntryRef<'_>> {
        first(self.as_ref(), key, flags)
    }

    /// Every entry whose key matches `key`, as [`DictionaryRef::get_all`].
    pub fn get_all<'k>(&self, key: &'k CStr, flags: Flags) -> Matches<'_, 'k> {
        Matches::new(self.as_ref(), key, flags)
    }

    /// The entries as one string, as [`DictionaryRef::serialize`]; `""` when
    /// empty.
    ///
    /// # Errors
    ///
    /// As [`DictionaryRef::serialize`].
    pub fn serialize(&self, key_val_sep: u8, pairs_sep: u8) -> Result<HeapString, i32> {
        serialize(self.as_ref(), key_val_sep, pairs_sep)
    }
}

impl<'a> IntoIterator for &'a Dict {
    type Item = DictionaryEntryRef<'a>;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> DictionaryRef<'a> {
    /// Wraps: av_dict_count
    ///
    /// The number of entries, never 0 for a dictionary (an empty one is a
    /// [`Dict`] holding none).
    pub fn len(&self) -> usize {
        count(Some(*self))
    }

    /// Whether the dictionary has no entries, which a dictionary never has.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Wraps: av_dict_iterate
    ///
    /// The entries in order: insertion order, except where a write replaced
    /// or deleted an entry (see [`Dict::set`] and [`Dict::remove`]).
    pub fn iter(&self) -> Iter<'a> {
        Iter::new(Some(*self))
    }

    /// Wraps: av_dict_get
    ///
    /// The first entry whose key matches `key`: equal to it ignoring ASCII
    /// case, or with equal case under [`Flags::MATCH_CASE`], and with
    /// [`Flags::IGNORE_SUFFIX`] any key starting with it. Other flags are
    /// ignored.
    pub fn get(&self, key: &CStr, flags: Flags) -> Option<DictionaryEntryRef<'a>> {
        first(Some(*self), key, flags)
    }

    /// Wraps: av_dict_get
    ///
    /// Every entry whose key matches `key` as [`get`](Self::get) matches it,
    /// in order: C's `av_dict_get` called again after each match. With
    /// [`Flags::IGNORE_SUFFIX`] and the empty key, every entry.
    pub fn get_all<'k>(&self, key: &'k CStr, flags: Flags) -> Matches<'a, 'k> {
        Matches::new(Some(*self), key, flags)
    }

    /// Wraps: av_dict_get_string
    ///
    /// The entries as one string: each key, `key_val_sep` and value, the
    /// pairs separated by `pairs_sep`, with a `\` before each separator, `'`
    /// and `\` in keys and values, and before white space at either end of
    /// one, which [`Dict::parse_pairs`] reads back.
    ///
    /// # Errors
    ///
    /// `AVERROR(EINVAL)` for a separator that is NUL or `\`, or two equal
    /// separators; `AVERROR(ENOMEM)`.
    pub fn serialize(&self, key_val_sep: u8, pairs_sep: u8) -> Result<HeapString, i32> {
        serialize(Some(*self), key_val_sep, pairs_sep)
    }
}

impl<'a> IntoIterator for DictionaryRef<'a> {
    type Item = DictionaryEntryRef<'a>;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

/// The dictionary of `m` as C's `const AVDictionary *`: NULL for none, the
/// empty dictionary.
fn c_ptr(m: Option<DictionaryRef<'_>>) -> *const ffi::AVDictionary {
    m.map_or(ptr::null(), |d| d.as_ptr())
}

/// `av_dict_count` of `m`; 0 for none.
fn count(m: Option<DictionaryRef<'_>>) -> usize {
    // SAFETY: NULL or a live dictionary the handle's shared borrow keeps
    // unwritten; C only reads its count.
    let n = unsafe { ffi::av_dict_count(c_ptr(m)) };
    usize::try_from(n).expect("a dictionary's count is not negative")
}

/// `av_dict_get` of the first entry of `m` whose key matches `key`.
fn first<'a>(
    m: Option<DictionaryRef<'a>>,
    key: &CStr,
    flags: Flags,
) -> Option<DictionaryEntryRef<'a>> {
    // SAFETY: NULL or a live dictionary the handle's shared borrow keeps
    // unwritten, no previous entry, and a live key C only reads.
    let e = unsafe { ffi::av_dict_get(c_ptr(m), key.as_ptr(), ptr::null(), flags.0) };
    // SAFETY: NULL or an entry of the dictionary, which the borrow keeps
    // live and unwritten for `'a`.
    unsafe { DictionaryEntryRef::from_ptr(e) }
}

/// `av_dict_get_string` of `m`; `""` for none.
fn serialize(m: Option<DictionaryRef<'_>>, kv: u8, pairs: u8) -> Result<HeapString, i32> {
    let mut out: *mut c_char = ptr::null_mut();
    let (m, kv, pairs) = (c_ptr(m), kv as c_char, pairs as c_char);
    // SAFETY: NULL or a live dictionary the handle's shared borrow keeps
    // unwritten, which C only reads; `out` is a writable slot.
    let ret = unsafe { ffi::av_dict_get_string(m, &mut out, kv, pairs) };
    if ret < 0 {
        return Err(ret);
    }
    // SAFETY: on success `out` is a fresh `av_malloc`-family string the
    // caller owns and releases with `av_free`.
    unsafe { HeapString::from_raw(out) }.ok_or(AVERROR_BUG)
}

/// The entries of a dictionary in order, from [`DictionaryRef::iter`] or
/// [`Dict::iter`]: `av_dict_iterate` from the first entry to the last.
#[derive(Clone)]
pub struct Iter<'a> {
    /// The dictionary; `None` once the walk is over (or for an empty one).
    dict: Option<DictionaryRef<'a>>,
    /// The entry of `dict` returned last; `None` before the first.
    prev: Option<DictionaryEntryRef<'a>>,
    /// The entries not yet returned.
    left: usize,
}

impl<'a> Iter<'a> {
    fn new(dict: Option<DictionaryRef<'a>>) -> Self {
        Self {
            dict,
            prev: None,
            left: count(dict),
        }
    }
}

impl fmt::Debug for Iter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Iter")
            .field("left", &self.left)
            .finish_non_exhaustive()
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = DictionaryEntryRef<'a>;

    fn next(&mut self) -> Option<DictionaryEntryRef<'a>> {
        let prev = self.prev.map_or(ptr::null(), |e| e.as_ptr());
        // SAFETY: NULL or the dictionary the borrow keeps live and unwritten,
        // and NULL or the entry of it C returned last, as C requires.
        let e = unsafe { ffi::av_dict_iterate(c_ptr(self.dict), prev) };
        // SAFETY: NULL or an entry of the dictionary, live for `'a`.
        let Some(entry) = (unsafe { DictionaryEntryRef::from_ptr(e.cast_mut()) }) else {
            // C restarts from a NULL `prev`; no dictionary stays empty.
            self.dict = None;
            return None;
        };
        self.prev = Some(entry);
        self.left -= 1;
        Some(entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left, Some(self.left))
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl FusedIterator for Iter<'_> {}

/// The entries whose key matches a key, from [`DictionaryRef::get_all`] or
/// [`Dict::get_all`]: `av_dict_get` called again after each match.
#[derive(Clone)]
pub struct Matches<'a, 'k> {
    /// The dictionary; `None` once the walk is over (or for an empty one).
    dict: Option<DictionaryRef<'a>>,
    /// The entry of `dict` returned last; `None` before the first.
    prev: Option<DictionaryEntryRef<'a>>,
    key: &'k CStr,
    flags: Flags,
}

impl<'a, 'k> Matches<'a, 'k> {
    fn new(dict: Option<DictionaryRef<'a>>, key: &'k CStr, flags: Flags) -> Self {
        Self {
            dict,
            prev: None,
            key,
            flags,
        }
    }
}

impl fmt::Debug for Matches<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Matches")
            .field("key", &self.key)
            .field("flags", &self.flags)
            .finish_non_exhaustive()
    }
}

impl<'a> Iterator for Matches<'a, '_> {
    type Item = DictionaryEntryRef<'a>;

    fn next(&mut self) -> Option<DictionaryEntryRef<'a>> {
        let prev = self.prev.map_or(ptr::null(), |e| e.as_ptr());
        let (m, key) = (c_ptr(self.dict), self.key.as_ptr());
        // SAFETY: NULL or the dictionary the borrow keeps live and unwritten,
        // NULL or the entry of it C returned last, and a live key C only
        // reads.
        let e = unsafe { ffi::av_dict_get(m, key, prev, self.flags.0) };
        // SAFETY: NULL or an entry of the dictionary, live for `'a`.
        let Some(entry) = (unsafe { DictionaryEntryRef::from_ptr(e) }) else {
            // C restarts from a NULL `prev`; no dictionary matches nothing.
            self.dict = None;
            return None;
        };
        self.prev = Some(entry);
        Some(entry)
    }
}

impl FusedIterator for Matches<'_, '_> {}

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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
    fn clone_keeps_keys_c_would_merge() {
        // Without AV_DICT_MULTIKEY, C keeps one of `a` and `A`.
        let merged = adopt_c(&PAIRS, 0);
        assert_eq!(c_entries(merged.as_c_ptr()).len(), 3);
        let d = Dict::from_entries(PAIRS).unwrap();
        assert_eq!(entries(&d.clone()), owned(&PAIRS));
        assert_eq!(entries(&d.try_clone().unwrap()), owned(&PAIRS));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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

    /// Options wave 1, batch 0: the flags, the writers' slot plumbing, the
    /// ownership `set_owned` hands over on every path (which LeakSanitizer
    /// checks), and the iterators' bookkeeping.
    mod readers_and_writers {
        use super::*;

        fn heap(s: &CStr) -> HeapString {
            // SAFETY: `av_strdup` reads a live C string.
            let p = unsafe { ffi::av_strdup(s.as_ptr()) };
            // SAFETY: a fresh `av_malloc`-family string we own.
            unsafe { HeapString::from_raw(p) }.unwrap()
        }

        #[test]
        fn flags_are_cs_bits() {
            let all = [
                (Flags::NONE, 0),
                (Flags::MATCH_CASE, ffi::AV_DICT_MATCH_CASE),
                (Flags::IGNORE_SUFFIX, ffi::AV_DICT_IGNORE_SUFFIX),
                (Flags::DONT_OVERWRITE, ffi::AV_DICT_DONT_OVERWRITE),
                (Flags::APPEND, ffi::AV_DICT_APPEND),
                (Flags::MULTIKEY, ffi::AV_DICT_MULTIKEY),
                (Flags::DEDUP, ffi::AV_DICT_DEDUP),
            ];
            for (f, bits) in all {
                assert_eq!(f.bits(), bits as c_int);
            }
            let mut f = Flags::MULTIKEY | Flags::DEDUP;
            f |= Flags::MATCH_CASE;
            assert_eq!(
                f.bits(),
                (ffi::AV_DICT_MULTIKEY | ffi::AV_DICT_DEDUP | ffi::AV_DICT_MATCH_CASE) as c_int
            );
            assert_eq!(Flags::default(), Flags::NONE);
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn owned_strings_are_stored_or_freed_on_every_path() {
            let mut d = Dict::new();
            d.set_owned(heap(c"k"), heap(c"v"), Flags::NONE).unwrap();
            // Kept entry: C frees both strings.
            d.set_owned(heap(c"K"), heap(c"w"), Flags::DONT_OVERWRITE)
                .unwrap();
            // Duplicate pair: C frees both.
            let dedup = Flags::MULTIKEY | Flags::DEDUP;
            d.set_owned(heap(c"k"), heap(c"v"), dedup).unwrap();
            // Appended: C frees the value it appended.
            d.set_owned(heap(c"k"), heap(c"2"), Flags::APPEND).unwrap();
            // Replaced: the old key and value are freed.
            d.set_owned(heap(c"K"), heap(c"x"), Flags::NONE).unwrap();
            assert_eq!(entries(&d), owned(&[(c"K", c"x")]));
            d.set_owned(heap(c"a"), heap(c""), Flags::MULTIKEY).unwrap();
            assert_eq!(entries(&d), owned(&[(c"K", c"x"), (c"a", c"")]));
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn removing_the_last_entry_empties_the_owner() {
            let mut d = Dict::from_entries([(c"a", c"1")]).unwrap();
            d.remove(c"b", Flags::NONE).unwrap();
            assert_eq!(d.len(), 1);
            d.remove(c"A", Flags::MATCH_CASE).unwrap();
            assert_eq!(d.len(), 1);
            d.remove(c"A", Flags::NONE).unwrap();
            assert!(d.is_empty() && d.as_ref().is_none());
            // Removing from the empty dictionary leaves it empty, as does a
            // value parse that fails before storing anything.
            d.remove(c"a", Flags::NONE).unwrap();
            assert_eq!(
                d.parse_pairs(c"=v", c"=", c":", Flags::NONE),
                Err(-(ffi::EINVAL as i32))
            );
            assert!(d.is_empty() && d.as_ref().is_none());
        }

        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn iterators_count_down_and_stay_done() {
            let d = Dict::from_entries(PAIRS).unwrap();
            let mut it = d.iter();
            assert_eq!(it.len(), 4);
            assert_eq!(format!("{it:?}"), "Iter { left: 4, .. }");
            assert_eq!(it.next().map(|e| e.key()), Some(c"a"));
            assert_eq!(it.size_hint(), (3, Some(3)));
            assert_eq!(it.by_ref().count(), 3);
            // C would start over from a NULL `prev`.
            assert!(it.next().is_none() && it.next().is_none());
            assert_eq!(it.len(), 0);
            let mut m = d.get_all(c"a", Flags::NONE);
            assert_eq!(format!("{m:?}"), r#"Matches { key: "a", flags: Flags(0), .. }"#);
            assert_eq!(m.by_ref().count(), 2);
            assert!(m.next().is_none() && m.next().is_none());
            let empty = Dict::new();
            assert_eq!(empty.iter().len(), 0);
            assert!(empty.iter().next().is_none());
            assert!(empty.get_all(c"", Flags::IGNORE_SUFFIX).next().is_none());
            assert!(empty.get(c"", Flags::IGNORE_SUFFIX).is_none());
        }

        /// Writers under an `av_max_alloc` limit: the failing write reports
        /// `ENOMEM` and leaves the entries it did not reach as they were.
        #[test]
        #[cfg_attr(miri, ignore = "calls C")]
        fn allocation_failures_keep_the_entries() {
            if !isolated(
                "libavutil::dict::unit_tests::readers_and_writers::allocation_failures_keep_the_entries",
            ) {
                return;
            }
            let long = CString::new(vec![b'x'; 1000]).unwrap();
            let enomem = Err(-(ffi::ENOMEM as i32));
            let mut d = Dict::from_entries([(c"a", c"1"), (c"b", c"2")]).unwrap();
            let src = Dict::from_entries([(c"c", c"3"), (c"d", long.as_c_str())]).unwrap();
            super::super::super::mem::set_max_alloc(500);
            assert_eq!(d.set(c"a", &long, Flags::NONE), enomem);
            assert_eq!(d.set(c"z", &long, Flags::NONE), enomem);
            assert_eq!(d.set(&long, c"1", Flags::NONE), enomem);
            assert_eq!(d.remove(&long, Flags::NONE), enomem);
            assert_eq!(d.parse_pairs(c"e=5:f=xx", c"=", c":", Flags::NONE), Ok(()));
            assert_eq!(d.copy_from(src.as_ref(), Flags::NONE), enomem);
            // The pair before the long one was copied.
            assert_eq!(
                entries(&d),
                owned(&[
                    (c"a", c"1"),
                    (c"b", c"2"),
                    (c"e", c"5"),
                    (c"f", c"xx"),
                    (c"c", c"3")
                ])
            );
            let mut e = Dict::new();
            assert_eq!(e.set(c"k", &long, Flags::NONE), enomem);
            assert!(e.is_empty());
            // Appending grows the old value past the limit.
            super::super::super::mem::set_max_alloc(i32::MAX as usize);
            let half = CString::new(vec![b'h'; 300]).unwrap();
            e.set(c"k", &half, Flags::NONE).unwrap();
            super::super::super::mem::set_max_alloc(500);
            assert_eq!(e.set(c"k", &half, Flags::APPEND), enomem);
            assert_eq!(entries(&e), [(c"k".to_owned(), half.clone())]);
            super::super::super::mem::set_max_alloc(i32::MAX as usize);
        }
    }
}
