//! `libavutil/random_seed.h`'s `get` beside `av_get_random_seed`: the seeds
//! are random, so there is no value to compare, only the distribution both
//! sides draw from.

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::random_seed::get;

#[test]
fn seeds_vary_on_each_side() {
    let rust: Vec<u32> = (0..8).map(|_| get()).collect();
    // SAFETY: `av_get_random_seed` takes no arguments.
    let c: Vec<u32> = (0..8)
        .map(|_| unsafe { ffi::av_get_random_seed() })
        .collect();
    // Eight equal 32-bit draws happen with probability 2^-224.
    for draws in [&rust, &c] {
        assert!(draws.iter().any(|&s| s != draws[0]), "{draws:x?}");
    }
    // Both draw all 32 bits: some seed has the top bit set, and some not.
    let all: Vec<u32> = rust
        .iter()
        .chain(&c)
        .copied()
        .chain((0..48).map(|_| get()))
        .collect();
    assert!(all.iter().any(|s| s >> 31 == 1) && all.iter().any(|s| s >> 31 == 0));
}
