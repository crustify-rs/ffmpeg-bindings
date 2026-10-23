// A transfer writes the frame it was given exclusively, so its source cannot
// be the same frame.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let _ = unsafe { frame.as_mut().transfer_data_from(frame.as_ref()) };
}
