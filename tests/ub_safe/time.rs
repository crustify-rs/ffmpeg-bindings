//! UB tests for `libavutil/time.h`'s [`usleep`] and clocks, through the safe
//! API only, from many threads at once.

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::time::{now, now_relative, relative_is_monotonic, usleep};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn sleeping_on_many_threads_at_once() {
    let workers: Vec<_> = (0..8u32)
        .map(|i| thread::spawn(move || (0..20).try_for_each(|j| usleep(i * 10 + j))))
        .collect();
    for w in workers {
        assert_eq!(w.join().unwrap(), Ok(()));
    }
    assert_eq!(usleep(0), Ok(()));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn clocks_are_read_concurrently() {
    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                let mono = relative_is_monotonic();
                let mut last = now_relative();
                for _ in 0..1000 {
                    let t = now_relative();
                    assert!(!mono || t >= last);
                    last = t;
                    assert!(now() > 0);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
