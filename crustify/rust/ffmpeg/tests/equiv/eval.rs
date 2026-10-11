//! Equivalence of `libavutil/eval.h`'s `parse_number` with the raw
//! `av_strtod`: the value (bit for bit, NaN included) and where C stopped.

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use ffmpeg::libavutil::eval::parse_number;
use ffmpeg::libavutil::ffi;

/// `av_strtod` in C: the value and the number of bytes it consumed.
fn c_strtod(s: &CStr) -> (f64, usize) {
    let mut tail: *mut c_char = core::ptr::null_mut();
    // SAFETY: `s` is NUL-terminated and `tail` a writable local.
    let d = unsafe { ffi::av_strtod(s.as_ptr(), &mut tail) };
    (d, tail as usize - s.as_ptr() as usize)
}

fn rust_strtod(s: &CStr) -> (f64, usize) {
    let (d, rest) = parse_number(s);
    (d, s.count_bytes() - rest.count_bytes())
}

#[track_caller]
fn agree(s: &CStr) {
    let (c, rust) = (c_strtod(s), rust_strtod(s));
    assert_eq!(
        (rust.0.to_bits(), rust.1),
        (c.0.to_bits(), c.1),
        "{s:?}: {rust:?} vs {c:?}"
    );
}

#[test]
fn listed_numbers_agree() {
    for s in [
        c"",
        c" ",
        c"0",
        c"-0",
        c"1",
        c"1.5",
        c"-1.5e3",
        c"1e400",
        c"-1e400",
        c"1e-400",
        c"nan",
        c"-nan",
        c"NaN(1)",
        c"inf",
        c"-Infinity",
        c"0x",
        c"0X",
        c"0x10",
        c"0X1F",
        c"0xg",
        c"0x1p4",
        c"0x-1",
        c"0x18446744073709551615",
        c"0x18446744073709551616",
        c"18446744073709551616",
        c"1dB",
        c"20dB",
        c"-6dB",
        c"1dBB",
        c"1d",
        c"1D",
        c"1y",
        c"1z",
        c"1a",
        c"1f",
        c"1p",
        c"1n",
        c"1u",
        c"1m",
        c"1c",
        c"1d",
        c"1h",
        c"1k",
        c"1K",
        c"1M",
        c"1G",
        c"1T",
        c"1P",
        c"1E",
        c"1Z",
        c"1Y",
        c"1Ki",
        c"1Mi",
        c"1Gi",
        c"1Ti",
        c"1Pi",
        c"1Ei",
        c"1Zi",
        c"1Yi",
        c"1mi",
        c"1ui",
        c"1KB",
        c"1KiB",
        c"1B",
        c"1BB",
        c"1i",
        c"1q",
        c"1F",
        c"1e",
        c"1e+",
        c"1.5Gi rest",
        c".",
        c"+.5k",
        c"  -.5M",
        c"k",
        c"B",
        c"dB",
        c"\x7f",
        c"\xff",
        c"1\xc3\xa9",
        c"1[",
        c"1`",
        c"1{",
        c"1z!",
        c"0xffK",
        c"0x10KiB",
    ] {
        agree(s);
    }
}

/// Whether the suite runs under BorrowSanitizer.
///
/// BSan reports a false out-of-bounds access at `av_strtod`'s `next[0]`
/// (`eval.c:120`) after many calls in one thread: `next` is an
/// uninitialised local that uninstrumented glibc `strtod` writes, so its
/// shadow keeps the provenance a freed earlier frame left in the slot,
/// while the byte C reads lies inside the live string (ASan agrees). The
/// exhaustive sweeps skip under BSan; the listed inputs still run.
fn under_bsan() -> bool {
    std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
}

/// Every string of up to four bytes over the bytes that steer C's number
/// and suffix handling.
#[test]
fn short_strings_agree() {
    if under_bsan() {
        return;
    }
    let alphabet = b"01x.-e9dBEKiMmz Y";
    let mut s = Vec::new();
    for len in 0..=4u32 {
        for mut i in 0..alphabet.len().pow(len) {
            s.clear();
            for _ in 0..len {
                s.push(alphabet[i % alphabet.len()]);
                i /= alphabet.len();
            }
            agree(&CString::new(s.as_slice()).unwrap());
        }
    }
}

/// Every byte from `'E'` to `'z'` after a number, alone and before `i` and
/// `B`: C indexes its SI table with it.
#[test]
fn every_suffix_byte_agrees() {
    if under_bsan() {
        return;
    }
    for b in 1..=255u8 {
        for tail in [&b""[..], b"i", b"B", b"iB", b"x"] {
            let s = CString::new([&b"3"[..], &[b], tail].concat()).unwrap();
            agree(&s);
        }
    }
}
