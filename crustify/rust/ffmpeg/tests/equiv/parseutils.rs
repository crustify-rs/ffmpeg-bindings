//! Equivalence of `libavutil/parseutils.h`'s parsers with the raw C calls,
//! on the same inputs: results, errors, the out-parameters C writes on
//! success, and the messages C logs.

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_void};
use core::ptr::{self, null_mut};
use std::ffi::CString;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::log::{self, Callback, Record};
use ffmpeg::libavutil::opt::ObjectRef;
use ffmpeg::libavutil::parseutils::{
    parse_color, parse_color_prefix, parse_color_with_context, parse_date, parse_duration,
    parse_video_size,
};

use crate::log::LogState;

/// `av_parse_color` in C, into a caller's buffer, which C may write partly
/// on failure.
fn c_color_into(rgba: &mut [u8; 4], color: &CStr, slen: c_int, log_ctx: *mut c_void) -> c_int {
    // SAFETY: `rgba` holds 4 writable bytes, `color` is NUL-terminated, every
    // caller passes an `slen` C handles (negative, or within
    // `hex_offset..INT_MAX`) and no NaN opacity, and `log_ctx` is NULL or a
    // live context.
    unsafe { ffi::av_parse_color(rgba.as_mut_ptr(), color.as_ptr(), slen, log_ctx) }
}

fn c_color(color: &CStr, slen: c_int) -> Result<[u8; 4], i32> {
    let mut rgba = [0; 4];
    let ret = c_color_into(&mut rgba, color, slen, null_mut());
    if ret < 0 { Err(ret) } else { Ok(rgba) }
}

/// The bytes C's `av_parse_color` skips as a hex marker.
fn hex_offset(color: &CStr) -> usize {
    match color.to_bytes() {
        [b'#', ..] => 1,
        [b'0', b'x', ..] => 2,
        _ => 0,
    }
}

/// Every name in C's color table.
fn color_names() -> Vec<&'static CStr> {
    (0..)
        .map_while(|i| {
            // SAFETY: `av_get_known_color_name` bounds-checks the index and
            // returns NULL past the table, or a static name.
            let name = unsafe { ffi::av_get_known_color_name(i, ptr::null_mut()) };
            // SAFETY: a non-NULL name is a static NUL-terminated string.
            (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) })
        })
        .collect()
}

/// Colors on which C is defined, valid and not.
const COLORS: &[&CStr] = &[
    c"red",
    c"Red",
    c"RED",
    c"aliceblue",
    c"ALICEBLUE@0.5",
    c"ff0000",
    c"FF0000",
    c"ff000080",
    c"#ff0000",
    c"#ff000080",
    c"0xff0000",
    c"0xff000080",
    c"0Xff0000",
    c"#ff0000@0x7f",
    c"red@0.5",
    c"red@0.999",
    c"red@1",
    c"red@1.0001",
    c"red@0",
    c"red@-0",
    c"red@-0.1",
    c"red@0x80",
    c"red@0xff",
    c"red@0x100",
    c"red@0x",
    c"red@0xg",
    c"red@",
    c"red@ 0.5",
    c"red@0.5 ",
    c"red@inf",
    c"red@nanx",
    c"red@1e-400",
    c"red@@0.5",
    c"@0.5",
    c"@",
    c"",
    c"#",
    c"0x",
    c"#12345",
    c"#1234567",
    c"123456789",
    c"ggg",
    c"nosuchcolor",
    c"re d",
    c"0x0x112233",
    c"##112233",
];

#[test]
fn every_table_name_agrees_in_any_case() {
    let names = color_names();
    assert!(names.len() > 100);
    for name in names {
        let upper = CString::new(name.to_bytes().to_ascii_uppercase()).unwrap();
        for s in [name, &upper] {
            assert_eq!(parse_color(s), c_color(s, -1), "{s:?}");
            let with_alpha = CString::new([s.to_bytes(), b"@0.25"].concat()).unwrap();
            assert_eq!(
                parse_color(&with_alpha),
                c_color(&with_alpha, -1),
                "{with_alpha:?}"
            );
        }
    }
}

#[test]
fn colors_agree_whole() {
    for &color in COLORS {
        assert_eq!(parse_color(color), c_color(color, -1), "{color:?}");
    }
}

/// Every prefix length C handles (from the end of the marker), and lengths
/// past the string and past C's 127-byte copy.
#[test]
fn colors_agree_at_every_prefix() {
    let long = CString::new(format!("#{}@0x80", "a".repeat(140))).unwrap();
    let long_valid = CString::new(format!("ff0000{}", " ".repeat(130))).unwrap();
    for color in COLORS.iter().copied().chain([&*long, &*long_valid]) {
        let lens = hex_offset(color)..=color.count_bytes() + 3;
        for len in lens.chain([
            127,
            128,
            129,
            130,
            255,
            256,
            257,
            1000,
            i32::MAX as usize - 1,
        ]) {
            let expected = if nan_opacity(color, len) {
                // C converts the NaN to a byte (UB): Rust refuses it.
                Err(-22)
            } else {
                c_color(color, len as c_int)
            };
            assert_eq!(parse_color_prefix(color, len), expected, "{color:?} {len}");
            let got = parse_color_with_context(color, Some(len), None);
            assert_eq!(got, expected, "{color:?} {len}");
        }
    }
}

/// Whether the first `len` bytes of `color` end in an `@nan` opacity, on
/// which C reaches UB (enough for [`COLORS`], whose only such prefix is
/// `red@nan`).
fn nan_opacity(color: &CStr, len: usize) -> bool {
    let prefix = &color.to_bytes()[..len.min(color.count_bytes())];
    prefix
        .iter()
        .position(|&b| b == b'@')
        .is_some_and(|at| prefix[at + 1..].eq_ignore_ascii_case(b"nan"))
}

/// `random` and `bikeshed` agree in success and in the alpha the specifier
/// sets; the color itself is random on both sides.
#[test]
fn random_colors_agree_in_alpha() {
    for color in [
        c"random@0.5",
        c"bikeshed@0x10",
        c"RANDOM@1",
        c"#random@0",
        c"random@2",
    ] {
        let (rust, c) = (parse_color(color), c_color(color, -1));
        assert_eq!(rust.map(|p| p[3]), c.map(|p| p[3]), "{color:?}");
    }
    assert!(parse_color(c"random").is_ok() && c_color(c"random", -1).is_ok());
}

/// A C caller reusing one buffer sees the same colors as the fresh arrays
/// Rust returns, after every successful call of a sequence; failures in
/// between leave partial writes in C's buffer only.
#[test]
fn sequences_agree_after_every_success() {
    let mut buf = [0u8; 4];
    for (step, &color) in COLORS.iter().cycle().take(COLORS.len() * 3).enumerate() {
        let slen = if step % 2 == 0 {
            -1
        } else {
            color.count_bytes() as c_int
        };
        let ret = c_color_into(&mut buf, color, slen, null_mut());
        let rust = if slen < 0 {
            parse_color(color)
        } else {
            parse_color_prefix(color, slen as usize)
        };
        if ret < 0 {
            assert_eq!(rust, Err(ret), "{color:?}");
        } else {
            assert_eq!(rust, Ok(buf), "{color:?}");
        }
    }
}

/// The messages each logs at `AV_LOG_ERROR`, with its context class.
type Line = (c_int, Option<String>, String);

thread_local! {
    static LINES: RefCell<Option<Vec<Line>>> = const { RefCell::new(None) };
}

/// `line` with every context address (` @ 0x...`) masked.
fn mask_addresses(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find(" @ 0x") {
        out.push_str(&rest[..at]);
        out.push_str(" @ ADDR");
        rest = rest[at + 5..].trim_start_matches(|c: char| c.is_ascii_hexdigit());
    }
    out.push_str(rest);
    out
}

/// Captures this thread's messages while [`captured`] runs.
struct Capture;

impl Callback for Capture {
    fn log(record: Record<'_>) {
        LINES.with(|l| {
            if let Some(lines) = l.borrow_mut().as_mut() {
                let mut prefix = true;
                let line = record.to_line(&mut prefix).unwrap().into_string().unwrap();
                let class = record
                    .class()
                    .and_then(|c| c.name())
                    .map(|n| n.to_string_lossy().into_owned());
                lines.push((record.level().raw(), class, mask_addresses(&line)));
            }
        });
    }
}

/// The lines `f` logs on this thread. The caller holds the [`LogState`]
/// lock.
fn captured(f: impl FnOnce()) -> Vec<Line> {
    LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
    log::set_callback::<Capture>();
    f();
    log::set_default_callback();
    LINES.with(|l| l.borrow_mut().take().unwrap())
}

/// Runs the test `name` alone in a child process, so that no other test
/// logs through the callback it installs; true in that child. (BSan reports
/// a false use-after-free in another thread's `av_strtod` while a capturing
/// callback is installed; see `equiv/imgutils.rs::isolated`.)
fn isolated(name: &str) -> bool {
    const KEY: &str = "CRUSTIFY_EQUIV_PARSEUTILS_ISOLATED";
    if std::env::var_os(KEY).is_some() {
        return true;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--test-threads=1"])
        .env(KEY, "1")
        .status()
        .unwrap();
    assert!(status.success(), "isolated run failed: {status}");
    false
}

/// A logging context named by its class.
#[repr(C)]
struct Ctx {
    class: *const ffi::AVClass,
}

struct Class(ffi::AVClass);
// SAFETY: the class is immutable and its pointers address immutable statics.
unsafe impl Sync for Class {}

static CLASS: Class = Class(ffi::AVClass {
    class_name: c"crustify-color".as_ptr(),
    item_name: Some(ffi::av_default_item_name),
    option: ptr::null(),
    version: 0,
    log_level_offset_offset: 0,
    parent_log_context_offset: 0,
    category: ffi::AVClassCategory::AV_CLASS_CATEGORY_NA,
    get_category: None,
    query_ranges: None,
    child_next: None,
    child_class_iterate: None,
    state_flags_offset: 0,
});

#[test]
fn failure_messages_agree() {
    if !isolated("parseutils::failure_messages_agree") {
        return;
    }
    let _s = LogState::lock();
    let mut ctx = Ctx { class: &CLASS.0 };
    let obj: *mut c_void = (&raw mut ctx).cast();
    for &color in COLORS {
        let len = color.count_bytes();
        let c = captured(|| {
            let mut buf = [0; 4];
            c_color_into(&mut buf, color, -1, null_mut());
            c_color_into(&mut buf, color, len as c_int, null_mut());
            c_color_into(&mut buf, color, -1, obj);
        });
        let rust = captured(|| {
            let _ = parse_color(color);
            let _ = parse_color_prefix(color, len);
            // SAFETY: `ctx` is a live object of a well-formed class without
            // options, which nothing writes while the handle lives.
            let log_ctx = unsafe { ObjectRef::from_ptr(obj) };
            let _ = parse_color_with_context(color, None, log_ctx);
        });
        assert_eq!(rust, c, "{color:?}");
        if parse_color(color).is_err() {
            assert_eq!(c.len(), 3, "{color:?}");
            assert_eq!(c[2].1.as_deref(), Some("crustify-color"));
        }
    }
}

/// `av_parse_video_size` in C.
fn c_size(size: &CStr) -> Result<(i32, i32), i32> {
    let (mut w, mut h) = (-7, -7);
    // SAFETY: `w` and `h` are writable locals and `size` is NUL-terminated.
    let ret = unsafe { ffi::av_parse_video_size(&mut w, &mut h, size.as_ptr()) };
    if ret < 0 {
        assert_eq!((w, h), (-7, -7), "C wrote a failed size");
        Err(ret)
    } else {
        Ok((w, h))
    }
}

const ABBREVIATIONS: [&CStr; 55] = [
    c"ntsc",
    c"pal",
    c"qntsc",
    c"qpal",
    c"sntsc",
    c"spal",
    c"film",
    c"ntsc-film",
    c"sqcif",
    c"qcif",
    c"cif",
    c"4cif",
    c"16cif",
    c"qqvga",
    c"qvga",
    c"vga",
    c"svga",
    c"xga",
    c"uxga",
    c"qxga",
    c"sxga",
    c"qsxga",
    c"hsxga",
    c"wvga",
    c"wxga",
    c"wsxga",
    c"wuxga",
    c"woxga",
    c"wqhd",
    c"wqsxga",
    c"wquxga",
    c"whsxga",
    c"whuxga",
    c"cga",
    c"ega",
    c"hd480",
    c"hd720",
    c"hd1080",
    c"quadhd",
    c"2k",
    c"2kdci",
    c"2kflat",
    c"2kscope",
    c"4k",
    c"4kdci",
    c"4kflat",
    c"4kscope",
    c"nhd",
    c"hqvga",
    c"wqvga",
    c"fwqvga",
    c"hvga",
    c"qhd",
    c"uhd2160",
    c"uhd4320",
];

#[test]
fn video_sizes_agree() {
    for size in ABBREVIATIONS {
        assert!(parse_video_size(size).is_ok(), "{size:?}");
        assert_eq!(parse_video_size(size), c_size(size), "{size:?}");
        let upper = CString::new(size.to_bytes().to_ascii_uppercase()).unwrap();
        assert_eq!(parse_video_size(&upper), c_size(&upper), "{upper:?}");
    }
    for size in [
        c"640x480",
        c"640X480",
        c"640*480",
        c" 640x 480",
        c"+640x+480",
        c"-640x480",
        c"640x480 ",
        c"640x480x",
        c"640x",
        c"x480",
        c"640",
        c"0x0",
        c"0x10",
        c"1x1",
        c"2147483647x2147483647",
        c"2147483648x1",
        c"4294967297x1",
        c"4294967296x1",
        c"99999999999999999999x1",
        c"0x1f",
        c"010x010",
        c"",
        c"x",
        c"xx",
        c"1xx1",
        c"1x1x1",
        c"\t7\n8",
    ] {
        assert_eq!(parse_video_size(size), c_size(size), "{size:?}");
    }
}

/// `av_parse_time` in C.
fn c_time(s: &CStr, duration: bool) -> Result<i64, i32> {
    let mut t = 0;
    // SAFETY: `t` is a writable local and `s` is NUL-terminated.
    let ret = unsafe { ffi::av_parse_time(&mut t, s.as_ptr(), c_int::from(duration)) };
    if ret < 0 {
        assert_eq!(t, i64::MIN, "C stores INT64_MIN on failure");
        Err(ret)
    } else {
        Ok(t)
    }
}

const DURATIONS: &[&CStr] = &[
    c"0",
    c"1",
    c"1.5",
    c"-1.5",
    c"--5",
    c"-",
    c"1:02",
    c"-1:02",
    c"1:02:03",
    c"1:02:03.25",
    c"9999:59:59.999999",
    c"10000:00:00",
    c"0:60",
    c"1:2:3",
    c"01:02:03.1234567",
    c".5",
    c"5.",
    c"500ms",
    c"1.5ms",
    c"1.5us",
    c"5s",
    c"5 s",
    c"5m",
    c"5msx",
    c"9223372036854775807us",
    c"9223372036854775808us",
    c"-9223372036854775808us",
    c"--9223372036854775808us",
    c"9223372036854",
    c"9223372036855",
    c"-9223372036854.775808",
    c"9223372036854775ms",
    c"9223372036854776ms",
    c"",
    c"s",
    c"ms",
    c"+5",
    c" 5",
    c"0x10",
    c"1e3",
];

#[test]
fn durations_agree() {
    for &d in DURATIONS {
        assert_eq!(parse_duration(d), c_time(d, true), "{d:?}");
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

/// Dates that do not depend on the current time; local ones depend on the
/// time zone, which both sides share.
const DATES: &[&CStr] = &[
    c"1970-01-01T00:00:00Z",
    c"1970-01-01T00:00:00.5Z",
    c"1970-01-01 00:00:00",
    c"19700101T000000Z",
    c"19700101 000000",
    c"2020-02-29t12:34:56z",
    c"9999-12-31T23:59:59.999999Z",
    c"0000-01-01T00:00:00Z",
    c"2020-02-29T12:00:00+05:30",
    c"2020-02-29T12:00:00-0800",
    c"2020-02-29T12:00:00-08",
    c"2020-02-29T12:00:00+24",
    c"2020-02-29T12:00:00+",
    c"2020 - 1 - 2 3:4:5",
    c"2020-1-2T3:4:5Z",
    c"2020-13-01T00:00:00",
    c"2020-01-32T00:00:00",
    c"2020-01-01T24:00:00",
    c"2020-01-01T00:60:00",
    c"2020-01-01",
    c"2020-01-01T",
    c"2020-01-01T00:00:00Zjunk",
    c"2020-01-01T00:00:00Z+01",
    c"",
    c"T",
    c"Z",
    c"nowx",
    c"2020-06-15T12:00:00",
];

#[test]
fn dates_agree() {
    let _t = local_time();
    for &d in DATES {
        assert_eq!(parse_date(d), c_time(d, false), "{d:?}");
    }
}

/// Dates that take the current date or time: Rust's result lies between
/// C's before and after it (equal, unless the clock crosses a second, or
/// midnight, in between).
#[test]
fn current_dates_agree() {
    let _t = local_time();
    for d in [
        c"now",
        c"NOW",
        c"12:00:00",
        c"12:00:00Z",
        c"120000.5",
        c"T12:00:00",
        c"23:59:59.999999z",
        c"12:00:00+01:00",
        c"00:00",
    ] {
        let before = c_time(d, false);
        let rust = parse_date(d);
        let after = c_time(d, false);
        match (before, rust, after) {
            (Ok(b), Ok(r), Ok(a)) => assert!(b == r || r == a || (b <= r && r <= a), "{d:?}"),
            (b, r, a) => assert!(r == b || r == a, "{d:?}: {b:?} {r:?} {a:?}"),
        }
    }
}

/// Options wave 0, batch 3.
///
/// Equivalence of `libavutil/parseutils.h`'s URL tag lookup and named color
/// enumeration with the raw C functions, on independently owned buffers.
mod info_tags_and_colors {
    use core::ffi::{CStr, c_char, c_int};
    use core::ptr;
    use std::ffi::CString;

    use ffmpeg::libavutil::ffi;
    use ffmpeg::libavutil::parseutils::{find_info_tag, known_color_name};

    /// C's result and buffer for `tag` in `info` with a buffer of `len` bytes
    /// (at least 1), every byte `0xAA` before the call.
    fn c_lookup(len: usize, tag: &CStr, info: &CStr) -> (bool, Vec<u8>) {
        let mut buf = vec![0xAAu8; len];
        // SAFETY: `buf` holds `len >= 1` writable bytes; the strings are live.
        let ret = unsafe {
            ffi::av_find_info_tag(
                buf.as_mut_ptr().cast::<c_char>(),
                len as c_int,
                tag.as_ptr(),
                info.as_ptr(),
            )
        };
        (ret != 0, buf)
    }

    #[test]
    fn tags_are_found_and_written_as_in_c() {
        let long = format!("?{}={}&b=2", "t".repeat(200), "v".repeat(300));
        let infos = [
            "?a=1&b=x+y&c",
            "a=1&b=2",
            "?b=&b=3",
            "?c",
            "",
            "?=v&a",
            "&&a=1",
            long.as_str(),
        ];
        let tags = ["a", "b", "c", "", "z", &"t".repeat(127), &"t".repeat(128)];
        for info in infos {
            let info = CString::new(info).unwrap();
            for tag in tags {
                let tag = CString::new(tag).unwrap();
                for len in [1, 2, 3, 5, 16, 400] {
                    let (found, c_buf) = c_lookup(len, &tag, &info);
                    let mut buf = vec![0xAAu8; len];
                    let value = find_info_tag(&mut buf, &tag, &info).map(CStr::to_owned);
                    assert_eq!(value.is_some(), found, "{tag:?} in {info:?}, {len}");
                    assert_eq!(buf, c_buf, "{tag:?} in {info:?}, {len}");
                    if let Some(v) = value {
                        assert_eq!(v.as_bytes_with_nul(), &c_buf[..v.as_bytes_with_nul().len()]);
                    }
                }
                // An empty buffer finds what a one-byte one does.
                let (found, _) = c_lookup(1, &tag, &info);
                assert_eq!(find_info_tag(&mut [], &tag, &info).is_some(), found);
            }
        }
    }

    #[test]
    fn the_color_table_reads_as_in_c() {
        for i in (0..=200).chain([i32::MAX as usize]) {
            let mut rgb = ptr::null();
            // SAFETY: `rgb` is a writable slot.
            let name = unsafe { ffi::av_get_known_color_name(i as c_int, &mut rgb) };
            match known_color_name(i) {
                None => assert!(name.is_null(), "{i}"),
                Some((n, c)) => {
                    assert_eq!(n.as_ptr(), name, "{i}");
                    // SAFETY: C's non-NULL result points at a 3-byte entry.
                    assert_eq!(c, unsafe { rgb.cast::<[u8; 3]>().read() }, "{i}");
                }
            }
        }
        // C bounds-checks the index as unsigned.
        let mut rgb = ptr::null();
        // SAFETY: as above.
        assert!(unsafe { ffi::av_get_known_color_name(-1, &mut rgb) }.is_null());
    }
}
