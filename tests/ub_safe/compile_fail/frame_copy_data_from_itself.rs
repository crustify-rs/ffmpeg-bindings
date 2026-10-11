// A frame cannot copy its planes from itself: the destination is borrowed
// exclusively, the source shared.
use ffmpeg::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut dst = frame.as_mut();
    dst.copy_data_from(frame.as_ref()).unwrap();
}
