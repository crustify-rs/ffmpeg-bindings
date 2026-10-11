// A frame's buffer reference is borrowed from it, so it cannot outlive it.
use ffmpeg::libavutil::buffer::BufferReference;
use ffmpeg::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    frame
        .as_mut()
        .set_buffer(0, Some(BufferReference::alloc_zeroed(8).unwrap()));
    let buf = frame.as_ref().buffer(0).unwrap();
    drop(frame);
    assert_eq!(buf.len(), 8);
}
