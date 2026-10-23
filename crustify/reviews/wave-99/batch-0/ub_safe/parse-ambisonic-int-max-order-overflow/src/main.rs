//! `ChannelLayout::parse` of an `ambisonic <order>` whose order reads as
//! `INT_MAX`: C's range check adds one to it (`channel_layout.c:341`) and
//! squares the wrapped sum (`:346`), signed overflows UBSan reports, after
//! which C returns success with an ambisonic layout of no channels.
//!
//! Build with `CRUSTIFY_FFMPEG_VARIANT=asan-ubsan`,
//! `RUSTFLAGS=-Zsanitizer=address` and run with the sanitized libraries on
//! `LD_LIBRARY_PATH` (see README.md). UBSan recovers by default, so `main`
//! re-runs itself with `UBSAN_OPTIONS=halt_on_error=1` and exits with the
//! child's status: non-zero on the affected commit, zero once the wrapper
//! refuses the input.
#![forbid(unsafe_code)]

use std::env;
use std::process::{Command, exit};

use ffmpeg::libavutil::libavutil::channel_layout::ChannelLayout;

const DESCRIPTIONS: [&core::ffi::CStr; 6] = [
    c"ambisonic 2147483647",
    c"ambisonic 0x7fffffff",
    c"ambisonic  +2147483647",
    c"ambisonic -2147483649",
    c"ambisonic 2147483647+stereo",
    c"ambisonic 1+ambisonic 2147483647",
];

fn main() {
    if env::var_os("UBSAN_OPTIONS").is_none() {
        let status = Command::new(env::current_exe().unwrap())
            .env("UBSAN_OPTIONS", "halt_on_error=1")
            .status()
            .unwrap();
        exit(status.code().unwrap_or(1));
    }
    for description in DESCRIPTIONS {
        match ChannelLayout::parse(description) {
            Ok(layout) => println!("{description:?}: accepted as {:?}", layout.as_ref()),
            Err(e) => println!("{description:?}: refused with {e}"),
        }
    }
}
