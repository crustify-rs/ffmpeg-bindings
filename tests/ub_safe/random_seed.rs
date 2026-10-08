//! UB tests for `libavutil/random_seed.h`'s `get`, from many threads at once.

use std::thread;

use ffmpeg::libavutil::random_seed::get;

#[test]
fn seeds_are_drawn_concurrently() {
    let handles: Vec<_> = (0..8)
        .map(|_| thread::spawn(|| (0..16).map(|_| get()).fold(0u32, |a, s| a ^ s)))
        .collect();
    for h in handles {
        let _ = h.join().unwrap();
    }
}
