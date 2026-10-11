//! Safe wrappers over `libavutil/cpu.h`.
//!
//! The CPU count override is one process-wide C atomic, read and written with
//! relaxed ordering, so every thread may query or force it; a forced count
//! applies to every later [`cpu_count`] in the process, C callers included.
//!
//! FFmpeg also picks its optimized code paths, and the data alignment they
//! need, from one process-wide set of CPU extension flags (`ffi::AV_CPU_FLAG_*`).
//! [`cpu_flags`] reads it, detecting the running CPU on first use;
//! [`force_cpu_flags`] overrides it, and is `unsafe` because both what it
//! enables and when it changes the set bear on memory safety.
//!
//! [`parse_cpu_caps`] updates a set of `AV_CPU_FLAG_*` bits from a flags
//! string such as `"sse2+avx-mmx"`.

use core::ffi::{CStr, c_int, c_uint};

use super::eval;
use super::ffi;

/// Wraps: av_cpu_count
///
/// The number of logical CPU cores this process may run on (its scheduler
/// affinity), or the count [`force_cpu_count`] set. The first call logs the
/// detected count at `AV_LOG_DEBUG`.
#[inline]
pub fn cpu_count() -> i32 {
    // SAFETY: `av_cpu_count` takes no arguments; it reads the process
    // affinity into a local set, and the override and its log-once flag are
    // C atomics, so concurrent calls do not race.
    unsafe { ffi::av_cpu_count() }
}

/// Wraps: av_cpu_force_count
///
/// Makes every later [`cpu_count`] in the process return `count` when it is
/// at least 1, and restores detection when it is less.
#[inline]
pub fn force_cpu_count(count: i32) {
    // SAFETY: `av_cpu_force_count` takes an integer by value and stores it in
    // a C atomic, which concurrent readers and writers may share.
    unsafe { ffi::av_cpu_force_count(count) }
}

/// Wraps: av_cpu_max_align
///
/// The largest data alignment, in bytes, FFmpeg's code may require on this
/// CPU: 8, 16, 32 or 64 on x86, by the SIMD extensions both the build and the
/// current CPU flags mask enable.
#[inline]
pub fn cpu_max_align() -> usize {
    // SAFETY: `av_cpu_max_align` takes no arguments and reads the CPU flags
    // through `av_get_cpu_flags`, whose cache is a C atomic.
    unsafe { ffi::av_cpu_max_align() }
}

/// Wraps: av_get_cpu_flags
///
/// The CPU extensions FFmpeg's optimized code may use, as `AV_CPU_FLAG_*`
/// bits: those detected on the running CPU at the first call, or the set
/// [`force_cpu_flags`] last forced.
///
/// The set is one relaxed atomic, so concurrent calls are race-free; two
/// first calls may both run detection and store the same value.
#[must_use]
pub fn cpu_flags() -> c_int {
    // SAFETY: `av_get_cpu_flags` takes no arguments and accesses only its
    // atomic flag set.
    unsafe { ffi::av_get_cpu_flags() }
}

/// Wraps: av_force_cpu_flags
///
/// Replaces the CPU flags [`cpu_flags`] reports with `flags`, or, for `-1`, makes
/// the next [`cpu_flags`] detect the running CPU again. On x86 a set naming an
/// SSE-or-later extension without `AV_CPU_FLAG_MMX` gets it added, with a
/// warning logged.
///
/// # Safety
///
/// - Every extension `flags` names must be supported by the running CPU
///   (`-1` always qualifies). Code selected for an unsupported extension may
///   fault, or decode as other instructions and compute wrong offsets.
/// - No FFmpeg state that chose its code paths or buffer alignment under the
///   previous set may meet code or buffers chosen under the new one: the
///   alignment FFmpeg allocates with ([`cpu_max_align`], e.g. 32 bytes for
///   AVX) changes with the set, and code selected for a wider extension
///   assumes it. Force the flags before using any other FFmpeg facility, as
///   FFmpeg's own tools do.
pub unsafe fn force_cpu_flags(flags: c_int) {
    // SAFETY: `av_force_cpu_flags` takes its argument by value and stores it
    // into its atomic flag set; the caller upholds what the new set implies
    // for later FFmpeg calls.
    unsafe { ffi::av_force_cpu_flags(flags) }
}

/// `AVERROR(EINVAL)`: C's code for a flags string it cannot evaluate.
const EINVAL: i32 = -22;

/// `AVERROR(ERANGE)`: C's code for a value that is not a 32-bit flag set.
const ERANGE: i32 = -34;

/// The longest token C evaluates whole: it splits longer runs into pieces of
/// 255 bytes (`buf[256]` in `set_string_number`).
const TOKEN_MAX: usize = 255;

/// Wraps: av_parse_cpu_caps
///
/// Applies the flags string `s` to `flags`, token by token: a token names a
/// set of `AV_CPU_FLAG_*` bits (a CPU feature such as `sse2` or `neon`, a
/// CPU model such as `k8`, `all`, `none`, or a number), and its prefix says
/// what to do with them: `+` sets them, `-` clears them, and no prefix
/// replaces `flags` with them. Tokens split at each `+` and `-`.
///
/// `Err` holds C's negative `AVERROR` code for the first token that fails;
/// the tokens before it have been applied. Beyond C, the wrapper rejects
/// with `AVERROR(EINVAL)` what would reach undefined behaviour in C's option
/// evaluator:
///
/// - a token of anything but ASCII letters, digits, `_` and `.`: C evaluates
///   it as an arithmetic expression, whose evaluator converts out-of-range
///   doubles to integers (`ld(1e300)`);
/// - an empty token after a `+` or `-` (`"++sse"`, `"sse-"`), which C
///   evaluates together with the rest of the string as one expression.
///
/// A token after `+` or `-` is combined as C combines it: its value,
/// truncated to an integer (`+2.5` sets bit 1, `-0.999` clears nothing,
/// `-5G` clears the low 32 bits of 5·10⁹), is or-ed into `flags` or masked
/// out of it, and the call fails with C's `AVERROR(ERANGE)` when the
/// combination is not a 32-bit flag set (`+0x100000000`, `+min`). C converts
/// the value to an `int64_t` before combining it, which overflows for `max`,
/// `inf` or `1e300`: the wrapper fails those with `AVERROR(ERANGE)` instead
/// of reaching that undefined behaviour.
pub fn parse_cpu_caps(flags: &mut u32, s: &CStr) -> Result<(), i32> {
    let mut rest = s.to_bytes();
    if rest.is_empty() {
        // Nothing to split: C's own result for the empty string.
        return token_flags(b"").map(|bits| *flags = bits);
    }
    while !rest.is_empty() {
        let (op, body) = match rest {
            [op @ (b'+' | b'-'), body @ ..] => (Some(*op), body),
            _ => (None, rest),
        };
        let len = body
            .iter()
            .take(TOKEN_MAX)
            .take_while(|&&c| c != b'+' && c != b'-')
            .count();
        let token = &body[..len];
        let valid = |c: &u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.');
        if token.is_empty() || !token.iter().all(valid) {
            return Err(EINVAL);
        }
        *flags = match op {
            None => token_flags(token)?,
            Some(op) => combine_flags(*flags, op, token)?,
        };
        rest = &body[len..];
    }
    Ok(())
}

/// A valid token of at most [`TOKEN_MAX`] bytes, NUL-terminated for C.
fn c_token(token: &[u8]) -> [u8; TOKEN_MAX + 1] {
    let mut buf = [0u8; TOKEN_MAX + 1];
    buf[..token.len()].copy_from_slice(token);
    buf
}

/// The flag set C evaluates a lone, unprefixed token to.
///
/// Unprefixed, C range-checks the token's value before converting it, so
/// any value is safe to evaluate; a valid token of at most [`TOKEN_MAX`]
/// bytes is evaluated whole, with no expression operator in it.
fn token_flags(token: &[u8]) -> Result<u32, i32> {
    let buf = c_token(token);
    let mut bits: c_uint = 0;
    // SAFETY: `bits` is a live, writable local, which C reads and writes
    // only for the call. `buf` holds the token NUL-terminated (it is at most
    // `TOKEN_MAX` bytes and the rest of `buf` is zero), which C only reads.
    let ret = unsafe { ffi::av_parse_cpu_caps(&mut bits, buf.as_ptr().cast()) };
    if ret < 0 { Err(ret) } else { Ok(bits) }
}

/// `flags` with the token after `+` or `-` applied as C applies it.
///
/// C evaluates the token to a double, converts it to an `int64_t`
/// (truncating), ors it into `flags` or masks it out, and range-checks the
/// combination as it does a bare token's value. Unprefixed evaluation cannot
/// stand in for the operand: it rounds the value to the nearest integer and
/// rejects values outside the 32-bit range, where C truncates and combines.
fn combine_flags(flags: u32, op: u8, token: &[u8]) -> Result<u32, i32> {
    let operand = match expression_value(token) {
        // C's conversion is defined below 2^63 only (`i64::MAX as f64` rounds
        // up to 2^63); beyond it, or for an infinity, C has undefined
        // behaviour, reported as the out-of-range value it is.
        Some(d) if d >= i64::MIN as f64 && d < i64::MAX as f64 => d as i64,
        Some(_) => return Err(ERANGE),
        // A CPU-flag name, which C resolves to its exact bits, or a token C
        // rejects with its own error.
        None => i64::from(token_flags(token)?),
    };
    let combined = if op == b'+' {
        i64::from(flags) | operand
    } else {
        i64::from(flags) & !operand
    };
    // C stores the combination as a double and accepts it when it rounds
    // within the 32-bit range; integral, it also passes C's check for a
    // fractional part.
    if !(-1.5..=4_294_967_295.5).contains(&(combined as f64)) {
        return Err(ERANGE);
    }
    Ok(combined as i32 as u32)
}

/// The value C's expression evaluator gives a bare token: a number, read
/// whole by `av_strtod` with its SI and `dB` suffixes, or one of the
/// constants the option evaluator and the expression evaluator define.
/// `None` for anything else, which C resolves as a CPU-flag name or rejects,
/// and for a NaN, which C rejects.
fn expression_value(token: &[u8]) -> Option<f64> {
    let buf = c_token(token);
    let buf = CStr::from_bytes_until_nul(&buf).expect("a valid token leaves a NUL");
    let (d, rest) = eval::parse_number(buf);
    if !token.is_empty() && rest.is_empty() {
        return (!d.is_nan()).then_some(d);
    }
    match token {
        // The option evaluator's constants: the option's default, bounds,
        // and the flag sets `none` and `all` (`~0`).
        b"default" | b"none" => Some(0.0),
        b"all" => Some(-1.0),
        b"max" => Some(i64::MAX as f64),
        b"min" => Some(i64::MIN as f64),
        // The expression evaluator's own.
        b"E" => Some(ffi::M_E),
        b"PI" => Some(ffi::M_PI),
        b"PHI" => Some(ffi::M_PHI),
        b"QP2LAMBDA" => Some(f64::from(ffi::FF_QP2LAMBDA)),
        _ => None,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// One test owns the process-wide override, so no other test observes a
    /// forced count.
    #[test]
    fn forced_count_overrides_detection_until_reset() {
        let detected = cpu_count();
        assert!(detected >= 1);
        force_cpu_count(3);
        assert_eq!(cpu_count(), 3);
        force_cpu_count(i32::MAX);
        assert_eq!(cpu_count(), i32::MAX);
        for reset in [0, -1, i32::MIN] {
            force_cpu_count(7);
            force_cpu_count(reset);
            assert_eq!(cpu_count(), detected);
        }
    }

    #[test]
    fn max_align_is_a_power_of_two_of_at_least_8() {
        let a = cpu_max_align();
        assert!(a.is_power_of_two() && (8..=64).contains(&a), "{a}");
    }

    #[test]
    fn operator_tokens_rejected_before_c() {
        for s in [
            c"(1)",
            c"1*2",
            c"ld(1e300)",
            c"sse2|avx",
            c"sse 2",
            c"++sse",
            c"+",
        ] {
            let mut flags = 0x55;
            assert_eq!(parse_cpu_caps(&mut flags, s), Err(EINVAL), "{s:?}");
            assert_eq!(flags, 0x55, "{s:?}");
        }
    }

    #[test]
    fn earlier_tokens_stay_applied_on_failure() {
        let mut flags = 0;
        assert_eq!(parse_cpu_caps(&mut flags, c"0x10+(1)"), Err(EINVAL));
        assert_eq!(flags, 0x10);
        // A trailing operator fails after the token before it, as in C.
        assert_eq!(parse_cpu_caps(&mut flags, c"0x20-"), Err(EINVAL));
        assert_eq!(flags, 0x20);
    }

    #[test]
    fn out_of_range_operands_fail_without_reaching_c_overflow() {
        // Operands C converts to `int64_t` out of range: undefined behaviour
        // there, `ERANGE` here, as for the in-range operands whose
        // combination C rejects.
        for s in [
            c"+1e300",
            c"-1e300",
            c"+max",
            c"-max",
            c"-inf",
            c"+INFINITY",
            c"+1Y",
            c"+9223372036854775808",
            c"+0x100000000",
            c"+min",
            c"+1e18",
        ] {
            let mut flags = 7;
            assert_eq!(parse_cpu_caps(&mut flags, s), Err(ERANGE), "{s:?}");
            assert_eq!(flags, 7, "{s:?}");
        }
        // The largest double below 2^63 is the largest operand C converts
        // without overflow; its low 32 bits are clear.
        let mut flags = 7;
        parse_cpu_caps(&mut flags, c"-9223372036854774784").unwrap();
        assert_eq!(flags, 7);
    }

    #[test]
    fn prefixed_operands_truncate_as_c_does() {
        // Rounding the operand, as C's unprefixed evaluation does, would
        // change other bits than C's truncation.
        for (s, expected) in [
            (c"+2.5", 0x57),
            (c"-0.999", 0x55),
            (c"+1.9995", 0x55),
            (c"-1.9995", 0x54),
            (c"-0.5", 0x55),
            (c"+5d", 0x55),
            (c"-3h", 0x55 & !300),
            (c"+PI", 0x57),
            (c"-E", 0x55 & !2),
            (c"+QP2LAMBDA", 0x55 | 118),
            (c"-min", 0x55),
            (c"-0x100000000", 0x55),
            (c"-1e18", 0x55 & !(1_000_000_000_000_000_000u64 as u32)),
            (c"+all", u32::MAX),
            (c"-none", 0x55),
        ] {
            let mut flags = 0x55;
            parse_cpu_caps(&mut flags, s).unwrap();
            assert_eq!(flags, expected, "{s:?}");
        }
        // A name C's option table resolves, even one `av_strtod` reads a
        // prefix of, and a token C rejects, both through C's own evaluation.
        let mut flags = 0x55;
        if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
            parse_cpu_caps(&mut flags, c"+3dnow").unwrap();
            assert_eq!(flags, 0x55 | ffi::AV_CPU_FLAG_3DNOW);
        }
        assert_eq!(parse_cpu_caps(&mut flags, c"-3dnowx"), Err(EINVAL));
    }

    #[test]
    fn expression_values_follow_av_strtod_and_the_evaluators_constants() {
        assert_eq!(expression_value(b"1.5"), Some(1.5));
        assert_eq!(expression_value(b"0x10"), Some(16.0));
        assert_eq!(expression_value(b"1Ki"), Some(1024.0));
        assert_eq!(expression_value(b"2kB"), Some(16000.0));
        assert_eq!(expression_value(b"all"), Some(-1.0));
        assert_eq!(expression_value(b"PHI"), Some(ffi::M_PHI));
        assert_eq!(expression_value(b"inf"), Some(f64::INFINITY));
        // Trailing bytes, NaN, and names C's option table resolves.
        for t in [&b"3dnow"[..], b"1x", b"0x", b"nan", b"sse2", b"", b"PIx", b"E3"] {
            assert_eq!(expression_value(t), None, "{t:?}");
        }
    }

    #[test]
    fn numeric_tokens_set_and_clear_bits() {
        let mut flags = 0;
        parse_cpu_caps(&mut flags, c"0x11+0x100-0x1").unwrap();
        assert_eq!(flags, 0x110);
        parse_cpu_caps(&mut flags, c"-all").unwrap();
        assert_eq!(flags, 0);
        parse_cpu_caps(&mut flags, c"all").unwrap();
        assert_eq!(flags, u32::MAX);
    }

    #[test]
    fn long_tokens_split_at_c_bound() {
        // 255 digits then 1: C evaluates the 255-digit piece (out of range)
        // first, as the wrapper does.
        let s = format!("{}1\0", "9".repeat(TOKEN_MAX));
        let s = CStr::from_bytes_with_nul(s.as_bytes()).unwrap();
        let mut flags = 0;
        assert_eq!(parse_cpu_caps(&mut flags, s), Err(ERANGE));
        // 254 zeros then 1 is one token, 256 bytes split as 255 + 1.
        let s = format!("{}12\0", "0".repeat(TOKEN_MAX - 1));
        let s = CStr::from_bytes_with_nul(s.as_bytes()).unwrap();
        parse_cpu_caps(&mut flags, s).unwrap();
        assert_eq!(flags, 2);
    }
}
