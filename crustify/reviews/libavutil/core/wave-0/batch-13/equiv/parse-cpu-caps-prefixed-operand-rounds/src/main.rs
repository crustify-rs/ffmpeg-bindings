//! `parse_cpu_caps` against `av_parse_cpu_caps` on `+`/`-` operands that are
//! not integers: C truncates the operand to an `int64_t` before combining
//! it, while the wrapper on the affected commit evaluated it through C's
//! unprefixed path, which rounds to the nearest integer. The two sides then
//! disagree on which bit changes, and this program exits 1 at the first
//! mismatch. With the fix every comparison holds and it exits 0.
//!
//! Every operand here lies below 2^63 in magnitude, so C converts it without
//! undefined behaviour.
use core::ffi::CStr;
use std::process::ExitCode;

use ffmpeg::libavutil::cpu::parse_cpu_caps;
use ffmpeg::libavutil::ffi;

/// `av_parse_cpu_caps` in C, on its own flag word.
fn c_parse(flags: &mut u32, s: &CStr) -> Result<(), i32> {
    // SAFETY: `flags` is a live, writable word and `s` a live NUL-terminated
    // string, both used for the call only; the operands are finite and below
    // 2^63 in magnitude.
    let ret = unsafe { ffi::av_parse_cpu_caps(flags, s.as_ptr()) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

fn main() -> ExitCode {
    let mut status = ExitCode::SUCCESS;
    for s in [
        c"-0.999",
        c"+1.9995",
        c"-1.9995",
        c"+2.5",
        c"-0.5",
        c"+PI",
        c"-5G",
        c"-min",
        c"-0x100000000",
        c"sse2+2.5-0.999",
    ] {
        let (mut c, mut r) = (0x55u32, 0x55u32);
        let cret = c_parse(&mut c, s);
        let rret = parse_cpu_caps(&mut r, s);
        let verdict = if (cret, c) == (rret, r) {
            "agree"
        } else {
            status = ExitCode::FAILURE;
            "DIFFER"
        };
        println!("{s:?} from 0x55: C {cret:?} -> {c:#x}, Rust {rret:?} -> {r:#x}: {verdict}");
    }
    status
}
