// A frame cannot move into itself: both handles are exclusive.
use ffmpeg::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut dst = frame.as_mut();
    dst.move_from(frame.as_mut());
}
