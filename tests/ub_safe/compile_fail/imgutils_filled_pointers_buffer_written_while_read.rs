// A buffer cannot be written while planes laid out in it are read.
use ffmpeg_bindings::libavutil::libavutil::imgutils::fill_pointers;
use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

fn main() {
    let mut buf = vec![0u8; 64];
    let planes = fill_pointers(&buf, PixelFormat::GRAY8, 8, [8, 0, 0, 0]).unwrap();
    buf[0] = 1;
    println!("{}", planes.data[0][0]);
}
