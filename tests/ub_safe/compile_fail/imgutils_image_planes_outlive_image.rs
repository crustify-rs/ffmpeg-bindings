// An image's planes cannot outlive the image whose block holds them.
use ffmpeg_bindings::libavutil::libavutil::imgutils::Image;
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;

fn main() {
    let planes;
    {
        let image = Image::alloc(8, 8, PixelFormat::GRAY8, 1).unwrap();
        planes = image.planes();
    }
    println!("{}", planes.data[0].len());
}
