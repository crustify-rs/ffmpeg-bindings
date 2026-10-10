//! A line read fills a destination borrowed exclusively, so it cannot be a
//! plane the same call reads.

use ffmpeg::libavutil::imgutils::Planes;
use ffmpeg::libavutil::pixdesc::read_image_line;
use ffmpeg::libavutil::pixfmt::PixelFormat;

fn main() {
    let desc = PixelFormat::GRAY16LE.descriptor().unwrap();
    let mut row = [0u16; 4];
    let bytes: &[u8] = view_of(&row);
    let planes = Planes {
        data: [bytes, &[], &[], &[]],
        linesizes: [8, 0, 0, 0],
    };
    read_image_line(&mut row, planes, desc, 0, 0, 0, false).unwrap();
}

/// Any byte view borrowed from `r`.
fn view_of(r: &[u16; 4]) -> &[u8] {
    let _ = r;
    &[]
}
