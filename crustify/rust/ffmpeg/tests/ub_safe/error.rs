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
