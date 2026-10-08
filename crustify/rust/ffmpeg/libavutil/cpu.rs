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
/// A token after `+` or `-` must also denote a 32-bit flag set on its own
/// (an integer in `-1..=u32::MAX`), or the call fails with C's
/// `AVERROR(ERANGE)`: C converts its value to an `int64_t` before checking
/// it, which overflows for `max` or `1e300`. C accepts some such tokens that
/// clear bits (`-0x100000000` and `+0.5` change nothing); the wrapper
/// rejects them.
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
        let bits = token_flags(token)?;
        *flags = match op {
            Some(b'+') => *flags | bits,
            Some(_) => *flags & !bits,
            None => bits,
        };
        rest = &body[len..];
    }
    Ok(())
}

/// The flag set C evaluates a lone, unprefixed token to.
///
/// Unprefixed, C range-checks the token's value before converting it, so
/// any value is safe to evaluate; a valid token of at most [`TOKEN_MAX`]
/// bytes is evaluated whole, with no expression operator in it.
fn token_flags(token: &[u8]) -> Result<u32, i32> {
    let mut buf = [0u8; TOKEN_MAX + 1];
    buf[..token.len()].copy_from_slice(token);
    let mut bits: c_uint = 0;
    // SAFETY: `bits` is a live, writable local, which C reads and writes
    // only for the call. `buf` holds the token NUL-terminated (it is at most
    // `TOKEN_MAX` bytes and the rest of `buf` is zero), which C only reads.
    let ret = unsafe { ffi::av_parse_cpu_caps(&mut bits, buf.as_ptr().cast()) };
    if ret < 0 { Err(ret) } else { Ok(bits) }
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

    /// `AVERROR(ERANGE)`.
    const ERANGE: i32 = -34;

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
        for s in [
            c"+1e300",
            c"-1e300",
            c"+max",
            c"-inf",
            c"+0x100000000",
            c"-0.5",
        ] {
            let mut flags = 7;
            assert_eq!(parse_cpu_caps(&mut flags, s), Err(ERANGE), "{s:?}");
            assert_eq!(flags, 7, "{s:?}");
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
