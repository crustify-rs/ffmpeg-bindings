//! UB tests of `libavutil/error.h`'s [`describe`]: every buffer size,
//! including none, and every class of code, across threads.

use ffmpeg::libavutil::error::describe;

const CODES: [i32; 8] = [
    -(i32::from_le_bytes(*b"EOF ")),
    -(i32::from_le_bytes(*b"BUG!")),
    -12,
    -22,
    0,
    i32::MIN,
    i32::MAX,
    -0x7777_7777,
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn every_buffer_size_stays_in_bounds() {
    for code in CODES {
        for size in 0..=80 {
            // The buffer is exactly `size` bytes on the heap, so ASan sees a
            // write past it.
            let mut buf = vec![0xA5u8; size].into_boxed_slice();
            let text = describe(code, &mut buf).unwrap_or_else(|e| e);
            assert!(text.count_bytes() < size.max(1), "{code} into {size}");
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn concurrent_descriptions_do_not_race() {
    std::thread::scope(|scope| {
        for code in CODES {
            scope.spawn(move || {
                let mut buf = [0u8; 64];
                for _ in 0..100 {
                    let _ = describe(code, &mut buf);
                }
            });
        }
    });
}

/// `make_error_string` into exact-size buffers of every length up to past
/// the longest description, so a write past the end lands in a sanitizer
/// redzone.
mod make_error_string {
    use std::thread;

    use ffmpeg::libavutil::error::{MAX_STRING_SIZE, make_error_string};

    /// FFmpeg codes, negated `errno`s, unknown values and the extremes.
    const CODES: [i32; 10] = [
        -0x2046_4f45, // AVERROR_EOF
        -0x4144_4e49, // AVERROR_INVALIDDATA
        -0x2bb2_afa8, // AVERROR_EXPERIMENTAL
        -1,
        -12,
        0,
        1,
        9999,
        i32::MIN,
        i32::MAX,
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn every_buffer_length_holds_its_string() {
        for errnum in CODES {
            for len in 0..=MAX_STRING_SIZE + 16 {
                let mut buf = vec![0x5a_u8; len].into_boxed_slice();
                let s = make_error_string(errnum, &mut buf);
                assert!(s.to_bytes().len() < len.max(1));
                let n = s.to_bytes().len();
                assert!(len == 0 || buf[n] == 0);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_string_borrows_the_buffer() {
        let mut buf = [0u8; MAX_STRING_SIZE];
        let s = make_error_string(CODES[0], &mut buf).to_owned();
        buf.fill(0xff);
        assert!(!s.as_bytes().is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptions_are_written_concurrently() {
        let handles: Vec<_> = (0..4)
            .map(|t| {
                thread::spawn(move || {
                    let mut buf = [0u8; MAX_STRING_SIZE];
                    for i in 0..2000 {
                        let errnum = CODES[(i + t) % CODES.len()];
                        let _ = make_error_string(errnum, &mut buf[..(i % MAX_STRING_SIZE) + 1]);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }
}
