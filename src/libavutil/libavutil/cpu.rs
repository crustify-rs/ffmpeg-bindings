//! Safe wrappers over `libavutil/cpu.h`.
//!
//! The CPU count override is one process-wide C atomic, read and written with
//! relaxed ordering, so every thread may query or force it; a forced count
//! applies to every later [`cpu_count`] in the process, C callers included.

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
