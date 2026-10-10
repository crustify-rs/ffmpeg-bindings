// An image's planes cannot be read while they are borrowed for writing.
use ffmpeg::libavutil::imgutils::Image;
use ffmpeg::libavutil::pixfmt::PixelFormat;

fn main() {
    let mut image = Image::alloc(8, 8, PixelFormat::GRAY8, 1).unwrap();
    let written = image.planes_mut();
    let read = image.planes();
    written.data[0][0] = 1;
    println!("{}", read.data[0][0]);
}
