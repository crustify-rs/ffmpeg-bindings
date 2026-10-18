//! `FrameMut::alloc_extended_data` copies external plane pointers lent
//! through `set_data_ptr` into the frame's new `extended_data` array, where
//! they outlive the loan: once the caller replaces the plane and frees the
//! old memory, as `set_data_ptr`'s `# Safety` allows ("until the plane is
//! cleared or replaced"), the array still points at it, and
//! `FrameRef::try_clone` lets C copy from the freed block.
//!
//! The frame is stereo planar audio that is not reference counted, so
//! `av_frame_clone` copies its data through `extended_data`
//! (`av_frame_copy` -> `frame_copy_audio` -> `av_samples_copy`).
//!
//! `cargo run` re-executes itself with `ASAN_OPTIONS=halt_on_error=1` and
//! exits with the child's status: non-zero, with an ASan
//! heap-use-after-free report, on the affected commit.

use ffmpeg::libavutil::channel_layout::ChannelLayout;
use ffmpeg::libavutil::frame::Frame;
use ffmpeg::libavutil::samplefmt::SampleFormat;

const CHILD: &str = "CRUSTIFY_REPRODUCER_CHILD";
const SAMPLES: usize = 64;

fn scenario() {
    let mut left = vec![1u8; SAMPLES];
    let mut right = vec![2u8; SAMPLES];
    let mut frame = Frame::new().expect("a frame");
    let mut m = frame.as_mut();
    m.set_sample_format(SampleFormat::U8P);
    m.set_sample_count(SAMPLES as i32);
    m.set_linesize(0, SAMPLES as i32);
    m.set_channel_layout(ChannelLayout::unspecified(2))
        .expect("no separate extended_data yet");
    // SAFETY: the frame holds no buffer; each block holds the SAMPLES bytes
    // C reads for a U8P channel, and stays alive and unwritten elsewhere
    // until its plane is replaced or the frame dropped.
    unsafe {
        m.set_data_ptr(0, left.as_mut_ptr());
        m.set_data_ptr(1, right.as_mut_ptr());
    }
    m.alloc_extended_data().expect("an extended_data array");
    // Replace the left channel's plane; its old block's loan ends here.
    let mut new_left = vec![3u8; SAMPLES];
    // SAFETY: as above, for the new block, which outlives the frame.
    unsafe { m.set_data_ptr(0, new_left.as_mut_ptr()) };
    drop(left);
    eprintln!("cloning after replacing plane 0 and freeing its old block");
    let copy = frame.as_ref().try_clone();
    eprintln!(
        "no report: the clone {}",
        if copy.is_some() { "succeeded" } else { "was refused" }
    );
    drop((copy, frame, new_left, right));
}

fn main() {
    if std::env::var_os(CHILD).is_some() {
        scenario();
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .env(CHILD, "1")
        .env("ASAN_OPTIONS", "halt_on_error=1:detect_leaks=1")
        .env("UBSAN_OPTIONS", "halt_on_error=1")
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
