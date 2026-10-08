//! Equivalence of `libavutil/avstring.h`'s [`EscapeMode`] with the C enum, as
//! `av_escape` observes it.

use core::ffi::{CStr, c_char, c_int, c_void};

use ffmpeg::libavutil::avstring::EscapeMode;
use ffmpeg::libavutil::ffi;

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

    use ffmpeg::libavutil::avstring::{
        find, strip_prefix, strlcat, strlcatf, strlcpy, strncasecmp, strnlen, to_ascii_lowercase,
        to_ascii_uppercase, tokens, utf8_decode,
    };
    use ffmpeg::libavutil::ffi;

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
    fn strncasecmp_orders_as_av_strncasecmp() {
        for a in STRINGS {
            for b in STRINGS {
                for n in 0..=8 {
                    // SAFETY: live strings, read up to their NULs or `n`.
                    let c = unsafe { ffi::av_strncasecmp(a.as_ptr(), b.as_ptr(), n) };
                    assert_eq!(strncasecmp(a, b, n), c.cmp(&0), "{a:?} {b:?} {n}");
                }
            }
        }
        assert_eq!(strncasecmp(c"ABCx", c"abcy", 3), Ordering::Equal);
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

    #[test]
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

    use ffmpeg::libavutil::avstring::{append_path_component, basename, format};
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::mem::HeapString;

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
    fn format_of_a_nul_matches_c_reading_the_same_text() {
        // C's `%s` stops at the NUL; so does the Rust result.
        let text = c"before";
        // SAFETY: `%s` takes the live string `text`.
        let c = CString(unsafe { ffi::av_asprintf(c"%s".as_ptr(), text.as_ptr()) });
        let r = format(format_args!("before\0after"));
        assert_eq!(bytes(&r), c.bytes());
    }

    #[test]
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
        // Joins of 6, 7 and 8 bytes, through `av_malloc` and, with an empty
        // side, through `av_strdup`.
        let joins: [(&CStr, &CStr); 6] = [
            (c"abc", c"de"),
            (c"abc", c"def"),
            (c"abcd", c"efg"),
            (c"", c"abcdefg"),
            (c"abcdefgh", c""),
            (c"abc/", c"/defg"),
        ];
        for (path, component) in joins {
            // SAFETY: both are live NUL-terminated strings, only read.
            let c = CString(unsafe {
                ffi::av_append_path_component(path.as_ptr(), component.as_ptr())
            });
            let r = append_path_component(path, component);
            assert_eq!(bytes(&r), c.bytes(), "{path:?} + {component:?}");
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

    use ffmpeg::libavutil::avstring::{dirname, get_token, is_ascii_digit};
    use ffmpeg::libavutil::ffi;

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
