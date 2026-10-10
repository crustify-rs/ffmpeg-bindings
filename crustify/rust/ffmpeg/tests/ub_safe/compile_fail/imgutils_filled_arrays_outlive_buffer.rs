// Planes laid out in a buffer cannot outlive it.
use ffmpeg::libavutil::imgutils::fill_arrays_mut;
use ffmpeg::libavutil::pixfmt::PixelFormat;

fn main() {
    let planes;
    {
        let mut buf = vec![0u8; 64];
        planes = fill_arrays_mut(&mut buf, PixelFormat::GRAY8, 8, 8, 1).unwrap();
    }
    planes.data[0][0] = 1;
}
