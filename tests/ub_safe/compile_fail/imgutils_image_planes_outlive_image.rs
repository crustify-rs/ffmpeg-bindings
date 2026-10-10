// An image's planes cannot outlive the image whose block holds them.
use ffmpeg::libavutil::imgutils::Image;
use ffmpeg::libavutil::pixfmt::PixelFormat;

fn main() {
    let planes;
    {
        let image = Image::alloc(8, 8, PixelFormat::GRAY8, 1).unwrap();
        planes = image.planes();
    }
    println!("{}", planes.data[0].len());
}
