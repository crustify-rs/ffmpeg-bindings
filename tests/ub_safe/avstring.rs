//! UB tests for `libavutil/avstring.h`'s [`EscapeMode`], character classes
//! and name matching, through the safe API only. Strings are exact-size
//! `CString` allocations, so a read past a terminator lands in a sanitizer
//! redzone.

use core::ffi::{CStr, c_int};
use core::num::NonZeroU8;
use std::ffi::CString;
use std::thread;

use ffmpeg::libavutil::avstring::{
    EscapeMode, is_graph, is_space, is_xdigit, match_list, match_name,
};
use ffmpeg::libavutil::ffi;

const ALL: [EscapeMode; 4] = [
    EscapeMode::Auto,
    EscapeMode::Backslash,
    EscapeMode::Quote,
    EscapeMode::Xml,
];

#[test]
fn every_mode_encodes_to_a_c_enumerator() {
    let known = [
        ffi::AVEscapeMode::AV_ESCAPE_MODE_AUTO,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_BACKSLASH,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_QUOTE,
        ffi::AVEscapeMode::AV_ESCAPE_MODE_XML,
    ];
    for m in ALL {
        assert!(known.contains(&m.into_raw()));
        assert!(!format!("{m:?}").is_empty());
    }
}

#[test]
fn modes_cross_threads() {
    let back = thread::spawn(|| ALL.map(ffi::AVEscapeMode::from))
        .join()
        .unwrap();
    assert_eq!(back, ALL.map(EscapeMode::into_raw));
}

/// The string helpers on exact-size heap buffers, so a read or write past a
/// slice, or a leftover borrow, reaches the sanitizers.
mod helpers {
    use core::ffi::{CStr, c_int};
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::avstring::{
        cmp_n_ignore_ascii_case, find, strip_prefix, strlcat, strlcatf, strlcpy, strnlen,
        to_ascii_lowercase, to_ascii_uppercase, tokens, utf8_decode,
    };
    use ffmpeg::libavutil::ffi;

    /// A heap copy of `bytes` with no spare capacity.
    fn exact(bytes: &[u8]) -> Box<[u8]> {
        bytes.into()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn copies_and_appends_stay_inside_every_buffer_size() {
        let long = CString::new(vec![b'q'; 4000]).unwrap();
        for src in [c"", c"abc", long.as_c_str()] {
            for size in [0, 1, 2, 3, 4, 17, 4000, 4001] {
                let mut buf = exact(&vec![0xa5; size]);
                let n = strlcpy(&mut buf, src);
                assert_eq!(n, src.count_bytes());
                if size > 0 {
                    assert_eq!(strnlen(&buf), n.min(size - 1));
                    let before = strnlen(&buf);
                    let total = strlcat(&mut buf, src).unwrap();
                    assert_eq!(total, before + src.count_bytes());
                    let total =
                        strlcatf(&mut buf, format_args!("{}{}", 7, src.to_str().unwrap())).unwrap();
                    assert!(total >= before);
                    assert!(buf.contains(&0));
                } else {
                    assert_eq!(strlcat(&mut buf, src), None);
                    assert_eq!(strlcatf(&mut buf, format_args!("x")), None);
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn appends_need_a_nul_inside_the_slice() {
        // The NUL lies just past the slice handed over, where C must not look.
        let mut buf = exact(b"abcd\0");
        assert_eq!(strlcat(&mut buf[..4], c"x"), None);
        assert_eq!(strlcatf(&mut buf[..4], format_args!("x")), None);
        assert_eq!(strlcat(&mut buf, c"x"), Some(5));
        assert_eq!(&buf[..], b"abcd\0");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn find_and_strnlen_read_only_the_slice() {
        let hay = exact(b"abcabcd");
        for len in 0..=hay.len() {
            let tail = exact(&hay[..len]);
            for needle in [c"", c"a", c"cd", c"abcabcd", c"abcabcde"] {
                if let Some(i) = find(&tail, needle) {
                    assert!(tail[i..].starts_with(needle.to_bytes()));
                }
            }
            assert_eq!(strnlen(&tail), len);
        }
        assert_eq!(find(&[], c""), Some(0));
        assert_eq!(find(&[], c"a"), None);
        assert_eq!(strnlen(&[]), 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn prefixes_and_comparisons_read_only_the_strings() {
        let s = CString::new("Prefix-rest").unwrap();
        let rest = strip_prefix(&s, c"Prefix-").unwrap();
        assert_eq!(rest, c"rest");
        assert_eq!(strip_prefix(&s, c"Prefix-rest-and-more"), None);
        assert_eq!(strip_prefix(&s, c"").unwrap(), s.as_c_str());
        assert_eq!(strip_prefix(c"", c""), Some(c""));
        let full = strip_prefix(&s, s.as_c_str()).unwrap();
        assert!(full.is_empty());
        drop(s);

        let a = CString::new("ABC").unwrap();
        let b = CString::new("abcdef").unwrap();
        for n in [0, 1, 3, 4, usize::MAX] {
            let _ = cmp_n_ignore_ascii_case(&a, &b, n);
            let _ = cmp_n_ignore_ascii_case(&b, &a, n);
        }
        assert!(cmp_n_ignore_ascii_case(&a, &a, usize::MAX).is_eq());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tokens_write_only_inside_the_string() {
        for input in [&b"\0"[..], b",\0", b"a,b,,c\0tail", b"abc\0", b",,,x,,,\0"] {
            let mut buf = exact(input);
            let all: Vec<&CStr> = tokens(&mut buf, c",").unwrap().collect();
            let owned: Vec<Vec<u8>> = all.iter().map(|t| t.to_bytes().to_vec()).collect();
            assert!(owned.iter().all(|t| !t.is_empty() && !t.contains(&b',')));
            // Everything after the first NUL is untouched.
            let nul = input.iter().position(|&b| b == 0).unwrap();
            assert_eq!(buf[nul..], input[nul..]);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tokens_outlive_the_iterator_and_later_tokens() {
        let mut buf = exact(b"x y z\0");
        let mut it = tokens(&mut buf, c" ").unwrap();
        let first = it.next().unwrap();
        let rest: Vec<&CStr> = it.collect();
        assert_eq!(first, c"x");
        assert_eq!(rest, [c"y", c"z"]);
        // The borrow ends with the tokens; the buffer is free again.
        buf[0] = b'w';
        assert_eq!(&buf[..], b"w\0y\0z\0");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn an_abandoned_iterator_leaves_the_buffer_usable() {
        let mut buf = exact(b"a b c\0");
        assert_eq!(tokens(&mut buf, c" ").unwrap().next(), Some(c"a"));
        assert_eq!(&buf[..], b"a\0b c\0");
        let again: Vec<&CStr> = tokens(&mut buf, c" ").unwrap().collect();
        assert_eq!(again, [c"a"]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utf8_decode_reads_only_the_slice() {
        let mut samples: Vec<Vec<u8>> = (0..=255u8).map(|b| vec![b]).collect();
        for lead in [0xc2u8, 0xe0, 0xed, 0xef, 0xf0, 0xf4, 0xf8, 0xfc, 0xfd] {
            for len in 1..=6 {
                let mut s = vec![lead];
                s.resize(len, 0xbf);
                samples.push(s);
            }
        }
        for sample in &samples {
            let buf = exact(sample);
            for flags in [0, ffi::AV_UTF8_FLAG_ACCEPT_ALL | 8, u32::MAX] {
                let mut rest = &buf[..];
                while let Some((r, n)) = utf8_decode(rest, flags) {
                    assert!((1..=rest.len()).contains(&n));
                    if let Ok(c) = r {
                        assert!(c <= i32::MAX as u32);
                    }
                    rest = &rest[n..];
                }
            }
        }
        assert_eq!(utf8_decode(&[], 0), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn case_mapping_accepts_any_int() {
        for c in [
            c_int::MIN,
            -129,
            -1,
            0,
            64,
            65,
            90,
            91,
            96,
            97,
            122,
            123,
            255,
            256,
            c_int::MAX,
        ] {
            let _ = (to_ascii_lowercase(c), to_ascii_uppercase(c));
        }
        assert_eq!(to_ascii_uppercase(c_int::from(b'q')), c_int::from(b'Q'));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn helpers_run_on_many_threads_at_once() {
        let workers: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    let mut buf = exact(&[0; 64]);
                    for j in 0..200 {
                        strlcpy(&mut buf, c"t");
                        strlcatf(&mut buf, format_args!("{i}-{j}")).unwrap();
                        let toks: Vec<Vec<u8>> = tokens(&mut buf, c"-")
                            .unwrap()
                            .map(|t| t.to_bytes().to_vec())
                            .collect();
                        assert_eq!(toks.len(), 2);
                        let _ = utf8_decode("\u{20ac}".as_bytes(), 0);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().unwrap();
        }
    }
}

/// `basename`, `append_path_component` and `format`: borrowed views, owned
/// results moved across threads, and formatting that panics part-way.
mod paths_and_format {
    use core::ffi::CStr;
    use core::fmt;
    use std::ffi::CString;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use ffmpeg::libavutil::avstring::{append_path_component, basename, format};
    use ffmpeg::libavutil::mem::HeapString;

    /// A path of up to 512 bytes of `n`-byte components, NUL-terminated.
    /// Small enough that BSan's runtime, whose memory grows with every byte
    /// view of a C string it later frees, stays bounded.
    fn long_path(n: usize) -> CString {
        let mut s = String::new();
        while s.len() + n < 512 {
            s.push_str(&"c".repeat(n));
            s.push('/');
        }
        CString::new(s).unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn basename_views_stay_inside_their_path() {
        let paths = [
            long_path(1),
            long_path(7),
            long_path(511),
            CString::default(),
        ];
        for path in &paths {
            let base = basename(path);
            let range = path.as_bytes_with_nul().as_ptr_range();
            if !path.is_empty() {
                assert!(range.contains(&base.as_ptr().cast()), "{base:?}");
            }
            // Reading the whole view stays in bounds.
            let _ = base.to_bytes().iter().map(|&b| u32::from(b)).sum::<u32>();
        }
        // An empty path's view is the library's static ".".
        let base: &'static CStr = basename(c"");
        assert_eq!(base, c".");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn joins_of_long_and_edge_paths_are_owned_and_bounded() {
        let parts = [
            long_path(3),
            long_path(511),
            CString::default(),
            CString::from(c"/"),
            CString::from(c"//"),
        ];
        for a in &parts {
            for b in &parts {
                let joined = append_path_component(a, b).unwrap();
                assert!(joined.as_bytes().len() <= a.as_bytes().len() + b.as_bytes().len() + 1);
                let copy = joined.clone();
                drop(joined);
                assert!(copy.as_bytes().iter().all(|&c| c != 0));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn format_handles_long_text_and_nuls_anywhere() {
        let long = "x".repeat(1000);
        let s = format(format_args!("{long}{}", 7)).unwrap();
        assert_eq!(s.as_bytes().len(), 1001);
        for nul in [0, 1, 500, 1000] {
            let mut text = long.clone();
            text.insert(nul, '\0');
            let s = format(format_args!("{text}")).unwrap();
            assert_eq!(s.as_bytes().len(), nul);
        }
    }

    /// Writes `n` pieces, then panics.
    struct PanicsAfter(usize);

    impl fmt::Display for PanicsAfter {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for _ in 0..self.0 {
                f.write_str("piece")?;
            }
            panic!("formatting failed");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_panicking_display_unwinds_before_c_allocates() {
        for n in [0, 1, 100] {
            let r = catch_unwind(AssertUnwindSafe(|| {
                format(format_args!("{}", PanicsAfter(n)))
            }));
            assert!(r.is_err());
        }
        // The next call is unaffected.
        assert_eq!(format(format_args!("ok")).unwrap().to_str(), Ok("ok"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn owned_results_move_and_drop_on_other_threads() {
        let made: Vec<HeapString> = (0..64)
            .map(|i| match i % 2 {
                0 => format(format_args!("item-{i}")).unwrap(),
                _ => append_path_component(c"dir", c"file").unwrap(),
            })
            .collect();
        let handles: Vec<_> = made
            .into_iter()
            .map(|s| std::thread::spawn(move || (s.clone(), s.as_bytes().len())))
            .collect();
        for h in handles {
            let (copy, len) = h.join().unwrap();
            assert_eq!(copy.as_bytes().len(), len);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn builders_run_concurrently() {
        std::thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    let mut path = format(format_args!("root{t}")).unwrap();
                    for i in 0..200 {
                        let part = format(format_args!("p{i}")).unwrap();
                        path = append_path_component(path.as_c_str(), part.as_c_str()).unwrap();
                        assert_eq!(basename(path.as_c_str()), part.as_c_str());
                    }
                });
            }
        });
    }
}

/// `dirname`, `get_token` and `is_ascii_digit` on every input shape.
mod paths_and_tokens {
    use std::thread;

    use ffmpeg::libavutil::avstring::{dirname, get_token, is_ascii_digit};

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dirname_takes_any_buffer() {
        let long: Vec<u8> = b"d/"
            .iter()
            .copied()
            .cycle()
            .take(4000)
            .chain([0])
            .collect();
        let mut inputs: Vec<Vec<u8>> = [
            &b""[..],
            b"\0",
            b"/\0",
            b"no-nul/at/all",
            b"a/\0/b\0",
            b"\xff\xfe/\0",
        ]
        .map(<[u8]>::to_vec)
        .into();
        inputs.push(long);
        for mut buf in inputs {
            let len = buf.len();
            let out = dirname(&mut buf).map(|d| d.to_bytes().to_vec());
            // A cut inside the buffer, or C's static `"."`.
            assert!(out.is_none_or(|d| d.len() < len || d == b"."));
            assert_eq!(buf.len(), len);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn dirname_walks_up_to_the_root() {
        let mut buf = b"/a/bb/ccc/dddd\0".to_vec();
        let mut seen = 0;
        while let Some(d) = dirname(&mut buf) {
            if d.is_empty() || d == c"." {
                break;
            }
            seen += 1;
            assert!(seen < 10);
        }
        assert_eq!(seen, 3);
        assert_eq!(&buf[..2], b"\0a");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn get_token_consumes_any_input_to_its_end() {
        let long = std::ffi::CString::new("x\\,y 'q,q' , ".repeat(60)).unwrap();
        let inputs = [
            c"",
            c" ",
            c"\\",
            c"'",
            c"'unterminated",
            c"a\\",
            c" , , ,",
            c"\\\\\\'",
            long.as_c_str(),
        ];
        for input in inputs {
            for term in [c",", c"", c" ", c"',\\"] {
                let mut cursor = input;
                for _ in 0..=input.count_bytes() {
                    let token = get_token(&mut cursor, term).unwrap();
                    assert!(token.as_bytes().len() <= input.count_bytes());
                    match cursor.to_bytes_with_nul().get(1..) {
                        Some(rest) if !rest.is_empty() => {
                            cursor = std::ffi::CStr::from_bytes_with_nul(rest).unwrap()
                        }
                        _ => break,
                    }
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn tokens_outlive_their_input_and_cross_threads() {
        let tokens = {
            let input = std::ffi::CString::new("one,two,three").unwrap();
            let mut cursor = input.as_c_str();
            let mut out = Vec::new();
            loop {
                out.push(get_token(&mut cursor, c",").unwrap());
                if cursor.is_empty() {
                    break;
                }
                cursor =
                    std::ffi::CStr::from_bytes_with_nul(&cursor.to_bytes_with_nul()[1..]).unwrap();
            }
            out
        };
        let back = thread::spawn(move || {
            tokens
                .iter()
                .map(|t| t.to_str().unwrap().len())
                .collect::<Vec<_>>()
        })
        .join()
        .unwrap();
        assert_eq!(back, [3, 3, 5]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn is_digit_takes_every_int_edge() {
        for c in [i32::MIN, -129, -1, 0, 47, 48, 57, 58, 255, 256, i32::MAX] {
            assert_eq!(is_ascii_digit(c), (48..=57).contains(&c));
        }
    }
}

// ---- case-insensitive search and replace, and av_sscanf ----

mod search_and_scan {
    use std::ffi::CString;

    use ffmpeg::libavutil::avstring::{
        SCAN_MAX_ARGS, ScanArg, ScanError, cmp_ignore_ascii_case, find_ignore_ascii_case,
        replace_ignore_ascii_case, scan, strip_prefix_ignore_ascii_case,
    };

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn matches_stay_inside_their_haystack() {
        // Heap strings of exact length, so ASan sees any read past them.
        for h in ["", "a", "Ab", "xxAbAB", "\u{e9}\u{e9}"] {
            let h = CString::new(h).unwrap();
            for n in [c"", c"a", c"AB", c"b", c"zz", c"\xc3\xa9"] {
                if let Some(at) = find_ignore_ascii_case(&h, n) {
                    assert!(at <= h.count_bytes());
                    let rest = &h.as_c_str()[at..];
                    assert!(cmp_ignore_ascii_case(rest, rest).is_eq());
                }
                if let Some(rest) = strip_prefix_ignore_ascii_case(&h, n) {
                    assert_eq!(rest.count_bytes() + n.count_bytes(), h.count_bytes());
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn replacements_of_every_shape_are_sound() {
        let long = CString::new("aB".repeat(2000)).unwrap();
        for (s, from, to) in [
            (c"", c"", c""),
            (c"abc", c"", c"xyz"),
            (c"abc", c"ABC", c""),
            (c"aaaa", c"a", c"aa"),
            (c"aaaa", c"AA", c"a"),
            (long.as_c_str(), c"b", c"\xff"),
            (long.as_c_str(), c"Ab", c""),
        ] {
            let r = replace_ignore_ascii_case(s, from, to).unwrap();
            assert!(r.len() <= s.count_bytes() * (to.count_bytes() + 1) + 1);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn scans_fill_exact_buffers_without_overrun() {
        let input = CString::new("x".repeat(5000)).unwrap();
        for width in 1..=32usize {
            // `%Ns` / `%N[x]` need N + 1 bytes, `%Nc` N: exactly that, on
            // the heap.
            for (conv, room) in [("s", width + 1), ("[x]", width + 1), ("c", width)] {
                let format = CString::new(format!("%{width}{conv}")).unwrap();
                let mut buf = vec![0u8; room].into_boxed_slice();
                let n = scan(&input, &format, &mut [(&mut buf[..]).into()]).unwrap();
                assert_eq!(n, Some(1));
                let mut short = vec![0u8; room - 1].into_boxed_slice();
                let e = scan(&input, &format, &mut [(&mut short[..]).into()]);
                assert_eq!(e, Err(ScanError::BufferTooSmall(0)));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn hostile_inputs_and_formats_are_sound() {
        let mut i = 0i32;
        let mut l = 0i64;
        let mut d = 0f64;
        let mut b = [0u8; 4];
        let digits = CString::new("9".repeat(10_000)).unwrap();
        assert_eq!(scan(&digits, c"%d", &mut [(&mut i).into()]), Ok(Some(1)));
        assert_eq!(scan(&digits, c"%lf", &mut [(&mut d).into()]), Ok(Some(1)));
        let exp = CString::new(format!("1e{}", "9".repeat(400))).unwrap();
        assert_eq!(scan(&exp, c"%lf", &mut [(&mut d).into()]), Ok(Some(1)));
        let hex = CString::new(format!("0x{}p-99999999999", "f".repeat(300))).unwrap();
        assert_eq!(scan(&hex, c"%la", &mut [(&mut d).into()]), Ok(Some(1)));
        for input in [
            c"", c" ", c"-", c"0x", c"+", c"nan(", c"infinit", c".e5", c"1e",
        ] {
            let _ = scan(input, c"%lld", &mut [(&mut l).into()]);
            let _ = scan(input, c"%lf", &mut [(&mut d).into()]);
            let _ = scan(input, c"%3c", &mut [(&mut b[..3]).into()]);
        }
        for format in [
            c"%99999999999d",
            c"%1$d",
            c"%ms",
            c"%s",
            c"%[",
            c"%",
            c"%Lf",
            c"%S",
        ] {
            assert!(
                scan(c"1", format, &mut [(&mut i).into()]).is_err(),
                "{format:?}"
            );
            assert!(
                scan(c"1", format, &mut [(&mut b).into()]).is_err(),
                "{format:?}"
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_destination_limit_is_enforced() {
        let mut many = [0i32; SCAN_MAX_ARGS + 1];
        let input = CString::new("1 ".repeat(SCAN_MAX_ARGS + 1)).unwrap();
        let mut args: Vec<ScanArg<'_>> = many.iter_mut().map(ScanArg::from).collect();
        let full = CString::new("%d".repeat(SCAN_MAX_ARGS + 1)).unwrap();
        assert_eq!(scan(&input, &full, &mut args), Err(ScanError::TooManyArgs));
        // BSan's runtime gives some of a long variadic call's stack-passed
        // pointers the provenance of a dead stack frame and reports C's
        // writes through them as use-after-free, with no Rust code involved:
        // `av_sscanf` called directly with 16 live `&raw mut` destinations
        // fails the same way, while ASan finds nothing and the values arrive.
        // The first four destinations travel in registers; the other tests
        // use at most three.
        if std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan") {
            return;
        }
        let max = CString::new("%d".repeat(SCAN_MAX_ARGS)).unwrap();
        assert_eq!(scan(&input, &max, &mut args[1..]), Ok(Some(SCAN_MAX_ARGS)));
        drop(args);
        assert!(many[1..].iter().all(|&v| v == 1));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_calls_do_not_race() {
        std::thread::scope(|scope| {
            for t in 0..8 {
                scope.spawn(move || {
                    for k in 0..200 {
                        let text = CString::new(format!("{t} {k} v{k}")).unwrap();
                        let (mut a, mut b, mut s) = (0i32, 0u32, [0u8; 8]);
                        let n = scan(
                            &text,
                            c"%d %u %7s",
                            &mut [(&mut a).into(), (&mut b).into(), (&mut s).into()],
                        );
                        assert_eq!(n, Ok(Some(3)));
                        assert_eq!((a, b), (t, k));
                        let r = replace_ignore_ascii_case(&text, c"V", c"w").unwrap();
                        assert!(find_ignore_ascii_case(r.as_c_str(), c"W").is_some());
                    }
                });
            }
        });
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn character_classes_take_any_int() {
    for c in (-300..=300).chain([c_int::MIN, c_int::MIN + 1, c_int::MAX - 1, c_int::MAX]) {
        let _ = (is_graph(c), is_space(c), is_xdigit(c));
    }
}

/// Strings over the separator-like bytes, of every short length, each its own
/// exact-size allocation.
fn strings() -> Vec<CString> {
    let alphabet = [b'a', b'A', b',', b'-', b';', 0xff];
    let mut out = vec![CString::default()];
    let mut layer = vec![Vec::new()];
    for _ in 0..3 {
        layer = layer
            .iter()
            .flat_map(|s: &Vec<u8>| {
                alphabet.iter().map(move |&b| {
                    let mut t = s.clone();
                    t.push(b);
                    t
                })
            })
            .collect();
        out.extend(layer.iter().map(|s| CString::new(s.clone()).unwrap()));
    }
    out.push(c"ALL".to_owned());
    out.push(c"-ALL,all,".to_owned());
    out.push(CString::new(vec![b','; 300]).unwrap());
    out.push(CString::new([b"a,".repeat(200), b"ALL".to_vec()].concat()).unwrap());
    out
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn match_name_stays_inside_its_strings() {
    let all = strings();
    for name in all.iter().step_by(5) {
        for names in &all {
            let _ = match_name(name, names);
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn match_list_stays_inside_its_strings_for_every_separator() {
    let all = strings();
    let short: Vec<&CStr> = all.iter().take(43).map(CString::as_c_str).collect();
    // Separators in the alphabet, around it, and at the signed-`char` edges.
    for sep in [
        0x01, b',', b';', b'-', b'a', b'A', b'b', 0x7f, 0x80, 0xfe, 0xff,
    ] {
        let separator = NonZeroU8::new(sep).unwrap();
        for name in &short {
            for list in &short {
                let _ = match_list(name, list, separator);
            }
        }
    }
    let comma = NonZeroU8::new(b',').unwrap();
    for name in &all {
        for list in all.iter().rev().take(4) {
            let _ = match_list(name, list, comma);
            let _ = match_list(list, name, comma);
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn matching_runs_concurrently() {
    let handles: Vec<_> = (0..4)
        .map(|t| {
            thread::spawn(move || {
                let all = strings();
                let sep = NonZeroU8::new([b',', b';', b'a', 0xff][t]).unwrap();
                for (i, name) in all.iter().enumerate().step_by(7) {
                    let list = &all[(i * 31) % all.len()];
                    let _ = (match_name(name, list), match_list(name, list, sep));
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

/// [`escape`](ffmpeg::libavutil::avstring::escape) (core wave 1, batch 2) over
/// every byte, every mode and flag set, inputs that grow most when escaped,
/// and results dropped or kept past their inputs, here and on other threads.
mod escaping {
    use std::ffi::CString;
    use std::thread;

    use ffmpeg::libavutil::avstring::{EscapeFlags, EscapeMode, escape};
    use ffmpeg::libavutil::mem::HeapString;

    const MODES: [EscapeMode; 4] = [
        EscapeMode::Auto,
        EscapeMode::Backslash,
        EscapeMode::Quote,
        EscapeMode::Xml,
    ];

    fn flag_sets() -> [EscapeFlags; 6] {
        [
            EscapeFlags::empty(),
            EscapeFlags::WHITESPACE,
            EscapeFlags::STRICT,
            EscapeFlags::XML_SINGLE_QUOTES | EscapeFlags::XML_DOUBLE_QUOTES,
            EscapeFlags::from_bits_retain(-1),
            EscapeFlags::from_bits_retain(i32::MIN),
        ]
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_byte_in_every_mode() {
        let all: Vec<u8> = (1..=255).collect();
        let src = CString::new(all).unwrap();
        for mode in MODES {
            for flags in flag_sets() {
                for special in [None, Some(c""), Some(src.as_c_str())] {
                    let out = escape(&src, special, mode, flags).unwrap();
                    assert!(out.as_bytes().len() >= 255);
                    assert!(!out.as_bytes().contains(&0));
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn worst_case_growth_stays_in_bounds() {
        // XML turns `&` into five bytes; quoting turns `'` into four.
        for (byte, mode, factor) in [
            (b'&', EscapeMode::Xml, 5),
            (b'\'', EscapeMode::Quote, 4),
            (b' ', EscapeMode::Backslash, 2),
        ] {
            for len in [1, 2, 63, 64, 65, 1000] {
                let src = CString::new(vec![byte; len]).unwrap();
                let out = escape(&src, Some(c" "), mode, EscapeFlags::WHITESPACE).unwrap();
                assert!(out.as_bytes().len() >= len * factor, "{mode:?} {len}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn results_outlive_their_inputs() {
        let kept: Vec<HeapString> = (0..32)
            .map(|i| {
                let src = CString::new(format!("item {i}: 'a' & <b>")).unwrap();
                let special = CString::new(":").unwrap();
                escape(&src, Some(&special), MODES[i % 4], flag_sets()[i % 6]).unwrap()
            })
            .collect();
        let total: usize = kept.iter().map(|s| s.as_bytes().len()).sum();
        assert!(total > 32 * 10);
        drop(kept);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn escapes_run_on_many_threads_at_once() {
        let workers: Vec<_> = (0..8)
            .map(|t| {
                thread::spawn(move || {
                    let src = CString::new(format!(" thread {t} 'quoted' <&> ")).unwrap();
                    (0..500)
                        .map(|i| {
                            let s = escape(&src, None, MODES[i % 4], flag_sets()[i % 6]).unwrap();
                            s.as_bytes().len()
                        })
                        .sum::<usize>()
                })
            })
            .collect();
        for w in workers {
            assert!(w.join().unwrap() > 0);
        }
        let shared: HeapString =
            escape(c"shared", None, EscapeMode::Quote, EscapeFlags::empty()).unwrap();
        let moved = thread::spawn(move || shared.to_str().map(str::to_owned));
        assert_eq!(moved.join().unwrap().as_deref(), Ok("'shared'"));
    }
}
