//! Safe wrappers over `libavutil/eval.h`.
//!
//! [`parse_number`] reads a number with FFmpeg's SI, binary and `dB`
//! suffixes (`av_strtod`).
//!
//! [`Expr`] is a parsed arithmetic expression (`AVExpr`), opaque outside
//! `eval.c`: a tree of `av_malloc` nodes whose root also owns the
//! expression's `st()`/`ld()` registers and random-number state. It is owned
//! as an [`ExprBox`], built by [`Expr::parse`] (or
//! [`Expr::parse_with_context`], which logs a failure under a parent context)
//! and released with its whole tree by `av_expr_free`. C has no copy routine,
//! so the box is not `Clone`.
//!
//! Parsing reads the constant and function names only during the call: a
//! node records the *position* of the constant it names, and a copy of the
//! function pointer it calls. Evaluation therefore needs one value for each
//! constant position the expression uses, which the expression cannot report
//! (`AVExpr` keeps no count), so a caller that evaluates must keep the
//! constant count it parsed with.
//!
//! [`parse_and_eval`] evaluates an arithmetic expression such as
//! `"1+2^3+5*5+sin(2/3)"` in one step, with named constants and Rust
//! functions the expression may call by name. The evaluator converts some
//! values to integers without a range check, which is undefined behaviour in
//! C for an out-of-range value; the wrapper refuses, before C runs, the
//! expressions that could reach such a conversion (see [`parse_and_eval`]).
//!
//! A raw seam remains where the dependency has no safe handle: the opaque
//! pointer a user function receives from the evaluator.

use core::any::Any;
use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::panic::{self, AssertUnwindSafe};

use ffibox::{CBox, define_ctype, impl_cdrop};

use super::ffi;
use super::opt::{AVERROR_BUG, ObjectRef};

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

// A parsed tree is `av_malloc` nodes plus the root's `var` and `prng_state`
// blocks, all released by `av_expr_free` through `av_free`, which any thread
// may call; `av_expr_parse` hands the whole tree over to its caller and keeps
// no pointer to it. Not `Sync`: evaluation writes the root's registers.
//
// SAFETY: a node holds only scalars, child pointers of the same tree and
// function pointers: libm builtins, or user functions that, per `Expr`'s
// invariant, any thread may call. Nothing in it is tied to a thread, so the
// sole owner, or an exclusive handle, moves between threads as a `Box` of a
// tree does.
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
        Self::parse_with_context(s, const_names, unary, binary, log_offset, None)
    }

    /// Wraps: av_expr_parse
    ///
    /// As [`parse`](Self::parse), logging a failure as a message of C's
    /// `Eval` context whose parent is `log_ctx`, which names it in the
    /// message's prefix; `None` is no parent. The expression keeps no pointer
    /// to `log_ctx`.
    pub fn parse_with_context(
        s: &CStr,
        const_names: &[&CStr],
        unary: &[(&CStr, UnaryFn)],
        binary: &[(&CStr, BinaryFn)],
        log_offset: i32,
        log_ctx: Option<ObjectRef<'_>>,
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
        let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
        let mut expr = ptr::null_mut();
        // Everything below lives for the call, which is as long as C reads it.
        // SAFETY: `expr` is a writable slot; `s` and every name are live
        // NUL-terminated strings in NULL-terminated arrays, each function
        // array has an entry per name, `log_ctx` is NULL or a live object led
        // by a well-formed class (`ObjectRef`'s contract), and `log_offset`
        // cannot overflow the error level C adds it to.
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

/// `AVERROR(EINVAL)`: C's code for an expression it cannot parse, and the
/// wrapper's for one it refuses.
const EINVAL: i32 = -(ffi::EINVAL as i32);

/// `AV_LOG_TRACE`, the highest level C logs a parse error at plus the
/// offset of a quiet evaluation.
const LOG_TRACE: c_int = ffi::AV_LOG_TRACE as c_int;

/// `AV_LOG_FATAL`: C adds a context's level offset only to messages at or
/// above it.
const LOG_FATAL: c_int = ffi::AV_LOG_FATAL as c_int;

/// The most functions of each arity one evaluation takes: each needs its own
/// C trampoline, since C passes a function only the shared `opaque`.
pub const MAX_FUNCS: usize = 16;

/// A named function of one argument an expression may call: the name it is
/// called by and the function.
pub type Func1<'f> = (&'f CStr, &'f dyn Fn(f64) -> f64);

/// A named function of two arguments an expression may call: the name it is
/// called by and the function.
pub type Func2<'f> = (&'f CStr, &'f dyn Fn(f64, f64) -> f64);

/// Wraps: av_expr_parse_and_eval
///
/// The value of `expr`, an arithmetic expression in FFmpeg's syntax, such as
/// `"1+2^3+5*5+sin(2/3)"`. Whitespace is ignored. The expression may name
/// each of `consts` for its value, call each of `funcs1` and `funcs2` by
/// name, and use the evaluator's own constants (`PI`, `E`, ...) and
/// functions (`sin`, `max`, `st`, `ld`, ...); a built-in function's name
/// shadows a function of the same name in `funcs1` or `funcs2`. Parse errors
/// are logged with `log_offset` added to their level, as messages of an
/// `"Eval"` context whose parent is `log_ctx`.
///
/// # Errors
///
/// C's negative `AVERROR` code: `AVERROR(EINVAL)` for an expression it
/// cannot parse or whose value is NaN, `AVERROR(ENOMEM)` when memory runs
/// out.
///
/// Also `AVERROR(EINVAL)`, before C runs, for more than [`MAX_FUNCS`]
/// functions of either arity, and for an expression that could reach one of
/// the evaluator's unchecked `double` to integer conversions, which are
/// undefined behaviour for a NaN or out-of-range value (`ld(1e300)`):
///
/// - an index argument of `ld`, `st`, `random`, `randomi` or `taylor`, the
///   level argument of `print`, or an argument of `gcd`, `bitand` or
///   `bitor` that is not a plain decimal number (`5`, `-2.5`) in range: the
///   `int` range for an index or level, the `int64_t` range for the others;
/// - a `print` level at or above `AV_LOG_FATAL` that overflows an `int` once
///   `log_offset` is added, or a `log_offset` that does so for C's own
///   messages (above `INT_MAX - AV_LOG_TRACE`);
/// - `random` or `randomi` together with `st` or `root`, which store
///   arbitrary values in the variables `random` seeds itself from.
///
/// A function in `funcs1` or `funcs2` that panics makes the evaluation
/// return NaN to C without calling Rust again, and the panic resumes once C
/// has returned.
pub fn parse_and_eval(
    expr: &CStr,
    consts: &[(&CStr, f64)],
    funcs1: &[Func1<'_>],
    funcs2: &[Func2<'_>],
    log_offset: i32,
    log_ctx: Option<ObjectRef<'_>>,
) -> Result<f64, i32> {
    if funcs1.len() > MAX_FUNCS || funcs2.len() > MAX_FUNCS || refuses(expr.to_bytes(), log_offset)
    {
        return Err(EINVAL);
    }
    let names = |n: &mut dyn Iterator<Item = &CStr>| {
        n.map(CStr::as_ptr)
            .chain([ptr::null()])
            .collect::<Vec<*const c_char>>()
    };
    let const_names = names(&mut consts.iter().map(|c| c.0));
    let const_values: Vec<f64> = consts.iter().map(|c| c.1).chain([0.0]).collect();
    let func1_names = names(&mut funcs1.iter().map(|f| f.0));
    let func2_names = names(&mut funcs2.iter().map(|f| f.0));
    let calls1: Vec<Option<Call1>> = CALLS1[..funcs1.len()]
        .iter()
        .copied()
        .map(Some)
        .chain([None])
        .collect();
    let calls2: Vec<Option<Call2>> = CALLS2[..funcs2.len()]
        .iter()
        .copied()
        .map(Some)
        .chain([None])
        .collect();
    let funcs = Funcs {
        funcs1,
        funcs2,
        panic: Cell::new(None),
    };
    let log_ctx = log_ctx.map_or(ptr::null_mut(), |o| o.as_ptr());
    let mut value = f64::NAN;
    // SAFETY: every array is NULL-terminated, or as long as the names that
    // index it; the names and `expr` are live C strings; trampoline `i`
    // reads `funcs1[i]` / `funcs2[i]` through `opaque`, a `Funcs` that
    // outlives the call; `log_ctx` is NULL or a live object whose class is
    // well-formed, as `ObjectRef` guarantees; and `refuses` rejected every
    // expression that could reach C's unchecked conversions.
    let ret = unsafe {
        ffi::av_expr_parse_and_eval(
            &mut value,
            expr.as_ptr(),
            const_names.as_ptr(),
            const_values.as_ptr(),
            func1_names.as_ptr(),
            calls1.as_ptr(),
            func2_names.as_ptr(),
            calls2.as_ptr(),
            (&raw const funcs).cast_mut().cast(),
            log_offset,
            log_ctx,
        )
    };
    if let Some(payload) = funcs.panic.take() {
        panic::resume_unwind(payload);
    }
    if ret < 0 { Err(ret) } else { Ok(value) }
}

/// The C type of a one-argument function of an expression.
type Call1 = unsafe extern "C" fn(*mut c_void, f64) -> f64;

/// The C type of a two-argument function of an expression.
type Call2 = unsafe extern "C" fn(*mut c_void, f64, f64) -> f64;

/// The Rust functions of one evaluation, which the trampolines reach through
/// C's `opaque`, and the first panic one of them raised.
struct Funcs<'f> {
    funcs1: &'f [Func1<'f>],
    funcs2: &'f [Func2<'f>],
    panic: Cell<Option<Box<dyn Any + Send>>>,
}

impl Funcs<'_> {
    /// `f()`, or NaN once a function has panicked; a panic is caught and
    /// kept, so that it does not unwind through C.
    fn call(&self, f: impl FnOnce() -> f64) -> f64 {
        let pending = self.panic.take();
        if pending.is_some() {
            self.panic.set(pending);
            return f64::NAN;
        }
        panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
            self.panic.set(Some(payload));
            f64::NAN
        })
    }
}

/// The C functions of an evaluation's Rust functions, one per index.
impl Funcs<'_> {
    /// The C function of `funcs1[I]`.
    ///
    /// # Safety
    ///
    /// `opaque` must address a live [`Funcs`] whose `funcs1` has more than
    /// `I` entries.
    unsafe extern "C" fn call1<const I: usize>(opaque: *mut c_void, x: f64) -> f64 {
        // SAFETY: `parse_and_eval` passes its live `Funcs` as `opaque` and
        // hands C trampoline `I` only when `funcs1` has an entry `I`; C calls
        // it on the evaluating thread, during the call.
        let funcs = unsafe { &*opaque.cast::<Funcs<'_>>() };
        funcs.call(|| (funcs.funcs1[I].1)(x))
    }

    /// The C function of `funcs2[I]`.
    ///
    /// # Safety
    ///
    /// As [`call1`](Self::call1), for `funcs2`.
    unsafe extern "C" fn call2<const I: usize>(opaque: *mut c_void, x: f64, y: f64) -> f64 {
        // SAFETY: as `call1`, for `funcs2`.
        let funcs = unsafe { &*opaque.cast::<Funcs<'_>>() };
        funcs.call(|| (funcs.funcs2[I].1)(x, y))
    }
}

macro_rules! trampolines {
    ($call:ident, $ty:ty; $($i:literal)*) => {
        [$(Funcs::$call::<$i> as $ty),*]
    };
}

const CALLS1: [Call1; MAX_FUNCS] =
    trampolines!(call1, Call1; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15);
const CALLS2: [Call2; MAX_FUNCS] =
    trampolines!(call2, Call2; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15);

/// How a built-in function converts one of its arguments to an integer.
#[derive(Clone, Copy)]
enum Conversion {
    /// To `int`, through `av_clip(x, ...)`: an index or a clip bound.
    Int,
    /// To `int`, as `print`'s log level, which C then offsets.
    Level,
    /// To `int64_t` or `long`.
    Int64,
}

/// Whether C's evaluator could reach undefined behaviour evaluating `expr`
/// with `log_offset`, by the rules [`parse_and_eval`] lists: an unchecked
/// `double` to integer conversion of a non-literal or out-of-range argument,
/// a log level overflowing `int` once offset, or `random` seeding itself
/// from a variable `st` or `root` may have set to anything.
///
/// The check is syntactic and conservative: it reads every place C could
/// parse a built-in call, and lets through only arguments whose value it
/// knows.
pub(crate) fn refuses(expr: &[u8], log_offset: i32) -> bool {
    if log_offset > c_int::MAX - LOG_TRACE {
        return true;
    }
    // C drops every `av_isspace` byte before parsing.
    let s: Vec<u8> = expr
        .iter()
        .copied()
        .filter(|c| !matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .collect();
    let identifier = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let (mut seeds, mut stores) = (false, false);
    let mut i = 0;
    while i < s.len() {
        if !identifier(s[i]) {
            i += 1;
            continue;
        }
        // C parses a name only where an identifier starts; one starting with
        // a digit is a number.
        let start = i;
        while i < s.len() && identifier(s[i]) {
            i += 1;
        }
        if s[start].is_ascii_digit() {
            continue;
        }
        use Conversion::*;
        let converted: &[(usize, Conversion)] = match &s[start..i] {
            b"ld" => &[(0, Int)],
            b"st" => {
                stores = true;
                &[(0, Int)]
            }
            b"root" => {
                stores = true;
                &[]
            }
            b"random" | b"randomi" => {
                seeds = true;
                &[(0, Int)]
            }
            b"taylor" => &[(2, Int)],
            b"print" => &[(1, Level)],
            b"gcd" | b"bitand" | b"bitor" => &[(0, Int64), (1, Int64)],
            _ => continue,
        };
        // C takes the arguments after the first `(` that follows the name,
        // wherever it is; with none, it fails to parse.
        let Some(open) = s[i..].iter().position(|&c| c == b'(') else {
            continue;
        };
        let args = arguments(&s[i + open + 1..]);
        for &(index, conversion) in converted {
            let Some(arg) = args.get(index) else {
                // `taylor` and `print` read their last argument only when
                // present; a missing required one fails to parse.
                continue;
            };
            if !in_range(arg, conversion, log_offset) {
                return true;
            }
        }
    }
    seeds && stores
}

/// Whether `expr` may call the evaluator's `time()`, whose value changes
/// from one evaluation to the next: an identifier `time`, once C has dropped
/// the white space. A caller that previews a value before C evaluates the
/// same expression must refuse such an expression, since C's own evaluation
/// may then produce another value than the preview checked.
pub(crate) fn reads_clock(expr: &[u8]) -> bool {
    let s: Vec<u8> = expr
        .iter()
        .copied()
        .filter(|c| !matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .collect();
    s.split(|&c| !(c.is_ascii_alphanumeric() || c == b'_'))
        .any(|name| name == b"time")
}

/// The top-level, comma-separated arguments of a call, from just after its
/// `(` to its matching `)` or the end of the expression.
fn arguments(s: &[u8]) -> Vec<&[u8]> {
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, &c) in s.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' if depth == 0 => {
                args.push(&s[start..i]);
                return args;
            }
            b')' => depth -= 1,
            b',' if depth == 0 => {
                args.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(&s[start..]);
    args
}

/// Whether `arg` is a plain decimal number whose `conversion` C performs
/// without undefined behaviour.
fn in_range(arg: &[u8], conversion: Conversion, log_offset: i32) -> bool {
    let digits = arg
        .strip_prefix(b"+")
        .or_else(|| arg.strip_prefix(b"-"))
        .unwrap_or(arg);
    let plain = digits.iter().any(u8::is_ascii_digit)
        && digits.iter().all(|&c| c.is_ascii_digit() || c == b'.')
        && digits.iter().filter(|&&c| c == b'.').count() <= 1;
    let Some(v) = plain
        .then(|| core::str::from_utf8(arg).ok()?.parse::<f64>().ok())
        .flatten()
    else {
        return false;
    };
    match conversion {
        Conversion::Int => v > -2_147_483_649.0 && v < 2_147_483_648.0,
        Conversion::Int64 => {
            (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&v)
        }
        Conversion::Level => {
            let fits = v > -2_147_483_649.0 && v < 2_147_483_648.0;
            // `as` truncates toward zero, as C's conversion does.
            let level = v as i64;
            fits && (level < i64::from(LOG_FATAL)
                || level + i64::from(log_offset) <= i64::from(c_int::MAX))
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// Options wave 0, batch-1 review: whether an expression may call
    /// `time()`, after C drops the white space.
    #[test]
    fn clock_readers_are_found_by_name() {
        for e in [
            &b"time(0)"[..],
            b"1+time(1)",
            b"t i m e(0)",
            b"if(x,time(0))",
            b"time",
        ] {
            assert!(reads_clock(e), "{}", e.escape_ascii());
        }
        for e in [
            &b""[..],
            b"timer(0)",
            b"atime(0)",
            b"time_(0)",
            b"1e5",
            b"st(0,1)",
        ] {
            assert!(!reads_clock(e), "{}", e.escape_ascii());
        }
    }

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

    #[test]
    fn refuses_non_literal_or_out_of_range_conversions() {
        for e in [
            "ld(1e300)",
            "ld(1+1)",
            "ld(x)",
            "st(nan,1)",
            "st(2147483648,1)",
            "random(-2147483649)",
            "randomi(0.5e10,0,1)",
            "taylor(1,2,1e10)",
            "print(1,inf)",
            "gcd(1e19,2)",
            "gcd(2,9223372036854775808)",
            "bitand(1,0x10)",
            "bitor(1e300,1)",
            "1+ld(1e300)",
            "ld+(1e300)",
            "ld ( 1 e 3 0 0 )",
            "(gcd(1,2), ld(ld(0)))",
            "st(0,-1);random(0)",
            "root(random(0)-1,1)",
        ] {
            assert!(refuses(e.as_bytes(), 0), "{e}");
        }
    }

    #[test]
    fn accepts_literal_in_range_conversions_and_other_names() {
        for e in [
            "1+2^3+5*5+sin(2/3)",
            "ld(0)",
            "st(9, 1e300)+ld(-5)",
            "random(0)",
            "randomi(1,0,10)",
            "taylor(1,2)",
            "taylor(1,2,3.5)",
            "print(1)",
            "print(1,-8)",
            "gcd(12,18)",
            "bitand(-9223372036854775808,1)",
            "bitor(5.,.5)",
            "xld(1e300)",
            "ld_(1e300)",
            "2ld(1e300)",
            "ld",
            "st(0,1);root(ld(0)-1,2)",
        ] {
            assert!(!refuses(e.as_bytes(), 0), "{e}");
        }
    }

    #[test]
    fn print_levels_and_offsets_must_not_overflow() {
        assert!(!refuses(b"print(1,2147483647)", 0));
        assert!(refuses(b"print(1,2147483647)", 1));
        assert!(!refuses(b"print(1,7)", c_int::MAX - LOG_TRACE));
        assert!(!refuses(b"print(1,56)", c_int::MAX - LOG_TRACE));
        assert!(refuses(b"print(1,57)", c_int::MAX - LOG_TRACE));
        assert!(!refuses(b"print(1,2147483647)", c_int::MIN));
        assert!(refuses(b"1", c_int::MAX - LOG_TRACE + 1));
        assert!(!refuses(b"1", c_int::MAX - LOG_TRACE));
    }

    #[test]
    fn arguments_split_at_top_level_commas() {
        assert_eq!(
            arguments(b"1,(2,3),f(4))+5"),
            [&b"1"[..], b"(2,3)", b"f(4)"]
        );
        assert_eq!(arguments(b"1,2"), [&b"1"[..], b"2"]);
        assert_eq!(arguments(b")"), [&b""[..]]);
    }

    #[test]
    fn trampolines_reach_their_own_entry() {
        let double = |x: f64| x * 2.0;
        let funcs1: Vec<Func1<'_>> = vec![(c"f", &double); MAX_FUNCS];
        let funcs = Funcs {
            funcs1: &funcs1,
            funcs2: &[],
            panic: Cell::new(None),
        };
        let opaque = (&raw const funcs).cast_mut().cast();
        for (i, call) in CALLS1.iter().enumerate() {
            // SAFETY: `opaque` addresses `funcs`, which has `MAX_FUNCS`
            // one-argument entries.
            assert_eq!(unsafe { call(opaque, i as f64) }, 2.0 * i as f64);
        }
    }

    #[test]
    fn a_panic_is_kept_and_later_calls_return_nan() {
        let calls = Cell::new(0);
        let boom = |_: f64| -> f64 {
            calls.set(calls.get() + 1);
            panic!("boom")
        };
        let funcs1: [Func1<'_>; 1] = [(c"boom", &boom)];
        let funcs = Funcs {
            funcs1: &funcs1,
            funcs2: &[],
            panic: Cell::new(None),
        };
        let opaque = (&raw const funcs).cast_mut().cast();
        // SAFETY: `opaque` addresses `funcs`, which has one entry.
        assert!(unsafe { CALLS1[0](opaque, 1.0) }.is_nan());
        // SAFETY: as above.
        assert!(unsafe { CALLS1[0](opaque, 1.0) }.is_nan());
        assert_eq!(calls.get(), 1);
        assert!(funcs.panic.take().is_some());
    }
}
