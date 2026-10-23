//! Safe wrappers over `libavutil/avstring.h`.
//!
//! [`format()`], [`append_path_component`] and [`escape`] return a
//! [`HeapString`]: an `av_malloc`-family string the caller owns, released by
//! `av_free`.
//! [`basename`] borrows from its argument instead.
//!
//! Locale-independent string helpers. The case-insensitive ones fold ASCII
//! letters only, among them [`cmp_ignore_ascii_case`],
//! [`cmp_n_ignore_ascii_case`], [`strip_prefix_ignore_ascii_case`],
//! [`find_ignore_ascii_case`] and [`replace_ignore_ascii_case`]; [`scan`] is
//! C's `sscanf` with each destination checked against the format before the
//! call.

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_long, c_longlong, c_short, c_uint, c_void};
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::num::NonZeroU8;
use core::ops::{BitAnd, BitOr, BitOrAssign};
use core::{fmt, ptr};
use std::borrow::Cow;
use std::ffi::CString;

use super::ffi;
use super::mem::{HeapString, string_from_c_str};

/// Wraps: `AVEscapeMode`
/// Handwritten
///
/// How `av_escape` escapes special characters.
///
/// A Rust `enum`: the mode only ever flows from the caller into C as an
/// argument, and C never stores or returns one, so the safe surface names the
/// documented modes only. (C treats any other value as
/// [`Backslash`](Self::Backslash).)
///
/// Not every bit pattern is a mode, so C memory never reads as one: the
/// plain-element arrays reject it.
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::{avstring::EscapeMode, mem::alloc_array_zeroed};
/// let _ = alloc_array_zeroed::<EscapeMode>(1);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EscapeMode {
    /// Let the library choose; currently [`Backslash`](Self::Backslash), which
    /// a later version may change.
    #[default]
    Auto,
    /// Backslash escaping.
    Backslash,
    /// Single-quote escaping.
    Quote,
    /// XML non-markup character data escaping.
    Xml,
}

impl EscapeMode {
    /// The C encoding of this mode.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVEscapeMode {
        match self {
            Self::Auto => ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO,
            Self::Backslash => ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
            Self::Quote => ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE,
            Self::Xml => ffi::AVEscapeMode::AV_ESCAPE_MODE_XML,
        }
    }
}

impl From<EscapeMode> for ffi::AVEscapeMode {
    #[inline]
    fn from(mode: EscapeMode) -> Self {
        mode.into_raw()
    }
}

/// The `AV_ESCAPE_FLAG_*` flags that refine how [`escape`] escapes.
///
/// C takes them as an `int` and ignores bits it does not name, so any bit
/// pattern is a valid value and the set operations carry unnamed bits along.
#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EscapeFlags(c_int);

impl EscapeFlags {
    /// Every named flag with its constant's name, in declaration order.
    const KNOWN: [(Self, &'static str); 4] = [
        (Self::WHITESPACE, "WHITESPACE"),
        (Self::STRICT, "STRICT"),
        (Self::XML_SINGLE_QUOTES, "XML_SINGLE_QUOTES"),
        (Self::XML_DOUBLE_QUOTES, "XML_DOUBLE_QUOTES"),
    ];
    /// Backslash mode: escape only the caller's special characters, not
    /// those `av_get_token` treats as special (the quote, the backslash and
    /// leading or trailing whitespace).
    pub const STRICT: Self = Self(ffi::AV_ESCAPE_FLAG_STRICT.cast_signed());
    /// Backslash mode: also escape every whitespace character, wherever it
    /// is.
    pub const WHITESPACE: Self = Self(ffi::AV_ESCAPE_FLAG_WHITESPACE.cast_signed());
    /// XML mode: also escape `"`, for double-quoted attributes.
    pub const XML_DOUBLE_QUOTES: Self = Self(ffi::AV_ESCAPE_FLAG_XML_DOUBLE_QUOTES.cast_signed());
    /// XML mode: also escape `'`, for single-quoted attributes.
    pub const XML_SINGLE_QUOTES: Self = Self(ffi::AV_ESCAPE_FLAG_XML_SINGLE_QUOTES.cast_signed());

    /// The set with no flag.
    #[inline]
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The set with exactly `bits`, named or not.
    #[inline]
    #[must_use]
    pub const fn from_bits_retain(bits: c_int) -> Self {
        Self(bits)
    }

    /// The set's raw bits, as C takes them.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> c_int {
        self.0
    }

    /// Whether no bit is set.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit of `other` is set in `self`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl fmt::Debug for EscapeFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EscapeFlags(")?;
        let mut rest = self.0;
        let mut first = true;
        for &(flag, name) in &Self::KNOWN {
            if self.contains(flag) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                rest &= !flag.0;
                first = false;
            }
        }
        if rest != 0 || first {
            if !first {
                f.write_str(" | ")?;
            }
            write!(f, "{rest:#x}")?;
        }
        f.write_str(")")
    }
}

impl BitOr for EscapeFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for EscapeFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for EscapeFlags {
    type Output = Self;

    #[inline]
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

/// Wraps: `av_escape`
///
/// `src` escaped by `mode`, as a new C string:
///
/// - [`Backslash`](EscapeMode::Backslash) (and [`Auto`](EscapeMode::Auto)) puts a backslash before
///   each character of `special`, and, unless `flags` has [`STRICT`](EscapeFlags::STRICT), before
///   single quotes, backslashes, leading and trailing whitespace, and with
///   [`WHITESPACE`](EscapeFlags::WHITESPACE) all whitespace;
/// - [`Quote`](EscapeMode::Quote) encloses the text in single quotes, spelling each quote inside as
///   `'\''`;
/// - [`Xml`](EscapeMode::Xml) replaces `&`, `<` and `>` by entities, and quotes too under the
///   `XML_*` flags.
///
/// `special` is used by backslash mode only. `None` when the result cannot be
/// allocated, which includes a result of `i32::MAX` bytes or more.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::{EscapeFlags, EscapeMode, escape};
/// let s = escape(
///     c"a b:c",
///     Some(c":"),
///     EscapeMode::Backslash,
///     EscapeFlags::empty(),
/// )
/// .unwrap();
/// assert_eq!(s.to_str(), Ok("a b\\:c"));
/// let s = escape(
///     c"<it's>",
///     None,
///     EscapeMode::Xml,
///     EscapeFlags::XML_SINGLE_QUOTES,
/// )
/// .unwrap();
/// assert_eq!(s.to_str(), Ok("&lt;it&apos;s&gt;"));
/// ```
pub fn escape(
    src: &CStr,
    special: Option<&CStr>,
    mode: EscapeMode,
    flags: EscapeFlags,
) -> Option<HeapString> {
    let mut dst: *mut c_char = ptr::null_mut();
    let special = special.map_or(ptr::null(), CStr::as_ptr);
    // SAFETY: `dst` is a live, writable out-slot; `src` is a NUL-terminated
    // string and `special` NULL or one, which `av_escape` only reads during
    // the call; the mode and flags go by value, and C accepts any.
    let ret = unsafe {
        ffi::av_escape(
            &raw mut dst,
            src.as_ptr(),
            special,
            mode.into(),
            flags.bits(),
        )
    };
    if ret < 0 {
        // C leaves `dst` untouched or NULL on failure.
        return None;
    }
    // SAFETY: on success `dst` is a fresh, uniquely-owned, NUL-terminated
    // `av_malloc`-family block (`av_bprint_finalize`'s `av_realloc` or
    // `av_memdup` copy), which `av_free` releases.
    unsafe { HeapString::from_raw(dst) }
}

/// Wraps: `av_strstart`
///
/// The rest of `s` after `prefix`, or `None` when `s` does not start with
/// `prefix`. An empty prefix matches every string.
#[must_use]
pub fn strip_prefix<'a>(s: &'a CStr, prefix: &CStr) -> Option<&'a CStr> {
    let mut rest: *const c_char = ptr::null();
    // SAFETY: `s` and `prefix` are live NUL-terminated strings, which
    // `av_strstart` only reads, stopping at either NUL; `rest` is a writable
    // slot.
    let matched = unsafe { ffi::av_strstart(s.as_ptr(), prefix.as_ptr(), &raw mut rest) };
    if matched == 0 {
        return None;
    }
    // On a match C stores the address of the first byte after the prefix in
    // `s`: an offset no greater than its length.
    let offset = rest as usize - s.as_ptr() as usize;
    Some(&s[offset..])
}

/// Wraps: `av_strnstr`
///
/// The offset of the first occurrence of `needle` in `haystack`, or `None`
/// when it has none; an empty `needle` matches at 0. The whole slice is
/// searched, NUL bytes included: pass a sub-slice to search a prefix.
#[must_use]
pub fn find(haystack: &[u8], needle: &CStr) -> Option<usize> {
    let base: *const c_char = haystack.as_ptr().cast();
    // SAFETY: `av_strnstr` reads `needle`, a live NUL-terminated string, and
    // compares it against windows of `haystack` that end within its
    // `haystack.len()` bytes (none for an empty slice); it writes nothing.
    let found = unsafe { ffi::av_strnstr(base, needle.as_ptr(), haystack.len()) };
    (!found.is_null()).then(|| found as usize - base as usize)
}

/// Wraps: `av_strlcpy`
///
/// Copies `src` into `dst`, truncated to `dst.len() - 1` bytes, and
/// NUL-terminates it unless `dst` is empty; the bytes after the NUL are left
/// alone. Returns `src`'s length, so a result of at least `dst.len()` means
/// the copy was truncated.
pub fn strlcpy(dst: &mut [u8], src: &CStr) -> usize {
    // SAFETY: `av_strlcpy` writes at most `dst.len()` bytes from the start of
    // `dst` (none when it is empty) and reads `src`, a live NUL-terminated
    // string, up to its NUL.
    unsafe { ffi::av_strlcpy(dst.as_mut_ptr().cast(), src.as_ptr(), dst.len()) }
}

/// Wraps: `av_strlcat`
///
/// Appends `src` to the string `dst` holds before its first NUL, to a total
/// of at most `dst.len() - 1` bytes, NUL-terminated. Returns the length of
/// the string untruncated appending would give, the string in `dst` plus
/// `src`; when `dst` has no room after its string, it is left alone.
/// `None` when `dst` holds no NUL, and so no string to append to.
pub fn strlcat(dst: &mut [u8], src: &CStr) -> Option<usize> {
    if !dst.contains(&0) {
        return None;
    }
    // SAFETY: `dst` holds a NUL, so `av_strlcat`'s `strlen(dst)` stops at an
    // offset `len < dst.len()`; it then writes at most `dst.len() - len`
    // bytes from `dst + len`, and reads `src`, a live NUL-terminated string,
    // up to its NUL.
    Some(unsafe { ffi::av_strlcat(dst.as_mut_ptr().cast(), src.as_ptr(), dst.len()) })
}

/// Wraps: `av_strlcatf`
///
/// Appends the text `args` formats to the string `dst` holds before its
/// first NUL, as [`strlcat`] appends a string, and never writes outside
/// `dst`. Returns the length of the string untruncated appending would give.
/// The text ends at its first NUL, as C's `%s` reads it. `None` when `dst`
/// holds no NUL.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::strlcatf;
/// let mut buf = [0u8; 8];
/// assert_eq!(strlcatf(&mut buf, format_args!("{}-{}", 12, "ab")), Some(5));
/// assert_eq!(&buf[..6], b"12-ab\0");
/// ```
pub fn strlcatf(dst: &mut [u8], args: fmt::Arguments<'_>) -> Option<usize> {
    if !dst.contains(&0) {
        return None;
    }
    let text = text_until_nul(args);
    // SAFETY: as for `strlcat`, `strlen(dst)` stops at an offset
    // `len < dst.len()`, and `vsnprintf` writes at most `dst.len() - len`
    // bytes from `dst + len`. The format `%s` consumes exactly one argument,
    // `text`, a live NUL-terminated string.
    Some(unsafe {
        ffi::av_strlcatf(
            dst.as_mut_ptr().cast(),
            dst.len(),
            c"%s".as_ptr(),
            text.as_ptr(),
        )
    })
}

/// The text `args` formats, up to its first NUL.
pub(crate) fn text_until_nul(args: fmt::Arguments<'_>) -> CString {
    let mut bytes = std::fmt::format(args).into_bytes();
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes.truncate(len);
    bytes.push(0);
    // The vector now holds exactly one NUL, at its end.
    CString::from_vec_with_nul(bytes).unwrap_or_default()
}

/// Wraps: `av_strnlen`
///
/// The number of bytes of `s` before its first NUL, or `s.len()` when it has
/// none.
#[must_use]
pub fn strnlen(s: &[u8]) -> usize {
    // SAFETY: `av_strnlen` reads at most `s.len()` bytes of `s`, stopping at
    // the first NUL, and writes nothing.
    unsafe { ffi::av_strnlen(s.as_ptr().cast(), s.len()) }
}

/// Wraps: `av_strncasecmp`
///
/// Compares at most the first `n` bytes of `a` and `b`, as
/// [`cmp_ignore_ascii_case`] compares whole strings: ASCII letters
/// regardless of case, locale-independently; bytes are ordered as unsigned
/// after lowercasing, and a string orders before its extensions. With `n`
/// of 0 everything is equal.
#[must_use]
pub fn cmp_n_ignore_ascii_case(a: &CStr, b: &CStr, n: usize) -> Ordering {
    // SAFETY: `a` and `b` are live NUL-terminated strings; `av_strncasecmp`
    // reads them in step and stops at the first NUL of either, a difference,
    // or after `n` bytes, and writes nothing.
    let diff = unsafe { ffi::av_strncasecmp(a.as_ptr(), b.as_ptr(), n) };
    diff.cmp(&0)
}

/// Wraps: `av_strtok`
///
/// An iterator over the tokens of a string, built by [`tokens`]: the
/// non-empty runs of bytes not in its delimiter set.
///
/// Splits the string in place, as `strtok_r` does: each yielded token is
/// NUL-terminated by overwriting the delimiter that ends it, so once the
/// iterator and its tokens are gone, the buffer holds the tokens separated by
/// NULs. Tokens stay valid while later ones are found, since C only writes
/// past them:
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::tokens;
/// let mut buf = *b"a,,b c\0";
/// let all: Vec<_> = tokens(&mut buf, c", ").unwrap().collect();
/// assert_eq!(all, [c"a", c"b", c"c"]);
/// assert_eq!(&buf, b"a\0,b\0c\0");
/// ```
#[derive(Debug)]
pub struct Tokens<'a> {
    /// The string for the next `av_strtok` call: the buffer's start on the
    /// first, NULL afterwards.
    s: *mut c_char,
    /// `av_strtok`'s `saveptr`.
    save: *mut c_char,
    delim: &'a CStr,
    _buf: PhantomData<&'a mut [u8]>,
}

/// The tokens of the string `buf` holds before its first NUL, separated by
/// any of the bytes of `delim`; `None` when `buf` holds no NUL. See
/// [`Tokens`].
pub fn tokens<'a>(buf: &'a mut [u8], delim: &'a CStr) -> Option<Tokens<'a>> {
    if !buf.contains(&0) {
        return None;
    }
    Some(Tokens {
        s: buf.as_mut_ptr().cast(),
        save: ptr::null_mut(),
        delim,
        _buf: PhantomData,
    })
}

impl<'a> Iterator for Tokens<'a> {
    type Item = &'a CStr;

    fn next(&mut self) -> Option<&'a CStr> {
        let s = core::mem::replace(&mut self.s, ptr::null_mut());
        // SAFETY: on the first call `s` is the start of the buffer this
        // iterator borrows exclusively for `'a`, which holds a NUL; later, `s`
        // is NULL and `save` is NULL or where the previous call left it, in
        // that buffer at or before its first NUL. Either way `av_strtok` reads
        // and writes the buffer only up to that NUL (`strspn` / `strcspn`
        // stop there), writing one NUL over the delimiter that ends a token,
        // after every token already yielded; `delim` is a live
        // NUL-terminated string and `save` a writable slot.
        let token = unsafe { ffi::av_strtok(s, self.delim.as_ptr(), &raw mut self.save) };
        if token.is_null() {
            return None;
        }
        // SAFETY: `token` is a NUL-terminated run in the buffer, which lives
        // for `'a`; neither later calls, which write only after its NUL, nor
        // anything else writes those bytes while the buffer is borrowed.
        Some(unsafe { CStr::from_ptr(token) })
    }
}

// Once `av_strtok` returns NULL it leaves `save` NULL, and with `s` NULL every
// later call returns NULL again.
impl FusedIterator for Tokens<'_> {}

/// Wraps: `av_toupper`
///
/// `c` with an ASCII lowercase letter mapped to uppercase, locale-independently;
/// any other value, `EOF` and bytes above 127 included, is returned as is.
#[must_use]
pub fn to_ascii_uppercase(c: c_int) -> c_int {
    // SAFETY: `av_toupper` takes and returns a value, touching no memory.
    unsafe { ffi::av_toupper(c) }
}

/// Wraps: `av_tolower`
///
/// `c` with an ASCII uppercase letter mapped to lowercase, locale-independently;
/// any other value, `EOF` and bytes above 127 included, is returned as is.
#[must_use]
pub fn to_ascii_lowercase(c: c_int) -> c_int {
    // SAFETY: `av_tolower` takes and returns a value, touching no memory.
    unsafe { ffi::av_tolower(c) }
}

/// Why [`utf8_decode`] rejected a sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Utf8Error {
    averror: c_int,
    code_point: Option<u32>,
}

impl Utf8Error {
    /// The `AVERROR` code C returned, `AVERROR(EILSEQ)`.
    #[must_use]
    pub fn code(self) -> c_int {
        self.averror
    }

    /// The value a well-formed sequence decoded to when the flags rejected
    /// it (above U+10FFFF, a surrogate, a non-character, or an XML-invalid
    /// control code), or `None` for a malformed sequence.
    #[must_use]
    pub fn code_point(self) -> Option<u32> {
        self.code_point
    }
}

impl fmt::Display for Utf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code_point {
            Some(code) => write!(f, "UTF-8 code point {code:#x} rejected by the flags"),
            None => f.write_str("invalid UTF-8 sequence"),
        }
    }
}

impl std::error::Error for Utf8Error {}

/// Wraps: `av_utf8_decode`
///
/// Decodes the UTF-8 sequence at the start of `buf` under `flags`, a
/// combination of the `ffi::AV_UTF8_FLAG_*` constants: the code point or why
/// it was rejected, and the number of bytes consumed, which is at least 1.
/// `None` when `buf` is empty.
///
/// The decoder accepts the original sequences of up to six bytes, so a code
/// point may exceed U+10FFFF, up to `i32::MAX`, when
/// `AV_UTF8_FLAG_ACCEPT_INVALID_BIG_CODES` allows it.
///
/// What is consumed follows C, not the sequence's length: exactly one byte
/// for an invalid lead byte, for a tail byte that is not `10xxxxxx` (however
/// many tail bytes before it were fine: `E2 82 41` consumes only `E2`, and
/// the next call starts at `82`), and for a sequence `buf` cuts short
/// (`E2 82` at its end consumes only `E2`); the whole sequence otherwise,
/// for an overlong encoding (no code point) as for a well-formed one that
/// `flags` rejects (its code point).
#[must_use]
pub fn utf8_decode(buf: &[u8], flags: c_uint) -> Option<(Result<u32, Utf8Error>, usize)> {
    let range = buf.as_ptr_range();
    let mut p = range.start;
    // C stores only code points below 2^31, so a negative value means unset.
    let mut code: i32 = -1;
    // SAFETY: `p` and `range.end` delimit `buf`; `av_utf8_decode` reads bytes
    // from `p` only while below `range.end`, advances `p` within that range,
    // and writes `code` and `p`, both writable slots.
    let ret = unsafe { ffi::av_utf8_decode(&raw mut code, &raw mut p, range.end, flags) };
    let consumed = p as usize - range.start as usize;
    if consumed == 0 {
        // C consumes nothing only for an empty buffer, returning 0.
        return None;
    }
    let code_point = u32::try_from(code).ok();
    let result = match code_point {
        Some(code) if ret >= 0 => Ok(code),
        _ => Err(Utf8Error {
            averror: ret,
            code_point,
        }),
    };
    Some((result, consumed))
}

/// Wraps: `av_asprintf`
///
/// The text `args` formats, as a new C string; build `args` with
/// [`format_args!`]. `None` when it cannot be allocated, which includes text
/// of `c_int::MAX` bytes or more and text whose length plus its NUL exceeds
/// the `av_max_alloc` limit (`INT_MAX` by default).
///
/// Rust formats the text, since C's `printf` arguments have no safe Rust
/// form; `av_asprintf` then measures it, allocates the string and copies it
/// in through a `"%.*s"` conversion. A C string ends at its first NUL, so the
/// result holds the text before the first NUL `args` produces.
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::format;
/// let s = format(format_args!("{}x{}", 1920, 1080)).unwrap();
/// assert_eq!(s.to_str(), Ok("1920x1080"));
/// ```
pub fn format(args: fmt::Arguments<'_>) -> Option<HeapString> {
    let text = args
        .as_str()
        .map_or_else(|| Cow::Owned(args.to_string()), Cow::Borrowed);
    let bytes = text.as_bytes();
    let bytes = bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |nul| &bytes[..nul]);
    // `av_asprintf` sizes its buffer as the `int` length plus one, which
    // overflows at `INT_MAX`; longer text does not fit the precision.
    let len = c_int::try_from(bytes.len())
        .ok()
        .filter(|&n| n < c_int::MAX)?;
    // An empty slice's pointer is dangling, which `%s` may not receive.
    let src = if bytes.is_empty() {
        c"".as_ptr()
    } else {
        bytes.as_ptr().cast()
    };
    // SAFETY: the format is a literal whose one conversion, `%.*s`, takes the
    // `int` precision `len` and the `char *` `src`, which is valid for reads of
    // `len` bytes, all `vsnprintf` reads: it stops at the precision. `len` is
    // below `INT_MAX`, so the buffer size `len + 1` does not overflow.
    let ptr = unsafe { ffi::av_asprintf(c"%.*s".as_ptr(), len, src) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned, NUL-terminated
    // `av_malloc` block, which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

/// Wraps: `av_basename`
///
/// The last component of `path`: the part after its last `'/'`, which is
/// empty when `path` ends in one, or `path` itself when it has none; `"."`
/// for an empty `path` (as C returns for NULL too).
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::basename;
/// assert_eq!(basename(c"/usr/lib/libavutil.so"), c"libavutil.so");
/// assert_eq!(basename(c""), c".");
/// ```
#[must_use]
pub fn basename(path: &CStr) -> &CStr {
    // SAFETY: `path` is a live NUL-terminated string, which `av_basename`
    // only reads.
    let base = unsafe { ffi::av_basename(path.as_ptr()) };
    // SAFETY: `av_basename` never returns NULL: it returns a pointer to the
    // NUL-terminated tail of `path`, which the borrow keeps alive and
    // unchanged, or to the static literal `"."`, which outlives it.
    unsafe { CStr::from_ptr(base) }
}

/// Wraps: `av_append_path_component`
///
/// `path` and `component` joined by a `'/'`: one is added when neither side
/// has one at the join, and when both do, one of the two is dropped (so
/// `"a//"` and `"//b"` join as `"a///b"`). When either side is empty, the
/// result is a copy of the other. `None` when the string cannot be
/// allocated: C asks for `path.count_bytes() + component.count_bytes() + 2`
/// bytes whatever the join, so that sum exceeding the `av_max_alloc` limit
/// (`INT_MAX` by default) fails even when the joined text and its NUL would
/// fit.
///
/// (C also accepts NULL for either side, treated as an empty string except
/// that two NULLs yield NULL.)
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::append_path_component;
/// let p = append_path_component(c"/tmp/", c"/out.mkv").unwrap();
/// assert_eq!(p.to_str(), Ok("/tmp/out.mkv"));
/// ```
#[must_use]
pub fn append_path_component(path: &CStr, component: &CStr) -> Option<HeapString> {
    // SAFETY: both are live NUL-terminated strings, which
    // `av_append_path_component` only reads.
    let ptr = unsafe { ffi::av_append_path_component(path.as_ptr(), component.as_ptr()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned, NUL-terminated
    // `av_malloc` (or `av_strdup`) block, which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

/// Wraps: `av_dirname`
///
/// The parent directory of the path held by `path` up to its first NUL, found
/// as C does: `path` is truncated in place at its last `/` and returned up to
/// there, or, when it has no `/`, left as is while `"."` is returned. `None`,
/// without calling C, when `path` holds no NUL.
///
/// This is C's textual cut, not POSIX `dirname`: a path whose only `/` leads
/// it (`"/usr"`) gives the empty string, and a trailing `/` is cut rather than
/// skipped (`"a/b/"` gives `"a/b"`). Pass a copy to keep the path intact.
pub fn dirname(path: &mut [u8]) -> Option<&CStr> {
    if !path.contains(&0) {
        return None;
    }
    let base = path.as_mut_ptr();
    // SAFETY: `path` is writable and holds a NUL, so `base` is a live
    // NUL-terminated string that nothing else borrows during the call.
    // `av_dirname` reads it up to that NUL (`strrchr`), writes at most one NUL
    // over its last `/`, before the original NUL, and keeps no pointer.
    let ret = unsafe { ffi::av_dirname(base.cast()) };
    if ret.cast::<u8>() == base.cast_const() {
        CStr::from_bytes_until_nul(path).ok()
    } else {
        // C returns its static `"."` whenever it has not cut `path`.
        Some(c".")
    }
}

/// Wraps: `av_get_token`
///
/// The next token of `*buf`, unescaped, and `*buf` advanced past it to the
/// first unescaped character of `term` or to its end.
///
/// Leading and trailing whitespace is dropped unless escaped with `\` or
/// quoted between `'`; a `\` takes the next character literally, and `'...'`
/// copies its contents verbatim. `None` when the token cannot be allocated,
/// which leaves `*buf` as it was; the allocation is `buf.count_bytes() + 1`
/// bytes, so it fails above the `av_max_alloc` limit too.
///
/// # Panics
///
/// Only if C breaks the guarantee that the suffix keeps the NUL.
pub fn get_token(buf: &mut &CStr, term: &CStr) -> Option<HeapString> {
    let start = buf.as_ptr();
    let mut cursor = start;
    // SAFETY: `cursor` points to `*buf`, a live NUL-terminated string, and
    // `term` is one too; `av_get_token` only reads both, and writes `cursor`
    // once, to a position within `*buf` no later than its NUL.
    let token = unsafe { ffi::av_get_token(&raw mut cursor, term.as_ptr()) };
    let bytes = buf.to_bytes_with_nul();
    let advanced = (cursor as usize).wrapping_sub(start as usize);
    assert!(advanced < bytes.len(), "av_get_token left the string");
    *buf = CStr::from_bytes_until_nul(&bytes[advanced..]).expect("the suffix keeps the NUL");
    // SAFETY: a non-NULL `token` is a fresh, uniquely-owned, NUL-terminated
    // `av_realloc` block, which `av_free` releases.
    unsafe { HeapString::from_raw(token) }
}

/// Wraps: `av_isdigit`
///
/// Whether `c` is an ASCII digit, `'0'..='9'`, whatever the locale; every
/// other `int`, `EOF` and non-ASCII bytes included, is not.
#[must_use]
pub fn is_ascii_digit(c: c_int) -> bool {
    // SAFETY: `av_isdigit` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_isdigit(c) != 0 }
}

/// Wraps: `av_strcasecmp`
///
/// Compares `a` and `b` byte by byte with ASCII letters folded to lower case,
/// as [`u8::eq_ignore_ascii_case`] does; other bytes compare by value.
#[must_use]
pub fn cmp_ignore_ascii_case(a: &CStr, b: &CStr) -> Ordering {
    // SAFETY: both are live NUL-terminated strings, which C only reads, up to
    // the first difference or terminator.
    unsafe { ffi::av_strcasecmp(a.as_ptr(), b.as_ptr()) }.cmp(&0)
}

/// Wraps: `av_stristart`
///
/// The rest of `s` after `prefix`, when `s` starts with `prefix` ignoring
/// ASCII case; `None` otherwise. An empty `prefix` matches, leaving all of
/// `s`.
#[must_use]
pub fn strip_prefix_ignore_ascii_case<'a>(s: &'a CStr, prefix: &CStr) -> Option<&'a CStr> {
    let mut rest: *const c_char = ptr::null();
    // SAFETY: both are live NUL-terminated strings, which C only reads, and
    // `rest` is a live, writable local, which C overwrites only on a match.
    let matched = unsafe { ffi::av_stristart(s.as_ptr(), prefix.as_ptr(), &raw mut rest) };
    // On a match C stores the address just past the prefix within `s`: an
    // offset no greater than its length.
    (matched != 0).then(|| &s[rest as usize - s.as_ptr() as usize..])
}

/// Wraps: `av_stristr`
///
/// The offset of the first occurrence of `needle` in `haystack`, ignoring
/// ASCII case, or `None` when it has none; an empty `needle` matches at 0.
/// `&haystack[offset..]` is the rest from the match on.
#[must_use]
pub fn find_ignore_ascii_case(haystack: &CStr, needle: &CStr) -> Option<usize> {
    // SAFETY: both are live NUL-terminated strings, which C only reads; it
    // writes nothing through the result it hands back non-const.
    let found = unsafe { ffi::av_stristr(haystack.as_ptr(), needle.as_ptr()) };
    // A match lies within `haystack`, at or before its terminator.
    (!found.is_null()).then(|| found as usize - haystack.as_ptr() as usize)
}

/// Wraps: `av_strireplace`
///
/// A copy of `s` with every occurrence of `from`, matched ignoring ASCII case
/// left to right without overlap, replaced by `to`; `None` when it cannot be
/// allocated.
///
/// An empty `from` occurs nowhere, so the result is a copy of `s`. (C loops
/// forever on it: its search matches the empty string at the same position
/// again and again without advancing.)
#[must_use]
pub fn replace_ignore_ascii_case(s: &CStr, from: &CStr, to: &CStr) -> Option<HeapString> {
    if from.is_empty() {
        return string_from_c_str(s);
    }
    // SAFETY: all three are live NUL-terminated strings, which C only reads;
    // a non-empty `from` advances C's search past each match.
    let ptr = unsafe { ffi::av_strireplace(s.as_ptr(), from.as_ptr(), to.as_ptr()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned NUL-terminated
    // string from `av_bprint_finalize` (an `av_malloc`- or `av_realloc`-family
    // block), which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

/// The most destinations one [`scan`] call binds.
pub const SCAN_MAX_ARGS: usize = 16;

/// One destination of a [`scan`] conversion.
///
/// An integer conversion stores into an integer of the size its length
/// modifier names, signed or unsigned, and `%p` into a pointer-sized one; a
/// floating-point conversion into an `f32` (no modifier) or an `f64` (`l`);
/// and `%c`, `%s` and `%[` into a byte buffer, which must hold the
/// conversion's width (plus the terminator `%s` and `%[` add). The `From`
/// conversions build one from a mutable reference.
#[derive(Debug)]
pub enum ScanArg<'a> {
    /// A signed byte: `%hhd`, `%hhn`.
    I8(&'a mut i8),
    /// An unsigned byte: `%hhu`, `%hhx`.
    U8(&'a mut u8),
    /// A `short`: `%hd`.
    I16(&'a mut i16),
    /// An `unsigned short`: `%hu`.
    U16(&'a mut u16),
    /// An `int`: `%d`, `%i`, `%n`.
    I32(&'a mut i32),
    /// An `unsigned`: `%u`, `%x`, `%o`.
    U32(&'a mut u32),
    /// A 64-bit integer: `%lld`, `%jd`, and `%ld` where `long` is 64-bit.
    I64(&'a mut i64),
    /// An unsigned 64-bit integer: `%llu`, `%llx`, and `%lu` where `long` is
    /// 64-bit.
    U64(&'a mut u64),
    /// A pointer-sized signed integer: `%zd`, `%td` where `long` is
    /// pointer-sized.
    Isize(&'a mut isize),
    /// A pointer-sized unsigned integer: `%zu`, and the address `%p` reads.
    Usize(&'a mut usize),
    /// A `float`: `%f`, `%e`, `%g`, `%a`.
    F32(&'a mut f32),
    /// A `double`: `%lf`, `%le`, `%lg`, `%la`.
    F64(&'a mut f64),
    /// A byte buffer: `%c` (no terminator), `%s` and `%[` (terminated).
    Bytes(&'a mut [u8]),
}

macro_rules! scan_arg_from {
    ($($t:ty => $v:ident),* $(,)?) => {$(
        impl<'a> From<&'a mut $t> for ScanArg<'a> {
            fn from(dst: &'a mut $t) -> Self {
                Self::$v(dst)
            }
        }
    )*};
}
scan_arg_from!(
    i8 => I8, u8 => U8, i16 => I16, u16 => U16, i32 => I32, u32 => U32,
    i64 => I64, u64 => U64, isize => Isize, usize => Usize, f32 => F32,
    f64 => F64, [u8] => Bytes,
);

impl<'a, const N: usize> From<&'a mut [u8; N]> for ScanArg<'a> {
    fn from(dst: &'a mut [u8; N]) -> Self {
        Self::Bytes(dst)
    }
}

impl ScanArg<'_> {
    /// The integer's size in bytes, for an integer destination.
    fn int_size(&self) -> Option<usize> {
        Some(match self {
            Self::I8(_) | Self::U8(_) => 1,
            Self::I16(_) | Self::U16(_) => 2,
            Self::I32(_) | Self::U32(_) => 4,
            Self::I64(_) | Self::U64(_) => 8,
            Self::Isize(_) | Self::Usize(_) => size_of::<usize>(),
            Self::F32(_) | Self::F64(_) | Self::Bytes(_) => return None,
        })
    }

    /// The destination's address, which C writes through.
    fn as_mut_ptr(&mut self) -> *mut c_void {
        match self {
            Self::I8(p) => ptr::from_mut(*p).cast(),
            Self::U8(p) => ptr::from_mut(*p).cast(),
            Self::I16(p) => ptr::from_mut(*p).cast(),
            Self::U16(p) => ptr::from_mut(*p).cast(),
            Self::I32(p) => ptr::from_mut(*p).cast(),
            Self::U32(p) => ptr::from_mut(*p).cast(),
            Self::I64(p) => ptr::from_mut(*p).cast(),
            Self::U64(p) => ptr::from_mut(*p).cast(),
            Self::Isize(p) => ptr::from_mut(*p).cast(),
            Self::Usize(p) => ptr::from_mut(*p).cast(),
            Self::F32(p) => ptr::from_mut(*p).cast(),
            Self::F64(p) => ptr::from_mut(*p).cast(),
            Self::Bytes(b) => b.as_mut_ptr().cast(),
        }
    }
}

/// Why [`scan`] rejected a format before calling C. Each variant carries
/// the byte offset of the `%` that starts the offending conversion, where
/// there is one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScanError {
    /// A conversion C does not implement, or one this wrapper cannot bind
    /// safely: `%m` allocation, `%n$` positions, `%C` / `%S`, `L`, length
    /// modifiers on `%c`, `%s`, `%[` and `%p`, or a `%` ending the format.
    Unsupported(usize),
    /// A `%[` whose set has no closing `]`.
    UnterminatedSet(usize),
    /// A field width beyond `i32::MAX`, which overflows C's width counter.
    WidthOverflow(usize),
    /// A `%s` or `%[` without a positive width, which would write past any
    /// buffer.
    Unbounded(usize),
    /// A conversion with no destination left to bind.
    MissingArg(usize),
    /// A destination of the wrong kind or size for its conversion.
    Mismatch(usize),
    /// A byte buffer too small for its conversion's width.
    BufferTooSmall(usize),
    /// More destinations than conversions.
    ExtraArgs,
    /// More than [`SCAN_MAX_ARGS`] destinations.
    TooManyArgs,
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Unsupported(at) => write!(f, "unsupported conversion at byte {at}"),
            Self::UnterminatedSet(at) => write!(f, "unterminated %[ set at byte {at}"),
            Self::WidthOverflow(at) => write!(f, "field width overflows at byte {at}"),
            Self::Unbounded(at) => write!(f, "unbounded string conversion at byte {at}"),
            Self::MissingArg(at) => write!(f, "no destination for the conversion at byte {at}"),
            Self::Mismatch(at) => write!(f, "wrong destination for the conversion at byte {at}"),
            Self::BufferTooSmall(at) => {
                write!(f, "buffer too small for the conversion at byte {at}")
            }
            Self::ExtraArgs => f.write_str("more destinations than conversions"),
            Self::TooManyArgs => write!(f, "more than {SCAN_MAX_ARGS} destinations"),
        }
    }
}

impl std::error::Error for ScanError {}

/// The length modifier of a conversion, as `av_sscanf` reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanSize {
    /// None: `int`, `float`.
    Default,
    /// `hh`: `char`.
    Char,
    /// `h`: `short`.
    Short,
    /// `l`, `z`, `t`: `long`; `double` for a floating-point conversion.
    Long,
    /// `ll`, `j`: `long long`.
    LongLong,
    /// `L`.
    LongDouble,
}

/// Checks `args` against the conversions of `format`, as `av_sscanf` parses
/// them, and returns their addresses in conversion order, NULL-padded to
/// [`SCAN_MAX_ARGS`].
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the format string, as C's scanner is"
)]
#[expect(
    clippy::match_same_arms,
    reason = "the arms follow C's cases one for one"
)]
#[expect(
    clippy::items_after_statements,
    reason = "the item is declared where the code it serves begins"
)]
fn bind_scan_args(
    format: &[u8],
    args: &mut [ScanArg<'_>],
) -> Result<[*mut c_void; SCAN_MAX_ARGS], ScanError> {
    if args.len() > SCAN_MAX_ARGS {
        return Err(ScanError::TooManyArgs);
    }
    let mut ptrs = [ptr::null_mut(); SCAN_MAX_ARGS];
    let mut bound = 0;
    let at = |i: usize| format.get(i).copied();
    let mut i = 0;
    while let Some(c) = at(i) {
        // Whitespace and literal bytes bind nothing, and nor does `%%`.
        if c != b'%' {
            i += 1;
            continue;
        }
        if at(i + 1) == Some(b'%') {
            i += 2;
            continue;
        }
        let start = i;
        i += 1;
        let suppress = at(i) == Some(b'*');
        if suppress {
            i += 1;
        } else if at(i).is_some_and(|c| c.is_ascii_digit()) && at(i + 1) == Some(b'$') {
            return Err(ScanError::Unsupported(start));
        }
        let mut width: i32 = 0;
        while let Some(d) = at(i).filter(u8::is_ascii_digit) {
            width = width
                .checked_mul(10)
                .and_then(|w| w.checked_add(i32::from(d - b'0')))
                .ok_or(ScanError::WidthOverflow(start))?;
            i += 1;
        }
        let size = match (at(i), at(i + 1)) {
            (Some(b'h'), Some(b'h')) => ScanSize::Char,
            (Some(b'h'), _) => ScanSize::Short,
            (Some(b'l'), Some(b'l')) | (Some(b'j'), _) => ScanSize::LongLong,
            (Some(b'l' | b'z' | b't'), _) => ScanSize::Long,
            (Some(b'L'), _) => ScanSize::LongDouble,
            _ => ScanSize::Default,
        };
        i += match size {
            ScanSize::Default => 0,
            ScanSize::Char => 2,
            ScanSize::LongLong if at(i) == Some(b'l') => 2,
            _ => 1,
        };
        let conv = at(i).ok_or(ScanError::Unsupported(start))?;
        i += 1;
        /// What a conversion stores.
        enum Store {
            Int(usize),
            Float(usize),
            Bytes(usize),
        }
        let store = match (conv, size) {
            (b'd' | b'i' | b'o' | b'u' | b'x' | b'X' | b'n', size) => Store::Int(match size {
                ScanSize::Default => size_of::<c_int>(),
                ScanSize::Char => size_of::<c_char>(),
                ScanSize::Short => size_of::<c_short>(),
                ScanSize::Long => size_of::<c_long>(),
                ScanSize::LongLong => size_of::<c_longlong>(),
                ScanSize::LongDouble => return Err(ScanError::Unsupported(start)),
            }),
            (b'a' | b'e' | b'f' | b'g' | b'A' | b'E' | b'F' | b'G', ScanSize::Default) => {
                Store::Float(size_of::<f32>())
            }
            (b'a' | b'e' | b'f' | b'g' | b'A' | b'E' | b'F' | b'G', ScanSize::Long) => {
                Store::Float(size_of::<f64>())
            }
            (b'p', ScanSize::Default) => Store::Int(size_of::<*mut c_void>()),
            (b'c', ScanSize::Default) => Store::Bytes(width.max(1).unsigned_abs() as usize),
            (b's' | b'[', ScanSize::Default) => {
                if conv == b'[' {
                    // C reads `^`, then a leading `]` or `-` as a member, then
                    // members up to the first `]`.
                    if at(i) == Some(b'^') {
                        i += 1;
                    }
                    if matches!(at(i), Some(b']' | b'-')) {
                        i += 1;
                    }
                    let close = format[i.min(format.len())..]
                        .iter()
                        .position(|&c| c == b']')
                        .ok_or(ScanError::UnterminatedSet(start))?;
                    i += close + 1;
                }
                if width == 0 && !suppress {
                    return Err(ScanError::Unbounded(start));
                }
                Store::Bytes(width.unsigned_abs() as usize + 1)
            }
            _ => return Err(ScanError::Unsupported(start)),
        };
        if suppress {
            continue;
        }
        let arg = args.get_mut(bound).ok_or(ScanError::MissingArg(start))?;
        let fits = match (store, &*arg) {
            (Store::Int(n), arg) => arg.int_size() == Some(n),
            (Store::Float(4), ScanArg::F32(_)) | (Store::Float(8), ScanArg::F64(_)) => true,
            (Store::Float(_), _) => false,
            (Store::Bytes(n), ScanArg::Bytes(b)) => {
                if b.len() < n {
                    return Err(ScanError::BufferTooSmall(start));
                }
                true
            }
            (Store::Bytes(_), _) => false,
        };
        if !fits {
            return Err(ScanError::Mismatch(start));
        }
        ptrs[bound] = arg.as_mut_ptr();
        bound += 1;
    }
    if bound != args.len() {
        return Err(ScanError::ExtraArgs);
    }
    Ok(ptrs)
}

/// Wraps: `av_sscanf`
///
/// Reads `input` by `format`, as C's `sscanf` in the C locale, storing each
/// conversion into the next destination of `args`.
///
/// Returns the number of destinations assigned, which stops at the first
/// conversion that fails to match, or `None` when the input ends before any
/// is assigned (C's `EOF`). Destinations past the last assigned one are
/// unchanged, except that a failed `%c` may have written part of its buffer.
///
/// `format` is checked against `args` before C runs, so a mismatch is a
/// [`ScanError`] rather than a stray write: every unsuppressed conversion
/// needs exactly one destination of its type ([`ScanArg`]), `%s` and `%[`
/// need a width (`%15s` into 16 bytes), and conversions C mishandles are
/// refused (see [`ScanError::Unsupported`]).
///
/// ```
/// use ffmpeg_bindings::libavutil::libavutil::avstring::scan;
/// let (mut w, mut h, mut name) = (0i32, 0i32, [0u8; 8]);
/// let n = scan(
///     c"640x480 vga",
///     c"%dx%d %7s",
///     &mut [(&mut w).into(), (&mut h).into(), (&mut name).into()],
/// );
/// assert_eq!(n, Ok(Some(3)));
/// assert_eq!((w, h, &name[..4]), (640, 480, &b"vga\0"[..]));
/// ```
///
/// # Errors
///
/// Returns the number of destinations assigned, which stops at the first conversion that fails to
/// match, or `None` when the input ends before any is assigned (C's `EOF`).
pub fn scan(
    input: &CStr,
    format: &CStr,
    args: &mut [ScanArg<'_>],
) -> Result<Option<usize>, ScanError> {
    let p = bind_scan_args(format.to_bytes(), args)?;
    // SAFETY: `input` and `format` are live NUL-terminated strings, which C
    // only reads. `bind_scan_args` checked that `format` has one conversion
    // C implements per address in `p`, in order, and that each address is an
    // exclusively borrowed destination of the size that conversion stores
    // (with room for each `%c` / `%s` / `%[` width and terminator), so C's
    // `va_arg` reads only bound addresses and writes only within them. The
    // NULL padding is never read. C keeps no pointer after returning.
    let ret = unsafe {
        ffi::av_sscanf(
            input.as_ptr(),
            format.as_ptr(),
            p[0],
            p[1],
            p[2],
            p[3],
            p[4],
            p[5],
            p[6],
            p[7],
            p[8],
            p[9],
            p[10],
            p[11],
            p[12],
            p[13],
            p[14],
            p[15],
        )
    };
    Ok(usize::try_from(ret).ok())
}

/// Wraps: `av_isgraph`
///
/// Whether `c` is a printable ASCII character other than space (33 to 126),
/// whatever the locale. Any `int` is accepted, `EOF` included.
#[must_use]
pub fn is_graph(c: c_int) -> bool {
    // SAFETY: `av_isgraph` takes its argument by value and reads no memory.
    unsafe { ffi::av_isgraph(c) != 0 }
}

/// Wraps: `av_isspace`
///
/// Whether `c` is ASCII white space (space, `\t`, `\n`, `\v`, `\f` or `\r`),
/// whatever the locale. Unlike [`u8::is_ascii_whitespace`], the vertical tab
/// counts. Any `int` is accepted.
#[must_use]
pub fn is_space(c: c_int) -> bool {
    // SAFETY: `av_isspace` takes its argument by value and reads no memory.
    unsafe { ffi::av_isspace(c) != 0 }
}

/// Wraps: `av_isxdigit`
///
/// Whether `c` is an ASCII hexadecimal digit, either case, whatever the
/// locale. Any `int` is accepted.
#[must_use]
pub fn is_xdigit(c: c_int) -> bool {
    // SAFETY: `av_isxdigit` takes its argument by value and reads no memory.
    unsafe { ffi::av_isxdigit(c) != 0 }
}

/// Wraps: `av_match_name`
///
/// Whether `name` matches the comma-separated `names`: an entry equal to it
/// ignoring ASCII case, or `ALL`, matches; an entry prefixed with `-` that
/// matches makes the result `false`. The first matching entry decides.
#[must_use]
pub fn match_name(name: &CStr, names: &CStr) -> bool {
    // SAFETY: both are live NUL-terminated strings, which `av_match_name`
    // only reads, never past their terminators.
    unsafe { ffi::av_match_name(name.as_ptr(), names.as_ptr()) != 0 }
}

/// Wraps: `av_match_list`
///
/// Whether `name` and `list`, both `separator`-separated lists, share an
/// entry, compared as C compares them.
///
/// The separator is non-zero: with a NUL separator, C steps past the
/// terminator of `list` and reads out of bounds.
#[must_use]
pub fn match_list(name: &CStr, list: &CStr, separator: NonZeroU8) -> bool {
    // C compares `char`s, so a byte above 0x7f is its signed `char` value on
    // both sides.
    let separator = separator.get().cast_signed();
    // SAFETY: both are live NUL-terminated strings; with a non-NUL separator,
    // `av_match_list` reads each only up to its terminator.
    unsafe { ffi::av_match_list(name.as_ptr(), list.as_ptr(), separator) != 0 }
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
mod unit_tests {
    use super::*;

    const ALL: [EscapeMode; 4] = [
        EscapeMode::Auto,
        EscapeMode::Backslash,
        EscapeMode::Quote,
        EscapeMode::Xml,
    ];

    #[test]
    fn default_is_auto() {
        assert_eq!(EscapeMode::default(), EscapeMode::Auto);
        assert_eq!(
            ffi::AVEscapeMode::from(EscapeMode::default()),
            ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO
        );
    }

    #[test]
    fn modes_encode_to_distinct_values() {
        for (i, a) in ALL.iter().enumerate() {
            assert_eq!(ffi::AVEscapeMode::from(*a), a.into_raw());
            for b in &ALL[i + 1..] {
                assert_ne!(a.into_raw(), b.into_raw());
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_copies_static_and_formatted_text() {
        let s = format(format_args!("static")).unwrap();
        assert_eq!(s.to_str(), Ok("static"));
        let s = format(format_args!("{}-{:03}", "n", 7)).unwrap();
        assert_eq!(s.to_str(), Ok("n-007"));
        let s = format(format_args!("")).unwrap();
        assert!(s.is_empty() && !s.as_ptr().is_null());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_stops_at_the_first_nul() {
        let s = format(format_args!("ab{}cd", '\0')).unwrap();
        assert_eq!(s.as_bytes(), b"ab");
        let s = format(format_args!("\0tail")).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_passes_percent_signs_through() {
        let s = format(format_args!("100% %s %n %{}", 'd')).unwrap();
        assert_eq!(s.to_str(), Ok("100% %s %n %d"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn basename_borrows_the_last_component() {
        let path = c"a/b/c.txt";
        let base = basename(path);
        assert_eq!(base, c"c.txt");
        // A view into `path`, not a copy.
        assert_eq!(base.as_ptr(), path.as_ptr().wrapping_add(4));
        assert_eq!(basename(c"plain"), c"plain");
        assert_eq!(basename(c"dir/"), c"");
        assert_eq!(basename(c"/"), c"");
        assert_eq!(basename(c""), c".");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn append_path_component_merges_the_join() {
        let cases: [(&CStr, &CStr, &str); 8] = [
            (c"a", c"b", "a/b"),
            (c"a/", c"b", "a/b"),
            (c"a", c"/b", "a/b"),
            (c"a/", c"/b", "a/b"),
            (c"a//", c"//b", "a///b"),
            (c"", c"b", "b"),
            (c"a", c"", "a"),
            (c"", c"", ""),
        ];
        for (path, component, joined) in cases {
            let p = append_path_component(path, component).unwrap();
            assert_eq!(p.to_str(), Ok(joined), "{path:?} + {component:?}");
        }
    }

    #[test]
    fn debug_names_the_variants() {
        let names: Vec<_> = ALL.iter().map(|m| format!("{m:?}")).collect();
        assert_eq!(names, ["Auto", "Backslash", "Quote", "Xml"]);
    }

    #[test]
    fn appends_reject_a_buffer_without_a_string_before_c() {
        let mut full = *b"abc";
        assert_eq!(strlcat(&mut full, c"d"), None);
        assert_eq!(strlcatf(&mut full, format_args!("d")), None);
        assert_eq!(strlcat(&mut [], c""), None);
        assert_eq!(strlcatf(&mut [], format_args!("")), None);
        assert_eq!(&full, b"abc");
    }

    #[test]
    fn tokens_reject_a_buffer_without_a_string_before_c() {
        let mut buf = *b"a b";
        assert!(tokens(&mut buf, c" ").is_none());
        assert!(tokens(&mut [], c" ").is_none());
        assert_eq!(&buf, b"a b");
    }

    #[test]
    fn formatted_text_ends_at_its_first_nul() {
        assert_eq!(
            text_until_nul(format_args!("ab{}cd", '\0')).as_bytes(),
            b"ab"
        );
        assert_eq!(text_until_nul(format_args!("{}", 12)).as_bytes(), b"12");
        assert_eq!(text_until_nul(format_args!("")).as_bytes(), b"");
        assert_eq!(text_until_nul(format_args!("\0")).as_bytes(), b"");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tokens_are_fused_and_debuggable() {
        let mut buf = *b" x \0";
        let mut it = tokens(&mut buf, c" ").unwrap();
        assert!(format!("{it:?}").starts_with("Tokens"));
        assert_eq!(it.next(), Some(c"x"));
        for _ in 0..3 {
            assert_eq!(it.next(), None);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utf8_error_reports_code_and_code_point() {
        let (r, n) = utf8_decode(b"\xed\xa0\x80", 0).unwrap();
        let e = r.unwrap_err();
        assert_eq!(n, 3);
        assert!(e.code() < 0);
        assert_eq!(e.code_point(), Some(0xD800));
        assert_eq!(
            e.to_string(),
            "UTF-8 code point 0xd800 rejected by the flags"
        );

        let (r, n) = utf8_decode(b"\x80", 0).unwrap();
        let e = r.unwrap_err();
        assert_eq!((n, e.code_point()), (1, None));
        assert_eq!(e.to_string(), "invalid UTF-8 sequence");
        assert!(format!("{e:?}").starts_with("Utf8Error"));
        let _: &dyn std::error::Error = &e;
    }

    #[test]
    fn dirname_rejects_a_path_without_nul_before_c() {
        let mut no_nul = *b"dir/file";
        assert_eq!(dirname(&mut no_nul), None);
        assert_eq!(&no_nul, b"dir/file");
        assert_eq!(dirname(&mut []), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dirname_reads_only_up_to_the_first_nul() {
        let mut path = *b"a\0b/c\0";
        assert_eq!(dirname(&mut path), Some(c"."));
        assert_eq!(&path, b"a\0b/c\0");
        let mut path = *b"x/y\0/z/w";
        assert_eq!(dirname(&mut path), Some(c"x"));
        assert_eq!(&path, b"x\0y\0/z/w");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn get_token_cursor_stays_on_the_input() {
        let input = c"a , b";
        let mut cursor = input;
        let t = get_token(&mut cursor, c",").unwrap();
        assert_eq!((t.to_str(), cursor), (Ok("a"), c", b"));
        assert_eq!(cursor.as_ptr(), input.as_ptr().wrapping_add(2));
    }

    /// `bind_scan_args` on `format`, with its addresses dropped.
    fn bind(format: &str, args: &mut [ScanArg<'_>]) -> Result<usize, ScanError> {
        let ptrs = bind_scan_args(format.as_bytes(), args)?;
        Ok(ptrs.iter().take_while(|p| !p.is_null()).count())
    }

    #[test]
    fn scan_binds_each_destination_in_order() {
        let (mut a, mut b, mut c, mut d) = (0i32, 0u8, 0f64, [0u8; 4]);
        let mut args: [ScanArg<'_>; 4] = [
            (&mut a).into(),
            (&mut b).into(),
            (&mut c).into(),
            (&mut d).into(),
        ];
        let expected: Vec<*mut c_void> = args.iter_mut().map(ScanArg::as_mut_ptr).collect();
        let ptrs = bind_scan_args(b" %d%%x %hhx,%lf %3c%*s", &mut args).unwrap();
        assert_eq!(&ptrs[..4], &expected[..]);
        assert!(ptrs[4..].iter().all(|p| p.is_null()));
    }

    #[test]
    fn scan_bound_addresses_stay_writable_until_the_call() {
        // What C does with the addresses: write through each, after binding
        // returned and before the destinations are read again.
        let mut many = [0i32; SCAN_MAX_ARGS];
        let mut buf = [0u8; 4];
        let mut args: Vec<ScanArg<'_>> = many[1..].iter_mut().map(ScanArg::from).collect();
        args.push((&mut buf).into());
        let format = format!("{}%3s", "%d".repeat(SCAN_MAX_ARGS - 1));
        let ptrs = bind_scan_args(format.as_bytes(), &mut args).unwrap();
        for (i, p) in ptrs[..SCAN_MAX_ARGS - 1].iter().enumerate() {
            // SAFETY: each address is an exclusively borrowed `i32`
            // destination, which nothing else touches until `args` drops.
            unsafe { p.cast::<i32>().write(i as i32 + 1) };
        }
        // SAFETY: the last address is the 4-byte buffer.
        unsafe { ptrs[SCAN_MAX_ARGS - 1].cast::<[u8; 4]>().write(*b"abc\0") };
        drop(args);
        let expected: Vec<i32> = (0..SCAN_MAX_ARGS as i32).collect();
        assert_eq!(&many[..], &expected[..]);
        assert_eq!(&buf, b"abc\0");
    }

    #[test]
    fn scan_integer_sizes_follow_the_length_modifier() {
        let (mut i8_, mut i16_, mut i32_, mut i64_, mut us) = (0i8, 0i16, 0i32, 0i64, 0usize);
        assert_eq!(bind("%hhd", &mut [(&mut i8_).into()]), Ok(1));
        assert_eq!(bind("%hn", &mut [(&mut i16_).into()]), Ok(1));
        assert_eq!(bind("%o", &mut [(&mut i32_).into()]), Ok(1));
        assert_eq!(bind("%lld", &mut [(&mut i64_).into()]), Ok(1));
        assert_eq!(bind("%jx", &mut [(&mut i64_).into()]), Ok(1));
        assert_eq!(bind("%zu", &mut [(&mut us).into()]), Ok(1));
        assert_eq!(bind("%p", &mut [(&mut us).into()]), Ok(1));
        assert_eq!(
            bind("%ld", &mut [(&mut i64_).into()]),
            if size_of::<c_long>() == 8 {
                Ok(1)
            } else {
                Err(ScanError::Mismatch(0))
            }
        );
        // Signedness is free; size is not.
        let mut u = 0u32;
        assert_eq!(bind("%d", &mut [(&mut u).into()]), Ok(1));
        assert_eq!(
            bind("%hd", &mut [(&mut i32_).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%d", &mut [(&mut i64_).into()]),
            Err(ScanError::Mismatch(0))
        );
        let mut f = 0f32;
        assert_eq!(
            bind("%d", &mut [(&mut f).into()]),
            Err(ScanError::Mismatch(0))
        );
    }

    #[test]
    fn scan_floats_and_bytes_check_kind_and_room() {
        let (mut f, mut d, mut i) = (0f32, 0f64, 0i32);
        assert_eq!(bind("%G", &mut [(&mut f).into()]), Ok(1));
        assert_eq!(bind("%la", &mut [(&mut d).into()]), Ok(1));
        assert_eq!(
            bind("%f", &mut [(&mut d).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%le", &mut [(&mut f).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%e", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%Lf", &mut [(&mut d).into()]),
            Err(ScanError::Unsupported(0))
        );

        let mut buf = [0u8; 4];
        assert_eq!(bind("%c", &mut [(&mut buf[..1]).into()]), Ok(1));
        assert_eq!(bind("%0c", &mut [(&mut buf[..1]).into()]), Ok(1));
        assert_eq!(bind("%4c", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%5c", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(
            bind("%c", &mut [(&mut buf[..0]).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(bind("%3s", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%4s", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(bind("%3[a-z]", &mut [(&mut buf).into()]), Ok(1));
        assert_eq!(
            bind("%s", &mut [(&mut buf).into()]),
            Err(ScanError::Unbounded(0))
        );
        assert_eq!(
            bind("%0[a]", &mut [(&mut buf).into()]),
            Err(ScanError::Unbounded(0))
        );
        assert_eq!(
            bind("%3s", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        assert_eq!(
            bind("%c", &mut [(&mut i).into()]),
            Err(ScanError::Mismatch(0))
        );
        // Suppressed, nothing is written, so no width is needed.
        assert_eq!(bind("%*s%*[^x]", &mut []), Ok(0));
    }

    #[test]
    fn scan_sets_end_at_c_closing_bracket() {
        let mut buf = [0u8; 8];
        let mut i = 0i32;
        // A leading `]` or `-` (after an optional `^`) is a member.
        for f in ["%7[]a]%d", "%7[^]a]%d", "%7[-a]%d", "%7[^-]%d", "%7[a-]%d"] {
            assert_eq!(
                bind(f, &mut [(&mut buf).into(), (&mut i).into()]),
                Ok(2),
                "{f}"
            );
        }
        assert_eq!(
            bind("%7[abc", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(
            bind("%7[]", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(
            bind("%7[^", &mut [(&mut buf).into()]),
            Err(ScanError::UnterminatedSet(0))
        );
        assert_eq!(bind("x%*[", &mut []), Err(ScanError::UnterminatedSet(1)));
    }

    #[test]
    fn scan_refuses_what_c_mishandles() {
        let mut i = 0i32;
        let mut buf = [0u8; 8];
        for (f, at) in [
            ("%", 0),
            ("ab%", 2),
            ("%1$d", 0),
            ("%md", 0),
            ("%5ms", 0),
            ("%C", 0),
            ("%5S", 0),
            ("%q", 0),
            ("%hq", 0),
            ("% d", 0),
            ("%Ld", 0),
            ("%Ln", 0),
            ("%lc", 0),
            ("%l5s", 0),
            ("%hp", 0),
            ("%*$d", 0),
        ] {
            let r = bind(f, &mut [(&mut i).into()]);
            let r2 = bind(f, &mut [(&mut buf).into()]);
            assert_eq!(r, Err(ScanError::Unsupported(at)), "{f}");
            assert_eq!(r2, Err(ScanError::Unsupported(at)), "{f}");
        }
    }

    #[test]
    fn scan_width_overflow_is_refused() {
        let mut buf = [0u8; 8];
        assert_eq!(
            bind("%2147483647c", &mut [(&mut buf).into()]),
            Err(ScanError::BufferTooSmall(0))
        );
        assert_eq!(
            bind("%2147483648c", &mut [(&mut buf).into()]),
            Err(ScanError::WidthOverflow(0))
        );
        assert_eq!(
            bind(" %99999999999d", &mut []),
            Err(ScanError::WidthOverflow(1))
        );
    }

    #[test]
    fn scan_destination_count_must_match() {
        let (mut a, mut b) = (0i32, 0i32);
        assert_eq!(
            bind("%d %d", &mut [(&mut a).into()]),
            Err(ScanError::MissingArg(3))
        );
        assert_eq!(
            bind("%d", &mut [(&mut a).into(), (&mut b).into()]),
            Err(ScanError::ExtraArgs)
        );
        assert_eq!(
            bind("%*d %%", &mut [(&mut a).into()]),
            Err(ScanError::ExtraArgs)
        );
        assert_eq!(bind("", &mut []), Ok(0));

        let mut many = [0i32; SCAN_MAX_ARGS + 1];
        let mut args: Vec<ScanArg<'_>> = many.iter_mut().map(ScanArg::from).collect();
        let fmt = "%d".repeat(SCAN_MAX_ARGS + 1);
        assert_eq!(bind(&fmt, &mut args), Err(ScanError::TooManyArgs));
        assert_eq!(bind(&fmt[2..], &mut args[1..]), Ok(SCAN_MAX_ARGS));
    }

    #[test]
    fn scan_error_display_names_the_offset() {
        assert_eq!(
            ScanError::Unsupported(3).to_string(),
            "unsupported conversion at byte 3"
        );
        assert_eq!(
            ScanError::TooManyArgs.to_string(),
            "more than 16 destinations"
        );
        let all = [
            ScanError::Unsupported(1),
            ScanError::UnterminatedSet(1),
            ScanError::WidthOverflow(1),
            ScanError::Unbounded(1),
            ScanError::MissingArg(1),
            ScanError::Mismatch(1),
            ScanError::BufferTooSmall(1),
            ScanError::ExtraArgs,
            ScanError::TooManyArgs,
        ];
        let texts: std::collections::HashSet<String> = all.iter().map(|e| e.to_string()).collect();
        assert_eq!(texts.len(), all.len());
        let _: &dyn std::error::Error = &all[0];
    }

    #[test]
    fn scan_arg_from_picks_the_variant() {
        let mut a = 0i8;
        assert!(matches!(ScanArg::from(&mut a), ScanArg::I8(_)));
        let mut b = 0u16;
        assert!(matches!(ScanArg::from(&mut b), ScanArg::U16(_)));
        let mut c = 0isize;
        assert!(matches!(ScanArg::from(&mut c), ScanArg::Isize(_)));
        let mut d = [0u8; 3];
        assert!(matches!(ScanArg::from(&mut d), ScanArg::Bytes(b) if b.len() == 3));
        let e = &mut d[..2];
        assert!(matches!(ScanArg::from(e), ScanArg::Bytes(b) if b.len() == 2));
        assert!(format!("{:?}", ScanArg::from(&mut a)).starts_with("I8("));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn empty_replace_pattern_copies_the_input() {
        // C would loop forever; the wrapper returns the input unchanged.
        let r = replace_ignore_ascii_case(c"abc", c"", c"x").unwrap();
        assert_eq!(r.as_bytes(), b"abc");
        let r = replace_ignore_ascii_case(c"", c"", c"").unwrap();
        assert!(r.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn character_classes() {
        for c in -1..=256 {
            let ascii = u8::try_from(c).ok().filter(u8::is_ascii);
            assert_eq!(
                is_graph(c),
                ascii.is_some_and(|b| b.is_ascii_graphic()),
                "{c}"
            );
            assert_eq!(
                is_xdigit(c),
                ascii.is_some_and(|b| b.is_ascii_hexdigit()),
                "{c}"
            );
            let space = ascii.is_some_and(|b| b.is_ascii_whitespace() || b == 0x0B);
            assert_eq!(is_space(c), space, "{c}");
        }
        for c in [c_int::MIN, -129, 0x161, c_int::MAX] {
            assert!(!is_graph(c) && !is_space(c) && !is_xdigit(c));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn match_name_entries_negation_and_all() {
        assert!(match_name(c"h264", c"mpeg4,H264"));
        assert!(!match_name(c"h264", c"h26"));
        assert!(!match_name(c"h26", c"h264"));
        assert!(match_name(c"anything", c"ALL"));
        assert!(!match_name(c"anything", c"all"));
        assert!(!match_name(c"h264", c"-h264,ALL"));
        assert!(match_name(c"vp9", c"-h264,ALL"));
        assert!(!match_name(c"x", c""));
        assert!(!match_name(c"", c"a,"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn match_list_any_common_entry() {
        let comma = NonZeroU8::new(b',').unwrap();
        assert!(match_list(c"a,b", c"c,b", comma));
        assert!(!match_list(c"a,b", c"c,d", comma));
        assert!(!match_list(c"", c"", comma));
        assert!(!match_list(c"ab", c"a", comma));
        let high = NonZeroU8::new(0xFF).unwrap();
        assert!(match_list(c"x\xffy", c"y", high));
    }

    #[test]
    fn escape_flags_combine_and_print() {
        let both = EscapeFlags::WHITESPACE | EscapeFlags::STRICT;
        assert_eq!(both.bits(), 3);
        assert!(both.contains(EscapeFlags::STRICT));
        assert!(!both.contains(EscapeFlags::XML_SINGLE_QUOTES));
        assert_eq!(both & EscapeFlags::STRICT, EscapeFlags::STRICT);
        let mut f = EscapeFlags::empty();
        assert!(f.is_empty());
        f |= EscapeFlags::XML_DOUBLE_QUOTES;
        assert_eq!(f, EscapeFlags::from_bits_retain(8));
        assert_eq!(EscapeFlags::default(), EscapeFlags::empty());
        assert_eq!(format!("{:?}", EscapeFlags::empty()), "EscapeFlags(0x0)");
        assert_eq!(format!("{both:?}"), "EscapeFlags(WHITESPACE | STRICT)");
        assert_eq!(
            format!("{:?}", EscapeFlags::from_bits_retain(0x14)),
            "EscapeFlags(XML_SINGLE_QUOTES | 0x10)"
        );
        assert_eq!(
            format!("{:?}", EscapeFlags::from_bits_retain(0x20)),
            "EscapeFlags(0x20)"
        );
    }

    #[test]
    fn escape_flags_debug_forwards_writer_errors() {
        /// Accepts `budget` writes, then fails.
        struct Budget(usize);
        impl fmt::Write for Budget {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                self.0 = self.0.checked_sub(1).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let f = EscapeFlags::from_bits_retain(0x13);
        let full = format!("{f:?}");
        assert_eq!(full, "EscapeFlags(WHITESPACE | STRICT | 0x10)");
        let mut budget = 0;
        while fmt::write(&mut Budget(budget), format_args!("{f:?}")).is_err() {
            budget += 1;
        }
        assert!(budget > 4);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn escape_by_each_mode() {
        let none = EscapeFlags::empty();
        let esc = |src, special, mode, flags| {
            escape(src, special, mode, flags)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(esc(c" a'b ", None, EscapeMode::Auto, none), "\\ a\\'b\\ ");
        assert_eq!(
            esc(
                c"a:b",
                Some(c":"),
                EscapeMode::Backslash,
                EscapeFlags::STRICT
            ),
            "a\\:b"
        );
        assert_eq!(
            esc(c"a b", None, EscapeMode::Backslash, EscapeFlags::WHITESPACE),
            "a\\ b"
        );
        assert_eq!(esc(c"it's", None, EscapeMode::Quote, none), "'it'\\''s'");
        assert_eq!(
            esc(c"<&>\"", None, EscapeMode::Xml, none),
            "&lt;&amp;&gt;\""
        );
        assert_eq!(
            esc(c"\"", None, EscapeMode::Xml, EscapeFlags::XML_DOUBLE_QUOTES),
            "&quot;"
        );
        assert_eq!(esc(c"", None, EscapeMode::Quote, none), "''");
        assert_eq!(esc(c"", None, EscapeMode::Backslash, none), "");
    }
}
