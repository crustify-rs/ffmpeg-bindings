//! UB tests for `libavutil/timestamp.h`'s [`make_string`], through the safe
//! API only: the text stays within the caller's buffer.

use std::thread;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::timestamp::make_string;

const SIZE: usize = ffi::AV_TS_MAX_STRING_SIZE as usize;

#[test]
fn extreme_stamps_fit_a_heap_buffer() {
    let mut buf = Box::new([0xffu8; SIZE]);
    for ts in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX] {
        let s = make_string(&mut buf, ts);
        assert!(s.count_bytes() < SIZE);
    }
}

#[test]
fn stamps_format_on_many_threads_at_once() {
    let workers: Vec<_> = (0..8i64)
        .map(|i| {
            thread::spawn(move || {
                let mut buf = [0; SIZE];
                (0..500)
                    .map(|j| make_string(&mut buf, i * 1_000_003 - j).count_bytes())
                    .sum::<usize>()
            })
        })
        .collect();
    for w in workers {
        assert!(w.join().unwrap() > 0);
    }
}
