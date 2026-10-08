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

use core::ffi::c_int;

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
}
