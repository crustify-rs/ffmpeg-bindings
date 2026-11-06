//! Safe `FrameMut::make_writable` on an audio frame of `INT_MAX - 31`
//! samples reaches a signed `int` overflow in C.
//!
//! The wrapper copies a frame that is not writable into buffers C allocates
//! with `av_frame_get_buffer(copy, 0)`. For audio, align 0 makes
//! `av_samples_get_buffer_size` round the sample count up to a multiple of
//! 32 with `FFALIGN(nb_samples, 32)`, i.e. `nb_samples + 32 - 1` in `int`
//! (`libavutil/samplefmt.c:137`), after a range check (`nb_samples >
//! INT_MAX - 31`) that lets `INT_MAX - 31` through. `alloc_buffers` guards
//! this count; the copy paths (`make_writable`, `try_clone_from`,
//! `try_clone`) do not.
//!
//! `cargo run` (ASan/UBSan build) exits non-zero on the affected commit:
//! `main` re-runs itself with `UBSAN_OPTIONS=halt_on_error=1`, so the
//! recovered UBSan report becomes the child's exit status.

#![forbid(unsafe_code)]

use std::env;
use std::process::{Command, exit};

use ffmpeg::libavutil::libavutil::channel_layout::ChannelLayout;
use ffmpeg::libavutil::libavutil::frame::Frame;
use ffmpeg::libavutil::libavutil::samplefmt::SampleFormat;

const CHILD: &str = "CRUSTIFY_REPRO_CHILD";

fn scenario() {
    let mut frame = Frame::new().expect("a frame");
    let mut m = frame.as_mut();
    m.set_sample_format(SampleFormat::U8);
    m.set_sample_count(i32::MAX - 31);
    m.set_channel_layout(ChannelLayout::default_for(1))
        .expect("a layout");
    // Not reference counted, so not writable: the wrapper copies it.
    let ret = m.make_writable();
    eprintln!("make_writable: {ret:?}");
}

fn main() {
    if env::var_os(CHILD).is_some() {
        scenario();
        return;
    }
    let status = Command::new(env::current_exe().unwrap())
        .env(CHILD, "1")
        .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
        .status()
        .expect("the child runs");
    exit(status.code().unwrap_or(1));
}
