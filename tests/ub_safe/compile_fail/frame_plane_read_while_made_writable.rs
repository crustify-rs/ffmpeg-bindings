// `make_writable` may replace the buffers a plane view reads, so no view may
// live across it.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;
use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    m.set_pixel_format(PixelFormat::GRAY8);
    m.set_width(4);
    m.set_height(4);
    m.alloc_buffers(0).unwrap();
    let plane = frame.as_ref().plane(0).unwrap();
    frame.as_mut().make_writable().unwrap();
    assert_eq!(plane[0], 0);
}
