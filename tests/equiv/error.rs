//! Equivalence of `libavutil/error.h`'s [`describe`] with `av_strerror`, on
//! independent buffers of every size.

use core::ffi::CStr;

use ffmpeg_bindings::libavutil::libavutil::error::describe;
use ffmpeg_bindings::libavutil::libavutil::ffi;

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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// `make_error_string` against `av_make_error_string` on independently
/// owned buffers.
mod make_error_string {
    use core::ffi::c_int;

    use ffmpeg_bindings::libavutil::libavutil::error::{MAX_STRING_SIZE, make_error_string};
    use ffmpeg_bindings::libavutil::libavutil::ffi;

    /// `FFERRTAG(a, b, c, d)`.
    const fn tag(a: u8, b: u8, c: u8, d: u8) -> c_int {
        -((a as c_int) | (b as c_int) << 8 | (c as c_int) << 16 | (d as c_int) << 24)
    }

    /// FFmpeg's own codes, negated `errno`s, unknown values and the extremes.
    const CODES: [c_int; 16] = [
        tag(b'E', b'O', b'F', b' '),
        tag(b'I', b'N', b'D', b'A'),
        tag(0xF8, b'D', b'E', b'C'),
        tag(0xF8, b'4', b'0', b'4'),
        tag(b'B', b'U', b'G', b'!'),
        -0x2BB2_AFA8,
        -0x636E_6701,
        -1,
        -12,
        -22,
        -110,
        0,
        1,
        12345,
        c_int::MIN,
        c_int::MAX,
    ];

    /// `av_make_error_string` into a fresh `len`-byte buffer: the whole buffer
    /// afterwards, and whether it returned the buffer. A size-0 call gets a valid
    /// one-byte buffer it may not write: glibc's `strerror_r` takes no NULL.
    fn c_error_string(len: usize, errnum: c_int) -> (Vec<u8>, bool) {
        let mut buf = vec![0xA5_u8; len.max(1)];
        let ptr = buf.as_mut_ptr().cast();
        // SAFETY: `ptr` is valid for `len.max(1)` bytes of writes, and C writes
        // at most `len`.
        let ret = unsafe { ffi::av_make_error_string(ptr, len, errnum) };
        if len == 0 {
            assert_eq!(buf, [0xA5], "C wrote into a size-0 buffer");
            buf.clear();
        }
        (buf, ret == ptr)
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn descriptions_and_buffers_match_at_every_size() {
        for errnum in CODES {
            for len in (0..=MAX_STRING_SIZE + 8).chain([256]) {
                let (c, returned_buf) = c_error_string(len, errnum);
                assert!(returned_buf);
                let mut rust = vec![0xA5_u8; len];
                let s = make_error_string(errnum, &mut rust).to_bytes().to_vec();
                assert_eq!(rust, c, "buffer, {errnum} in {len}");
                let c_str = c.split(|&b| b == 0).next().filter(|_| len > 0);
                assert_eq!(s, c_str.unwrap_or_default(), "string, {errnum} in {len}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reusing_one_buffer_matches_step_by_step() {
        let mut rust = [0u8; MAX_STRING_SIZE];
        let mut c = [0u8; MAX_STRING_SIZE];
        for errnum in CODES.iter().chain(CODES.iter().rev()) {
            let s = make_error_string(*errnum, &mut rust).to_owned();
            // SAFETY: `c` is valid for `MAX_STRING_SIZE` bytes of writes.
            unsafe { ffi::av_make_error_string(c.as_mut_ptr().cast(), c.len(), *errnum) };
            assert_eq!(rust, c, "{errnum}");
            assert_eq!(s.as_bytes(), c.split(|&b| b == 0).next().unwrap());
        }
    }
}
