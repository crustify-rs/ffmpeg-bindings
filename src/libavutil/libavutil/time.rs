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
