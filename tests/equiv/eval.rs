//! Equivalence of `libavutil/eval.h`'s `parse_number` with the raw
//! `av_strtod`: the value (bit for bit, NaN included) and where C stopped.

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use ffmpeg::libavutil::eval::parse_number;
use ffmpeg::libavutil::ffi;

/// `av_strtod` in C: the value and the number of bytes it consumed.
fn c_strtod(s: &CStr) -> (f64, usize) {
    let mut tail: *mut c_char = core::ptr::null_mut();
    // SAFETY: `s` is NUL-terminated and `tail` a writable local.
    let d = unsafe { ffi::av_strtod(s.as_ptr(), &mut tail) };
    (d, tail as usize - s.as_ptr() as usize)
}

fn rust_strtod(s: &CStr) -> (f64, usize) {
    let (d, rest) = parse_number(s);
    (d, s.count_bytes() - rest.count_bytes())
}

#[track_caller]
fn agree(s: &CStr) {
    let (c, rust) = (c_strtod(s), rust_strtod(s));
    assert_eq!(
        (rust.0.to_bits(), rust.1),
        (c.0.to_bits(), c.1),
        "{s:?}: {rust:?} vs {c:?}"
    );
}

#[test]
fn listed_numbers_agree() {
    for s in [
        c"",
        c" ",
        c"0",
        c"-0",
        c"1",
        c"1.5",
        c"-1.5e3",
        c"1e400",
        c"-1e400",
        c"1e-400",
        c"nan",
        c"-nan",
        c"NaN(1)",
        c"inf",
        c"-Infinity",
        c"0x",
        c"0X",
        c"0x10",
        c"0X1F",
        c"0xg",
        c"0x1p4",
        c"0x-1",
        c"0x18446744073709551615",
        c"0x18446744073709551616",
        c"18446744073709551616",
        c"1dB",
        c"20dB",
        c"-6dB",
        c"1dBB",
        c"1d",
        c"1D",
        c"1y",
        c"1z",
        c"1a",
        c"1f",
        c"1p",
        c"1n",
        c"1u",
        c"1m",
        c"1c",
        c"1d",
        c"1h",
        c"1k",
        c"1K",
        c"1M",
        c"1G",
        c"1T",
        c"1P",
        c"1E",
        c"1Z",
        c"1Y",
        c"1Ki",
        c"1Mi",
        c"1Gi",
        c"1Ti",
        c"1Pi",
        c"1Ei",
        c"1Zi",
        c"1Yi",
        c"1mi",
        c"1ui",
        c"1KB",
        c"1KiB",
        c"1B",
        c"1BB",
        c"1i",
        c"1q",
        c"1F",
        c"1e",
        c"1e+",
        c"1.5Gi rest",
        c".",
        c"+.5k",
        c"  -.5M",
        c"k",
        c"B",
        c"dB",
        c"\x7f",
        c"\xff",
        c"1\xc3\xa9",
        c"1[",
        c"1`",
        c"1{",
        c"1z!",
        c"0xffK",
        c"0x10KiB",
    ] {
        agree(s);
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

/// Every string of up to four bytes over the bytes that steer C's number
/// and suffix handling.
#[test]
fn short_strings_agree() {
    if under_bsan() {
        return;
    }
    let alphabet = b"01x.-e9dBEKiMmz Y";
    let mut s = Vec::new();
    for len in 0..=4u32 {
        for mut i in 0..alphabet.len().pow(len) {
            s.clear();
            for _ in 0..len {
                s.push(alphabet[i % alphabet.len()]);
                i /= alphabet.len();
            }
            agree(&CString::new(s.as_slice()).unwrap());
        }
    }
}

/// Every byte from `'E'` to `'z'` after a number, alone and before `i` and
/// `B`: C indexes its SI table with it.
#[test]
fn every_suffix_byte_agrees() {
    if under_bsan() {
        return;
    }
    for b in 1..=255u8 {
        for tail in [&b""[..], b"i", b"B", b"iB", b"x"] {
            let s = CString::new([&b"3"[..], &[b], tail].concat()).unwrap();
            agree(&s);
        }
    }
}

/// Equivalence of `libavutil/eval.h`'s owned expressions with the raw C
/// parser: every expression is parsed once by `av_expr_parse` on its own
/// copies of the names and once by `Expr::parse`, and both trees are then
/// observed through C's evaluator and counters, step by step.
mod expr {
    use core::cell::RefCell;
    use core::ffi::{CStr, c_int, c_uint, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::eval::{BinaryFn, Expr, UnaryFn};
    use ffmpeg::libavutil::ffi;

    /// `AVERROR(EINVAL)`.
    const EINVAL: c_int = -22;

    /// A level offset that moves C's parse errors (`AV_LOG_ERROR`) below the
    /// default log level, so the bulk runs stay quiet.
    const QUIET: c_int = 40;

    /// The opaque pointer every evaluation passes; never dereferenced.
    const OPAQUE: usize = 0x5eed;

    /// One call of a user function, as it reached the function.
    #[derive(Debug, PartialEq)]
    enum Call {
        Unary(u8, usize, u64),
        Binary(u8, usize, u64, u64),
    }

    thread_local! {
        /// The calls user functions received on this thread.
        static CALLS: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    }

    fn record(call: Call) {
        CALLS.with(|c| c.borrow_mut().push(call));
    }

    extern "C" fn twice(opaque: *mut c_void, x: f64) -> f64 {
        record(Call::Unary(0, opaque as usize, x.to_bits()));
        2.0 * x
    }

    extern "C" fn negate(opaque: *mut c_void, x: f64) -> f64 {
        record(Call::Unary(1, opaque as usize, x.to_bits()));
        -x
    }

    extern "C" fn minus(opaque: *mut c_void, a: f64, b: f64) -> f64 {
        record(Call::Binary(0, opaque as usize, a.to_bits(), b.to_bits()));
        a - b
    }

    extern "C" fn hyp(opaque: *mut c_void, a: f64, b: f64) -> f64 {
        record(Call::Binary(1, opaque as usize, a.to_bits(), b.to_bits()));
        a.hypot(b)
    }

    /// The names and functions the corpus parses with.
    const CONSTS: [&CStr; 3] = [c"X", c"Y", c"XY"];
    const UNARY: [(&CStr, UnaryFn); 2] = [(c"f", twice), (c"neg", negate)];
    const BINARY: [(&CStr, BinaryFn); 2] = [(c"g", minus), (c"hyp", hyp)];

    /// A tree C parsed itself, released by `av_expr_free`.
    struct CExpr(*mut ffi::AVExpr);

    impl Drop for CExpr {
        fn drop(&mut self) {
            // SAFETY: a tree `av_expr_parse` handed us, released once.
            unsafe { ffi::av_expr_free(self.0) }
        }
    }

    /// `av_expr_parse` on its own copies of every name, dropped as soon as it
    /// returns: C reads them only during the call.
    fn c_parse(
        s: &CStr,
        consts: &[&CStr],
        unary: &[(&CStr, UnaryFn)],
        binary: &[(&CStr, BinaryFn)],
        log_offset: c_int,
    ) -> Result<CExpr, c_int> {
        fn array(names: impl Iterator<Item = CString>) -> (Vec<CString>, Vec<*const i8>) {
            let owned: Vec<CString> = names.collect();
            let ptrs = owned
                .iter()
                .map(|n| n.as_ptr())
                .chain([ptr::null()])
                .collect();
            (owned, ptrs)
        }
        let (_c, c_ptrs) = array(consts.iter().map(|&n| n.to_owned()));
        let (_u, u_ptrs) = array(unary.iter().map(|(n, _)| (*n).to_owned()));
        let (_b, b_ptrs) = array(binary.iter().map(|(n, _)| (*n).to_owned()));
        let u_fns: Vec<_> = unary
            .iter()
            .map(|&(_, f)| Some(f as _))
            .chain([None])
            .collect();
        let b_fns: Vec<_> = binary
            .iter()
            .map(|&(_, f)| Some(f as _))
            .chain([None])
            .collect();
        let s = s.to_owned();
        let mut e = ptr::null_mut();
        // SAFETY: a writable slot; NUL-terminated strings in NULL-terminated
        // arrays, function arrays as long as their name arrays, all live for the
        // call; no log context.
        let ret = unsafe {
            ffi::av_expr_parse(
                &mut e,
                s.as_ptr(),
                c_ptrs.as_ptr(),
                u_ptrs.as_ptr(),
                u_fns.as_ptr(),
                b_ptrs.as_ptr(),
                b_fns.as_ptr(),
                log_offset,
                ptr::null_mut(),
            )
        };
        if ret < 0 {
            assert!(e.is_null(), "C left a tree on failure");
            Err(ret)
        } else {
            assert!(!e.is_null());
            Ok(CExpr(e))
        }
    }

    /// What C can observe of a tree: its variable and function counts, then one
    /// value and the user calls it made per evaluation.
    #[derive(Debug, PartialEq)]
    struct Observed {
        vars: Vec<c_uint>,
        unary: Vec<c_uint>,
        binary: Vec<c_uint>,
        steps: Vec<(u64, Vec<Call>)>,
    }

    /// Observes the tree at `e` through C, evaluating it `steps` times with
    /// `values` (one per constant name).
    ///
    /// # Safety
    ///
    /// `e` must be a live root that only names constants and functions of
    /// `CONSTS`, `UNARY` and `BINARY`, and evaluates without undefined behaviour
    /// or endless loops.
    unsafe fn observe(e: *mut ffi::AVExpr, values: &[f64; 3], steps: usize) -> Observed {
        let mut vars = vec![0; CONSTS.len()];
        let mut unary = vec![0; UNARY.len()];
        let mut binary = vec![0; BINARY.len()];
        // SAFETY: zeroed counters of the sizes passed, and a live tree.
        unsafe {
            assert_eq!(
                ffi::av_expr_count_vars(e, vars.as_mut_ptr(), vars.len() as c_int),
                0
            );
            assert_eq!(ffi::av_expr_count_func(e, unary.as_mut_ptr(), 2, 1), 0);
            assert_eq!(ffi::av_expr_count_func(e, binary.as_mut_ptr(), 2, 2), 0);
        }
        let steps = (0..steps)
            .map(|_| {
                CALLS.with(|c| c.borrow_mut().clear());
                // SAFETY: `values` covers every constant the tree names, and the
                // caller vouches for the evaluation.
                let v = unsafe { ffi::av_expr_eval(e, values.as_ptr(), OPAQUE as *mut c_void) };
                (v.to_bits(), CALLS.with(|c| c.take()))
            })
            .collect();
        Observed {
            vars,
            unary,
            binary,
            steps,
        }
    }

    /// Parses `s` both ways and checks that the outcomes agree, then that the
    /// trees evaluate alike for each of `inputs`, three steps each.
    fn check(s: &CStr, log_offset: c_int, inputs: &[[f64; 3]]) -> Result<(), c_int> {
        let c = c_parse(s, &CONSTS, &UNARY, &BINARY, log_offset);
        let r = Expr::parse(s, &CONSTS, &UNARY, &BINARY, log_offset);
        match (&c, &r) {
            (Err(a), Err(b)) => {
                assert_eq!(a, b, "{s:?}");
                Err(*a)
            }
            (Ok(c), Ok(r)) => {
                for values in inputs {
                    let r = r.as_c_ptr();
                    // SAFETY: per each caller, `s` only names the corpus' names
                    // and evaluates without UB; both trees are live.
                    let (a, b) = unsafe { (observe(c.0, values, 3), observe(r, values, 3)) };
                    assert_eq!(a, b, "{s:?} with {values:?}");
                }
                Ok(())
            }
            _ => panic!(
                "{s:?}: C gave {:?}, Rust {:?}",
                c.as_ref().err(),
                r.as_ref().err()
            ),
        }
    }

    const INPUTS: [[f64; 3]; 4] = [
        [3.0, 4.0, 5.0],
        [0.0, -0.0, 1e-300],
        [f64::INFINITY, f64::NAN, -1.5],
        [-7.25, 1e300, 2.0],
    ];

    #[test]
    fn valid_expressions_parse_and_evaluate_alike() {
        if under_bsan() {
            return;
        }
        for s in [
            c"1+2^3+5*5+sin(2/3)",
            c"X*Y - XY",
            c" X  *  Y ",
            c"XY",
            c"f(X) + neg(Y)",
            c"g(X, Y) * hyp(Y, XY)",
            c"f(f(f(X)))",
            c"g(f(X), neg(g(Y, 1)))",
            c"PI * E + PHI - QP2LAMBDA",
            c"-X + Y - (-Y)",
            c"1 2",
            c"X Y",
            c"1K + 2Mi + 3B + 4KiB + 6dB",
            c"0x10 + 1e3 + 2.5e-1",
            c"max(X, Y) + min(X, Y) + hypot(X, Y) + atan2(Y, X)",
            c"if(gt(X, Y), X, Y) + ifnot(eq(X, Y), 1, 2)",
            c"between(X, 0, 10) + clip(X, -1, 1) + lerp(X, Y, 0.5)",
            c"floor(X) + ceil(Y) + trunc(XY) + round(X) + sgn(Y)",
            c"abs(X) + sqrt(abs(Y)) + exp(-abs(XY)) + log(abs(X) + 1)",
            c"isnan(Y) + isinf(X) + not(X) + squish(X) + gauss(Y)",
            c"pow(abs(X), 0.5) + X^2",
            c"(((X)))",
            c"st(0, X); st(1, ld(0) + Y); ld(1) * 2",
            c"st(2, ld(2) + 1)",
            c"st(3, ld(3) + f(X)); ld(3)",
            c"random(0)",
            c"st(4, 42); random(4)",
            c"randomi(5, 10, 20)",
            c"X; Y; XY",
            c"taylor(1, 1)",
            c"1/0 + -1/0",
        ] {
            check(s, 0, &INPUTS).unwrap();
        }
    }

    #[test]
    fn invalid_expressions_fail_alike() {
        if under_bsan() {
            return;
        }
        for s in [
            c"",
            c" ",
            c"1+",
            c"(1",
            c"1)",
            c"unknown",
            c"f()",
            c"nosuch(1)",
            c"g(1, 2, 3, 4)",
            c"max(1)",
            c"sin(1, 2)",
            c"f(X",
            c"Z",
            c"x",
            c",",
            c"st(0)",
            c"-X + Y - -Y",
            c"1 +* 2",
        ] {
            assert_eq!(check(s, QUIET, &INPUTS), Err(EINVAL), "{s:?}");
        }
        // Nesting is bounded: 50 levels parse, 101 do not.
        let nested =
            |n: usize| CString::new(format!("{}1{}", "(".repeat(n), ")".repeat(n))).unwrap();
        check(&nested(50), 0, &INPUTS).unwrap();
        assert_eq!(check(&nested(101), QUIET, &INPUTS), Err(EINVAL));
        let calls =
            |n: usize| CString::new(format!("{}X{}", "f(".repeat(n), ")".repeat(n))).unwrap();
        check(&calls(20), 0, &INPUTS).unwrap();
        assert_eq!(check(&calls(101), QUIET, &INPUTS), Err(EINVAL));
        // An operator chain deeper than 100 tree levels is refused as if out of
        // memory.
        let enomem = Err(-(ffi::ENOMEM as c_int));
        for op in ["+", "-", "*", "/", "^", ";"] {
            let chain = |n: usize| CString::new(vec!["X"; n].join(op)).unwrap();
            check(&chain(100), 0, &INPUTS).unwrap();
            assert_eq!(check(&chain(102), QUIET, &INPUTS), enomem, "{op}");
        }
        // A call adds a level too: around a 100-level chain it is one too many,
        // which C reports as malformed.
        let chain = vec!["X"; 101].join("*");
        let s = CString::new(format!("sin({chain})")).unwrap();
        assert_eq!(check(&s, QUIET, &INPUTS), Err(EINVAL));
        let s = CString::new(format!("({chain})")).unwrap();
        check(&s, 0, &INPUTS).unwrap();
    }

    #[test]
    fn names_match_at_their_first_position() {
        if under_bsan() {
            return;
        }
        // A repeated name binds the first entry; a longer name is not shadowed
        // by its prefix.
        let consts = [c"A", c"A", c"AB"];
        let unary: [(&CStr, UnaryFn); 2] = [(c"f", twice), (c"f", negate)];
        let binary: [(&CStr, BinaryFn); 2] = [(c"g", hyp), (c"g", minus)];
        for s in [c"A + AB", c"f(A)", c"g(A, AB)", c"AB*A"] {
            let c = c_parse(s, &consts, &unary, &binary, 0).unwrap();
            let r = Expr::parse(s, &consts, &unary, &binary, 0).unwrap();
            // SAFETY: both trees only name the lists above (counted within the
            // corpus' sizes) and evaluate without UB.
            let (a, b) = unsafe {
                (
                    observe(c.0, &[1.0, 2.0, 3.0], 2),
                    observe(r.as_c_ptr(), &[1.0, 2.0, 3.0], 2),
                )
            };
            assert_eq!(a, b, "{s:?}");
        }
        // Empty lists name nothing, but the evaluator's own names remain.
        let c = c_parse(c"PI + sin(1)", &[], &[], &[], 0).unwrap();
        let r = Expr::parse(c"PI + sin(1)", &[], &[], &[], 0).unwrap();
        // SAFETY: as above, with no user names.
        let (a, b) = unsafe {
            (
                observe(c.0, &[0.0; 3], 2),
                observe(r.as_c_ptr(), &[0.0; 3], 2),
            )
        };
        assert_eq!(a, b);
        for s in [c"X", c"f(1)", c"g(1, 2)"] {
            assert_eq!(c_parse(s, &[], &[], &[], QUIET).err(), Some(EINVAL));
            assert_eq!(Expr::parse(s, &[], &[], &[], QUIET).err(), Some(EINVAL));
        }
    }

    #[test]
    fn names_need_not_outlive_the_parse() {
        if under_bsan() {
            return;
        }
        let owned: Vec<CString> = ["first", "second"]
            .iter()
            .map(|n| CString::new(*n).unwrap())
            .collect();
        let fname = CString::new("dbl").unwrap();
        let e = {
            let consts: Vec<&CStr> = owned.iter().map(CString::as_c_str).collect();
            Expr::parse(c"second * dbl(first)", &consts, &[(&fname, twice)], &[], 0).unwrap()
        };
        drop(owned);
        drop(fname);
        CALLS.with(|c| c.borrow_mut().clear());
        // SAFETY: a live root naming two constants and a function ignoring
        // `opaque`.
        let v = unsafe { ffi::av_expr_eval(e.as_c_ptr(), [3.0, 5.0].as_ptr(), ptr::null_mut()) };
        assert_eq!(v, 30.0);
        assert_eq!(
            CALLS.with(|c| c.take()),
            [Call::Unary(0, 0, 3f64.to_bits())]
        );
    }

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

    /// A tiny xorshift generator, so the random corpus is reproducible.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn pick<T: Copy>(&mut self, items: &[T]) -> T {
            items[(self.next() % items.len() as u64) as usize]
        }
    }

    /// A random expression over the corpus' names and the evaluator functions
    /// that convert no value to an integer (those are undefined in C for
    /// out-of-range values, a defect of evaluation, not of parsing).
    fn generate(rng: &mut Rng, depth: u32, out: &mut String) {
        const ATOMS: &[&str] = &[
            "1", "0", "-3", "2.5", "1e308", "1e-320", "0x7f", "X", "Y", "XY", "PI", "E", "1K",
            "2Mi", "3B", "5dB", "10%",
        ];
        const UNARY_FNS: &[&str] = &[
            "sin", "cos", "exp", "log", "abs", "sqrt", "floor", "ceil", "trunc", "round", "not",
            "isnan", "isinf", "sgn", "squish", "gauss", "f", "neg",
        ];
        const BINARY_FNS: &[&str] = &[
            "max", "min", "eq", "gte", "gt", "lte", "lt", "hypot", "atan2", "pow", "if", "ifnot",
            "g", "hyp",
        ];
        const TERNARY_FNS: &[&str] = &["if", "ifnot", "between", "clip", "lerp"];
        const OPS: &[&str] = &["+", "-", "*", "/", "^", ";"];
        let choice = if depth > 5 { 0 } else { rng.next() % 6 };
        let call = |rng: &mut Rng, name: &str, n: usize, out: &mut String| {
            out.push_str(name);
            out.push('(');
            for i in 0..n {
                if i > 0 {
                    out.push(',');
                }
                generate(rng, depth + 1, out);
            }
            out.push(')');
        };
        match choice {
            0 | 1 => out.push_str(rng.pick(ATOMS)),
            2 => {
                let name = rng.pick(UNARY_FNS);
                call(rng, name, 1, out)
            }
            3 => {
                let name = rng.pick(BINARY_FNS);
                call(rng, name, 2, out)
            }
            4 => {
                let name = rng.pick(TERNARY_FNS);
                call(rng, name, 3, out)
            }
            _ => {
                generate(rng, depth + 1, out);
                out.push_str(rng.pick(OPS));
                generate(rng, depth + 1, out);
            }
        }
    }

    /// Random expressions and random corruptions of them (a byte deleted,
    /// duplicated or replaced), which mostly fail to parse.
    #[test]
    fn random_expressions_parse_and_evaluate_alike() {
        if under_bsan() {
            return;
        }
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let (mut parsed, mut failed) = (0, 0);
        for round in 0..1500 {
            let mut s = String::new();
            generate(&mut rng, 0, &mut s);
            if round % 3 == 2 && !s.is_empty() {
                let i = (rng.next() % s.len() as u64) as usize;
                let mut bytes = s.into_bytes();
                match rng.next() % 3 {
                    0 => {
                        bytes.remove(i);
                    }
                    1 => bytes.insert(i, bytes[i]),
                    _ => bytes[i] = rng.pick(b"()+,*XfZ1 "),
                }
                s = String::from_utf8(bytes).unwrap();
            }
            let s = CString::new(s).unwrap();
            // Corruptions keep to the safe vocabulary: no byte spells a new
            // function name out of the generated ones, so evaluation stays
            // defined.
            match check(&s, QUIET, &INPUTS[..2]) {
                Ok(()) => parsed += 1,
                Err(_) => failed += 1,
            }
        }
        assert!(
            parsed > 500 && failed > 100,
            "{parsed} parsed, {failed} failed"
        );
    }

    /// The `av_max_alloc` limit is process-global, so this test lowers it only in
    /// a child process running nothing else, and passes when the child does.
    #[test]
    fn allocation_failures_match_c() {
        if under_bsan() {
            return;
        }
        const ISOLATED: &str = "CRUSTIFY_EQUIV_EVAL_ISOLATED";
        if std::env::var_os(ISOLATED).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "eval::expr::allocation_failures_match_c"])
                .args(["--test-threads=1", "--nocapture"])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success(), "isolated run failed: {status}");
            return;
        }
        let long = CString::new("1+".repeat(90) + "1").unwrap();
        let mut outcomes = Vec::new();
        // Limits below every allocation, between them, and above them: the
        // whitespace-free copy of the text, the nodes, the root's registers.
        for limit in [0, 1, 8, 16, 32, 64, 80, 128, 160, 181, 182, 256, 1024, 4096] {
            ffmpeg::libavutil::mem::set_max_alloc(limit);
            for s in [c"1", c"X*f(Y)", c"st(0, 1); random(0)", long.as_c_str()] {
                let c = c_parse(s, &CONSTS, &UNARY, &BINARY, QUIET).map(drop);
                let r = Expr::parse(s, &CONSTS, &UNARY, &BINARY, QUIET).map(drop);
                assert_eq!(c, r, "{s:?} at {limit}");
                outcomes.push(r);
            }
        }
        ffmpeg::libavutil::mem::set_max_alloc(c_int::MAX as usize);
        let enomem = -(ffi::ENOMEM as c_int);
        assert!(outcomes.contains(&Err(enomem)) && outcomes.contains(&Ok(())));
    }
}

/// Options wave 0, batch 3.
///
/// Equivalence of `libavutil/eval.h`'s `parse_and_eval` with the raw C
/// evaluator: values and error codes over expressions with constants and
/// functions, the Rust closures against equivalent C functions.
mod expressions {
    use core::ffi::{CStr, c_char, c_void};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::eval::{Func1, Func2, parse_and_eval};
    use ffmpeg::libavutil::ffi;

    unsafe extern "C" fn c_square(_: *mut c_void, x: f64) -> f64 {
        x * x
    }

    unsafe extern "C" fn c_half(_: *mut c_void, x: f64) -> f64 {
        x / 2.0
    }

    unsafe extern "C" fn c_hyp(_: *mut c_void, x: f64, y: f64) -> f64 {
        x.hypot(y)
    }

    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    const CONSTS: [(&CStr, f64); 3] = [(c"A", 3.0), (c"B", -0.5), (c"W", 1920.0)];

    /// The raw C evaluation of `e` with [`CONSTS`], `square`, `half` and `hyp`.
    fn c_eval(e: &CStr, log_offset: i32) -> (i32, f64) {
        let names: Vec<*const c_char> = CONSTS
            .iter()
            .map(|c| c.0.as_ptr())
            .chain([ptr::null()])
            .collect();
        let values: Vec<f64> = CONSTS.iter().map(|c| c.1).collect();
        let f1_names = [c"square".as_ptr(), c"half".as_ptr(), ptr::null()];
        let f1: [Option<unsafe extern "C" fn(*mut c_void, f64) -> f64>; 3] =
            [Some(c_square), Some(c_half), None];
        let f2_names = [c"hyp".as_ptr(), ptr::null()];
        let f2: [Option<unsafe extern "C" fn(*mut c_void, f64, f64) -> f64>; 2] =
            [Some(c_hyp), None];
        let mut d = 0.0;
        // SAFETY: NULL-terminated name arrays with matching value and function
        // arrays, live strings; callers pass only expressions free of C's
        // unchecked conversions.
        let ret = unsafe {
            ffi::av_expr_parse_and_eval(
                &mut d,
                e.as_ptr(),
                names.as_ptr(),
                values.as_ptr(),
                f1_names.as_ptr(),
                f1.as_ptr(),
                f2_names.as_ptr(),
                f2.as_ptr(),
                ptr::null_mut(),
                log_offset,
                ptr::null_mut(),
            )
        };
        (ret, d)
    }

    /// The same evaluation through the safe API.
    fn rust_eval(e: &CStr, log_offset: i32) -> (i32, f64) {
        let square = |x: f64| x * x;
        let half = |x: f64| x / 2.0;
        let hyp = |x: f64, y: f64| x.hypot(y);
        let funcs1: [Func1<'_>; 2] = [(c"square", &square), (c"half", &half)];
        let funcs2: [Func2<'_>; 1] = [(c"hyp", &hyp)];
        match parse_and_eval(e, &CONSTS, &funcs1, &funcs2, log_offset, None) {
            Ok(v) => (0, v),
            Err(err) => (err, f64::NAN),
        }
    }

    #[track_caller]
    fn assert_same(e: &CStr, log_offset: i32) {
        let (c, r) = (c_eval(e, log_offset), rust_eval(e, log_offset));
        assert_eq!(c.0, r.0, "{e:?}");
        assert!(
            c.1.to_bits() == r.1.to_bits() || (c.1.is_nan() && r.1.is_nan()),
            "{e:?}: {c:?} {r:?}"
        );
    }

    #[test]
    fn expressions_evaluate_as_in_c() {
        for e in [
            c"1+2^3+5*5+sin(2/3)",
            c"A*W+B",
            c"square(A)+half(W)+hyp(A,4)",
            c"square(half(hyp(3,4)))",
            c" 1 k + 2Mi + 3dB - -4 ",
            c"-3dB",
            c"0x1F+010",
            c"PI*E*PHI+QP2LAMBDA",
            c"st(0,A);st(1,ld(0)*2);ld(1)",
            c"gcd(12,18)+bitand(12,10)+bitor(1,2)",
            c"random(0)+random(0)+randomi(1,0,100)",
            c"taylor(ld(0)+1,1,2)",
            c"root(ld(0)^2-W,100)",
            // No sign change in [0, 1]: `root` then widens around its best
            // point (the second search phase).
            c"root(1.5-ld(0),1)",
            c"if(lt(A,B),1,2)+ifnot(0,3)+between(A,0,4)+clip(9,0,4)",
            c"mod(7,3)+max(A,B)+min(A,B)+trunc(-2.5)+round(2.5)+floor(-1.5)+ceil(1.2)",
            c"lerp(0,10,0.25)+sgn(-3)+not(0)+isnan(0)+isinf(1/0)+squish(1)+gauss(0)",
            c"atan2(1,1)+hypot(3,4)+sqrt(16)+abs(-1)+log(E)+exp(0)",
            c"1/0",
            c"-1/0",
            c"0/0",
            c"sqrt(-1)",
            c"",
            c"1+",
            c"nosuch",
            c"square(1,2)",
            c"hyp(1)",
            c"(((1)))",
            c"1;2;3",
            c"print(A)",
            c"print(A,-8)",
        ] {
            assert_same(e, 0);
            assert_same(e, 64);
            assert_same(e, -100);
        }
    }

    #[test]
    fn random_arithmetic_evaluates_as_in_c() {
        // BSan reports C's reads through the end pointers glibc's uninstrumented
        // `strtod` / `strtol` / `strtoul` store (`eval.c:120`, `parseutils.c:391`)
        // as accesses of freed or foreign allocations, depending on earlier
        // frames (the shared recipes' stale-shadow class); the parses here are
        // C's own, so BSan covers these paths in the typed-setter tests instead.
        if under_bsan() {
            return;
        }
        const TOKENS: [&str; 20] = [
            "A", "B", "W", "1", "2.5", "0", "+", "-", "*", "/", "^", "(", ")", "square(", "half(",
            "hyp(", ",", "max(", "1e300", "PI",
        ];
        let mut state = 0x1234_5678_9abc_def1u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..5_000 {
            let n = 1 + next() % 10;
            let e: String = (0..n)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect();
            assert_same(&CString::new(e).unwrap(), 64);
        }
    }
}

/// Equivalence of `Expr::parse_with_context` with `av_expr_parse` given the
/// same parent log context: the outcome, and every message C logs, with its
/// level, class and formatted line (context addresses masked: C's own `Eval`
/// context lives on its stack). The C reference also runs with a parent led
/// by a NULL class, which C treats as no parent, as the wrapper's `None`.
mod parse_with_context {
    use core::cell::RefCell;
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr;

    use ffmpeg::libavutil::eval::Expr;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{self, Callback, Record};
    use ffmpeg::libavutil::opt::ObjectRef;

    use crate::log::LogState;

    type Line = (c_int, Option<String>, String);

    thread_local! {
        static LINES: RefCell<Option<Vec<Line>>> = const { RefCell::new(None) };
    }

    /// `line` with every context address (` @ 0x...`) masked.
    fn mask_addresses(line: &str) -> String {
        let mut out = String::new();
        let mut rest = line;
        while let Some(at) = rest.find(" @ 0x") {
            out.push_str(&rest[..at]);
            out.push_str(" @ ADDR");
            rest = rest[at + 5..].trim_start_matches(|c: char| c.is_ascii_hexdigit());
        }
        out.push_str(rest);
        out
    }

    /// Captures this thread's messages while [`captured`] runs.
    struct Capture;

    impl Callback for Capture {
        fn log(record: Record<'_>) {
            LINES.with(|l| {
                if let Some(lines) = l.borrow_mut().as_mut() {
                    let mut prefix = true;
                    let line = record.to_line(&mut prefix).unwrap().into_string().unwrap();
                    let class = record
                        .class()
                        .and_then(|c| c.name())
                        .map(|n| n.to_string_lossy().into_owned());
                    lines.push((record.level().raw(), class, mask_addresses(&line)));
                }
            });
        }
    }

    /// The result and the lines `f` logs on this thread. The caller holds
    /// the [`LogState`] lock.
    fn captured(f: impl FnOnce() -> Result<(), c_int>) -> (Result<(), c_int>, Vec<Line>) {
        LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
        log::set_callback::<Capture>();
        let r = f();
        log::set_default_callback();
        (r, LINES.with(|l| l.borrow_mut().take().unwrap()))
    }

    /// Runs the test `name` alone in a child process, so that no other test
    /// logs through the callback it installs; true in that child.
    fn isolated(name: &str) -> bool {
        const KEY: &str = "CRUSTIFY_EQUIV_EVAL_PARSE_WITH_CONTEXT_ISOLATED";
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

    /// A logging context: its class first, as every C context has.
    #[repr(C)]
    struct Ctx {
        class: *const ffi::AVClass,
    }

    /// `av_expr_parse` of `s` with no names, `log_offset` and the parent
    /// `ctx`; the tree it builds is freed.
    fn c_parse(s: &CStr, log_offset: c_int, ctx: *mut c_void) -> Result<(), c_int> {
        let mut e = ptr::null_mut();
        // SAFETY: a writable slot, a literal, no names, and NULL or a live
        // context led by NULL or a valid class.
        let ret = unsafe {
            ffi::av_expr_parse(
                &mut e,
                s.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                log_offset,
                ctx,
            )
        };
        // SAFETY: NULL or the tree C just built, released once.
        unsafe { ffi::av_expr_free(e) };
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    #[test]
    fn parses_log_as_c_under_each_parent() {
        if super::under_bsan()
            || !isolated("eval::parse_with_context::parses_log_as_c_under_each_parent")
        {
            return;
        }
        let _state = LogState::lock();
        let class = ffi::AVClass {
            class_name: c"equiv-eval-parent".as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            version: (ffi::LIBAVUTIL_VERSION_MAJOR << 16
                | ffi::LIBAVUTIL_VERSION_MINOR << 8
                | ffi::LIBAVUTIL_VERSION_MICRO) as c_int,
            ..Default::default()
        };
        let mut parent = Ctx { class: &class };
        let parent: *mut c_void = (&raw mut parent).cast();
        let mut classless = Ctx { class: ptr::null() };
        let classless: *mut c_void = (&raw mut classless).cast();
        // SAFETY: a live context led by a valid class with no options, which
        // nothing writes during the test.
        let handle = unsafe { ObjectRef::from_ptr(parent) }.unwrap();
        // SAFETY: as above; a NULL class makes no handle.
        assert!(unsafe { ObjectRef::from_ptr(classless) }.is_none());
        for (c_ctx, rust_ctx) in [
            (ptr::null_mut(), None),
            (classless, None),
            (parent, Some(handle)),
        ] {
            for offset in [c_int::MIN, -16, -8, 0, 8, 16, 40] {
                for s in [c"2*3", c"2*", c"Y", c"nosuch(1)", c"1 )", c"(1", c"1+"] {
                    let c = captured(|| c_parse(s, offset, c_ctx));
                    let rust = captured(|| {
                        Expr::parse_with_context(s, &[], &[], &[], offset, rust_ctx).map(drop)
                    });
                    assert_eq!(rust, c, "{s:?} at {offset}");
                    // Errors at or below the default level reach the callback,
                    // prefixed by the parent when there is one.
                    if c.0.is_err() && offset <= 16 {
                        assert!(!c.1.is_empty(), "{s:?} at {offset}: nothing logged");
                        let named =
                            c.1.iter()
                                .all(|l| l.2.contains("[equiv-eval-parent @ ADDR]"));
                        assert_eq!(named, rust_ctx.is_some(), "{:?}", c.1);
                    }
                }
            }
        }
    }
}
