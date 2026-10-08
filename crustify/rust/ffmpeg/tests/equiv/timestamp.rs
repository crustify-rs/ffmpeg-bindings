//! Equivalence of `libavutil/timestamp.h`'s [`make_string`] with
//! `av_ts_make_string`.

use core::ffi::CStr;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::timestamp::make_string;

const SIZE: usize = ffi::AV_TS_MAX_STRING_SIZE as usize;

const STAMPS: [i64; 10] = [
    0,
    1,
    -1,
    90_000,
    -1_234_567_890_123,
    i64::MAX,
    i64::MAX - 1,
    i64::MIN + 1,
    i64::MIN,
    i32::MAX as i64 + 1,
];

/// `av_ts_make_string` into `buf`.
fn c_make_string(buf: &mut [u8; SIZE], ts: i64) -> *mut core::ffi::c_char {
    // SAFETY: `buf` is the `AV_TS_MAX_STRING_SIZE` bytes `av_ts_make_string`
    // writes at most.
    unsafe { ffi::av_ts_make_string(buf.as_mut_ptr().cast(), ts) }
}

#[test]
fn make_string_writes_as_av_ts_make_string() {
    for ts in STAMPS {
        let mut mine = [0xa5; SIZE];
        let mut theirs = [0xa5; SIZE];
        let ret = c_make_string(&mut theirs, ts);
        assert_eq!(ret.cast::<u8>(), theirs.as_mut_ptr());
        let base = mine.as_ptr();
        let r = make_string(&mut mine, ts);
        assert_eq!(r.as_ptr().cast::<u8>(), base);
        // SAFETY: `ret` is `theirs`, NUL-terminated by `snprintf`.
        assert_eq!(r, unsafe { CStr::from_ptr(ret) });
        assert_eq!(mine, theirs, "{ts}");
    }
}

#[test]
fn reused_buffers_agree_after_every_step() {
    let mut mine = [0; SIZE];
    let mut theirs = [0; SIZE];
    for ts in STAMPS.into_iter().chain(STAMPS.into_iter().rev()) {
        let _ = make_string(&mut mine, ts);
        c_make_string(&mut theirs, ts);
        assert_eq!(mine, theirs, "{ts}");
    }
}
