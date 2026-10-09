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

/// [`make_time_string`] through the safe API only: whatever the stamp and
/// time base, C's `snprintf` and trim stay within the caller's buffer.
mod time_string {
    use std::thread;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::rational::Rational;
    use ffmpeg::libavutil::timestamp::{make_string, make_time_string};

    const SIZE: usize = ffi::AV_TS_MAX_STRING_SIZE as usize;

    const STAMPS: [i64; 8] = [
        i64::MIN,
        i64::MIN + 1,
        -1_000_001,
        -1,
        0,
        1,
        90_000,
        i64::MAX,
    ];

    const BASES: [(i32, i32); 12] = [
        (1, 1),
        (1, 90_000),
        (1001, 30_000),
        (i32::MAX, 1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (-1, i32::MIN),
        (i32::MIN, i32::MIN),
        (0, 1),
        (1, 0),
        (-1, 0),
        (0, 0),
    ];

    #[test]
    fn every_stamp_and_base_fits_a_heap_buffer() {
        // An exact-size heap block: ASan reports any byte written past it.
        let mut buf = Box::new([0xffu8; SIZE]);
        let start = buf.as_ptr();
        for ts in STAMPS {
            for (num, den) in BASES {
                buf.fill(0xff);
                let s = make_time_string(&mut buf, ts, Rational::new(num, den));
                assert_eq!(s.as_ptr().cast::<u8>(), start);
                assert!(!s.is_empty() && s.count_bytes() < SIZE, "{ts} {num}/{den}");
            }
        }
    }

    #[test]
    fn text_borrowed_from_one_buffer_survives_writes_to_another() {
        let mut a = Box::new([0u8; SIZE]);
        let mut b = Box::new([0u8; SIZE]);
        let first = make_time_string(&mut a, 1500, Rational::new(1, 1000));
        assert_eq!(make_time_string(&mut b, -1, Rational::new(1, 0)), c"-inf");
        let _ = make_string(&mut b, 7);
        assert_eq!(first, c"1.5");
        drop(b);
        assert_eq!(first, c"1.5");
    }

    #[test]
    fn stamps_and_plain_strings_share_one_buffer() {
        let mut buf = Box::new([0u8; SIZE]);
        for ts in STAMPS {
            let n = make_string(&mut buf, ts).count_bytes();
            assert!(n < SIZE);
            let n = make_time_string(&mut buf, ts, Rational::new(1, 3)).count_bytes();
            assert!(n < SIZE);
        }
    }

    #[test]
    fn time_strings_format_on_many_threads_at_once() {
        let base = Rational::new(1001, 30_000);
        let workers: Vec<_> = (0..8i64)
            .map(|i| {
                thread::spawn(move || {
                    let mut buf = [0; SIZE];
                    (0..500)
                        .map(|j| make_time_string(&mut buf, i * 1_000_003 - j, base).count_bytes())
                        .sum::<usize>()
                })
            })
            .collect();
        for w in workers {
            assert!(w.join().unwrap() > 0);
        }
    }
}
