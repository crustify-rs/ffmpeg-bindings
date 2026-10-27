//! UB tests for `libavutil/eval.h`'s safe `parse_number`: suffix bytes at the
//! very end of exactly sized heap strings, so that `ASan` sees any read past
//! the NUL, and a rest that stays inside its input.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    trivial_casts,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
#![expect(
    clippy::no_effect_underscore_binding,
    let_underscore_drop,
    reason = "assertions keep the form of the C behaviour they check"
)]

use std::ffi::{CStr, CString};

use ffmpeg_bindings::libavutil::libavutil::eval::parse_number;

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
#[cfg_attr(miri, ignore = "calls C")]
fn rest_is_a_suffix_of_the_input() {
    for n in NUMBERS {
        let s = exact(n);
        let (_, rest) = parse_number(&s);
        let rest = rest.to_bytes();
        assert!(n.ends_with(rest), "{n:?}");
    }
}

/// Whether the suite runs under `BorrowSanitizer`.
///
/// `BSan` reports a false out-of-bounds access at `av_strtod`'s `next[0]`
/// (`eval.c:120`) after many calls in one thread: `next` is an
/// uninitialised local that uninstrumented glibc `strtod` writes, so its
/// shadow keeps the provenance a freed earlier frame left in the slot,
/// while the byte C reads lies inside the live string (`ASan` agrees). The
/// exhaustive sweeps skip under `BSan`; the listed inputs still run.
fn under_bsan() -> bool {
    std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan")
}

/// Every string of up to three bytes over the bytes that steer C's suffix
/// handling, each in its own exactly sized block.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

    use ffmpeg_bindings::libavutil::libavutil::eval::{BinaryFn, Expr, ExprBox, UnaryFn};

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

    /// Whether this run is under `BSan`, which misreports C's parser: every
    /// primary goes through `av_strtod`, whose end pointer glibc's uninstrumented
    /// `strtod` writes, so the local keeps the provenance an earlier frame left
    /// in its slot (often a freed parse buffer) and `next[0]` can read as a use
    /// after free. Which parse trips depends on the stack layout alone: two
    /// `av_expr_parse` calls from pure C, the second from a deeper frame, trip it
    /// at some depths. The tests that parse return early under it.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan")
    }

    fn parse(s: &CStr) -> Result<ExprBox, i32> {
        Expr::parse(s, &CONSTS, &UNARY, &BINARY, QUIET)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
    #[cfg_attr(miri, ignore = "calls C")]
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
            ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(limit);
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
        ffmpeg_bindings::libavutil::libavutil::mem::set_max_alloc(i32::MAX as usize);
        assert!(failed > 0);
        drop(parse(&long).unwrap());
    }
}

/// Options wave 0, batch 3.
///
/// UB tests of `libavutil/eval.h`'s safe evaluator: expressions that reach
/// C's unchecked integer conversions (refused before C runs), constants and
/// Rust functions called through C, re-entrant and panicking functions, and
/// log contexts.
mod expressions {
    use core::cell::Cell;
    use core::ffi::CStr;
    use std::ffi::CString;

    use ffmpeg_bindings::libavutil::libavutil::eval::{Func1, Func2, MAX_FUNCS, parse_and_eval};
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::opt::BareObject;

    const EINVAL: i32 = -22;

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan")
    }

    fn eval(e: &str) -> Result<f64, i32> {
        parse_and_eval(&CString::new(e).unwrap(), &[], &[], &[], 0, None)
    }

    /// Each reaches one of C's unchecked `double` to integer conversions (`UBSan`
    /// `float-cast-overflow` in `eval.c`) or an `int` overflow of its log level.
    #[test]
    fn conversions_c_leaves_undefined_are_refused() {
        for e in [
            "ld(1e300)",
            "ld(nan)",
            "ld(-1/0)",
            "st(1e10,1)",
            "random(1e300)",
            "randomi(-1e300,0,1)",
            "taylor(1,1,1e300)",
            "print(1,1e300)",
            "gcd(1e300,2)",
            "gcd(2,-1e19)",
            "bitand(1e300,1)",
            "bitor(1,-1e300)",
            "ld+(1e300)",
            "1+ld(2*1e300)",
            "st(0,-1);random(0)",
            "st(0,1e300);randomi(0,0,1)",
            "root(random(0),1)",
        ] {
            assert_eq!(eval(e), Err(EINVAL), "{e}");
        }
        assert_eq!(
            parse_and_eval(c"print(1,2147483600)", &[], &[], &[], 100, None),
            Err(EINVAL)
        );
        assert_eq!(
            parse_and_eval(c"1", &[], &[], &[], i32::MAX, None),
            Err(EINVAL)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn builtins_with_literal_arguments_evaluate() {
        for (e, v) in [
            ("ld(0)", 0.0),
            ("st(3,1e300);ld(3)", 1e300),
            ("gcd(12,18)", 6.0),
            ("bitand(12,10)", 8.0),
            (
                "bitor(-9223372036854775808,1)",
                -9_223_372_036_854_775_807.0,
            ),
            ("taylor(1,1)", core::f64::consts::E),
            ("print(2.5,-8)", 2.5),
            ("1+2^3+5*5", 34.0),
        ] {
            let got = eval(e).unwrap();
            assert!((got - v).abs() <= v.abs() * 1e-12, "{e}: {got}");
        }
        assert!((0.0..1.0).contains(&eval("random(0)").unwrap()));
        // Unparsable or NaN expressions fail in C.
        for e in ["", "1+", "nosuch", "ld(", "0/0", "sqrt(-1)", "(1"] {
            assert!(eval(e).is_err(), "{e}");
        }
        // An endless loop is C's to stop: this one ends.
        assert!(eval("st(0,0);while(lt(ld(0),5),st(0,ld(0)+1))").is_ok());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn constants_and_functions_reach_their_own_values() {
        let calls = Cell::new(0);
        let square = |x: f64| {
            calls.set(calls.get() + 1);
            x * x
        };
        let neg = |x: f64| -x;
        let hyp = |x: f64, y: f64| x.hypot(y);
        let funcs1: [Func1<'_>; 2] = [(c"square", &square), (c"neg", &neg)];
        let funcs2: [Func2<'_>; 1] = [(c"hyp", &hyp)];
        let consts = [(c"A", 3.0), (c"B", 4.0), (c"C", f64::NAN)];
        let v = parse_and_eval(
            c"square(A)+neg(B)+hyp(A,B)",
            &consts,
            &funcs1,
            &funcs2,
            0,
            None,
        );
        assert_eq!(v, Ok(10.0));
        assert_eq!(calls.get(), 1);
        // A NaN result is C's error.
        assert_eq!(
            parse_and_eval(c"C", &consts, &[], &[], 0, None),
            Err(EINVAL)
        );
        // A built-in name shadows a function of the same name.
        let funcs1: [Func1<'_>; 1] = [(c"sqrt", &neg)];
        assert_eq!(
            parse_and_eval(c"sqrt(4)", &[], &funcs1, &[], 0, None),
            Ok(2.0)
        );
        // Every trampoline, of both arities.
        let id = |x: f64| x;
        let add = |x: f64, y: f64| x + y;
        let names: Vec<CString> = (0..MAX_FUNCS)
            .map(|i| CString::new(format!("f{i}")).unwrap())
            .collect();
        let funcs1: Vec<Func1<'_>> = names
            .iter()
            .map(|n| (n.as_c_str(), &id as &dyn Fn(f64) -> f64))
            .collect();
        let funcs2: Vec<Func2<'_>> = names
            .iter()
            .map(|n| (n.as_c_str(), &add as &dyn Fn(f64, f64) -> f64))
            .collect();
        for (i, n) in names.iter().enumerate() {
            let n = n.to_str().unwrap();
            let e = CString::new(format!("{n}({i})")).unwrap();
            assert_eq!(parse_and_eval(&e, &[], &funcs1, &[], 0, None), Ok(i as f64));
            let e = CString::new(format!("{n}({i},1)")).unwrap();
            assert_eq!(
                parse_and_eval(&e, &[], &[], &funcs2, 0, None),
                Ok(i as f64 + 1.0)
            );
        }
        // C takes a name from `funcs1` first, and fails a two-argument call of it.
        assert_eq!(
            parse_and_eval(c"f3(1,2)", &[], &funcs1, &funcs2, 0, None),
            Err(EINVAL)
        );
        // One function too many of either arity is refused.
        let mut too_many = funcs1.clone();
        too_many.push((c"x", &id));
        assert_eq!(
            parse_and_eval(c"1", &[], &too_many, &[], 0, None),
            Err(EINVAL)
        );
        let mut too_many = funcs2.clone();
        too_many.push((c"x", &add));
        assert_eq!(
            parse_and_eval(c"1", &[], &[], &too_many, 0, None),
            Err(EINVAL)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn functions_may_evaluate_again() {
        let inner = |x: f64| parse_and_eval(c"X*2", &[(c"X", x)], &[], &[], 0, None).unwrap();
        let funcs1: [Func1<'_>; 1] = [(c"twice", &inner)];
        let v = parse_and_eval(c"twice(twice(3))", &[], &funcs1, &[], 0, None);
        assert_eq!(v, Ok(12.0));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn a_panicking_function_unwinds_after_c_returns() {
        let calls = Cell::new(0);
        let boom = |_: f64| -> f64 {
            calls.set(calls.get() + 1);
            panic!("boom")
        };
        let funcs1: [Func1<'_>; 1] = [(c"boom", &boom)];
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parse_and_eval(c"boom(1)+boom(2)", &[], &funcs1, &[], 0, None)
        }));
        assert!(r.is_err());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn log_contexts_and_offsets() {
        static CLASS: Class = Class::new(c"crustify-eval-ub", ClassCategory::NA);
        let ctx = BareObject::new(CLASS.as_ref()).unwrap();
        for (e, offset) in [
            (c"nosuch(1)", 0),
            (c"1+", -1000),
            (c"print(1)", 64),
            (c"print(1,57)", i32::MAX - 56 - 57),
        ] {
            let _ = parse_and_eval(e, &[], &[], &[], offset, Some(ctx.as_object()));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn expressions_from_many_threads() {
        std::thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    for i in 0..200 {
                        let e = CString::new(format!("st({t},{i});ld({t})*2")).unwrap();
                        assert_eq!(
                            parse_and_eval(&e, &[], &[], &[], 0, None),
                            Ok(2.0 * f64::from(i))
                        );
                    }
                });
            }
        });
    }

    /// Random expressions built from tokens near the refusal rules: each is
    /// refused, fails in C, or evaluates without a sanitizer report.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn random_expressions() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        const TOKENS: [&str; 24] = [
            "ld(",
            "st(",
            "random(",
            "randomi(",
            "taylor(",
            "print(",
            "gcd(",
            "bitand(",
            "bitor(",
            "root(",
            "1e300",
            "-1",
            "0",
            "2147483648",
            "nan",
            "inf",
            ",",
            ")",
            "(",
            "+",
            "*",
            "x",
            ";",
            "5",
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let x = |v: f64| v;
        let funcs1: [Func1<'_>; 1] = [(c"x", &x)];
        for _ in 0..20_000 {
            let n = 1 + next() % 8;
            let e: String = (0..n)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect();
            let e = CString::new(e).unwrap();
            // Quietly: most of these fail to parse.
            let _ = parse_and_eval(&e, &[(c"x" as &CStr, 1e300)], &funcs1, &[], 64, None);
        }
    }
}

/// `Expr::parse_with_context`: no parent context, a parent led by a class,
/// one dropped right after the parse (the tree keeps no pointer to it), and
/// parents parsing on several threads at once over one class, at level
/// offsets up to the overflow bound, on valid and invalid expressions.
mod parse_with_context {
    use core::ffi::{CStr, c_int, c_void};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::eval::{Expr, UnaryFn};
    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
    use ffmpeg_bindings::libavutil::libavutil::opt::{BareObject, ObjectRef};

    /// `AVERROR(EINVAL)`.
    const EINVAL: c_int = -22;

    /// The class every parent context here has.
    static CLASS: Class = Class::new(c"ub-eval-parent", ClassCategory::NA);

    /// Whether this run is under `BSan`, which misreports C's parser (see
    /// `expr::under_bsan`). The tests that parse return early under it.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan")
    }

    extern "C" fn twice(_: *mut c_void, x: f64) -> f64 {
        2.0 * x
    }

    /// The parses every context runs: valid and invalid expressions, at the
    /// offsets around the overflow bound. Offsets that keep the error level
    /// at or below the default log level make C log through the context.
    fn parses(ctx: Option<ObjectRef<'_>>) {
        let unary: [(&CStr, UnaryFn); 1] = [(c"f", twice)];
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        for offset in [c_int::MIN, -8, 0, 8, 40, max] {
            let e = Expr::parse_with_context(c"f(X) + 1", &[c"X"], &unary, &[], offset, ctx);
            drop(e.unwrap());
            for s in [c"f(", c"Y", c"nosuch(1)", c"1 )"] {
                let r = Expr::parse_with_context(s, &[c"X"], &unary, &[], offset, ctx);
                assert_eq!(r.err(), Some(EINVAL), "{s:?} at {offset}");
            }
        }
        // Rejected before C runs.
        for offset in [max + 1, c_int::MAX] {
            let r = Expr::parse_with_context(c"1", &[], &[], &[], offset, ctx);
            assert_eq!(r.err(), Some(EINVAL));
            let r = Expr::parse_with_context(c"1+", &[], &[], &[], offset, ctx);
            assert_eq!(r.err(), Some(EINVAL));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn no_context() {
        if under_bsan() {
            return;
        }
        parses(None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn context_with_a_class() {
        if under_bsan() {
            return;
        }
        let ctx = BareObject::new(CLASS.as_ref()).unwrap();
        parses(Some(ctx.as_object()));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn context_dropped_right_after_the_parse() {
        if under_bsan() {
            return;
        }
        let tree = {
            let ctx = Box::new(BareObject::new(CLASS.as_ref()).unwrap());
            let tree = Expr::parse_with_context(c"2*3", &[], &[], &[], 0, Some(ctx.as_object()));
            // This parse fails and logs through `ctx`.
            let failed = Expr::parse_with_context(c"2*", &[], &[], &[], 0, Some(ctx.as_object()));
            assert_eq!(failed.err(), Some(EINVAL));
            tree.unwrap()
        };
        // The tree outlives the context.
        drop(tree);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn contexts_on_many_threads() {
        if under_bsan() {
            return;
        }
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    let ctx = BareObject::new(CLASS.as_ref()).unwrap();
                    parses(Some(ctx.as_object()));
                });
            }
        });
    }
}

/// Options wave 1, batch 0: `Expression` and the tree counters through the
/// safe API: evaluation of every builtin the parse admits with constant
/// slices of every length the count allows (and a short one, which panics
/// before C reads it), registers and random state kept across evaluations,
/// opaque values of several types, counters of every size, and expressions
/// moved to and evaluated on other threads.
mod expression {
    use core::ffi::{CStr, c_void};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::thread;

    use ffmpeg_bindings::libavutil::libavutil::eval::{
        Arity, BinaryFn, ExprBox, Expression, UnaryFn,
    };

    /// As the other modules: `BSan` misreports C's number parse.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref() == Ok("bsan")
    }

    extern "C" fn plus_one(_: *mut c_void, x: f64) -> f64 {
        x + 1.0
    }

    extern "C" fn mul(_: *mut c_void, a: f64, b: f64) -> f64 {
        a * b
    }

    const NAMES: [&CStr; 4] = [c"A", c"B", c"C", c"D"];
    const UNARY: [(&CStr, UnaryFn); 1] = [(c"inc", plus_one)];
    const BINARY: [(&CStr, BinaryFn); 1] = [(c"mul", mul)];

    fn parse(s: &CStr) -> Result<Expression, i32> {
        Expression::parse(s, &NAMES, &UNARY, &BINARY, 40)
    }

    /// Expressions over every evaluator construct `Expression` accepts.
    const CORPUS: [&CStr; 12] = [
        c"A+B*C-D/2",
        c"inc(A) + mul(B, inc(C))",
        c"st(0, ld(0) + A); st(9, ld(9) - 1); ld(0) * ld(9)",
        c"random(3) + randomi(4, -10, 10)",
        c"while(lt(ld(1), 50), st(1, ld(1) + 1)) + ld(1)",
        c"taylor(ld(0) * 0 + A, 0.5, 2) + root(ld(7) * 0 + B - 3, 8)",
        c"gcd(48, 18) + bitand(12, 10) + bitor(1, 2) + print(A, 56)",
        c"squish(A) + gauss(B) + hypot(C, D) + atan2(A, B) + lerp(A, B, C)",
        c"if(gt(A, B), A, B) + ifnot(C, D) + between(A, 0, 1) + clip(A, B, C)",
        c"isnan(A) + isinf(B) + not(C) + sgn(D) + floor(A) + ceil(B) + trunc(C) + round(D)",
        c"exp(A) + log(abs(B)) + sqrt(abs(C)) + pow(abs(D), 0.5) + A^2",
        c"(A)",
    ];

    const VALUES: [[f64; 5]; 4] = [
        [1.0, 2.0, 3.0, 4.0, 5.0],
        [0.0, -0.0, f64::NAN, f64::INFINITY, -1.0],
        [1e300, -1e300, 1e-320, -5e-324, 0.5],
        [f64::MAX, f64::MIN, f64::EPSILON, -2.5, 7.0],
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_construct_evaluates_again_and_again() {
        if under_bsan() {
            return;
        }
        for s in CORPUS {
            let mut e = parse(s).unwrap_or_else(|err| panic!("{s:?}: {err}"));
            assert_eq!(e.constant_count(), 4);
            for values in &VALUES {
                for len in [4, 5] {
                    for _ in 0..3 {
                        let _ = e.eval(&values[..len]);
                    }
                }
                let mut token = [0u64; 4];
                let _ = e.eval_with(values, &mut token);
                let mut unit = ();
                let _ = e.eval_with(values, &mut unit);
            }
            let short = catch_unwind(AssertUnwindSafe(|| e.eval(&VALUES[0][..3])));
            assert!(short.is_err(), "{s:?}");
            // The tree survives the panic and evaluates on.
            let _ = e.eval(&VALUES[0]);
        }
    }

    #[test]
    fn refused_expressions_never_evaluate() {
        for s in [
            c"ld(A)",
            c"st(1e300, 1)",
            c"randomi(B, 0, 1)",
            c"st(0,1);random(0)",
            c"bitor(1e30, 1)",
        ] {
            assert_eq!(parse(s).err(), Some(-22), "{s:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn counters_of_every_size() {
        if under_bsan() {
            return;
        }
        for s in CORPUS {
            let e = parse(s).unwrap();
            let t = e.as_ref();
            for n in [0, 1, 2, 4, 100] {
                let mut counts = vec![u32::MAX - 3; n];
                let ok = t.count_constants(&mut counts).is_ok();
                assert_eq!(ok, n > 0);
                for arity in [Arity::Unary, Arity::Binary] {
                    let mut counts = vec![0; n];
                    assert_eq!(t.count_functions(arity, &mut counts).is_ok(), n > 0);
                    assert!(counts.iter().skip(1).all(|&c| c == 0));
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn expressions_move_between_threads() {
        if under_bsan() {
            return;
        }
        fn send<T: Send>() {}
        send::<Expression>();
        send::<Arity>();
        let exprs: Vec<Expression> = CORPUS.iter().map(|s| parse(s).unwrap()).collect();
        let back: Vec<Expression> = exprs
            .into_iter()
            .map(|mut e| {
                thread::spawn(move || {
                    for v in &VALUES {
                        let _ = e.eval(v);
                    }
                    e
                })
            })
            .map(|h| h.join().unwrap())
            .collect();
        // Trees given up for counting, released on yet another thread.
        let trees: Vec<ExprBox> = back.into_iter().map(Expression::into_tree).collect();
        thread::spawn(move || {
            for t in &trees {
                let mut c = [0; 4];
                t.as_ref().count_constants(&mut c).unwrap();
            }
            drop(trees);
        })
        .join()
        .unwrap();
        // Parsing and evaluating on many threads at once.
        thread::scope(|s| {
            for i in 0..8 {
                s.spawn(move || {
                    let mut e = parse(CORPUS[i % CORPUS.len()]).unwrap();
                    for v in &VALUES {
                        let _ = e.eval(v);
                    }
                });
            }
        });
    }
}
