//! A view over a frame's regions of interest borrows the frame: it cannot be
//! read once the frame is gone.
#![forbid(unsafe_code)]

use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, RegionOfInterest, SideDataType};

fn main() {
    let mut frame = Frame::new().unwrap();
    frame
        .as_mut()
        .add_regions_of_interest(&[RegionOfInterest::default()])
        .unwrap();
    let f = frame.as_ref();
    let view = f
        .find_side_data(SideDataType::REGIONS_OF_INTEREST)
        .unwrap()
        .regions_of_interest()
        .unwrap();
    drop(frame);
    assert_eq!(view.len(), 1);
}
