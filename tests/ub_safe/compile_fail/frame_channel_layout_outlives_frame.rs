// A frame's channel layout is a field of it, borrowed with it.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;

fn main() {
    let frame = Frame::new().unwrap();
    let layout = frame.as_ref().channel_layout();
    drop(frame);
    assert_eq!(layout.channel_count(), 0);
}
