// The frame's writable side data excludes reading the frame meanwhile.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    let entries = m.side_data_mut();
    let pts = frame.as_ref().pts();
    drop(entries);
    assert_eq!(pts, 0);
}
