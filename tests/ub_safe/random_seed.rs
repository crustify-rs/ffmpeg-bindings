//! UB tests for `libavutil/random_seed.h`'s `get`, from many threads at once.

use std::thread;

use ffmpeg::libavutil::random_seed::get;

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn seeds_are_drawn_concurrently() {
    let handles: Vec<_> = (0..8)
        .map(|_| thread::spawn(|| (0..16).map(|_| get()).fold(0u32, |a, s| a ^ s)))
        .collect();
    for h in handles {
        let _ = h.join().unwrap();
    }
}

// ---- av_random_bytes ----

/// UB tests of `libavutil/random_seed.h`'s [`random_bytes`] and
/// [`random_bytes_uninit`]: exact-size heap buffers of every small length,
/// uninitialised storage, and concurrent calls.
mod bytes {
    use core::mem::MaybeUninit;

    use ffmpeg::libavutil::random_seed::{random_bytes, random_bytes_uninit};

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn exact_buffers_stay_in_bounds() {
        for len in (0..=64).chain([4095, 4096, 65536]) {
            let mut buf = vec![0u8; len].into_boxed_slice();
            random_bytes(&mut buf).unwrap();
            let mut un = Box::<[u8]>::new_uninit_slice(len);
            let filled = random_bytes_uninit(&mut un).unwrap();
            // Every byte reads as initialised, which MSan / Miri would flag.
            let sum: u64 = filled.iter().map(|&b| u64::from(b)).sum();
            assert!(len < 16 || sum > 0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn uninit_view_writes_through_to_the_buffer() {
        let mut buf = [MaybeUninit::<u8>::uninit(); 32];
        let view = random_bytes_uninit(&mut buf).unwrap();
        view[0] = 7;
        assert_eq!(view[0], 7);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn concurrent_draws_do_not_race() {
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let mut buf = [0u8; 256];
                    for _ in 0..20 {
                        random_bytes(&mut buf).unwrap();
                    }
                });
            }
        });
    }
}
