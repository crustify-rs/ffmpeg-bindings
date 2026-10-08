//! UB tests for `libavutil/time.h`'s [`usleep`], through the safe API only.

use std::thread;

use ffmpeg::libavutil::time::usleep;

#[test]
fn sleeping_on_many_threads_at_once() {
    let workers: Vec<_> = (0..8u32)
        .map(|i| thread::spawn(move || (0..20).try_for_each(|j| usleep(i * 10 + j))))
        .collect();
    for w in workers {
        assert_eq!(w.join().unwrap(), Ok(()));
    }
    assert_eq!(usleep(0), Ok(()));
}
