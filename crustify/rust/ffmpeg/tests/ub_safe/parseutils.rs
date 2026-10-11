//! UB tests for `libavutil/parseutils.h`'s safe parsers: inputs at the edges
//! of each syntax, in exactly sized heap strings so that ASan sees any read
//! past the NUL, prefixes at every length, the C-UB inputs the wrappers
//! refuse, and calls from many threads.

use std::ffi::{CStr, CString};
use std::sync::{Mutex, MutexGuard, PoisonError};

use ffmpeg::libavutil::parseutils::{
    parse_color, parse_color_prefix, parse_date, parse_duration, parse_video_size,
};

const EINVAL: i32 = -22;
const ERANGE: i32 = -34;

/// `s` in a heap block of exactly its length plus the NUL.
fn exact(s: &[u8]) -> Box<CStr> {
    CString::new(s).unwrap().into_boxed_c_str()
}

const COLORS: [&[u8]; 34] = [
    b"red",
    b"RED",
    b"yellowgreen",
    b"AliceBlue",
    b"ff0000",
    b"FF0000AA",
    b"#ff0000",
    b"0xff0000",
    b"0Xff0000",
    b"red@0.5",
    b"red@0x80",
    b"red@1.0",
    b"red@0",
    b"red@-0",
    b"red@1e-400",
    b"red@ 0.5",
    b"red@0.5 ",
    b"red@2",
    b"red@0x100",
    b"red@0x",
    b"red@inf",
    b"red@nan",
    b"red@NaN(1)",
    b"red@-nan",
    b"red@@",
    b"random",
    b"bikeshed@0.5",
    b"#",
    b"0x",
    b"",
    b"@",
    b"#ffffffffff",
    b"ffff",
    b"\xff\xfe",
];

#[test]
fn colors_whole_and_at_every_prefix() {
    for color in COLORS {
        let s = exact(color);
        if let Err(e) = parse_color(&s) {
            assert_eq!(e, EINVAL, "{s:?}");
        }
        for len in (0..=color.len() + 2).chain([255, 256, 257, i32::MAX as usize, usize::MAX]) {
            if let Err(e) = parse_color_prefix(&s, len) {
                assert_eq!(e, EINVAL, "{s:?} {len}");
            }
        }
    }
}

/// The inputs on which C reaches UB fail instead: a NaN opacity (C converts
/// it to a byte) and a prefix ending inside the marker (C reads an
/// uninitialised buffer).
#[test]
fn c_ub_inputs_fail() {
    for color in [
        &b"red@nan"[..],
        b"#ff0000@nan",
        b"red@ -NAN(x_1)",
        b"random@nan",
    ] {
        assert_eq!(parse_color(&exact(color)), Err(EINVAL));
    }
    assert_eq!(parse_color_prefix(&exact(b"#ff0000"), 0), Err(EINVAL));
    assert_eq!(parse_color_prefix(&exact(b"0xff0000"), 1), Err(EINVAL));
    // `INT_MAX` as a length, where C's `slen - hex_offset + 1` overflows.
    assert_eq!(
        parse_color_prefix(&exact(b"ff0000"), i32::MAX as usize),
        Ok([255, 0, 0, 255])
    );
}

/// C copies at most 127 bytes after the marker into a stack buffer.
#[test]
fn long_colors() {
    for n in [0, 1, 6, 8, 125, 126, 127, 128, 4096] {
        for marker in ["", "#", "0x"] {
            for suffix in ["", "@0.5", "@nan", "@0x80"] {
                let s = exact(format!("{marker}{}{suffix}", "a".repeat(n)).as_bytes());
                let _ = parse_color(&s);
                let _ = parse_color_prefix(&s, n);
            }
        }
    }
}

const SIZES: [&[u8]; 22] = [
    b"vga",
    b"hd1080",
    b"4k",
    b"ntsc-film",
    b"VGA",
    b"640x480",
    b" 640x 480",
    b"+640*+480",
    b"640x480x",
    b"640x",
    b"x480",
    b"0x0",
    b"-1x-1",
    b"2147483647x2147483647",
    b"2147483648x1",
    b"4294967297x1",
    b"99999999999999999999x1",
    b"1",
    b"x",
    b"",
    b"640\x00",
    b"\xff",
];

#[test]
fn video_sizes() {
    for size in SIZES {
        let Ok(s) = CString::new(size) else { continue };
        match parse_video_size(&s.into_boxed_c_str()) {
            Ok((w, h)) => assert!(w > 0 && h > 0, "{size:?}"),
            Err(e) => assert_eq!(e, EINVAL, "{size:?}"),
        }
    }
}

const DURATIONS: [&[u8]; 30] = [
    b"0",
    b"1.5",
    b"-1.5",
    b"--5",
    b"1:02",
    b"1:02:03.5",
    b"9999:59:59.999999",
    b"10000:00:00",
    b"59:60",
    b"1.1234567890",
    b"500ms",
    b"500us",
    b"5s",
    b"1.5ms",
    b"1.5us",
    b"9223372036854775807us",
    b"9223372036854775808us",
    b"-9223372036854775808us",
    b"--9223372036854775808us",
    b"9223372036854",
    b"9223372036855",
    b"9223372036854.775807",
    b"-9223372036854.775808",
    b"",
    b"-",
    b"s",
    b"1m",
    b"1 s",
    b"m",
    b"u",
];

#[test]
fn durations() {
    for d in DURATIONS {
        if let Err(e) = parse_duration(&exact(d)) {
            assert!(e == EINVAL || e == ERANGE, "{d:?}: {e}");
        }
    }
}

/// Serializes this suite's local-time parses. glibc's `mktime` and
/// `localtime_r` refresh the cached time zone under glibc's internal
/// `tzset_lock`, which TSan cannot see, so it reports concurrent calls as
/// racing in `tzset_internal`; holding this lock shows TSan the order glibc's
/// lock already guarantees.
static LOCAL_TIME: Mutex<()> = Mutex::new(());

fn local_time() -> MutexGuard<'static, ()> {
    LOCAL_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

const DATES: [&[u8]; 24] = [
    b"now",
    b"NOW",
    b"1970-01-01T00:00:00Z",
    b"1970-01-01 00:00:00",
    b"19700101T000000Z",
    b"9999-12-31T23:59:59.999999Z",
    b"0000-01-01T00:00:00z",
    b"2020-02-29t12:00:00+05:30",
    b"2020-02-29 12:00:00-0800",
    b"2020-02-29 12:00:00-08",
    b"2020-02-29 12:00:00+",
    b"12:00:00",
    b"12:00:00Z",
    b"12:00:00+01:00",
    b"120000.5",
    b"2020 - 1 - 2 3:4:5",
    b"2020-13-01T00:00:00",
    b"2020-01-32T00:00:00",
    b"2020-01-01T24:00:00",
    b"2020-01-01",
    b"",
    b"T",
    b"Z",
    b"2020-01-01T00:00:00Zjunk",
];

#[test]
fn dates() {
    let _t = local_time();
    for d in DATES {
        if let Err(e) = parse_date(&exact(d)) {
            assert!(e == EINVAL || e == ERANGE, "{d:?}: {e}");
        }
    }
}

#[test]
fn many_threads() {
    std::thread::scope(|s| {
        for t in 0..8 {
            s.spawn(move || {
                for i in 0..200 {
                    assert_eq!(parse_color(c"red@0.5"), Ok([255, 0, 0, 127]));
                    assert!(parse_color(c"random").is_ok());
                    assert_eq!(parse_video_size(c"hd720"), Ok((1280, 720)));
                    assert_eq!(parse_duration(c"1.5"), Ok(1_500_000));
                    let date = format!("1970-01-01T00:00:{:02}Z", (i + t) % 60);
                    let date = CString::new(date).unwrap();
                    assert_eq!(parse_date(&date), Ok(((i + t) % 60) as i64 * 1_000_000));
                    let _t = local_time();
                    assert!(parse_date(c"12:00:00").is_ok());
                }
            });
        }
    });
}
