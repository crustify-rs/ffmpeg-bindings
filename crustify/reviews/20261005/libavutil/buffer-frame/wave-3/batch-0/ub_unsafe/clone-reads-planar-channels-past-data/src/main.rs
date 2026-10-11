//! A frame of planar audio with more channels than `data` holds, whose
//! external planes the caller lent through `set_data_ptr`, makes
//! `FrameRef::try_clone` let C read plane pointers past the frame's `data`
//! array.
//!
//! The frame is not reference counted (no buffer at index 0), so
//! `av_frame_clone` copies its data: `av_frame_copy` -> `frame_copy_audio`
//! reads `src->extended_data[i]` for every one of the layout's channels.
//! The frame's `extended_data` is its own eight-entry `data` array, so
//! channel 8's "pointer" is the next field, `linesize[0..2]`, and
//! `av_samples_copy` copies from that address.
//!
//! Every plane the caller stored is valid for as long as the frame lives,
//! for whatever C reads through the frame's geometry, as `set_data_ptr`'s
//! `# Safety` asks; everything else is safe setters.
//!
//! `cargo run` re-executes itself with `ASAN_OPTIONS=halt_on_error=1` and
//! exits with the child's status: non-zero, with an ASan SEGV report, on the
//! affected commit.

use ffmpeg::libavutil::channel_layout::ChannelLayout;
use ffmpeg::libavutil::frame::{Frame, NUM_DATA_POINTERS};
use ffmpeg::libavutil::samplefmt::SampleFormat;

const CHILD: &str = "CRUSTIFY_REPRODUCER_CHILD";
const SAMPLES: usize = 64;

fn scenario() {
    // One plane per `data` entry, each a channel of SAMPLES U8 samples, kept
    // alive past the frame.
    let mut planes: Vec<Vec<u8>> = (0..NUM_DATA_POINTERS)
        .map(|c| vec![c as u8; SAMPLES])
        .collect();
    let mut frame = Frame::new().expect("a frame");
    let mut m = frame.as_mut();
    m.set_sample_format(SampleFormat::U8P);
    m.set_sample_count(SAMPLES as i32);
    // C's convention for planar audio: linesize[0] is each plane's size.
    m.set_linesize(0, SAMPLES as i32);
    // Nine channels: one more than `data` holds.
    m.set_channel_layout(ChannelLayout::unspecified(NUM_DATA_POINTERS as i32 + 1))
        .expect("no separate extended_data yet");
    for (i, plane) in planes.iter_mut().enumerate() {
        // SAFETY: the frame holds no buffer; `plane` holds the SAMPLES bytes
        // C reads for a U8P channel, outlives the frame and is written by
        // nothing else meanwhile.
        unsafe { m.set_data_ptr(i, plane.as_mut_ptr()) };
    }
    eprintln!("cloning a 9-channel planar frame whose extended_data is data");
    let copy = frame.as_ref().try_clone();
    eprintln!(
        "no report: the clone {}",
        if copy.is_some() { "succeeded" } else { "was refused" }
    );
    drop((copy, frame, planes));
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
