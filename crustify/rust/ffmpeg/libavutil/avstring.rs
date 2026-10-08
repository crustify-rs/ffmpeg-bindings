//! Safe wrappers over `libavutil/avstring.h`.
//!
//! [`format`] and [`append_path_component`] return a [`HeapString`]: an
//! `av_malloc`-family string the caller owns, released by `av_free`.
//! [`basename`] borrows from its argument instead.

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_uint};
use core::fmt;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::ptr;
use std::borrow::Cow;
use std::ffi::CString;

use super::ffi;
use super::mem::HeapString;

/// Wraps: AVEscapeMode
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
/// use ffmpeg::libavutil::{avstring::EscapeMode, mem::alloc_array_zeroed};
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

/// Wraps: av_strstart
///
/// The rest of `s` after `prefix`, or `None` when `s` does not start with
/// `prefix`. An empty prefix matches every string.
pub fn strip_prefix<'a>(s: &'a CStr, prefix: &CStr) -> Option<&'a CStr> {
    let mut rest: *const c_char = ptr::null();
    // SAFETY: `s` and `prefix` are live NUL-terminated strings, which
    // `av_strstart` only reads, stopping at either NUL; `rest` is a writable
    // slot.
    let matched = unsafe { ffi::av_strstart(s.as_ptr(), prefix.as_ptr(), &mut rest) };
    if matched == 0 {
        return None;
    }
    // On a match C stores the address of the first byte after the prefix in
    // `s`: an offset no greater than its length.
    let offset = rest as usize - s.as_ptr() as usize;
    Some(&s[offset..])
}

/// Wraps: av_strnstr
///
/// The offset of the first occurrence of `needle` in `haystack`, or `None`
/// when it has none; an empty `needle` matches at 0. The whole slice is
/// searched, NUL bytes included: pass a sub-slice to search a prefix.
pub fn find(haystack: &[u8], needle: &CStr) -> Option<usize> {
    let base: *const c_char = haystack.as_ptr().cast();
    // SAFETY: `av_strnstr` reads `needle`, a live NUL-terminated string, and
    // compares it against windows of `haystack` that end within its
    // `haystack.len()` bytes (none for an empty slice); it writes nothing.
    let found = unsafe { ffi::av_strnstr(base, needle.as_ptr(), haystack.len()) };
    (!found.is_null()).then(|| found as usize - base as usize)
}

/// Wraps: av_strlcpy
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

/// Wraps: av_strlcat
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

/// Wraps: av_strlcatf
///
/// Appends the text `args` formats to the string `dst` holds before its
/// first NUL, as [`strlcat`] appends a string, and never writes outside
/// `dst`. Returns the length of the string untruncated appending would give.
/// The text ends at its first NUL, as C's `%s` reads it. `None` when `dst`
/// holds no NUL.
///
/// ```
/// use ffmpeg::libavutil::avstring::strlcatf;
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
fn text_until_nul(args: fmt::Arguments<'_>) -> CString {
    let mut bytes = std::fmt::format(args).into_bytes();
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes.truncate(len);
    bytes.push(0);
    // The vector now holds exactly one NUL, at its end.
    CString::from_vec_with_nul(bytes).unwrap_or_default()
}

/// Wraps: av_strnlen
///
/// The number of bytes of `s` before its first NUL, or `s.len()` when it has
/// none.
pub fn strnlen(s: &[u8]) -> usize {
    // SAFETY: `av_strnlen` reads at most `s.len()` bytes of `s`, stopping at
    // the first NUL, and writes nothing.
    unsafe { ffi::av_strnlen(s.as_ptr().cast(), s.len()) }
}

/// Wraps: av_strncasecmp
///
/// Compares at most the first `n` bytes of `a` and `b`, ASCII letters
/// regardless of case, locale-independently; bytes are ordered as unsigned
/// after lowercasing, and a string orders before its extensions.
pub fn strncasecmp(a: &CStr, b: &CStr, n: usize) -> Ordering {
    // SAFETY: `a` and `b` are live NUL-terminated strings; `av_strncasecmp`
    // reads them in step and stops at the first NUL of either, a difference,
    // or after `n` bytes, and writes nothing.
    let diff = unsafe { ffi::av_strncasecmp(a.as_ptr(), b.as_ptr(), n) };
    diff.cmp(&0)
}

/// Wraps: av_strtok
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
/// use ffmpeg::libavutil::avstring::tokens;
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
        let token = unsafe { ffi::av_strtok(s, self.delim.as_ptr(), &mut self.save) };
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

/// Wraps: av_toupper
///
/// `c` with an ASCII lowercase letter mapped to uppercase, locale-independently;
/// any other value, `EOF` and bytes above 127 included, is returned as is.
pub fn to_ascii_uppercase(c: c_int) -> c_int {
    // SAFETY: `av_toupper` takes and returns a value, touching no memory.
    unsafe { ffi::av_toupper(c) }
}

/// Wraps: av_tolower
///
/// `c` with an ASCII uppercase letter mapped to lowercase, locale-independently;
/// any other value, `EOF` and bytes above 127 included, is returned as is.
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
    pub fn code(self) -> c_int {
        self.averror
    }

    /// The value a well-formed sequence decoded to when the flags rejected
    /// it (above U+10FFFF, a surrogate, a non-character, or an XML-invalid
    /// control code), or `None` for a malformed sequence.
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

/// Wraps: av_utf8_decode
///
/// Decodes the UTF-8 sequence at the start of `buf` under `flags`, a
/// combination of the `ffi::AV_UTF8_FLAG_*` constants: the code point or why
/// it was rejected, and the number of bytes consumed, which is at least 1.
/// `None` when `buf` is empty.
///
/// The decoder accepts the original sequences of up to six bytes, so a code
/// point may exceed U+10FFFF (but not `i32::MAX`) when
/// `AV_UTF8_FLAG_ACCEPT_INVALID_BIG_CODES` allows it. On a malformed
/// sequence it consumes up to the offending byte; on a well-formed one that
/// `flags` rejects, the whole sequence.
pub fn utf8_decode(buf: &[u8], flags: c_uint) -> Option<(Result<u32, Utf8Error>, usize)> {
    let range = buf.as_ptr_range();
    let mut p = range.start;
    // C stores only code points below 2^31, so a negative value means unset.
    let mut code: i32 = -1;
    // SAFETY: `p` and `range.end` delimit `buf`; `av_utf8_decode` reads bytes
    // from `p` only while below `range.end`, advances `p` within that range,
    // and writes `code` and `p`, both writable slots.
    let ret = unsafe { ffi::av_utf8_decode(&mut code, &mut p, range.end, flags) };
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

/// Wraps: av_asprintf
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
/// use ffmpeg::libavutil::avstring::format;
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

/// Wraps: av_basename
///
/// The last component of `path`: the part after its last `'/'`, which is
/// empty when `path` ends in one, or `path` itself when it has none; `"."`
/// for an empty `path` (as C returns for NULL too).
///
/// ```
/// use ffmpeg::libavutil::avstring::basename;
/// assert_eq!(basename(c"/usr/lib/libavutil.so"), c"libavutil.so");
/// assert_eq!(basename(c""), c".");
/// ```
pub fn basename(path: &CStr) -> &CStr {
    // SAFETY: `path` is a live NUL-terminated string, which `av_basename`
    // only reads.
    let base = unsafe { ffi::av_basename(path.as_ptr()) };
    // SAFETY: `av_basename` never returns NULL: it returns a pointer to the
    // NUL-terminated tail of `path`, which the borrow keeps alive and
    // unchanged, or to the static literal `"."`, which outlives it.
    unsafe { CStr::from_ptr(base) }
}

/// Wraps: av_append_path_component
///
/// `path` and `component` joined by a `'/'`: one is added when neither side
/// has one at the join, and when both do, one of the two is dropped (so
/// `"a//"` and `"//b"` join as `"a///b"`). When either side is empty, the
/// result is a copy of the other. `None` when the string cannot be allocated,
/// which includes a result whose length plus its NUL exceeds the
/// `av_max_alloc` limit (`INT_MAX` by default).
///
/// (C also accepts NULL for either side, treated as an empty string except
/// that two NULLs yield NULL.)
///
/// ```
/// use ffmpeg::libavutil::avstring::append_path_component;
/// let p = append_path_component(c"/tmp/", c"/out.mkv").unwrap();
/// assert_eq!(p.to_str(), Ok("/tmp/out.mkv"));
/// ```
pub fn append_path_component(path: &CStr, component: &CStr) -> Option<HeapString> {
    // SAFETY: both are live NUL-terminated strings, which
    // `av_append_path_component` only reads.
    let ptr = unsafe { ffi::av_append_path_component(path.as_ptr(), component.as_ptr()) };
    // SAFETY: a non-NULL `ptr` is a fresh, uniquely-owned, NUL-terminated
    // `av_malloc` (or `av_strdup`) block, which `av_free` releases.
    unsafe { HeapString::from_raw(ptr) }
}

#[cfg(test)]
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
    fn format_copies_static_and_formatted_text() {
        let s = format(format_args!("static")).unwrap();
        assert_eq!(s.to_str(), Ok("static"));
        let s = format(format_args!("{}-{:03}", "n", 7)).unwrap();
        assert_eq!(s.to_str(), Ok("n-007"));
        let s = format(format_args!("")).unwrap();
        assert!(s.is_empty() && !s.as_ptr().is_null());
    }

    #[test]
    fn format_stops_at_the_first_nul() {
        let s = format(format_args!("ab{}cd", '\0')).unwrap();
        assert_eq!(s.as_bytes(), b"ab");
        let s = format(format_args!("\0tail")).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn format_passes_percent_signs_through() {
        let s = format(format_args!("100% %s %n %{}", 'd')).unwrap();
        assert_eq!(s.to_str(), Ok("100% %s %n %d"));
    }

    #[test]
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
    fn utf8_error_reports_code_and_code_point() {
        let (r, n) = utf8_decode(b"\xed\xa0\x80", 0).unwrap();
        let e = r.unwrap_err();
        assert_eq!(n, 3);
        assert!(e.code() < 0);
        assert_eq!(e.code_point(), Some(0xd800));
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
}
