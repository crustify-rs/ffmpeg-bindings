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

// ---- av_random_bytes ----

/// Equivalence of `libavutil/random_seed.h`'s [`random_bytes`] with
/// `av_random_bytes`: the same status, and buffers filled the same way (the
/// bytes themselves are random on both sides).
mod bytes {
    use core::mem::MaybeUninit;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::random_seed::{random_bytes, random_bytes_uninit};

    /// `av_random_bytes` in C, on its own buffer.
    fn c_random(buf: &mut [u8]) -> i32 {
        let mut empty = 0u8;
        let ptr = if buf.is_empty() {
            &raw mut empty
        } else {
            buf.as_mut_ptr()
        };
        // SAFETY: `ptr` is valid for writes of `buf.len()` bytes.
        unsafe { ffi::av_random_bytes(ptr, buf.len()) }
    }

    #[test]
    fn status_and_fill_match_c_across_lengths() {
        for len in [0, 1, 7, 16, 255, 4096] {
            let mut c = vec![0u8; len];
            let mut r = vec![0u8; len];
            let cret = c_random(&mut c);
            assert_eq!(
                random_bytes(&mut r).map_err(|e| e < 0),
                if cret < 0 { Err(true) } else { Ok(()) }
            );
            // Over 16 bytes, an all-zero fill has probability 2^-128.
            if len >= 16 {
                assert!(c.iter().any(|&b| b != 0) && r.iter().any(|&b| b != 0));
            }

            let mut u = vec![MaybeUninit::<u8>::uninit(); len];
            let filled = random_bytes_uninit(&mut u).unwrap();
            assert_eq!(filled.len(), len);
        }
    }

    #[test]
    fn repeated_calls_keep_drawing_fresh_bytes_as_c_does() {
        let mut c = [[0u8; 32]; 4];
        let mut r = [[0u8; 32]; 4];
        for (c, r) in c.iter_mut().zip(&mut r) {
            assert_eq!(c_random(c), 0);
            random_bytes(r).unwrap();
        }
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(c[i], c[j]);
                assert_ne!(r[i], r[j]);
            }
        }
    }
}
