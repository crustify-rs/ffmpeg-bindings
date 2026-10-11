//! UB tests for `libavutil/eval.h`'s safe `parse_number`: suffix bytes at the
//! very end of exactly sized heap strings, so that ASan sees any read past
//! the NUL, and a rest that stays inside its input.

use std::ffi::{CStr, CString};

use ffmpeg::libavutil::eval::parse_number;

/// `s` in a heap block of exactly its length plus the NUL.
fn exact(s: &[u8]) -> Box<CStr> {
    CString::new(s).unwrap().into_boxed_c_str()
}

const NUMBERS: [&[u8]; 30] = [
    b"",
    b"0",
    b"0x",
    b"0X",
    b"0xg",
    b"0x1p3",
    b"1d",
    b"1dB",
    b"1dBB",
    b"1K",
    b"1Ki",
    b"1KiB",
    b"1E",
    b"1e",
    b"1e+",
    b"1e400",
    b"-1e400Y",
    b"1y",
    b"1z",
    b"1i",
    b"1B",
    b"nan",
    b"-inf",
    b"infinity",
    b"  +.5",
    b".",
    b"\xff",
    b"18446744073709551616",
    b"0x18446744073709551616",
    b"1\xc3\xa9",
];

#[test]
fn rest_is_a_suffix_of_the_input() {
    for n in NUMBERS {
        let s = exact(n);
        let (_, rest) = parse_number(&s);
        let rest = rest.to_bytes();
        assert!(n.ends_with(rest), "{n:?}");
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

/// Every string of up to three bytes over the bytes that steer C's suffix
/// handling, each in its own exactly sized block.
#[test]
fn short_strings() {
    if under_bsan() {
        return;
    }
    let alphabet = b"0x1.dBEeKiMm+-zY";
    let mut s = Vec::new();
    for len in 0..=3u32 {
        for mut i in 0..alphabet.len().pow(len) {
            s.clear();
            for _ in 0..len {
                s.push(alphabet[i % alphabet.len()]);
                i /= alphabet.len();
            }
            let input = exact(&s);
            let (_, rest) = parse_number(&input);
            assert!(s.ends_with(rest.to_bytes()));
        }
    }
}

/// The rest outlives every other borrow of the input but the input itself.
#[test]
fn rest_borrows_the_input() {
    let s = exact(b"12k rest");
    let rest = {
        let (value, rest) = parse_number(&s);
        assert_eq!(value, 12_000.0);
        rest
    };
    assert_eq!(rest, c" rest");
}

#[test]
fn many_threads() {
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..500 {
                    assert_eq!(parse_number(c"1.5KiB!"), (12288.0, c"!"));
                }
            });
        }
    });
}
