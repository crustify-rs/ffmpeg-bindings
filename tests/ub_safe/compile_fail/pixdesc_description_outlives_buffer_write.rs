// A pixel format's description is borrowed from the buffer it was written
// into, which cannot be overwritten while it is read.
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;

fn main() {
    let mut buf = [0u8; 40];
    let text = PixelFormat::YUV420P.describe(&mut buf).unwrap();
    buf[0] = b'x';
    println!("{text:?} {}", buf[0]);
}
