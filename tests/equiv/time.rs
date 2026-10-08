//! Equivalence of `libavutil/time.h`'s [`usleep`] with `av_usleep`, and of its
//! clocks with `av_gettime`, `av_gettime_relative` and
//! `av_gettime_relative_is_monotonic`: each Rust reading falls between two C
//! readings of the same clock.

use std::time::{Duration, Instant};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::time::{now, now_relative, relative_is_monotonic, usleep};

#[test]
fn usleep_returns_and_sleeps_as_av_usleep() {
    for usec in [0, 1, 500, 1_500, 0] {
        let start = Instant::now();
        // SAFETY: `av_usleep` takes a duration by value.
        let c = unsafe { ffi::av_usleep(usec) };
        let c_slept = start.elapsed();
        let start = Instant::now();
        let r = usleep(usec);
        let slept = start.elapsed();
        assert_eq!(r, if c < 0 { Err(c) } else { Ok(()) });
        let min = Duration::from_micros(usec.into());
        assert!(c_slept >= min && slept >= min, "{usec}");
    }
}

#[test]
fn wall_clock_reads_between_c_readings() {
    for _ in 0..100 {
        // SAFETY: `av_gettime` takes no arguments.
        let before = unsafe { ffi::av_gettime() };
        let r = now();
        // SAFETY: as above.
        let after = unsafe { ffi::av_gettime() };
        // The wall clock may be stepped back between readings; retry then.
        if before <= after {
            assert!((before..=after).contains(&r), "{before} <= {r} <= {after}");
        }
    }
    // Microseconds since the epoch: past 2020, and not in seconds.
    assert!(now() > 1_577_836_800_000_000);
}

#[test]
fn relative_clock_reads_between_c_readings() {
    // SAFETY: `av_gettime_relative_is_monotonic` takes no arguments.
    let mono = unsafe { ffi::av_gettime_relative_is_monotonic() } != 0;
    assert_eq!(relative_is_monotonic(), mono);
    let mut last = i64::MIN;
    for _ in 0..100 {
        // SAFETY: `av_gettime_relative` takes no arguments.
        let before = unsafe { ffi::av_gettime_relative() };
        let r = now_relative();
        // SAFETY: as above.
        let after = unsafe { ffi::av_gettime_relative() };
        if mono {
            assert!(
                last <= before && before <= r && r <= after,
                "{last} {before} {r} {after}"
            );
        }
        last = after;
    }
}
