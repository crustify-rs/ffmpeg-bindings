// A plane's buffer reference is borrowed from its frame, so it cannot
// outlive it.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    m.set_pixel_format(PixelFormat::GRAY8);
    m.set_width(4);
    m.set_height(4);
    m.alloc_buffers(0).unwrap();
    let buf = frame.as_ref().plane_buffer(0).unwrap();
    drop(frame);
    assert_eq!(buf.len(), 0);
}
