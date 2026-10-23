//! Equivalence of `libavutil/timestamp.h`'s [`make_string`] with
//! `av_ts_make_string`.

use core::ffi::CStr;

use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::timestamp::make_string;

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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn reused_buffers_agree_after_every_step() {
    let mut mine = [0; SIZE];
    let mut theirs = [0; SIZE];
    for ts in STAMPS.into_iter().chain(STAMPS.into_iter().rev()) {
        let _ = make_string(&mut mine, ts);
        c_make_string(&mut theirs, ts);
        assert_eq!(mine, theirs, "{ts}");
    }
}

/// Equivalence of [`make_time_string`] with `av_ts_make_time_string2`.
mod time_string {
    use core::ffi::{CStr, c_char};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::timestamp::{make_string, make_time_string};

    const SIZE: usize = ffi::AV_TS_MAX_STRING_SIZE as usize;

    const STAMPS: [i64; 12] = [
        0,
        1,
        -1,
        1500,
        90_000,
        -1_234_567_890_123,
        1 << 53,
        i64::MAX,
        i64::MAX - 1,
        i64::MIN + 1,
        i64::MIN,
        i32::MAX as i64 + 1,
    ];

    const BASES: [(i32, i32); 15] = [
        (1, 1),
        (1, 1000),
        (1, 90_000),
        (1001, 30_000),
        (1, 3),
        (-1, 3),
        (3, -7),
        (i32::MAX, 1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (-1, i32::MIN),
        (0, 1),
        (1, 0),
        (-1, 0),
        (0, 0),
    ];

    /// `av_ts_make_time_string2` into `buf`.
    fn c_make_time_string(buf: &mut [u8; SIZE], ts: i64, tb: (i32, i32)) -> *mut c_char {
        let tb = ffi::AVRational {
            num: tb.0,
            den: tb.1,
        };
        // SAFETY: `buf` is the `AV_TS_MAX_STRING_SIZE` bytes
        // `av_ts_make_time_string2` writes at most; `tb` is passed by value.
        unsafe { ffi::av_ts_make_time_string2(buf.as_mut_ptr().cast(), ts, tb) }
    }

    /// Runs both on `fill`-prefilled buffers and compares the returned
    /// pointer, the text and every byte of the buffers.
    fn assert_same(ts: i64, (num, den): (i32, i32), fill: u8) {
        let mut mine = [fill; SIZE];
        let mut theirs = [fill; SIZE];
        let ret = c_make_time_string(&mut theirs, ts, (num, den));
        assert_eq!(ret.cast::<u8>(), theirs.as_mut_ptr());
        let base = mine.as_ptr();
        let r = make_time_string(&mut mine, ts, Rational::new(num, den));
        assert_eq!(r.as_ptr().cast::<u8>(), base);
        // SAFETY: `ret` is `theirs`, NUL-terminated by C.
        assert_eq!(r, unsafe { CStr::from_ptr(ret) }, "{ts} {num}/{den}");
        assert_eq!(mine, theirs, "{ts} {num}/{den}");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn make_time_string_writes_as_av_ts_make_time_string2() {
        for ts in STAMPS {
            for tb in BASES {
                assert_same(ts, tb, 0xa5);
                assert_same(ts, tb, 0);
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reused_buffers_agree_after_every_step() {
        // C leaves `snprintf`'s bytes after the trimmed NUL, so each call's
        // buffer depends on the previous one.
        let mut mine = [0; SIZE];
        let mut theirs = [0; SIZE];
        let steps = STAMPS
            .iter()
            .flat_map(|&ts| BASES.iter().map(move |&tb| (ts, tb)));
        for (ts, tb) in steps.clone().chain(steps.rev()) {
            let _ = make_time_string(&mut mine, ts, Rational::new(tb.0, tb.1));
            c_make_time_string(&mut theirs, ts, tb);
            assert_eq!(mine, theirs, "{ts} {tb:?}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn interleaved_with_make_string_agree_after_every_step() {
        let mut mine = [0x5a; SIZE];
        let mut theirs = [0x5a; SIZE];
        for (i, ts) in STAMPS.into_iter().enumerate() {
            let tb = BASES[i % BASES.len()];
            let _ = make_time_string(&mut mine, ts, Rational::new(tb.0, tb.1));
            c_make_time_string(&mut theirs, ts, tb);
            assert_eq!(mine, theirs, "{ts} {tb:?}");
            let _ = make_string(&mut mine, ts / 7);
            // SAFETY: `theirs` is the `AV_TS_MAX_STRING_SIZE` bytes
            // `av_ts_make_string` writes at most.
            unsafe { ffi::av_ts_make_string(theirs.as_mut_ptr().cast(), ts / 7) };
            assert_eq!(mine, theirs, "{ts}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn pseudo_random_stamps_and_bases_agree() {
        // A fixed xorshift sequence, so a failure reproduces.
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..4000 {
            let r = next();
            // Mix full-range stamps with small ones, whose values have
            // fractional digits to trim.
            let ts = if r & 1 == 0 {
                r as i64
            } else {
                (r as i64) >> (r % 63)
            };
            let s = next();
            let num = (s as i32) >> ((s >> 32) % 32);
            let den = ((s >> 32) as i32) >> (s % 32);
            assert_same(ts, (num, den), 0xa5);
        }
    }
}

/// The safe [`make_time_string_by_ref`](ffmpeg_bindings::libavutil::libavutil::timestamp::make_time_string_by_ref)
/// against raw `av_ts_make_time_string` (core wave 1, batch 2), on separate
/// buffers, compared whole after every write.
mod time_string_by_ref {
    use core::ffi::CStr;

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::rational::Rational;
    use ffmpeg_bindings::libavutil::libavutil::timestamp::make_time_string_by_ref;

    use super::{SIZE, STAMPS};

    /// Time bases from the usual to the degenerate: zero parts, negative
    /// parts and the extremes.
    const TBS: [(i32, i32); 11] = [
        (1, 1),
        (1, 1000),
        (1, 90000),
        (1001, 30000),
        (1, i32::MAX),
        (i32::MAX, 1),
        (-1, 25),
        (0, 1),
        (1, 0),
        (-1, 0),
        (0, 0),
    ];

    /// `av_ts_make_time_string` into `buf`.
    fn c_time_string(buf: &mut [u8; SIZE], ts: i64, tb: (i32, i32)) -> *mut core::ffi::c_char {
        let tb = ffi::AVRational {
            num: tb.0,
            den: tb.1,
        };
        // SAFETY: `buf` is the `AV_TS_MAX_STRING_SIZE` bytes C writes at most;
        // `tb` is a live struct C reads once.
        unsafe { ffi::av_ts_make_time_string(buf.as_mut_ptr().cast(), ts, &tb) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn by_ref_writes_as_av_ts_make_time_string() {
        let stamps = STAMPS
            .into_iter()
            .chain([3, 7, 1_000_001, 123_456_789, -42]);
        for ts in stamps {
            for tb in TBS {
                let mut mine = [0xa5; SIZE];
                let mut theirs = [0xa5; SIZE];
                let ret = c_time_string(&mut theirs, ts, tb);
                assert_eq!(ret.cast::<u8>(), theirs.as_mut_ptr());
                let base = mine.as_ptr();
                let q = Rational::new(tb.0, tb.1);
                let r = make_time_string_by_ref(&mut mine, ts, q.as_ref());
                assert_eq!(r.as_ptr().cast::<u8>(), base);
                // SAFETY: `ret` is `theirs`, NUL-terminated by C.
                assert_eq!(r, unsafe { CStr::from_ptr(ret) }, "{ts} {tb:?}");
                assert_eq!(mine, theirs, "{ts} {tb:?}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn reused_buffers_agree_after_every_step() {
        let mut mine = [0; SIZE];
        let mut theirs = [0; SIZE];
        for (i, ts) in STAMPS
            .into_iter()
            .chain(STAMPS.into_iter().rev())
            .enumerate()
        {
            let tb = TBS[i % TBS.len()];
            let q = Rational::new(tb.0, tb.1);
            let _ = make_time_string_by_ref(&mut mine, ts, q.as_ref());
            c_time_string(&mut theirs, ts, tb);
            assert_eq!(mine, theirs, "{ts} {tb:?}");
        }
    }
}
