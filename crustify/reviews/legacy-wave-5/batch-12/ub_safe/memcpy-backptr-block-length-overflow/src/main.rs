//! `copy_back_reference` (`av_memcpy_backptr`) with a run one byte longer
//! than a distance of 2^30: C doubles its `int` block length from 2^30 to
//! 2^31, which UBSan reports as a signed overflow, and then calls `memcpy`
//! with the wrapped length, which ASan reports as a negative size.
//!
//! Run with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`, `RUSTFLAGS=-Zsanitizer=address`
//! and the sanitized libraries on `LD_LIBRARY_PATH` (see README.md). Exits
//! non-zero on the affected commit.
#![forbid(unsafe_code)]

use ffmpeg::libavutil::libavutil::mem::copy_back_reference;

fn main() {
    let back = 1usize << 30;
    let len = back + 1;
    // Only the written gibibyte is touched; the source stays on zero pages.
    let mut buf = vec![0u8; back + len];
    buf[0] = 1;
    buf[back - 1] = 2;
    copy_back_reference(&mut buf, back, back, len).unwrap();
    assert_eq!((buf[back], buf[2 * back - 1], buf[2 * back]), (1, 2, 1));
    println!("filled {len} bytes at distance {back}");
}
