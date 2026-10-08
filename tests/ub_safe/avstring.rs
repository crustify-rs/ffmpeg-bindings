//! UB tests for `libavutil/avstring.h`'s [`EscapeMode`], through the safe API
//! only.

use std::thread;

use ffmpeg::libavutil::avstring::EscapeMode;
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
        find, strip_prefix, strlcat, strlcatf, strlcpy, strncasecmp, strnlen, to_ascii_lowercase,
        to_ascii_uppercase, tokens, utf8_decode,
    };
    use ffmpeg::libavutil::ffi;

    /// A heap copy of `bytes` with no spare capacity.
    fn exact(bytes: &[u8]) -> Box<[u8]> {
        bytes.into()
    }

    #[test]
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
    fn appends_need_a_nul_inside_the_slice() {
        // The NUL lies just past the slice handed over, where C must not look.
        let mut buf = exact(b"abcd\0");
        assert_eq!(strlcat(&mut buf[..4], c"x"), None);
        assert_eq!(strlcatf(&mut buf[..4], format_args!("x")), None);
        assert_eq!(strlcat(&mut buf, c"x"), Some(5));
        assert_eq!(&buf[..], b"abcd\0");
    }

    #[test]
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
            let _ = strncasecmp(&a, &b, n);
            let _ = strncasecmp(&b, &a, n);
        }
        assert!(strncasecmp(&a, &a, usize::MAX).is_eq());
    }

    #[test]
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
    fn an_abandoned_iterator_leaves_the_buffer_usable() {
        let mut buf = exact(b"a b c\0");
        assert_eq!(tokens(&mut buf, c" ").unwrap().next(), Some(c"a"));
        assert_eq!(&buf[..], b"a\0b c\0");
        let again: Vec<&CStr> = tokens(&mut buf, c" ").unwrap().collect();
        assert_eq!(again, [c"a"]);
    }

    #[test]
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
