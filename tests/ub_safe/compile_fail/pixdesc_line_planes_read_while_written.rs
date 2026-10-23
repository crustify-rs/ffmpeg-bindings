//! A line write borrows the planes exclusively: a read view of the same
//! planes cannot be live across it.

use ffmpeg_bindings::libavutil::libavutil::imgutils::PlanesMut;
use ffmpeg_bindings::libavutil::libavutil::pixdesc::{read_image_line, write_image_line};
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormat;

fn main() {
    let desc = PixelFormat::GRAY8.descriptor().unwrap();
    let mut row = [0u8; 4];
    let mut planes = PlanesMut {
        data: [&mut row[..], &mut [], &mut [], &mut []],
        linesizes: [4, 0, 0, 0],
    };
    let view = planes.as_planes();
    write_image_line(&[1u16; 4], &mut planes, desc, 0, 0, 0).unwrap();
    let mut back = [0u16; 4];
    read_image_line(&mut back, view, desc, 0, 0, 0, false).unwrap();
}
