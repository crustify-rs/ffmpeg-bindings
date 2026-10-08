//! Safe wrappers over `libavutil/time.h`.

use super::ffi;

/// Wraps: av_usleep
///
/// Sleeps for at least `usec` microseconds, rounded to the system timer's
/// precision, resuming after signal interruptions. Fails with the C error
/// code on platforms without a sleep primitive.
pub fn usleep(usec: u32) -> Result<(), i32> {
    // SAFETY: `av_usleep` takes a duration by value and touches no caller
    // memory.
    let ret = unsafe { ffi::av_usleep(usec) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Wraps: av_gettime
///
/// The wall-clock time in microseconds since the Unix epoch, or `-1` where
/// the platform has no clock.
#[must_use]
pub fn now() -> i64 {
    // SAFETY: `av_gettime` takes no arguments and has no preconditions.
    unsafe { ffi::av_gettime() }
}

/// Wraps: av_gettime_relative
///
/// A time in microseconds since an arbitrary point, monotonic when
/// [`relative_is_monotonic`] says so; otherwise the wall clock shifted by 42
/// hours.
#[must_use]
pub fn now_relative() -> i64 {
    // SAFETY: `av_gettime_relative` takes no arguments and has no
    // preconditions.
    unsafe { ffi::av_gettime_relative() }
}

/// Wraps: av_gettime_relative_is_monotonic
///
/// Whether [`now_relative`] reads a monotonic clock.
#[must_use]
pub fn relative_is_monotonic() -> bool {
    // SAFETY: `av_gettime_relative_is_monotonic` takes no arguments and has
    // no preconditions.
    unsafe { ffi::av_gettime_relative_is_monotonic() != 0 }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn usleep_sleeps_at_least_the_duration() {
        let start = Instant::now();
        assert_eq!(usleep(2_000), Ok(()));
        assert!(start.elapsed() >= Duration::from_micros(2_000));
        assert_eq!(usleep(0), Ok(()));
    }
}
