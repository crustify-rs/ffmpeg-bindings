//! Equivalence of `libavutil/time.h`'s [`usleep`] with `av_usleep`.

use std::time::{Duration, Instant};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::time::usleep;

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
