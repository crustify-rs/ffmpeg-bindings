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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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

/// Options wave 0, batch-1 review: `parse_color_with_context`, safe since
/// its log context became an `ObjectRef`. Every color, whole and at every
/// prefix, with no context and with bare objects on the stack and the heap
/// (freed after the calls), which C logs failures through.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn colors_with_log_contexts() {
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::BareObject;
    use ffmpeg::libavutil::parseutils::parse_color_with_context;

    static CLASS: Class = Class::new(c"crustify-color-ctx", ClassCategory::NA);
    let local = BareObject::new(CLASS.as_ref()).unwrap();
    let boxed = Box::new(local);
    for color in COLORS {
        let s = exact(color);
        let lens = (0..=color.len() + 2).map(Some).chain([
            None,
            Some(i32::MAX as usize),
            Some(usize::MAX),
        ]);
        for len in lens {
            let expected = match len {
                Some(len) => parse_color_prefix(&s, len),
                None => parse_color(&s),
            };
            for log_ctx in [None, Some(local.as_object()), Some(boxed.as_object())] {
                let got = parse_color_with_context(&s, len, log_ctx);
                // A random color differs from call to call.
                let random = [&b"random"[..], b"bikeshed"]
                    .iter()
                    .any(|r| color.to_ascii_lowercase().starts_with(r));
                if !random {
                    assert_eq!(got, expected, "{s:?} {len:?}");
                }
            }
        }
    }
    drop(boxed);
    assert_eq!(
        parse_color_with_context(&exact(b"red@nan"), None, Some(local.as_object())),
        Err(EINVAL)
    );
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
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
#[cfg_attr(miri, ignore = "calls C")]
fn dates() {
    let _t = local_time();
    for d in DATES {
        if let Err(e) = parse_date(&exact(d)) {
            assert!(e == EINVAL || e == ERANGE, "{d:?}: {e}");
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
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

/// Options wave 0, batch 3.
///
/// UB tests of `libavutil/parseutils.h`'s safe helpers: URL tag lookup into
/// buffers of every small size (C always writes a terminator), long tags and
/// values, and the named color table read past its end.
mod info_tags_and_colors {
    use std::ffi::CString;

    use ffmpeg::libavutil::parseutils::{find_info_tag, known_color_name};

    /// Looks `tag` up in `info` into exactly sized heap buffers of 0 to 8 bytes,
    /// so that ASan sees a write past any of them.
    fn every_size(tag: &str, info: &str) {
        let (tag, info) = (CString::new(tag).unwrap(), CString::new(info).unwrap());
        for len in 0..=8 {
            let mut buf = vec![0xAAu8; len].into_boxed_slice();
            if let Some(v) = find_info_tag(&mut buf, &tag, &info) {
                assert!(v.to_bytes().len() < len.max(1));
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn buffers_of_every_small_size() {
        every_size("a", "?a=1234567890");
        every_size("b", "?a=1&b=x+y+z&c");
        every_size("c", "?a=1&b=2&c");
        every_size("", "?=v");
        every_size("z", "");
        every_size("a", "a=1&&&a=2");
        every_size("a", "?a");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn long_tags_and_values() {
        let long_tag = "t".repeat(300);
        let info = format!("?{long_tag}={}&x=1", "v".repeat(5000));
        every_size(&long_tag, &info);
        // C compares the first 127 bytes of a tag only.
        let mut buf = vec![0u8; 64];
        assert!(
            find_info_tag(
                &mut buf,
                &CString::new(&long_tag[..127]).unwrap(),
                &CString::new(info.clone()).unwrap()
            )
            .is_some()
        );
        let mut big = vec![0u8; 8192];
        let v = find_info_tag(
            &mut big,
            &CString::new("x").unwrap(),
            &CString::new(info).unwrap(),
        )
        .unwrap();
        assert_eq!(v.to_bytes(), b"1");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn the_color_table_ends() {
        let mut n = 0;
        while let Some((name, rgb)) = known_color_name(n) {
            assert!(!name.to_bytes().is_empty());
            let _ = rgb;
            n += 1;
        }
        assert!(n > 100);
        for i in [
            n,
            n + 1,
            i32::MAX as usize,
            i32::MAX as usize + 1,
            usize::MAX,
        ] {
            assert!(known_color_name(i).is_none());
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn lookups_from_many_threads() {
        std::thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    for i in 0..500 {
                        let mut buf = [0u8; 16];
                        let info = CString::new(format!("?k{t}={i}")).unwrap();
                        let tag = CString::new(format!("k{t}")).unwrap();
                        let v = find_info_tag(&mut buf, &tag, &info).unwrap();
                        assert_eq!(v.to_str().unwrap(), i.to_string());
                        let _ = known_color_name(i);
                    }
                });
            }
        });
    }
}

/// Options wave 1, batch 1: ratios, video rates, date fields and UTC times
/// at the edges of each syntax, in exactly sized heap strings, with the
/// C-UB inputs the wrappers refuse, every `int` edge of a `Tm`, borrowed
/// rests outliving the `Tm`, and calls from many threads.
mod ratios_and_dates {
    use ffmpeg::libavutil::bprint::Tm;
    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::log::{Class, ClassCategory};
    use ffmpeg::libavutil::opt::BareObject;
    use ffmpeg::libavutil::parseutils::{
        parse_ratio, parse_ratio_with_context, parse_video_rate, small_strptime, timegm,
    };

    use super::{EINVAL, exact};

    const RATIOS: [&[u8]; 26] = [
        b"16:9",
        b"0:0",
        b"-2147483648:-1",
        b"2147483647:-2147483648",
        b"99999999999999999999:-99999999999999999999",
        b" \t\n7:\t3",
        b"7:3x",
        b"7:",
        b":",
        b"",
        b"1e308*10",
        b"-1e308*10",
        b"1/0",
        b"0/0",
        b"sqrt(-1)",
        b"PI*E",
        b"1e-320",
        b"((((((((((1))))))))))",
        b"ld(0)",
        b"ld(1e300)",
        b"st(0,1)+random(0)",
        b"print(1, 2147483647)",
        b"gcd(1e300, 2)",
        b"ntsc",
        b"\xff\xfe",
        b"1:1:1",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn ratios_at_every_bound_and_offset() {
        for r in RATIOS {
            let s = exact(r);
            for max in [i32::MIN, -1, 0, 1, 1_001_000, i32::MAX] {
                for offset in [
                    i32::MIN,
                    -8,
                    0,
                    ffi::AV_LOG_MAX_OFFSET as i32,
                    i32::MAX - 56,
                    i32::MAX,
                ] {
                    let got = parse_ratio_with_context(&s, max, offset, None);
                    if max < 0 {
                        assert_eq!(got, Err(EINVAL), "{r:?}");
                    }
                    if let Ok(q) = got {
                        let q = q.into_raw();
                        assert!(q.num.unsigned_abs() <= max.max(1) as u32, "{r:?} {max}");
                        assert!(q.den.unsigned_abs() <= max.max(1) as u32, "{r:?} {max}");
                    }
                }
            }
            let _ = parse_ratio(&s, 255);
        }
        // Refused before C: its unchecked conversions and offsets.
        for r in [&b"ld(1e300)"[..], b"st(0,1)+random(0)", b"gcd(1e300, 2)"] {
            assert_eq!(parse_ratio(&exact(r), 255), Err(EINVAL), "{r:?}");
        }
        assert_eq!(
            parse_ratio_with_context(c"x", 10, i32::MAX, None),
            Err(EINVAL)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn ratios_log_in_a_context() {
        static CLASS: Class = Class::new(c"crustify-ratio", ClassCategory::NA);
        let ctx = Box::new(BareObject::new(CLASS.as_ref()).unwrap());
        for r in RATIOS {
            let s = exact(r);
            let _ = parse_ratio_with_context(
                &s,
                100,
                ffi::AV_LOG_MAX_OFFSET as i32,
                Some(ctx.as_object()),
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn video_rates() {
        for r in RATIOS.iter().copied().chain([
            &b"ntsc-film"[..],
            b"pal",
            b"NTSC",
            b"1001000:1",
            b"0.0000001",
        ]) {
            if let Ok(q) = parse_video_rate(&exact(r)) {
                let q = q.into_raw();
                assert!(q.num > 0 && q.den > 0, "{r:?}");
            }
        }
        assert_eq!(parse_video_rate(c"ld(1e300)"), Err(EINVAL));
    }

    const FORMATS: [&[u8]; 14] = [
        b"%Y-%m-%d",
        b"%T",
        b"%J:%M:%S",
        b"%b %d",
        b"%B",
        b"%h",
        b"%%",
        b"%",
        b"%Y%",
        b"%q",
        b" ",
        b"%H %M",
        b"",
        b"%T%T%T",
    ];

    const INPUTS: [&[u8]; 16] = [
        b"2024-02-29",
        b"23:59:59",
        b"9999:59:59",
        b"December 31",
        b"dec",
        b"de",
        b"%",
        b"",
        b" \t\n\x0b\x0c\r",
        b"00:00:0000:00:0000:00:00",
        b"99999999",
        b"\xff",
        b"-1",
        b"+1",
        b"1:2:3",
        b"Feb",
    ];

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn strptime_rests_stay_within_the_input() {
        for fmt in FORMATS {
            let fmt = exact(fmt);
            for input in INPUTS {
                let s = exact(input);
                let mut tm = Tm::new();
                if let Some(rest) = small_strptime(&s, &fmt, tm.as_mut()) {
                    let off = rest.as_ptr() as usize - s.as_ptr() as usize;
                    assert!(off <= input.len(), "{input:?}");
                    assert_eq!(rest.to_bytes(), &input[off..]);
                }
                // The zone is never written: still the `'static` NULL.
                assert!(tm.as_ref().zone().is_none());
            }
        }
    }

    /// The rest borrows the input, not the `Tm`: it outlives the `Tm`, and
    /// the `Tm` is reused while it lives.
    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rests_outlive_the_tm() {
        let s = exact(b"12:34:56 rest");
        let rest = {
            let mut tm = Tm::new();
            let rest = small_strptime(&s, c"%T", tm.as_mut()).unwrap();
            assert_eq!(small_strptime(rest, c" ", tm.as_mut()), Some(c"rest"));
            rest
        };
        assert_eq!(rest, c" rest");
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn utc_times_at_every_int_edge() {
        let edges = [
            i32::MIN,
            i32::MIN + 1,
            -5_885_416,
            -1,
            0,
            1,
            12,
            59,
            60,
            5_881_616,
            i32::MAX - 1,
            i32::MAX,
        ];
        let mut tm = Tm::new();
        for &a in &edges {
            for &b in &edges {
                for field in 0..6 {
                    let mut m = tm.as_mut();
                    m.set_year(70);
                    m.set_mon(0);
                    m.set_mday(1);
                    m.set_hour(0);
                    m.set_min(0);
                    m.set_sec(0);
                    match field {
                        0 => m.set_year(a),
                        1 => m.set_mon(a),
                        2 => m.set_mday(a),
                        3 => m.set_hour(a),
                        4 => m.set_min(a),
                        _ => m.set_sec(a),
                    }
                    match (field + 1) % 6 {
                        0 => m.set_year(b),
                        1 => m.set_mon(b),
                        2 => m.set_mday(b),
                        3 => m.set_hour(b),
                        4 => m.set_min(b),
                        _ => m.set_sec(b),
                    }
                    let _ = timegm(tm.as_ref());
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn many_threads() {
        std::thread::scope(|s| {
            for t in 0..4 {
                s.spawn(move || {
                    for i in 0..200 {
                        let _ = parse_ratio(&exact(RATIOS[(i + t) % RATIOS.len()]), 1000);
                        let _ = parse_video_rate(&exact(RATIOS[(i * 7 + t) % RATIOS.len()]));
                        let mut tm = Tm::new();
                        let _ = small_strptime(
                            &exact(INPUTS[i % INPUTS.len()]),
                            &exact(FORMATS[t]),
                            tm.as_mut(),
                        );
                        let _ = timegm(tm.as_ref());
                    }
                });
            }
        });
    }
}
