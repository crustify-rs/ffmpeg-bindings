//! `OptionDefRef::eval_flags` of a flags value whose 255-byte first token
//! ends in a digit and whose second token is `ld(1e300)`.
//!
//! C's `set_string_number` cuts an unsigned flags value into tokens of at
//! most 255 bytes, so the second token starts in the middle of what the
//! wrapper's refusal (`eval::refuses` over the whole value) reads as one
//! number, `0ld`, and it never sees the `ld(` call. C evaluates
//! `ld(1e300)` as its own expression and converts `1e300` to an `int` for
//! `av_clip` (`eval.c:205`): UBSan `float-cast-overflow`.
//!
//! The class is C's `cpuflags` class, which the callback reads from the
//! record `cpu::parse_cpu_caps` logs; every call is safe.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address` and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md). UBSan recovers by default, so `main`
//! re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and exits with the
//! child's status: non-zero on the affected commit, zero once the wrapper
//! refuses the value.
#![forbid(unsafe_code)]

use std::cell::Cell;
use std::env;
use std::ffi::CString;
use std::process::{Command, exit};

use ffmpeg::libavutil::libavutil::cpu;
use ffmpeg::libavutil::libavutil::log::{self, Callback, Record};
use ffmpeg::libavutil::libavutil::opt::SearchFlags;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// The value: `0*` and 253 zeros (255 bytes, evaluating to 0), then
/// `ld(1e300)`. No `+` or `-`, so C splits it only at the 255-byte limit.
fn value() -> CString {
    let mut v = b"0*".to_vec();
    v.extend([b'0'; 253]);
    v.extend(b"ld(1e300)");
    CString::new(v).unwrap()
}

struct Probe;

impl Callback for Probe {
    fn log(record: Record<'_>) {
        if !ARMED.with(Cell::get) {
            return;
        }
        let Some(class) = record.class() else { return };
        if class.name() != Some(c"cpuflags") {
            return;
        }
        ARMED.with(|a| a.set(false));
        let option = class.find(c"flags", None, 0, SearchFlags::NONE).unwrap();
        let mut flags = 0;
        let ret = option.eval_flags(class, &value(), &mut flags);
        println!("eval_flags: {ret:?}, flags {flags:#x}");
    }
}

fn main() {
    if env::var_os("UBSAN_OPTIONS").is_none() {
        let status = Command::new(env::current_exe().unwrap())
            .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
            .status()
            .unwrap();
        exit(status.code().unwrap_or(1));
    }
    log::set_callback::<Probe>();
    ARMED.with(|a| a.set(true));
    let mut caps = 0;
    // Fails in C, which logs in the `cpuflags` class's name.
    let _ = cpu::parse_cpu_caps(&mut caps, c"no-such-flag");
    log::set_default_callback();
}
