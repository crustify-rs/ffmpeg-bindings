//! Equivalence of `libavutil/avstring.h`'s [`EscapeMode`] with the C enum, as
//! `av_escape` observes it, and of the character classes and name matching
//! with the C functions they wrap.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::num::NonZeroU8;

use ffmpeg_bindings::libavutil::libavutil::avstring::{
    EscapeMode, is_graph, is_space, is_xdigit, match_list, match_name,
};
use ffmpeg_bindings::libavutil::libavutil::ffi;

/// Every mode beside its C enumerator.
const MODES: [(EscapeMode, ffi::AVEscapeMode); 4] = [
    (EscapeMode::Auto, ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO),
    (
        EscapeMode::Backslash,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
    ),
    (EscapeMode::Quote, ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE),
    (EscapeMode::Xml, ffi::AVEscapeMode::AV_ESCAPE_MODE_XML),
];

/// `AV_ESCAPE_FLAG_WHITESPACE`, `_STRICT`, `_XML_SINGLE_QUOTES` and
/// `_XML_DOUBLE_QUOTES`, alone and combined.
const FLAGS: [c_int; 6] = [0, 1 << 0, 1 << 1, 1 << 2, 1 << 3, 0b1111];

const INPUTS: [&CStr; 7] = [
    c"",
    c"plain",
    c" leading and trailing ",
    c"it's a \\ back'slash",
    c"<tag attr=\"v\" alt='w'>&amp;</tag>",
    c"\t\n\r\x01\x7f",
    c"ünïcödé ' \" < >",
];

/// `av_escape(src, NULL, mode, flags)`: the escaped bytes, or the error code.
fn escape(
    src: &CStr,
    special: Option<&CStr>,
    mode: ffi::AVEscapeMode,
    flags: c_int,
) -> Result<Vec<u8>, c_int> {
    let mut dst: *mut c_char = core::ptr::null_mut();
    let special = special.map_or(core::ptr::null(), CStr::as_ptr);
    // SAFETY: `dst` is a writable out-slot; `src` and `special` are NULL or
    // live NUL-terminated strings, which `av_escape` only reads; it takes the
    // mode and flags by value and accepts any.
    let ret = unsafe { ffi::av_escape(&mut dst, src.as_ptr(), special, mode, flags) };
    if ret < 0 {
        assert!(dst.is_null());
        return Err(ret);
    }
    // SAFETY: on success `dst` is a fresh NUL-terminated `av_malloc`-family
    // string of `ret` bytes, which we own.
    let out = unsafe { CStr::from_ptr(dst) }.to_bytes().to_vec();
    // SAFETY: as above; freed once.
    unsafe { ffi::av_free(dst.cast::<c_void>()) };
    assert_eq!(out.len(), ret as usize);
    Ok(out)
}

#[test]
fn modes_encode_as_the_c_enumerators() {
    for (mode, c) in MODES {
        assert_eq!(mode.into_raw(), c);
        assert_eq!(ffi::AVEscapeMode::from(mode), c);
    }
    assert_eq!(
        EscapeMode::default().into_raw(),
        ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO
    );
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn av_escape_behaves_alike_for_each_encoding() {
    for (mode, c) in MODES {
        for flags in FLAGS {
            for special in [None, Some(c"lt"), Some(c" ")] {
                for src in INPUTS {
                    assert_eq!(
                        escape(src, special, mode.into(), flags),
                        escape(src, special, c, flags),
                        "{mode:?} flags {flags:#x} {special:?} {src:?}"
                    );
                }
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn chained_escapes_agree_after_every_step() {
    for (mode, c) in MODES {
        let mut rust = c"a 'b' <c> \\d".to_owned();
        let mut raw = rust.clone();
        for step in 0..4 {
            let r = escape(&rust, None, mode.into(), 0).unwrap();
            let k = escape(&raw, None, c, 0).unwrap();
            assert_eq!(r, k, "{mode:?} step {step}");
            rust = std::ffi::CString::new(r).unwrap();
            raw = std::ffi::CString::new(k).unwrap();
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn default_mode_escapes_as_backslash_today() {
    // `bprint.c` maps `AV_ESCAPE_MODE_AUTO` to backslash escaping, as
    // `EscapeMode::Auto` documents.
    for src in INPUTS {
        assert_eq!(
            escape(src, None, EscapeMode::default().into(), 0),
            escape(src, None, ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH, 0)
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn the_modes_are_observably_distinct_in_c() {
    let src = c"it's <x> \\ y";
    let outs: Vec<_> = [EscapeMode::Backslash, EscapeMode::Quote, EscapeMode::Xml]
        .map(|m| escape(src, None, m.into(), 0).unwrap())
        .into();
    assert_ne!(outs[0], outs[1]);
    assert_ne!(outs[0], outs[2]);
    assert_ne!(outs[1], outs[2]);
}

/// The string helpers of `libavutil/avstring.h`, each against its raw C
/// symbol on independently owned buffers.
mod helpers {
    use core::cmp::Ordering;
    use core::ffi::{CStr, c_char, c_int, c_uint};
    use core::ptr;

    use ffmpeg_bindings::libavutil::libavutil::avstring::{
        cmp_n_ignore_ascii_case, find, strip_prefix, strlcat, strlcatf, strlcpy, strnlen,
        to_ascii_lowercase, to_ascii_uppercase, tokens, utf8_decode,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    /// Filler for bytes neither side should write.
    const FILL: u8 = 0xa5;

    const STRINGS: [&CStr; 9] = [
        c"",
        c"a",
        c"abc",
        c"ABC",
        c"abcdef",
        c"aBcDeF",
        c"ab\xc3\xa9",
        c"AB\xc3\x89",
        c"\x7f\x80\xff",
    ];

    /// `av_strstart(s, prefix)`: the remainder's offset in `s` on a match.
    fn c_strstart(s: &CStr, prefix: &CStr) -> Option<usize> {
        let mut rest: *const c_char = ptr::null();
        // SAFETY: live strings and a writable slot.
        let ret = unsafe { ffi::av_strstart(s.as_ptr(), prefix.as_ptr(), &mut rest) };
        (ret != 0).then(|| rest as usize - s.as_ptr() as usize)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strip_prefix_matches_av_strstart() {
        for s in STRINGS {
            for prefix in STRINGS {
                let rust = strip_prefix(s, prefix);
                let c = c_strstart(s, prefix);
                assert_eq!(rust.map(|r| r.as_ptr() as usize - s.as_ptr() as usize), c);
                assert_eq!(rust.map(CStr::to_bytes), c.map(|o| &s.to_bytes()[o..]));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chained_strip_prefix_agrees_after_every_step() {
        let s = c"key=value;rest";
        let (mut rust, mut c) = (s, 0);
        for prefix in [c"key", c"", c"=", c"val", c"ue;", c"rest", c"x"] {
            let r = strip_prefix(rust, prefix);
            let rest = &s[c..];
            let co = c_strstart(rest, prefix).map(|o| o + c);
            assert_eq!(r.map(|r| r.as_ptr() as usize - s.as_ptr() as usize), co);
            if let (Some(r), Some(o)) = (r, co) {
                (rust, c) = (r, o);
            }
        }
        assert!(rust.is_empty() && c == s.count_bytes());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn find_matches_av_strnstr_at_every_length() {
        let hay: &[u8] = b"abcab\0cabcd\0";
        let needles = [
            c"", c"a", c"ab", c"abc", c"cab", c"d", c"bcd", c"abcab", c"zz",
        ];
        for len in 0..=hay.len() {
            for needle in needles {
                let mine = hay.to_vec();
                let theirs = hay.to_vec();
                // SAFETY: `av_strnstr` reads at most `len <= theirs.len()`
                // bytes of `theirs`, and the live string `needle`.
                let found =
                    unsafe { ffi::av_strnstr(theirs.as_ptr().cast(), needle.as_ptr(), len) };
                let c = (!found.is_null()).then(|| found as usize - theirs.as_ptr() as usize);
                assert_eq!(find(&mine[..len], needle), c, "{len} {needle:?}");
            }
        }
    }

    /// `av_strlcpy` into `size` bytes of a filled buffer: its return and the
    /// whole buffer.
    fn c_strlcpy(buf: &mut [u8], size: usize, src: &CStr) -> usize {
        assert!(size <= buf.len());
        // SAFETY: `av_strlcpy` writes at most `size` bytes of `buf`.
        unsafe { ffi::av_strlcpy(buf.as_mut_ptr().cast(), src.as_ptr(), size) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strlcpy_matches_av_strlcpy_at_every_size() {
        for src in STRINGS {
            for size in 0..=8 {
                let mut mine = [FILL; 10];
                let mut theirs = [FILL; 10];
                let r = strlcpy(&mut mine[..size], src);
                let c = c_strlcpy(&mut theirs, size, src);
                assert_eq!((r, mine), (c, theirs), "{src:?} {size}");
            }
        }
    }

    /// `av_strlcat` into `size` bytes of `buf`.
    fn c_strlcat(buf: &mut [u8], size: usize, src: &CStr) -> usize {
        assert!(buf[..size].contains(&0));
        // SAFETY: `buf` holds a NUL within `size` bytes, so `av_strlcat`
        // reads and writes within them.
        unsafe { ffi::av_strlcat(buf.as_mut_ptr().cast(), src.as_ptr(), size) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strlcat_matches_av_strlcat_at_every_size() {
        for start in [c"", c"x", c"xyz"] {
            for src in STRINGS {
                for size in start.count_bytes() + 1..=10 {
                    let mut mine = [FILL; 10];
                    mine[..=start.count_bytes()].copy_from_slice(start.to_bytes_with_nul());
                    let mut theirs = mine;
                    let r = strlcat(&mut mine[..size], src);
                    let c = c_strlcat(&mut theirs, size, src);
                    assert_eq!((r, mine), (Some(c), theirs), "{start:?} {src:?} {size}");
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strlcatf_matches_av_strlcatf_formats() {
        for start in [c"", c"x", c"xyz"] {
            for size in start.count_bytes() + 1..=12 {
                let mut mine = [FILL; 12];
                mine[..=start.count_bytes()].copy_from_slice(start.to_bytes_with_nul());
                let mut theirs = mine;
                let r = strlcatf(
                    &mut mine[..size],
                    format_args!("{}-{:x}/{:05.1}", -12, 255, 6.25349),
                );
                // SAFETY: as for `c_strlcat`; the format's directives consume
                // an `int`, an `unsigned` and a `double`.
                let c = unsafe {
                    ffi::av_strlcatf(
                        theirs.as_mut_ptr().cast(),
                        size,
                        c"%d-%x/%05.1f".as_ptr(),
                        -12 as c_int,
                        255 as c_uint,
                        6.25349f64,
                    )
                };
                assert_eq!((r, mine), (Some(c), theirs), "{start:?} {size}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appends_agree_after_every_step() {
        let mut mine = [FILL; 16];
        let mut theirs = [FILL; 16];
        let size = 12;
        assert_eq!(
            strlcpy(&mut mine[..size], c"ab"),
            c_strlcpy(&mut theirs, size, c"ab")
        );
        assert_eq!(mine, theirs);
        for (i, piece) in [c"cd", c"", c"efgh", c"ijkl", c"m"].into_iter().enumerate() {
            assert_eq!(
                strlcat(&mut mine[..size], piece),
                Some(c_strlcat(&mut theirs, size, piece))
            );
            assert_eq!(mine, theirs, "strlcat {i}");
            let r = strlcatf(&mut mine[..size], format_args!("{i}"));
            // SAFETY: as for `c_strlcat`; `%d` consumes one `int`.
            let c = unsafe {
                ffi::av_strlcatf(theirs.as_mut_ptr().cast(), size, c"%d".as_ptr(), i as c_int)
            };
            assert_eq!((r, mine), (Some(c), theirs), "strlcatf {i}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strnlen_matches_av_strnlen_on_every_prefix() {
        let s: &[u8] = b"abc\0def\0";
        for len in 0..=s.len() {
            for start in 0..=len {
                // SAFETY: `av_strnlen` reads at most `len - start` bytes from
                // `start`.
                let c = unsafe { ffi::av_strnlen(s[start..].as_ptr().cast(), len - start) };
                assert_eq!(strnlen(&s[start..len]), c);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cmp_n_ignore_ascii_case_orders_as_av_strncasecmp() {
        for a in STRINGS {
            for b in STRINGS {
                for n in 0..=8 {
                    // SAFETY: live strings, read up to their NULs or `n`.
                    let c = unsafe { ffi::av_strncasecmp(a.as_ptr(), b.as_ptr(), n) };
                    assert_eq!(
                        cmp_n_ignore_ascii_case(a, b, n),
                        c.cmp(&0),
                        "{a:?} {b:?} {n}"
                    );
                }
            }
        }
        assert_eq!(
            cmp_n_ignore_ascii_case(c"ABCx", c"abcy", 3),
            Ordering::Equal
        );
    }

    /// `av_strtok` over `buf` until it returns NULL: each token and where
    /// `saveptr` points after the call, as offsets into `buf`.
    fn c_strtok(buf: &mut [u8], delim: &CStr) -> Vec<(Vec<u8>, Option<usize>)> {
        assert!(buf.contains(&0));
        let base = buf.as_mut_ptr().cast::<c_char>();
        let (mut s, mut save) = (base, ptr::null_mut());
        let mut out = Vec::new();
        loop {
            // SAFETY: `s` is `buf`'s NUL-terminated string on the first call,
            // NULL afterwards with `save` from the previous one.
            let tok = unsafe { ffi::av_strtok(s, delim.as_ptr(), &mut save) };
            s = ptr::null_mut();
            if tok.is_null() {
                assert!(save.is_null());
                return out;
            }
            // SAFETY: a token is a NUL-terminated run of `buf`.
            let bytes = unsafe { CStr::from_ptr(tok) }.to_bytes().to_vec();
            out.push((
                bytes,
                (!save.is_null()).then(|| save as usize - base as usize),
            ));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tokens_split_as_av_strtok() {
        let inputs: [&[u8]; 8] = [
            b"\0",
            b",,,\0",
            b"a\0",
            b"a,b\0",
            b",a,,b,\0",
            b"  key = value ; x\0trailing",
            b"a b\tc\0d e\0",
            b"\xff,\x80\0",
        ];
        for input in inputs {
            for delim in [c",", c" ", c", ;=\t", c""] {
                let mut mine = input.to_vec();
                let mut theirs = input.to_vec();
                let c = c_strtok(&mut theirs, delim);
                let rust: Vec<Vec<u8>> = tokens(&mut mine, delim)
                    .unwrap()
                    .map(|t| t.to_bytes().to_vec())
                    .collect();
                let c_tokens: Vec<Vec<u8>> = c.into_iter().map(|(t, _)| t).collect();
                assert_eq!(rust, c_tokens, "{input:?} {delim:?}");
                assert_eq!(mine, theirs, "{input:?} {delim:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn interleaved_tokens_agree_after_every_step() {
        let input = b"one two  three four\0";
        let mut mine = input.to_vec();
        let mut theirs = input.to_vec();
        let base = theirs.as_mut_ptr().cast::<c_char>();
        let (mut s, mut save) = (base, ptr::null_mut());
        let mut it = tokens(&mut mine, c" ").unwrap();
        let mut steps = 0;
        loop {
            // SAFETY: `s` is `theirs`' NUL-terminated string on the first
            // call, NULL afterwards with `save` from the previous one.
            let tok = unsafe { ffi::av_strtok(s, c" ".as_ptr(), &mut save) };
            s = ptr::null_mut();
            // SAFETY: a non-NULL token is a NUL-terminated run of `theirs`.
            let c = (!tok.is_null()).then(|| unsafe { CStr::from_ptr(tok) }.to_bytes());
            assert_eq!(it.next().map(CStr::to_bytes), c, "step {steps}");
            if c.is_none() {
                break;
            }
            steps += 1;
        }
        assert_eq!((steps, it.next()), (4, None));
        assert_eq!(mine, theirs);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn case_mapping_matches_av_tolower_and_av_toupper() {
        let extremes = [c_int::MIN, c_int::MIN + 1, -1, c_int::MAX];
        for c in (-300..=300).chain(extremes) {
            // SAFETY: value-only functions.
            let (lo, up) = unsafe { (ffi::av_tolower(c), ffi::av_toupper(c)) };
            assert_eq!(to_ascii_lowercase(c), lo, "{c}");
            assert_eq!(to_ascii_uppercase(c), up, "{c}");
        }
    }

    /// Every step of decoding `buf` with `av_utf8_decode` under `flags`: its
    /// return, `*codep` (`-1` when unset) and the bytes consumed.
    fn c_utf8_steps(buf: &[u8], flags: c_uint) -> Vec<(c_int, i32, usize)> {
        let range = buf.as_ptr_range();
        let mut p = range.start;
        let mut out = Vec::new();
        while p < range.end {
            let before = p;
            let mut code = -1;
            // SAFETY: `p` and `range.end` delimit the unread part of `buf`.
            let ret = unsafe { ffi::av_utf8_decode(&mut code, &mut p, range.end, flags) };
            out.push((ret, code, p as usize - before as usize));
        }
        out
    }

    /// The same steps through [`utf8_decode`].
    fn rust_utf8_steps(mut buf: &[u8], flags: c_uint) -> Vec<(c_int, i32, usize)> {
        let mut out = Vec::new();
        while let Some((r, n)) = utf8_decode(buf, flags) {
            let step = match r {
                Ok(code) => (0, code as i32, n),
                Err(e) => (e.code(), e.code_point().map_or(-1, |c| c as i32), n),
            };
            out.push(step);
            buf = &buf[n..];
        }
        out
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utf8_decode_steps_as_av_utf8_decode() {
        let inputs: [&[u8]; 14] = [
            b"plain ascii\t\n\r\x01",
            "h\u{e9}llo \u{20ac} \u{1f600}".as_bytes(),
            b"\x80\xbf\xc0\x80\xc1\xbf",
            b"\xe0\x80\x80\xf0\x80\x80\x80",
            b"\xed\xa0\x80\xed\xbf\xbf",
            b"\xef\xbf\xbe\xef\xbf\xbf",
            b"\xf4\x90\x80\x80\xf7\xbf\xbf\xbf",
            b"\xf8\x88\x80\x80\x80\xfc\x84\x80\x80\x80\x80",
            b"\xfd\xbf\xbf\xbf\xbf\xbf\xfe\xff",
            b"\xc3",
            b"\xe2\x82",
            b"\xe2\x28\xa1",
            b"\xf0\x9f\x98",
            b"",
        ];
        for input in inputs {
            for flags in 0..16 {
                assert_eq!(
                    rust_utf8_steps(input, flags),
                    c_utf8_steps(input, flags),
                    "{input:x?} {flags}"
                );
            }
        }
    }

    /// The consumption rule the wrapper documents, checked against C on
    /// each malformed shape: one byte for a bad lead, a bad tail byte after
    /// good ones, or a sequence the buffer cuts short; the whole sequence
    /// for an overlong encoding, which yields no code point, and for the
    /// six-byte maximum, which decodes to `i32::MAX`.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utf8_decode_consumes_as_c_on_each_malformed_shape() {
        let big = ffi::AV_UTF8_FLAG_ACCEPT_INVALID_BIG_CODES;
        // `AVERROR(EILSEQ)`, as C reports it for a lone continuation byte.
        let eilseq = c_utf8_steps(b"\x80", 0)[0].0;
        assert!(eilseq < 0);
        let inputs: [&[u8]; 7] = [
            b"\x80",
            b"\xe2\x82\x41",
            b"\xe2\x82",
            b"\xe0\x80\x80",
            b"\xf4\x90\x80\x80",
            b"\xfd\xbf\xbf\xbf\xbf\xbf",
            b"\xfd\xbf\xbf\xbf\xbf\xbf",
        ];
        let flags = [0, 0, 0, 0, 0, 0, big];
        let first_steps = [
            (eilseq, -1, 1),
            (eilseq, -1, 1),
            (eilseq, -1, 1),
            (eilseq, -1, 3),
            (eilseq, 0x11_0000, 4),
            (eilseq, i32::MAX, 6),
            (0, i32::MAX, 6),
        ];
        for ((input, flags), first) in inputs.into_iter().zip(flags).zip(first_steps) {
            let c = c_utf8_steps(input, flags);
            assert_eq!(c[0], first, "{input:x?} {flags}");
            assert_eq!(rust_utf8_steps(input, flags), c, "{input:x?} {flags}");
        }
        // After a bad tail byte the next call restarts at it.
        assert_eq!(
            rust_utf8_steps(b"\xe2\x82\x41", 0),
            [(eilseq, -1, 1), (eilseq, -1, 1), (0, 0x41, 1)]
        );
        assert_eq!(
            utf8_decode(b"\xfd\xbf\xbf\xbf\xbf\xbf", big),
            Some((Ok(i32::MAX as u32), 6))
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utf8_decode_agrees_on_pseudo_random_bytes() {
        // A fixed linear congruential generator: reproducible bytes biased
        // towards lead and continuation bytes.
        let mut state = 0x2545_f491_u32;
        let mut bytes = Vec::with_capacity(1024);
        for _ in 0..1024 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let b = (state >> 24) as u8;
            bytes.push(if state & 0x100 != 0 { b | 0x80 } else { b });
        }
        for flags in [0, ffi::AV_UTF8_FLAG_ACCEPT_ALL, 8, 15] {
            assert_eq!(rust_utf8_steps(&bytes, flags), c_utf8_steps(&bytes, flags));
        }
    }
}

/// `basename`, `append_path_component` and `format` against `av_basename`,
/// `av_append_path_component` and `av_asprintf`, on independently owned
/// strings.
mod paths_and_format {
    use core::ffi::{CStr, c_char, c_double, c_int};
    use core::ptr::null;
    use std::process::Command;

    use ffmpeg_bindings::libavutil::libavutil::avstring::{append_path_component, basename, format};
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::HeapString;

    /// A string C returned, freed with `av_free` on drop.
    struct CString(*mut c_char);

    impl CString {
        fn bytes(&self) -> Option<&[u8]> {
            // SAFETY: `self.0` is NULL or a live NUL-terminated string we own.
            (!self.0.is_null()).then(|| unsafe { CStr::from_ptr(self.0) }.to_bytes())
        }
    }

    impl Drop for CString {
        fn drop(&mut self) {
            // SAFETY: `self.0` is NULL or a live `av_malloc`-family string we
            // own.
            unsafe { ffi::av_free(self.0.cast()) }
        }
    }

    /// The bytes of a Rust-owned result, as `CString::bytes`.
    fn bytes(s: &Option<HeapString>) -> Option<&[u8]> {
        s.as_ref().map(HeapString::as_bytes)
    }

    const PATHS: [&CStr; 14] = [
        c"",
        c"/",
        c"//",
        c"a",
        c"a/",
        c"/a",
        c"a/b",
        c"a//b",
        c"/a/b/",
        c"./file.mkv",
        c"C:\\dir\\file",
        c"dir/C:file",
        c"\xc3\xbc/\xc3\xb1",
        c"very/long/path/with/many/components/at/the/end.txt",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn basename_points_where_c_points() {
        for path in PATHS {
            // SAFETY: `path` is a live NUL-terminated string, only read.
            let c = unsafe { ffi::av_basename(path.as_ptr()) };
            let r = basename(path);
            if path.is_empty() {
                // Both return the library's static ".".
                assert_eq!(r, c".");
            }
            assert_eq!(r.as_ptr(), c, "{path:?}");
        }
        // C's NULL path reads as Rust's empty one.
        // SAFETY: `av_basename` accepts NULL.
        let c = unsafe { ffi::av_basename(null()) };
        assert_eq!(basename(c"").as_ptr(), c);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appended_paths_match_c_for_every_pair() {
        for path in PATHS {
            for component in PATHS {
                // SAFETY: both are live NUL-terminated strings, only read.
                let c = CString(unsafe {
                    ffi::av_append_path_component(path.as_ptr(), component.as_ptr())
                });
                let r = append_path_component(path, component);
                assert_eq!(bytes(&r), c.bytes(), "{path:?} + {component:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn c_null_sides_read_as_rust_empty_ones() {
        for s in PATHS {
            // SAFETY: `s` is a live NUL-terminated string; NULL is accepted
            // on either side.
            let (left, right) = unsafe {
                (
                    CString(ffi::av_append_path_component(null(), s.as_ptr())),
                    CString(ffi::av_append_path_component(s.as_ptr(), null())),
                )
            };
            assert_eq!(bytes(&append_path_component(c"", s)), left.bytes());
            assert_eq!(bytes(&append_path_component(s, c"")), right.bytes());
        }
        // Two NULLs are C's only NULL result outside allocation failure.
        // SAFETY: as above.
        let both = CString(unsafe { ffi::av_append_path_component(null(), null()) });
        assert_eq!(both.bytes(), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn paths_built_step_by_step_stay_equal() {
        // Each side extends its own previous result, compared after each step.
        let steps = [c"/", c"usr/", c"/lib", c"", c"x86_64", c"//libavutil.so"];
        let mut c = CString(core::ptr::null_mut());
        let mut r: Option<HeapString> = None;
        for (i, step) in steps.into_iter().enumerate() {
            // SAFETY: `c.0` is NULL or the live string of the previous step,
            // and `step` a live string; both only read.
            let next = CString(unsafe { ffi::av_append_path_component(c.0, step.as_ptr()) });
            c = next;
            r = append_path_component(r.as_ref().map_or(c"", HeapString::as_c_str), step);
            assert_eq!(bytes(&r), c.bytes(), "step {i}");
            let base = basename(r.as_ref().unwrap().as_c_str());
            // SAFETY: `c.0` is the live string of this step.
            let c_base = unsafe { CStr::from_ptr(ffi::av_basename(c.0)) };
            assert_eq!(base, c_base, "step {i}");
        }
        // Only the single slashes at a join are merged; a doubled one stays.
        assert_eq!(bytes(&r), Some(&b"/usr/lib/x86_64//libavutil.so"[..]));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_matches_av_asprintf_with_the_same_conversions() {
        // SAFETY: each format's conversions match the arguments that follow.
        let cases = unsafe {
            [
                (
                    CString(ffi::av_asprintf(c"plain".as_ptr())),
                    format(format_args!("plain")),
                ),
                (
                    CString(ffi::av_asprintf(c"".as_ptr())),
                    format(format_args!("")),
                ),
                (
                    CString(ffi::av_asprintf(
                        c"%d-%s-%05.2f".as_ptr(),
                        -7 as c_int,
                        c"x".as_ptr(),
                        1.23456 as c_double,
                    )),
                    format(format_args!("{}-{}-{:05.2}", -7, "x", 1.23456)),
                ),
                (
                    CString(ffi::av_asprintf(
                        c"%x/%X %%".as_ptr(),
                        255 as c_int,
                        4096 as c_int,
                    )),
                    format(format_args!("{:x}/{:X} %", 255, 4096)),
                ),
                (
                    CString(ffi::av_asprintf(
                        c"%s".as_ptr(),
                        c"\xc3\xbcnicode %s %n".as_ptr(),
                    )),
                    format(format_args!("ünicode %s %n")),
                ),
                (
                    CString(ffi::av_asprintf(
                        c"%.3s|%-4s|".as_ptr(),
                        c"abcdef".as_ptr(),
                        c"z".as_ptr(),
                    )),
                    format(format_args!("{:.3}|{:<4}|", "abcdef", "z")),
                ),
            ]
        };
        for (i, (c, r)) in cases.iter().enumerate() {
            assert_eq!(bytes(r), c.bytes(), "case {i}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_of_a_nul_matches_c_reading_the_same_text() {
        // C's `%s` stops at the NUL; so does the Rust result.
        let text = c"before";
        // SAFETY: `%s` takes the live string `text`.
        let c = CString(unsafe { ffi::av_asprintf(c"%s".as_ptr(), text.as_ptr()) });
        let r = format(format_args!("before\0after"));
        assert_eq!(bytes(&r), c.bytes());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn chained_formats_stay_equal() {
        // Each side formats its own previous result into the next.
        // SAFETY: a format without conversions takes no arguments.
        let mut c = CString(unsafe { ffi::av_asprintf(c"0".as_ptr()) });
        let mut r = format(format_args!("0"));
        for i in 1..=24_i32 {
            // SAFETY: `%s` takes the live previous string, `%d` the int.
            let next = CString(unsafe { ffi::av_asprintf(c"%s,%d".as_ptr(), c.0, i) });
            c = next;
            r = format(format_args!("{},{}", r.unwrap().to_str().unwrap(), i));
            assert_eq!(bytes(&r), c.bytes(), "step {i}");
        }
    }

    /// Set in the child process an isolated test re-runs itself in.
    const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

    /// The `av_max_alloc` limit is process-global, so this test lowers it only
    /// in a child process running nothing else, and passes when the child
    /// does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failures_at_the_av_max_alloc_limit_match_c() {
        const NAME: &str = "avstring::paths_and_format::failures_at_the_av_max_alloc_limit_match_c";
        if std::env::var_os(ISOLATED).is_none() {
            let status = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }

        // A string of `n` bytes takes `n + 1`, so 7 bytes fit an 8-byte limit
        // and 8 do not.
        const LIMIT: usize = 8;
        // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in
        // this process.
        unsafe { ffi::av_max_alloc(LIMIT) };

        for text in [c"", c"0123456", c"01234567", c"012345678"] {
            // SAFETY: `%s` takes the live string `text`.
            let c = CString(unsafe { ffi::av_asprintf(c"%s".as_ptr(), text.as_ptr()) });
            let r = format(format_args!("{}", text.to_str().unwrap()));
            assert_eq!(bytes(&r), c.bytes(), "{text:?}");
            assert_eq!(r.is_some(), text.count_bytes() < LIMIT, "{text:?}");
        }
        // Joins around the limit. C allocates `path + component + 2` bytes
        // whatever the join, so a join that merges or omits the slash fails
        // one byte before its text and NUL would have exceeded the limit.
        let joins: [(&CStr, &CStr); 9] = [
            (c"abc", c"de"),
            (c"abc", c"def"),
            (c"abcd", c"efg"),
            (c"", c"abcdef"),
            (c"", c"abcdefg"),
            (c"abcdefgh", c""),
            (c"ab/", c"cd"),
            (c"abc/", c"def"),
            (c"abc/", c"/defg"),
        ];
        for (path, component) in joins {
            // SAFETY: both are live NUL-terminated strings, only read.
            let c = CString(unsafe {
                ffi::av_append_path_component(path.as_ptr(), component.as_ptr())
            });
            let r = append_path_component(path, component);
            assert_eq!(bytes(&r), c.bytes(), "{path:?} + {component:?}");
            let asked = path.count_bytes() + component.count_bytes() + 2;
            assert_eq!(r.is_some(), asked <= LIMIT, "{path:?} + {component:?}");
        }

        // SAFETY: as above; the default limit again.
        unsafe { ffi::av_max_alloc(i32::MAX as usize) };
        assert!(format(format_args!("01234567")).is_some());
    }
}

/// `av_dirname`, `av_get_token` and `av_isdigit`, each against its raw C
/// symbol on independently owned buffers.
mod paths_and_tokens {
    use core::ffi::{CStr, c_char, c_int};

    use ffmpeg_bindings::libavutil::libavutil::avstring::{dirname, get_token, is_ascii_digit};
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    /// Paths around every cut `av_dirname` makes: none, a leading, inner,
    /// doubled or trailing `/`, DOS separators (plain bytes on this platform),
    /// and bytes after an inner NUL, which neither side may read.
    const PATHS: [&[u8]; 18] = [
        b"",
        b"/",
        b"//",
        b"a",
        b"a/",
        b"/a",
        b"a/b",
        b"a/b/",
        b"/a/b/c",
        b"a//b",
        b"./a",
        b"../",
        b"..",
        b"C:\\x\\y",
        b"c:file",
        b"a b/c d",
        b"x/y\0/z",
        b"\xff/\xfe",
    ];

    /// `av_dirname` on a NUL-terminated copy of `path`: the result, whether it
    /// is the buffer itself, and the buffer afterwards.
    fn c_dirname(path: &[u8]) -> (Vec<u8>, bool, Vec<u8>) {
        let mut buf = path.to_vec();
        buf.push(0);
        // SAFETY: `buf` is a writable NUL-terminated string, which `av_dirname`
        // reads up to its NUL and writes at most one NUL into.
        let ret = unsafe { ffi::av_dirname(buf.as_mut_ptr().cast()) };
        let same = ret.cast::<u8>() == buf.as_ptr();
        // SAFETY: `ret` is `buf` (alive) or C's static `"."`, NUL-terminated.
        let out = unsafe { CStr::from_ptr(ret) }.to_bytes().to_vec();
        (out, same, buf)
    }

    /// `dirname` on a NUL-terminated copy of `path`, as [`c_dirname`] reports it.
    fn rust_dirname(path: &[u8]) -> (Vec<u8>, bool, Vec<u8>) {
        let mut buf = path.to_vec();
        buf.push(0);
        let base = buf.as_ptr();
        let ret = dirname(&mut buf).unwrap();
        let (out, same) = (ret.to_bytes().to_vec(), ret.as_ptr().cast::<u8>() == base);
        (out, same, buf)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dirname_cuts_and_returns_as_av_dirname() {
        for path in PATHS {
            assert_eq!(
                rust_dirname(path),
                c_dirname(path),
                "{:?}",
                path.escape_ascii().to_string()
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_dirnames_agree_after_every_step() {
        for path in [&b"/usr/local/lib/x.so"[..], b"a/b/c/", b"rel/path"] {
            let (mut rust, mut c) = (path.to_vec(), path.to_vec());
            for step in 0..6 {
                let (r, k) = (rust_dirname(&rust), c_dirname(&c));
                assert_eq!(r, k, "{path:?} step {step}");
                rust = r.0;
                c = k.0;
            }
            assert_eq!(rust, b".");
        }
    }

    /// `av_get_token` from `*cursor`: the token, and `*cursor` advanced.
    fn c_get_token(cursor: &mut *const c_char, term: &CStr) -> Option<Vec<u8>> {
        // SAFETY: `*cursor` is a live NUL-terminated string and `term` one too;
        // `av_get_token` only reads both, and moves `*cursor` within the first.
        let tok = unsafe { ffi::av_get_token(cursor, term.as_ptr()) };
        if tok.is_null() {
            return None;
        }
        // SAFETY: `tok` is a fresh NUL-terminated `av_realloc` string we own.
        let out = unsafe { CStr::from_ptr(tok) }.to_bytes().to_vec();
        // SAFETY: as above; freed once.
        unsafe { ffi::av_free(tok.cast()) };
        Some(out)
    }

    /// Inputs exercising each `av_get_token` branch, with their terminators.
    const TOKEN_INPUTS: [(&CStr, &CStr); 12] = [
        (c"", c","),
        (c"   ", c","),
        (c"a,b,c", c","),
        (c"  lead , trail  ,", c","),
        (c"k1=v1:k2=v2", c"=:"),
        (c"esc\\,aped,next", c","),
        (c"'quoted, kept'  ,x", c","),
        (c"half'quote", c","),
        (c"trailing\\", c","),
        (c" \\  ' ' \\ ,", c","),
        (c"no terminator here", c""),
        (c"a;b|c;;d", c";|"),
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn token_sequences_agree_after_every_step() {
        for (input, term) in TOKEN_INPUTS {
            let mut rust = input;
            let mut c = input.as_ptr();
            for step in 0..8 {
                let r = get_token(&mut rust, term).map(|t| t.as_bytes().to_vec());
                let k = c_get_token(&mut c, term);
                assert_eq!(r, k, "{input:?} step {step}");
                assert_eq!(rust.as_ptr(), c, "{input:?} step {step}: cursor");
                if rust.is_empty() {
                    break;
                }
                // Both stop on a terminator; step over it on each side.
                rust = CStr::from_bytes_with_nul(&rust.to_bytes_with_nul()[1..]).unwrap();
                c = c.wrapping_add(1);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn is_digit_classifies_every_int_as_av_isdigit() {
        let extremes = [c_int::MIN, c_int::MIN + 1, -1, c_int::MAX - 1, c_int::MAX];
        for c in (-300..=300).chain(extremes) {
            // SAFETY: `av_isdigit` takes its argument by value.
            let k = unsafe { ffi::av_isdigit(c) } != 0;
            assert_eq!(is_ascii_digit(c), k, "{c}");
            assert_eq!(k, u8::try_from(c).is_ok_and(|b| b.is_ascii_digit()), "{c}");
        }
    }

    /// Set in the child process an isolated test re-runs itself in.
    const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

    /// The `av_max_alloc` limit is process-global, so this test lowers it only in
    /// a child process running nothing else, and passes when the child does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn get_token_failure_at_the_av_max_alloc_limit_matches_c() {
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "avstring::paths_and_tokens::get_token_failure_at_the_av_max_alloc_limit_matches_c",
                ])
                .args(["--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }

        // The token buffer is `strlen(*buf) + 1` bytes: 7 bytes of input fit an
        // 8-byte limit and 8 do not.
        // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in this
        // process.
        unsafe { ffi::av_max_alloc(8) };
        for input in [c"abc,def", c"abcd,efg", c"abcdefghijkl"] {
            let mut rust = input;
            let mut c = input.as_ptr();
            let r = get_token(&mut rust, c",").map(|t| t.as_bytes().to_vec());
            let k = c_get_token(&mut c, c",");
            assert_eq!(r, k, "{input:?}");
            assert_eq!(rust.as_ptr(), c, "{input:?}: cursor");
            assert_eq!(r.is_none(), input.count_bytes() >= 8, "{input:?}");
        }
        // SAFETY: as above; the default limit again.
        unsafe { ffi::av_max_alloc(i32::MAX as usize) };
    }
}

// ---- case-insensitive search and replace, and av_sscanf ----

mod search_and_scan {
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr::addr_of_mut;

    use ffmpeg_bindings::libavutil::libavutil::avstring::{
        ScanArg, cmp_ignore_ascii_case, find_ignore_ascii_case, replace_ignore_ascii_case, scan,
        strip_prefix_ignore_ascii_case,
    };
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    /// Strings that differ in case, length, and bytes outside ASCII.
    const WORDS: [&CStr; 14] = [
        c"",
        c"a",
        c"A",
        c"abc",
        c"ABC",
        c"abd",
        c"ab",
        c"aBcD",
        c"Z",
        c"[",
        c"`",
        c"\xc3\xa9t\xc3\xa9",
        c"\xc3\x89T\xc3\x89",
        c"\x7f\x80\xff",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn compare_matches_av_strcasecmp() {
        for a in WORDS {
            for b in WORDS {
                // SAFETY: both are live NUL-terminated strings, only read.
                let c = unsafe { ffi::av_strcasecmp(a.as_ptr(), b.as_ptr()) };
                assert_eq!(cmp_ignore_ascii_case(a, b), c.cmp(&0), "{a:?} vs {b:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strip_prefix_matches_av_stristart() {
        for s in WORDS {
            for p in WORDS {
                let mut rest: *const c_char = core::ptr::null();
                // SAFETY: both are live NUL-terminated strings, only read,
                // and `rest` a writable local.
                let c = unsafe { ffi::av_stristart(s.as_ptr(), p.as_ptr(), &mut rest) };
                let r = strip_prefix_ignore_ascii_case(s, p);
                assert_eq!(r.is_some(), c != 0, "{s:?} by {p:?}");
                // The suffix starts where C points, inside `s`.
                assert_eq!(
                    r.map_or(core::ptr::null(), CStr::as_ptr),
                    rest,
                    "{s:?} by {p:?}"
                );
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn find_matches_av_stristr() {
        let hay = [
            c"",
            c"Hello, World",
            c"aaab",
            c"xAbCabc",
            c"abcABC",
            c"\xc3\xa9t\xc3\xa9 E",
        ];
        for h in hay.into_iter().chain(WORDS) {
            for n in WORDS
                .into_iter()
                .chain([c"WORLD", c"o, w", c"aab", c"cab", c"t\xc3\xa9 e"])
            {
                // SAFETY: both are live NUL-terminated strings, only read.
                let c = unsafe { ffi::av_stristr(h.as_ptr(), n.as_ptr()) };
                let c = (!c.is_null()).then(|| c as usize - h.as_ptr() as usize);
                assert_eq!(find_ignore_ascii_case(h, n), c, "{n:?} in {h:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replace_matches_av_strireplace_step_by_step() {
        let steps: [(&CStr, &CStr); 6] = [
            (c"world", c"There"),
            (c"THERE", c"x"),
            (c"l", c"LL"),
            (c"ll", c""),
            (c"h", c"hh"),
            (c"\xc3\xa9", c"e"),
        ];
        let mut c_cur = b"Hello, World! hello world \xc3\xa9".to_vec();
        let mut r_cur = c_cur.clone();
        for (from, to) in steps {
            let cs = std::ffi::CString::new(c_cur.clone()).unwrap();
            // SAFETY: all three are live NUL-terminated strings, and `from`
            // is not empty, so C's search advances.
            let p = unsafe { ffi::av_strireplace(cs.as_ptr(), from.as_ptr(), to.as_ptr()) };
            assert!(!p.is_null());
            // SAFETY: `p` is a fresh NUL-terminated string we own.
            c_cur = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
            // SAFETY: as above; freed once.
            unsafe { ffi::av_free(p.cast()) };

            let rs = std::ffi::CString::new(r_cur).unwrap();
            r_cur = replace_ignore_ascii_case(&rs, from, to)
                .unwrap()
                .as_bytes()
                .to_vec();
            assert_eq!(r_cur, c_cur, "{from:?} -> {to:?}");
        }
    }

    /// Every destination type, on each side.
    #[derive(Debug, Default)]
    struct Dest {
        i0: i32,
        i1: i32,
        i2: i32,
        u: u32,
        hh: i8,
        uhh: u8,
        h: i16,
        uh: u16,
        ll: i64,
        ull: u64,
        z: usize,
        zs: isize,
        f: f32,
        d: f64,
        p: usize,
        b0: [u8; 12],
        b1: [u8; 12],
    }

    #[derive(Clone, Copy)]
    enum F {
        I0,
        I1,
        I2,
        U,
        Hh,
        Uhh,
        H,
        Uh,
        Ll,
        Ull,
        Z,
        Zs,
        Fl,
        D,
        P,
        B0,
        B1,
    }

    impl Dest {
        /// Each field as a destination, in `F` order.
        fn args(&mut self) -> [Option<ScanArg<'_>>; 17] {
            let Dest {
                i0,
                i1,
                i2,
                u,
                hh,
                uhh,
                h,
                uh,
                ll,
                ull,
                z,
                zs,
                f,
                d,
                p,
                b0,
                b1,
            } = self;
            [
                Some(i0.into()),
                Some(i1.into()),
                Some(i2.into()),
                Some(u.into()),
                Some(hh.into()),
                Some(uhh.into()),
                Some(h.into()),
                Some(uh.into()),
                Some(ll.into()),
                Some(ull.into()),
                Some(z.into()),
                Some(zs.into()),
                Some(f.into()),
                Some(d.into()),
                Some(p.into()),
                Some(b0.into()),
                Some(b1.into()),
            ]
        }

        /// Each field's address, for C.
        fn c_ptr(this: *mut Dest, f: F) -> *mut c_void {
            // SAFETY: `this` points at a live `Dest`; only addresses are taken.
            unsafe {
                match f {
                    F::I0 => addr_of_mut!((*this).i0).cast(),
                    F::I1 => addr_of_mut!((*this).i1).cast(),
                    F::I2 => addr_of_mut!((*this).i2).cast(),
                    F::U => addr_of_mut!((*this).u).cast(),
                    F::Hh => addr_of_mut!((*this).hh).cast(),
                    F::Uhh => addr_of_mut!((*this).uhh).cast(),
                    F::H => addr_of_mut!((*this).h).cast(),
                    F::Uh => addr_of_mut!((*this).uh).cast(),
                    F::Ll => addr_of_mut!((*this).ll).cast(),
                    F::Ull => addr_of_mut!((*this).ull).cast(),
                    F::Z => addr_of_mut!((*this).z).cast(),
                    F::Zs => addr_of_mut!((*this).zs).cast(),
                    F::Fl => addr_of_mut!((*this).f).cast(),
                    F::D => addr_of_mut!((*this).d).cast(),
                    F::P => addr_of_mut!((*this).p).cast(),
                    F::B0 => addr_of_mut!((*this).b0).cast(),
                    F::B1 => addr_of_mut!((*this).b1).cast(),
                }
            }
        }
    }

    /// Inputs and formats across every conversion, width, set form, `%n`,
    /// suppression, literal and failure path, with their destinations.
    const CASES: &[(&CStr, &CStr, &[F])] = &[
        (c"42", c"%d", &[F::I0]),
        (c"-17 0x1f 077", c"%d %x %o", &[F::I0, F::U, F::I1]),
        (c"0x1F 017 -0", c"%i %i %i", &[F::I0, F::I1, F::I2]),
        (c"300 200", c"%hhd %hhu", &[F::Hh, F::Uhh]),
        (c"65537", c"%hd", &[F::H]),
        (c"70000 -3", c"%hu %hx", &[F::Uh, F::H]),
        (c"-42 0x10", c"%zd %ti", &[F::Zs, F::Z]),
        (c"99999999999999999999", c"%lld", &[F::Ll]),
        (c"18446744073709551615", c"%llu", &[F::Ull]),
        (c"123 -5", c"%zu %jd", &[F::Z, F::Ll]),
        (c"12345", c"%3d%d", &[F::I0, F::I1]),
        (c"3.5e2", c"%f", &[F::Fl]),
        (c"0x1.8p1", c"%la", &[F::D]),
        (c"inf -nan", c"%lf %f", &[F::D, F::Fl]),
        (c"1e400 1e-400", c"%lg %G", &[F::D, F::Fl]),
        (c"-.5", c"%le", &[F::D]),
        (c"hello world", c"%11s %11s", &[F::B0, F::B1]),
        (c"abcdef", c"%3s%11s", &[F::B0, F::B1]),
        (c"abc", c"%2c", &[F::B0]),
        (c"a", c"%3c", &[F::B0]),
        (c"key=value;", c"%11[^=]=%11[^;]", &[F::B0, F::B1]),
        (c"]x-y.", c"%11[]x-y]", &[F::B0]),
        (c"--a!", c"%11[-a]%c", &[F::B0, F::B1]),
        (c"12 34", c"%d%n %d", &[F::I0, F::I1, F::I2]),
        (c"12 34", c"%*d %d", &[F::I0]),
        (c"7 8", c"%d %*s%n", &[F::I0, F::I1]),
        (c"0xdeadbeef", c"%p", &[F::P]),
        (c"", c"%d", &[F::I0]),
        (c"   ", c"%d", &[F::I0]),
        (c"abc", c"%d", &[F::I0]),
        (c"12 abc", c"%d %d", &[F::I0, F::I1]),
        (c"12", c"%d %d", &[F::I0, F::I1]),
        (c"100%", c"%d%%", &[F::I0]),
        (c"100 %  5", c"%d %% %d", &[F::I0, F::I1]),
        (c"x", c"y%d", &[F::I0]),
        (c"", c"x%d", &[F::I0]),
        (c"\t\n 9", c" %d", &[F::I0]),
        (c"5x", c"%dx%n", &[F::I0, F::I1]),
        (c"5", c"%*5c", &[]),
        (c"abc", c"%hhn%n", &[F::Hh, F::I0]),
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn sscanf_matches_av_sscanf_case_by_case_and_in_sequence() {
        // One pair of destinations, carried through every case: unassigned
        // destinations must keep earlier values on both sides alike.
        let (mut c, mut r) = (Dest::default(), Dest::default());
        for &(input, format, fields) in CASES.iter().chain(CASES.iter().rev()) {
            let cp = &raw mut c;
            let mut p = [core::ptr::null_mut::<c_void>(); 16];
            for (slot, &f) in p.iter_mut().zip(fields) {
                *slot = Dest::c_ptr(cp, f);
            }
            // SAFETY: `format` has one conversion per address in `p`, each a
            // live field of `c` of the type the conversion stores (every
            // `%s` / `%[` / `%c` width fits the 12-byte buffers); the NULL
            // padding is never read.
            let cret = unsafe {
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

            let mut all = r.args();
            let mut args: Vec<ScanArg<'_>> = fields
                .iter()
                .map(|&f| all[f as usize].take().unwrap())
                .collect();
            let rret = scan(input, format, &mut args).unwrap();
            drop(args);
            assert_eq!(
                rret.map_or(-1, |n| n as i32),
                cret,
                "{input:?} by {format:?}"
            );
            assert_eq!(
                format!("{r:?}"),
                format!("{c:?}"),
                "{input:?} by {format:?}"
            );
        }
    }

    /// Numeric inputs across C's integer and floating-point scanners: bases,
    /// prefixes, overflow, long mantissas, rounding halfway cases, denormals,
    /// hex floats, and partial `inf` / `nan` spellings.
    const NUMBERS: [&CStr; 48] = [
        c"0",
        c"123",
        c"-0",
        c"+7",
        c"0x",
        c"0xg",
        c"0X1fG",
        c"077",
        c"08",
        c"-",
        c"+",
        c"18446744073709551616",
        c"-9223372036854775809",
        c"42",
        c"0.1",
        c"123456789012",
        c"1234567890123456789012345",
        c"1.0000000000000000000000001",
        c"0.000001e-300",
        c"2.2250738585072011e-308",
        c"4.9e-324",
        c"1e-320",
        c"2e-324",
        c"123456789e-5",
        c"9007199254740993",
        c"9007199254740995",
        c"0.500000000000000166533453693773481063544750213623046875",
        c"1.7976931348623157e308",
        c"1.8e308",
        c"00000.0001",
        c".",
        c"1e",
        c"1e+",
        c"0x1p99999",
        c"0x0.0001p0",
        c"0x.p1",
        c"0x1p-1070",
        c"0x1.fffffffffffff8p1023",
        c"0x123456789abcdef0123p0",
        c"0x1.000000000000081p0",
        c"infinity",
        c"infinit",
        c"INF",
        c"nan(123)",
        c"nan(",
        c"nan(x y",
        c"NaN",
        c"1e400",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn sscanf_numbers_match_av_sscanf() {
        let formats: [(&CStr, F); 9] = [
            (c"%d", F::I0),
            (c"%i", F::I1),
            (c"%x", F::U),
            (c"%o", F::I2),
            (c"%lli", F::Ll),
            (c"%f", F::Fl),
            (c"%lf", F::D),
            (c"%la", F::D),
            (c"%3lg", F::D),
        ];
        let (mut c, mut r) = (Dest::default(), Dest::default());
        for input in NUMBERS {
            for (format, field) in formats {
                let p = Dest::c_ptr(&raw mut c, field);
                let n = core::ptr::null_mut::<c_void>();
                // SAFETY: `format` has one conversion, whose destination is
                // the live field of `c` at `p`, of the type it stores; the
                // NULL padding is never read.
                let cret = unsafe {
                    ffi::av_sscanf(
                        input.as_ptr(),
                        format.as_ptr(),
                        p,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                        n,
                    )
                };
                let mut all = r.args();
                let arg = all[field as usize].take().unwrap();
                let rret = scan(input, format, &mut [arg]).unwrap();
                assert_eq!(
                    rret.map_or(-1, |n| n as i32),
                    cret,
                    "{input:?} by {format:?}"
                );
                assert_eq!(
                    format!("{r:?}"),
                    format!("{c:?}"),
                    "{input:?} by {format:?}"
                );
            }
        }
    }

    /// Set in the child process an isolated test re-runs itself in.
    const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

    /// The `av_max_alloc` limit is process-global, so this test lowers it only
    /// in a child process running nothing else, and passes when the child
    /// does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replace_failure_at_the_av_max_alloc_limit_matches_c() {
        const NAME: &str =
            "avstring::search_and_scan::replace_failure_at_the_av_max_alloc_limit_matches_c";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }

        // The result is copied out of `AVBPrint`'s internal buffer with
        // `av_memdup(len + 1)`, or grown past it with `av_realloc`; both
        // respect the limit. The last case outgrows the internal buffer, so
        // C's search stops with an incomplete `AVBPrint`.
        const LIMIT: usize = 16;
        // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in
        // this process.
        unsafe { ffi::av_max_alloc(LIMIT) };
        let long = std::ffi::CString::new("ab".repeat(600)).unwrap();
        let mut outcomes = Vec::new();
        for (src, from, to) in [
            (c"abc", c"b", c"x"),
            (c"0123456789abcd", c"a", c"A"),
            (c"0123456789abcde", c"a", c"A"),
            (c"0123456789abcdef", c"a", c"A"),
            (c"ab", c"b", c"bbbbbbbbbbbbbbbbbbbb"),
            (long.as_c_str(), c"a", c"xyz"),
        ] {
            // SAFETY: all three are live NUL-terminated strings, and `from`
            // is not empty.
            let p = unsafe { ffi::av_strireplace(src.as_ptr(), from.as_ptr(), to.as_ptr()) };
            let c = (!p.is_null()).then(|| {
                // SAFETY: `p` is a fresh NUL-terminated string we own.
                let bytes = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
                // SAFETY: as above; freed once.
                unsafe { ffi::av_free(p.cast()) };
                bytes
            });
            let r = replace_ignore_ascii_case(src, from, to).map(|s| s.as_bytes().to_vec());
            assert_eq!(r, c, "{src:?}: {from:?} -> {to:?}");
            outcomes.push(r.is_some());
        }
        // Both the fitting and the failing side were exercised.
        assert_eq!(outcomes, [true, true, true, false, false, false]);
        // SAFETY: as above; the default limit again.
        unsafe { ffi::av_max_alloc(i32::MAX as usize) };
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn character_classes_match_c_on_every_byte_and_the_extremes() {
    let extremes = [
        c_int::MIN,
        c_int::MIN + 1,
        -129,
        -128,
        -2,
        -1,
        256,
        257,
        c_int::MAX,
    ];
    for c in (-1..=255).chain(extremes) {
        // SAFETY: the classifiers take their argument by value.
        let (g, s, x) = unsafe { (ffi::av_isgraph(c), ffi::av_isspace(c), ffi::av_isxdigit(c)) };
        assert_eq!(c_int::from(is_graph(c)), g, "isgraph({c})");
        assert_eq!(c_int::from(is_space(c)), s, "isspace({c})");
        assert_eq!(c_int::from(is_xdigit(c)), x, "isxdigit({c})");
    }
}

const NAMES: [&CStr; 9] = [
    c"", c"h264", c"H264", c"h26", c"h2645", c"all", c"ALL", c"-", c"a,b",
];

const NAME_LISTS: [&CStr; 14] = [
    c"",
    c",",
    c",,",
    c"h264",
    c"mpeg4,h264",
    c"mpeg4,H264,",
    c"-h264,ALL",
    c"ALL,-h264",
    c"-ALL",
    c"ALL",
    c"AL",
    c"ALLX",
    c"-",
    c"h26,h2645,-",
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn match_name_matches_av_match_name() {
    for name in NAMES {
        for names in NAME_LISTS {
            // SAFETY: both are live NUL-terminated strings, only read.
            let c = unsafe { ffi::av_match_name(name.as_ptr(), names.as_ptr()) };
            assert_eq!(
                c_int::from(match_name(name, names)),
                c,
                "{name:?} in {names:?}"
            );
        }
    }
}

const LISTS: [&CStr; 12] = [
    c"",
    c",",
    c"a",
    c"a,b",
    c"b,a",
    c"ab,a",
    c",a",
    c"a,",
    c"aa,bb,cc",
    c"x;y",
    c"\xff",
    c"a\xffb",
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn match_list_matches_av_match_list_for_every_separator() {
    for sep in [b',', b';', b' ', b'a', 0x01, 0x7f, 0x80, 0xff] {
        let separator = NonZeroU8::new(sep).unwrap();
        for name in LISTS {
            for list in LISTS {
                // SAFETY: both are live NUL-terminated strings and the
                // separator is not NUL, so C reads each only up to its
                // terminator.
                let c = unsafe { ffi::av_match_list(name.as_ptr(), list.as_ptr(), sep as c_char) };
                assert_eq!(
                    c_int::from(match_list(name, list, separator)),
                    c,
                    "{name:?} vs {list:?} on {sep:#x}"
                );
            }
        }
    }
}

/// The safe [`escape`](ffmpeg_bindings::libavutil::libavutil::avstring::escape) against raw
/// `av_escape` (core wave 1, batch 2), over every mode, flag set and special
/// set, alone and chained.
mod escape_wrapper {
    use core::ffi::{CStr, c_int};
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::avstring::{EscapeFlags, escape};
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    use super::{FLAGS, INPUTS, MODES};

    /// The safe wrapper's bytes, `Err` when it reports a failure.
    fn mine(
        src: &CStr,
        special: Option<&CStr>,
        mode: ffmpeg_bindings::libavutil::libavutil::avstring::EscapeMode,
        flags: c_int,
    ) -> Result<Vec<u8>, ()> {
        escape(src, special, mode, EscapeFlags::from_bits_retain(flags))
            .map(|s| s.as_bytes().to_vec())
            .ok_or(())
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn escape_is_av_escape() {
        let flags = FLAGS.iter().copied().chain([1 << 4, -1, c_int::MIN]);
        for (mode, raw) in MODES {
            for f in flags.clone() {
                for special in [None, Some(c""), Some(c"lt"), Some(c" "), Some(c"'\\<&")] {
                    for src in INPUTS {
                        let c = super::escape(src, special, raw, f).map_err(drop);
                        assert_eq!(
                            mine(src, special, mode, f),
                            c,
                            "{src:?} {special:?} {mode:?} {f:#x}"
                        );
                    }
                }
            }
        }
    }

    /// Set in the child process an isolated test re-runs itself in.
    const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

    /// The `av_max_alloc` limit is process-global, so this test lowers it only
    /// in a child process running nothing else, and passes when the child
    /// does.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn failures_at_the_av_max_alloc_limit_match_c() {
        const NAME: &str = "avstring::escape_wrapper::failures_at_the_av_max_alloc_limit_match_c";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }

        // `av_escape` fills an `AVBPrint` whose first kilobyte or so is
        // internal: a short result fails at the final `av_memdup`, a long one
        // while the buffer grows past it.
        const LIMIT: usize = 64;
        // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in
        // this process.
        unsafe { ffi::av_max_alloc(LIMIT) };
        let (mut ok, mut failed) = (0, 0);
        for len in [0, 10, 62, 63, 64, 100, 1000, 3000] {
            let src = CString::new(vec![b'a'; len]).unwrap();
            for (mode, raw) in MODES {
                let c = super::escape(&src, None, raw, 0).map_err(drop);
                let r = mine(&src, None, mode, 0);
                assert_eq!(r, c, "{len} {mode:?}");
                match r {
                    // The result and its NUL fit the limit.
                    Ok(v) => {
                        assert!(v.len() < LIMIT, "{len} {mode:?}");
                        ok += 1;
                    }
                    Err(()) => failed += 1,
                }
            }
        }
        assert!(ok > 0 && failed > 0, "{ok} {failed}");
        // SAFETY: as above; the default limit again.
        unsafe { ffi::av_max_alloc(i32::MAX as usize) };
        assert!(mine(c"0123456789abcdef", None, MODES[0].0, 0).is_ok());
    }
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn repeated_escapes_agree_after_every_step() {
        for (mode, raw) in MODES {
            let mut rust = CString::from(c"a 'quoted' <x> & \\ tail ");
            let mut c = rust.clone();
            for step in 0..5 {
                let r = escape(&rust, Some(c"x"), mode, EscapeFlags::WHITESPACE).unwrap();
                let k = super::escape(&c, Some(c"x"), raw, 1).unwrap();
                assert_eq!(r.as_bytes(), &k[..], "{mode:?} step {step}");
                rust = CString::new(r.as_bytes()).unwrap();
                c = CString::new(k).unwrap();
            }
        }
    }
}
