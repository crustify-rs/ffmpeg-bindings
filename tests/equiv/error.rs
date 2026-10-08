//! Equivalence of `libavutil/error.h`'s [`describe`] with `av_strerror`, on
//! independent buffers of every size.

use core::ffi::CStr;

use ffmpeg::libavutil::error::describe;
use ffmpeg::libavutil::ffi;

/// FFmpeg's own codes (`FFERRTAG`), `AVERROR(errno)` codes, and codes that
/// are neither.
const CODES: [i32; 14] = [
    -(i32::from_le_bytes(*b"EOF ")),
    -(i32::from_le_bytes(*b"BUG!")),
    -(i32::from_le_bytes([0xF8, b'4', b'0', b'4'])),
    -(i32::from_le_bytes(*b"EXT ")),
    -12, // AVERROR(ENOMEM)
    -22, // AVERROR(EINVAL)
    -2,  // AVERROR(ENOENT)
    0,
    1,
    -100_000,
    i32::MIN,
    i32::MAX,
    -0x7777_7777,
    -1,
];

#[test]
fn every_code_and_buffer_size_matches_c() {
    for errnum in CODES {
        for size in 0..=80 {
            let mut c = vec![0xA5u8; size];
            // SAFETY: `c` is `size` writable bytes (its pointer is valid and
            // unwritten when empty), which C writes at most.
            let cret = unsafe { ffi::av_strerror(errnum, c.as_mut_ptr().cast(), size) };
            let mut r = vec![0xA5u8; size];
            let rret = describe(errnum, &mut r);
            // Status and text agree, and so do the buffers byte for byte.
            assert_eq!(rret.is_ok(), cret >= 0, "{errnum} into {size}");
            let text = rret.unwrap_or_else(|e| e);
            let ctext = CStr::from_bytes_until_nul(&c).unwrap_or(c"");
            assert_eq!(text, ctext, "{errnum} into {size}");
            assert_eq!(r, c, "{errnum} into {size}");
        }
    }
}

#[test]
fn repeated_calls_into_one_buffer_match_c() {
    let mut c = [0u8; 24];
    let mut r = [0u8; 24];
    for errnum in CODES.iter().chain(CODES.iter().rev()) {
        // SAFETY: `c` is 24 writable bytes.
        let cret = unsafe { ffi::av_strerror(*errnum, c.as_mut_ptr().cast(), c.len()) };
        let rret = describe(*errnum, &mut r).is_ok();
        assert_eq!(rret, cret >= 0, "{errnum}");
        assert_eq!(r, c, "{errnum}");
    }
}
