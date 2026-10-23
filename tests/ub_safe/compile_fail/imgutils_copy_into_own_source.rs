// An image cannot be copied onto itself: its source planes are borrowed
// while its destination planes are.
use ffmpeg_bindings::libavutil::libavutil::imgutils::{Image, copy};
use ffmpeg_bindings::libavutil::libavutil::pixfmt::PixelFormat;

fn main() {
    let mut image = Image::alloc(8, 8, PixelFormat::GRAY8, 1).unwrap();
    let _ = copy(&mut image.planes_mut(), image.planes(), PixelFormat::GRAY8, 8, 8);
}
