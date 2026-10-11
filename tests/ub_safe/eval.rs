//! UB tests for `libavutil/eval.h`'s safe `parse_number`: suffix bytes at the
//! very end of exactly sized heap strings, so that ASan sees any read past
//! the NUL, and a rest that stays inside its input.

use std::ffi::{CStr, CString};

use ffmpeg::libavutil::eval::parse_number;

/// `s` in a heap block of exactly its length plus the NUL.
fn exact(s: &[u8]) -> Box<CStr> {
    CString::new(s).unwrap().into_boxed_c_str()
}

const NUMBERS: [&[u8]; 30] = [
    b"",
    b"0",
    b"0x",
    b"0X",
    b"0xg",
    b"0x1p3",
    b"1d",
    b"1dB",
    b"1dBB",
    b"1K",
    b"1Ki",
    b"1KiB",
    b"1E",
    b"1e",
    b"1e+",
    b"1e400",
    b"-1e400Y",
    b"1y",
    b"1z",
    b"1i",
    b"1B",
    b"nan",
    b"-inf",
    b"infinity",
    b"  +.5",
    b".",
    b"\xff",
    b"18446744073709551616",
    b"0x18446744073709551616",
    b"1\xc3\xa9",
];

#[test]
fn rest_is_a_suffix_of_the_input() {
    for n in NUMBERS {
        let s = exact(n);
        let (_, rest) = parse_number(&s);
        let rest = rest.to_bytes();
        assert!(n.ends_with(rest), "{n:?}");
    }
}

/// Whether the suite runs under BorrowSanitizer.
///
/// BSan reports a false out-of-bounds access at `av_strtod`'s `next[0]`
/// (`eval.c:120`) after many calls in one thread: `next` is an
/// uninitialised local that uninstrumented glibc `strtod` writes, so its
/// shadow keeps the provenance a freed earlier frame left in the slot,
/// while the byte C reads lies inside the live string (ASan agrees). The
/// exhaustive sweeps skip under BSan; the listed inputs still run.
fn under_bsan() -> bool {
    std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
}

/// Every string of up to three bytes over the bytes that steer C's suffix
/// handling, each in its own exactly sized block.
#[test]
fn short_strings() {
    if under_bsan() {
        return;
    }
    let alphabet = b"0x1.dBEeKiMm+-zY";
    let mut s = Vec::new();
    for len in 0..=3u32 {
        for mut i in 0..alphabet.len().pow(len) {
            s.clear();
            for _ in 0..len {
                s.push(alphabet[i % alphabet.len()]);
                i /= alphabet.len();
            }
            let input = exact(&s);
            let (_, rest) = parse_number(&input);
            assert!(s.ends_with(rest.to_bytes()));
        }
    }
}

/// The rest outlives every other borrow of the input but the input itself.
#[test]
fn rest_borrows_the_input() {
    let s = exact(b"12k rest");
    let rest = {
        let (value, rest) = parse_number(&s);
        assert_eq!(value, 12_000.0);
        rest
    };
    assert_eq!(rest, c" rest");
}

#[test]
fn many_threads() {
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..500 {
                    assert_eq!(parse_number(c"1.5KiB!"), (12288.0, c"!"));
                }
            });
        }
    });
}

/// UB tests of `libavutil/eval.h`'s owned expressions through the safe API:
/// parsing well-formed, malformed, deeply nested and oversized input with
/// borrowed names that die right after the parse, releasing trees on the
/// parsing thread and on others, parsing on many threads at once, and
/// allocation failure inside the parser.
mod expr {
    use std::ffi::{CStr, CString, c_void};
    use std::sync::Barrier;

    use ffmpeg::libavutil::eval::{BinaryFn, Expr, ExprBox, UnaryFn};

    /// `AVERROR(EINVAL)`.
    const EINVAL: i32 = -22;

    /// Keeps parse errors (`AV_LOG_ERROR`) below the default log level.
    const QUIET: i32 = 40;

    extern "C" fn twice(_: *mut c_void, x: f64) -> f64 {
        2.0 * x
    }

    extern "C" fn minus(_: *mut c_void, a: f64, b: f64) -> f64 {
        a - b
    }

    const CONSTS: [&CStr; 2] = [c"X", c"Y"];
    const UNARY: [(&CStr, UnaryFn); 1] = [(c"f", twice)];
    const BINARY: [(&CStr, BinaryFn); 1] = [(c"g", minus)];

    /// Whether this run is under BSan, which misreports C's parser: every
    /// primary goes through `av_strtod`, whose end pointer glibc's uninstrumented
    /// `strtod` writes, so the local keeps the provenance an earlier frame left
    /// in its slot (often a freed parse buffer) and `next[0]` can read as a use
    /// after free. Which parse trips depends on the stack layout alone: two
    /// `av_expr_parse` calls from pure C, the second from a deeper frame, trip it
    /// at some depths. The tests that parse return early under it.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    fn parse(s: &CStr) -> Result<ExprBox, i32> {
        Expr::parse(s, &CONSTS, &UNARY, &BINARY, QUIET)
    }

    #[test]
    fn well_formed_and_malformed_input() {
        if under_bsan() {
            return;
        }
        for s in [
            c"1",
            c"X*Y+f(X)-g(Y,X)",
            c"st(0, 1); while(lt(ld(0), 10), st(0, ld(0) + 1))",
            c"random(0) + randomi(1, 0, 10) + taylor(1, 1) + root(X - 1, 2)",
            c"print(X, 99)",
            c"1e400 / 0 + nan + inf",
            c"0x7fffffffffffffffffff",
            c"1KiB * 1GiB * 1PiB",
        ] {
            drop(parse(s).unwrap());
        }
        for s in [
            c"",
            c"(",
            c")",
            c"((",
            c"1+",
            c"*1",
            c"f(",
            c"g(1",
            c"g(1,",
            c"g(1,,2)",
            c"Z",
            c"x",
            c"f(1)(2)",
            c"sin()",
            c"\x01",
            c"\xff\xfe",
            c"%",
            c"1e",
            c"0x",
            c"st(0, ",
            c";",
            c"1;",
        ] {
            assert!(parse(s).is_err(), "{s:?}");
        }
        // Every byte alone, and after a valid prefix.
        for b in 1..=u8::MAX {
            for prefix in ["", "1+", "f(", "g(X,"] {
                let mut s = prefix.as_bytes().to_vec();
                s.push(b);
                let s = CString::new(s).unwrap();
                let _ = parse(&s);
            }
        }
    }

    #[test]
    fn nesting_up_to_and_past_the_limit() {
        if under_bsan() {
            return;
        }
        for (open, close) in [
            ("(", ")"),
            ("f(", ")"),
            ("g(1,", ")"),
            ("-(", ")"),
            ("sin(", ")"),
        ] {
            for n in [1, 50, 99, 100, 101, 1000, 10_000] {
                let s = CString::new(format!("{}X{}", open.repeat(n), close.repeat(n))).unwrap();
                let r = parse(&s);
                if n <= 50 {
                    assert!(r.is_ok(), "{open} x {n}");
                }
                if n > 100 {
                    assert_eq!(r.err(), Some(EINVAL), "{open} x {n}");
                }
            }
        }
        // Unbalanced runs fail without reading past the text.
        for n in [1, 100, 5000] {
            let s = CString::new("(".repeat(n)).unwrap();
            assert!(parse(&s).is_err());
            let s = CString::new(")".repeat(n)).unwrap();
            assert!(parse(&s).is_err());
        }
    }

    #[test]
    fn long_input_and_many_names() {
        if under_bsan() {
            return;
        }
        // Operator chains: each operator adds a tree level, and C refuses trees
        // deeper than 100 levels, as if out of memory.
        let s = CString::new(vec!["X"; 100].join("+")).unwrap();
        drop(parse(&s).unwrap());
        for n in [102, 2000] {
            let s = CString::new(vec!["X"; n].join("*")).unwrap();
            assert_eq!(parse(&s).err(), Some(-12));
        }
        // Whitespace-only and whitespace-padded text is copied without it.
        let s = CString::new(" ".repeat(4000) + "1" + &" ".repeat(4000)).unwrap();
        drop(parse(&s).unwrap());
        let s = CString::new(" ".repeat(4000)).unwrap();
        assert_eq!(parse(&s).err(), Some(EINVAL));

        // Many names, the last one used; names that are prefixes of others, and
        // the empty name.
        let owned: Vec<CString> = (0..3000)
            .map(|i| CString::new(format!("c{i}")).unwrap())
            .collect();
        let names: Vec<&CStr> = owned
            .iter()
            .map(CString::as_c_str)
            .chain([c"", c"c"])
            .collect();
        let funcs: Vec<(&CStr, UnaryFn)> = names.iter().map(|&n| (n, twice as UnaryFn)).collect();
        let binary: Vec<(&CStr, BinaryFn)> =
            names.iter().map(|&n| (n, minus as BinaryFn)).collect();
        for s in [
            c"c2999 + c0",
            c"c2999(c1) * c1(c2999, c0)",
            c"c + c(1)",
            c"c3000",
            c"+1",
        ] {
            let _ = Expr::parse(s, &names, &funcs, &binary, QUIET);
        }
    }

    #[test]
    fn names_die_right_after_the_parse() {
        if under_bsan() {
            return;
        }
        let e = {
            let x = CString::new("X").unwrap();
            let f = CString::new("f").unwrap();
            let g = CString::new("g").unwrap();
            let s = CString::new("g(f(X), X)").unwrap();
            Expr::parse(&s, &[x.as_c_str()], &[(&f, twice)], &[(&g, minus)], 0).unwrap()
        };
        // The tree keeps no pointer to the names: dropping them first, then the
        // tree, is clean.
        drop(e);
    }

    #[test]
    fn trees_are_released_on_other_threads() {
        if under_bsan() {
            return;
        }
        let trees: Vec<ExprBox> = (0..64)
            .map(|i| parse(if i % 2 == 0 { c"X+1" } else { c"f(g(X, Y))" }).unwrap())
            .collect();
        std::thread::scope(|s| {
            let mut trees = trees;
            while !trees.is_empty() {
                let batch: Vec<ExprBox> = trees.drain(..16.min(trees.len())).collect();
                s.spawn(move || drop(batch));
            }
        });
        // An exclusive handle moves too.
        let mut e = parse(c"X").unwrap();
        let m = e.as_mut();
        std::thread::scope(|s| {
            s.spawn(move || {
                let _moved = m;
            });
        });
        drop(e);
    }

    #[test]
    fn many_threads_parse_at_once() {
        if under_bsan() {
            return;
        }
        let barrier = Barrier::new(8);
        std::thread::scope(|s| {
            for t in 0..8 {
                let barrier = &barrier;
                s.spawn(move || {
                    barrier.wait();
                    let mut kept = Vec::new();
                    for i in 0..200 {
                        let s = CString::new(format!("X*{i}+f(Y)-g({t},X)")).unwrap();
                        kept.push(parse(&s).unwrap());
                        // Failures log in C's `Eval` context from every thread.
                        assert!(parse(c"1+").is_err());
                        if i % 7 == 0 {
                            kept.clear();
                        }
                    }
                });
            }
        });
    }

    /// The `av_max_alloc` limit is process-global, so this test lowers it only in
    /// a child process running nothing else, and passes when the child does.
    #[test]
    fn allocation_failure_inside_the_parser() {
        if under_bsan() {
            return;
        }
        const ISOLATED: &str = "CRUSTIFY_UB_SAFE_EVAL_ISOLATED";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "eval::expr::allocation_failure_inside_the_parser",
                ])
                .args(["--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }
        let long = CString::new(vec!["f(X)"; 100].join("*")).unwrap();
        let mut failed = 0;
        for limit in 0..=600 {
            ffmpeg::libavutil::mem::set_max_alloc(limit);
            for s in [c"1", c"g(X, f(Y)) + st(0, 1)", long.as_c_str()] {
                match parse(s) {
                    Ok(e) => drop(e),
                    Err(e) => {
                        assert_eq!(e, -12, "{s:?} at {limit}");
                        failed += 1;
                    }
                }
            }
        }
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        assert!(failed > 0);
        drop(parse(&long).unwrap());
    }
}
