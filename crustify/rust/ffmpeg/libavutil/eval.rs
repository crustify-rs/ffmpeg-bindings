//! Safe wrappers over `libavutil/eval.h`.
//!
//! [`parse_number`] reads a number with FFmpeg's SI, binary and `dB`
//! suffixes (`av_strtod`).
//!
//! [`Expr`] is a parsed arithmetic expression (`AVExpr`), opaque outside
//! `eval.c`: a tree of `av_malloc` nodes whose root also owns the
//! expression's `st()`/`ld()` registers and random-number state. It is owned
//! as an [`ExprBox`], built by [`Expr::parse`] (or
//! [`Expr::parse_with_context`]) and released with its whole tree by
//! `av_expr_free`. C has no copy routine, so the box is not `Clone`.
//!
//! Parsing reads the constant and function names only during the call: a
//! node records the *position* of the constant it names, and a copy of the
//! function pointer it calls. Evaluation therefore needs one value for each
//! constant position the expression uses, which the expression cannot report
//! (`AVExpr` keeps no count), so a caller that evaluates must keep the
//! constant count it parsed with.
//!
//! Raw seams remain where the dependency has no safe handle: the parent log
//! context of [`Expr::parse_with_context`], and the opaque pointer a user
//! function receives from the evaluator.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use ffibox::{CBox, define_ctype, impl_cdrop};

use super::ffi;
use super::opt::AVERROR_BUG;

/// Wraps: av_strtod
///
/// The number at the start of `numstr`, and the rest of `numstr` after it.
///
/// The number is what `strtod` reads (decimal or hexadecimal floats,
/// `inf`, `nan`), or, after `0x` or `0X`, a hexadecimal integer as `strtoul`
/// reads it. A suffix may follow: `dB` turns the value from decibels into a
/// ratio (`10^(d / 20)`); one of the SI prefixes `y z a f p n u m c d h k K M
/// G T P E Z Y` scales it by its power of 10, or of 1024 with an `i` after
/// it (`Ki`, `Mi`, ...); then a `B` multiplies it by 8.
///
/// When nothing parses (an empty string, only whitespace, no number), the
/// value is 0 and the rest is all of `numstr`.
///
/// ```
/// use ffmpeg::libavutil::eval::parse_number;
/// assert_eq!(parse_number(c"1.5KiB, more"), (12288.0, c", more"));
/// assert_eq!(parse_number(c"none"), (0.0, c"none"));
/// ```
pub fn parse_number(numstr: &CStr) -> (f64, &CStr) {
    let start = numstr.as_ptr();
    let mut tail: *mut c_char = ptr::null_mut();
    // SAFETY: `numstr` is a live NUL-terminated string that C only reads (a
    // suffix byte only after a non-NUL one), and `tail` a writable local, in
    // which C stores the address of the first byte of `numstr` it did not
    // consume.
    let value = unsafe { ffi::av_strtod(start, &mut tail) };
    let bytes = numstr.to_bytes_with_nul();
    let parsed = (tail as usize).wrapping_sub(start as usize);
    assert!(parsed < bytes.len(), "av_strtod left the string");
    let rest = CStr::from_bytes_with_nul(&bytes[parsed..]).expect("the suffix keeps the NUL");
    (value, rest)
}

/// `AVERROR(EINVAL)`.
const AVERROR_EINVAL: c_int = -(ffi::EINVAL as c_int);

define_ctype!(
    /// Wraps: AVExpr
    ///
    /// A parsed expression, owned as an [`ExprBox`]. Its layout is private
    /// to `eval.c`, so the handles have no field accessors; C routines read
    /// and evaluate it through a pointer.
    ///
    /// Every handle addresses a tree [`Expr::parse`] built, or one adopted
    /// under the same guarantees: each user function it calls stays callable
    /// for the tree's lifetime and accepts any opaque pointer from any
    /// thread, as the safe `extern "C" fn`s [`Expr::parse`] takes do.
    Expr,
    ExprRef,
    ExprMut,
    ffi::AVExpr
);

// SAFETY: a parsed tree is `av_malloc` nodes plus the root's `var` and
// `prng_state` blocks, all released by `av_expr_free` through `av_free`,
// which any thread may call; `av_expr_parse` hands the whole tree over to its
// caller and keeps no pointer to it. A node holds only scalars, child
// pointers of the same tree and function pointers: libm builtins, or user
// functions that, per `Expr`'s invariant, any thread may call. Nothing in it
// is tied to a thread, so the sole owner, or an exclusive handle, moves
// between threads as a `Box` of a tree does. Not `Sync`: evaluation writes
// the root's registers.
unsafe impl Send for Expr {}
// SAFETY: as for `Expr`: an exclusive handle is the only access to the tree
// while it lives.
unsafe impl Send for ExprMut<'_> {}

/// Wraps: av_expr_free
///
/// The release policy of an owned [`Expr`]: `av_expr_free` frees every node
/// of the tree, then the root's registers and random-number state.
///
/// Thread-agnostic: it releases through `av_free`, which any thread may
/// call, so an [`ExprBox`] is `Send`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ExprFree;

// SAFETY (of the binding): `av_expr_free` releases a tree `av_expr_parse`
// built, recursively and exactly once; it reads only the tree.
impl_cdrop!(ExprFree, Expr, ffi::av_expr_free);

/// An owned parsed expression, released with its whole tree by
/// `av_expr_free`.
///
/// Not `Clone`: C has no routine that copies a tree.
///
/// ```compile_fail,E0599
/// use ffmpeg::libavutil::eval::ExprBox;
/// fn copy(e: &ExprBox) -> ExprBox {
///     e.clone()
/// }
/// ```
///
/// Not `Sync` either: evaluation writes the tree's registers, so two threads
/// never share one.
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::eval::ExprBox;
/// fn share(e: &ExprBox) {
///     std::thread::scope(|s| {
///         s.spawn(|| e.as_ref());
///     });
/// }
/// ```
pub type ExprBox = CBox<Expr, ExprFree>;

/// A function of one argument an expression calls by name, as `funcs1` of
/// `av_expr_parse` holds them: it receives the opaque pointer the evaluator
/// is given, then the argument's value.
///
/// Safe, so calling it is sound whatever opaque pointer the evaluator
/// passes; a function that reads through that pointer must check it itself.
/// A panic in it aborts the process, as for any `extern "C" fn`.
pub type UnaryFn = extern "C" fn(*mut c_void, f64) -> f64;

/// A function of two arguments an expression calls by name, as `funcs2` of
/// `av_expr_parse` holds them; otherwise as [`UnaryFn`].
pub type BinaryFn = extern "C" fn(*mut c_void, f64, f64) -> f64;

/// `items` followed by `end`: the NULL-terminated arrays `av_expr_parse`
/// takes, built from Rust slices.
fn terminated<T: Copy>(items: impl Iterator<Item = T>, end: T) -> Vec<T> {
    items.chain(core::iter::once(end)).collect()
}

impl Expr {
    /// Wraps: av_expr_parse
    ///
    /// Parses `s` (for example `"1+2^3+5*5+sin(2/3)"`, whitespace ignored)
    /// into an expression that may name the constants `const_names`, the
    /// one-argument functions `unary` and the two-argument functions
    /// `binary`, besides the evaluator's own constants (`PI`, `E`, `PHI`,
    /// `QP2LAMBDA`) and functions. A name matches at the first position it
    /// is listed at; the names are not kept.
    ///
    /// Fails with `AVERROR(EINVAL)` for a malformed expression (logging why
    /// at `AV_LOG_ERROR + log_offset` in C's `Eval` class context), one
    /// whose parentheses or calls nest more than 100 levels deep, or a
    /// `log_offset` above `INT_MAX - AV_LOG_ERROR` (which would overflow the
    /// level C logs at, rejected before C runs). Fails with
    /// `AVERROR(ENOMEM)` when allocation fails, and also, as C reports it,
    /// for an expression whose tree would be more than 100 levels deep (a
    /// chain of more than 100 binary operators, for example).
    pub fn parse(
        s: &CStr,
        const_names: &[&CStr],
        unary: &[(&CStr, UnaryFn)],
        binary: &[(&CStr, BinaryFn)],
        log_offset: i32,
    ) -> Result<ExprBox, i32> {
        // SAFETY: a NULL log context is "no parent context".
        unsafe {
            Self::parse_with_context(s, const_names, unary, binary, log_offset, ptr::null_mut())
        }
    }

    /// Wraps: av_expr_parse
    ///
    /// As [`parse`](Self::parse), logging a failure with `log_ctx` as the
    /// parent log context.
    ///
    /// `log_ctx` stays a raw seam while logging contexts have no safe handle:
    /// pass NULL for no parent context. The expression keeps no pointer to
    /// it.
    ///
    /// # Safety
    ///
    /// `log_ctx` must be NULL or point to a live struct whose first member is
    /// a `const AVClass *` (NULL, or a valid class) that `av_log` and the
    /// installed log callback may read during the call, as
    /// `parent_log_context_offset` contexts require.
    pub unsafe fn parse_with_context(
        s: &CStr,
        const_names: &[&CStr],
        unary: &[(&CStr, UnaryFn)],
        binary: &[(&CStr, BinaryFn)],
        log_offset: i32,
        log_ctx: *mut c_void,
    ) -> Result<ExprBox, i32> {
        if log_offset > c_int::MAX - ffi::AV_LOG_ERROR as c_int {
            return Err(AVERROR_EINVAL);
        }
        // Borrowed from the slices, which outlive the call: C reads the names
        // only while parsing, and copies the function pointers it keeps.
        let consts = terminated(const_names.iter().map(|n| n.as_ptr()), ptr::null());
        let unary_names = terminated(unary.iter().map(|(n, _)| n.as_ptr()), ptr::null());
        let unary_fns = terminated(unary.iter().map(|&(_, f)| Some(f as _)), None);
        let binary_names = terminated(binary.iter().map(|(n, _)| n.as_ptr()), ptr::null());
        let binary_fns = terminated(binary.iter().map(|&(_, f)| Some(f as _)), None);
        let mut expr = ptr::null_mut();
        // Everything below lives for the call, which is as long as C reads it.
        // SAFETY: `expr` is a writable slot; `s` and every name are live
        // NUL-terminated strings in NULL-terminated arrays, each function
        // array has an entry per name, the caller vouches for `log_ctx`, and
        // `log_offset` cannot overflow the error level C adds it to.
        let ret = unsafe {
            ffi::av_expr_parse(
                &mut expr,
                s.as_ptr(),
                consts.as_ptr(),
                unary_names.as_ptr(),
                unary_fns.as_ptr(),
                binary_names.as_ptr(),
                binary_fns.as_ptr(),
                log_offset,
                log_ctx,
            )
        };
        if ret < 0 {
            // On failure C frees what it built and leaves the slot alone.
            return Err(ret);
        }
        // SAFETY: on success `expr` is a fresh tree the caller owns,
        // released by `av_expr_free`; its user functions are the safe
        // `extern "C" fn`s above, so it meets `Expr`'s invariant. C stores
        // it before returning success, so NULL would be a bug.
        unsafe { ExprBox::from_c(expr) }.ok_or(AVERROR_BUG)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn suffixes_scale_the_value() {
        assert_eq!(parse_number(c"2k"), (2000.0, c""));
        assert_eq!(parse_number(c"2K"), (2000.0, c""));
        assert_eq!(parse_number(c"2Ki"), (2048.0, c""));
        assert_eq!(parse_number(c"1B"), (8.0, c""));
        assert_eq!(parse_number(c"1MiB"), (8.0 * 1024.0 * 1024.0, c""));
        // `ff_exp10` is not exact.
        let (ratio, rest) = parse_number(c"20dB");
        assert!((ratio - 10.0).abs() < 1e-12, "{ratio}");
        assert_eq!(rest, c"");
        assert_eq!(parse_number(c"0x10"), (16.0, c""));
        assert_eq!(parse_number(c"0X10M"), (16e6, c""));
        // A letter without an SI power stays in the rest.
        assert_eq!(parse_number(c"3q"), (3.0, c"q"));
        assert_eq!(parse_number(c"3i"), (3.0, c"i"));
    }

    #[test]
    fn nothing_parsed_returns_the_whole_string() {
        for s in [c"", c"   ", c"x", c"-", c"k", c"B", c"dB"] {
            assert_eq!(parse_number(s), (0.0, s), "{s:?}");
        }
        // `0x` without digits reads the `0` only.
        assert_eq!(parse_number(c"0x"), (0.0, c"x"));
        let (nan, rest) = parse_number(c"nan!");
        assert!(nan.is_nan());
        assert_eq!(rest, c"!");
    }

    /// The rest borrows `numstr` itself, at the byte C stopped at.
    #[test]
    fn the_rest_is_a_suffix_of_the_input() {
        let s = c"  -1.25e1Mi tail";
        let (value, rest) = parse_number(s);
        assert_eq!(value, -12.5 * 1024.0 * 1024.0);
        assert_eq!(rest, c" tail");
        assert_eq!(
            rest.as_ptr() as usize - s.as_ptr() as usize,
            s.count_bytes() - rest.count_bytes()
        );
    }

    extern "C" fn twice(_: *mut c_void, x: f64) -> f64 {
        2.0 * x
    }

    extern "C" fn negate(_: *mut c_void, x: f64) -> f64 {
        -x
    }

    extern "C" fn minus(_: *mut c_void, a: f64, b: f64) -> f64 {
        a - b
    }

    extern "C" fn plus(_: *mut c_void, a: f64, b: f64) -> f64 {
        a + b
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

    /// Evaluates `e` with `consts` and no opaque pointer.
    fn eval(e: &mut ExprBox, consts: &[f64]) -> f64 {
        // SAFETY: `e` is a live root, `consts` covers every constant position
        // the callers' expressions use, and their functions ignore `opaque`.
        unsafe { ffi::av_expr_eval(e.as_c_ptr(), consts.as_ptr(), ptr::null_mut()) }
    }

    #[test]
    fn arrays_end_with_their_terminator() {
        assert_eq!(terminated([1, 2].into_iter(), 0), [1, 2, 0]);
        assert_eq!(
            terminated(core::iter::empty(), ptr::null::<c_char>()),
            [ptr::null()]
        );
        let fns = terminated([twice as UnaryFn].into_iter().map(Some), None);
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].unwrap()(ptr::null_mut(), 3.0), 6.0);
        assert!(fns[1].is_none());
    }

    #[test]
    fn names_reach_c_in_order_and_aligned_with_their_functions() {
        if under_bsan() {
            return;
        }
        // A repeated name binds its first entry, and each function its own
        // name's position.
        let mut e = Expr::parse(
            c"f(X) + g(Y, X) + h(X)",
            &[c"X", c"Y", c"X"],
            &[(c"f", twice), (c"h", twice), (c"f", negate)],
            &[(c"g", minus), (c"g", plus)],
            0,
        )
        .unwrap();
        assert_eq!(eval(&mut e, &[3.0, 4.0, 100.0]), 6.0 + 1.0 + 6.0);
        // Empty lists name nothing.
        assert_eq!(
            Expr::parse(c"X", &[], &[], &[], 40).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            Expr::parse(c"f(1)", &[], &[], &[], 40).unwrap_err(),
            AVERROR_EINVAL
        );
    }

    #[test]
    fn parse_builds_an_evaluable_tree() {
        if under_bsan() {
            return;
        }
        let mut e = Expr::parse(
            c"f(X) + g(Y, 1) * PI",
            &[c"X", c"Y"],
            &[(c"f", twice)],
            &[(c"g", minus)],
            0,
        )
        .unwrap();
        let v = eval(&mut e, &[3.0, 4.0]);
        assert_eq!(v, 6.0 + 3.0 * core::f64::consts::PI);
    }

    #[test]
    fn a_log_offset_overflowing_the_error_level_is_rejected_before_c() {
        if under_bsan() {
            return;
        }
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        // Even a valid expression: C is not reached.
        assert_eq!(
            Expr::parse(c"1", &[], &[], &[], max + 1).unwrap_err(),
            AVERROR_EINVAL
        );
        assert_eq!(
            Expr::parse(c"1", &[], &[], &[], c_int::MAX).unwrap_err(),
            AVERROR_EINVAL
        );
        // The largest offset C can add, and negative ones, reach C.
        assert!(Expr::parse(c"1", &[], &[], &[], max).is_ok());
        assert!(Expr::parse(c"1", &[], &[], &[], c_int::MIN).is_ok());
        assert_eq!(
            Expr::parse(c"1+", &[], &[], &[], c_int::MIN).unwrap_err(),
            AVERROR_EINVAL
        );
    }

    #[test]
    fn adopted_trees_are_released_by_av_expr_free() {
        if under_bsan() {
            return;
        }
        let mut raw = ptr::null_mut();
        // SAFETY: a writable slot, a literal, and no names or context.
        let ret = unsafe {
            ffi::av_expr_parse(
                &mut raw,
                c"st(0, 2); ld(0) * 3".as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                0,
                ptr::null_mut(),
            )
        };
        assert_eq!(ret, 0);
        // SAFETY: a fresh tree we own, with no user functions.
        let mut e = unsafe { ExprBox::from_c(raw) }.unwrap();
        assert_eq!(e.as_c_ptr(), raw);
        assert_eq!(eval(&mut e, &[]), 6.0);
        // Handing it back and adopting it again releases it once.
        let raw = e.into_c();
        // SAFETY: the tree `into_c` surrendered, owned again.
        let e = unsafe { ExprBox::from_c(raw) }.unwrap();
        drop(e);
        // SAFETY: NULL is not adopted.
        assert!(unsafe { ExprBox::from_c(ptr::null_mut()) }.is_none());
    }

    #[test]
    fn the_box_is_one_pointer_and_send() {
        fn send<T: Send>() {}
        send::<ExprBox>();
        send::<ExprMut<'_>>();
        assert_eq!(size_of::<ExprBox>(), size_of::<*mut ffi::AVExpr>());
        assert_eq!(size_of::<Option<ExprBox>>(), size_of::<*mut ffi::AVExpr>());
    }
}
