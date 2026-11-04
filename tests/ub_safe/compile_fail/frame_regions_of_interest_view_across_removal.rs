//! A view over a frame's regions of interest freezes the frame: its side
//! data cannot be removed while the view lives.
#![forbid(unsafe_code)]

use ffmpeg_bindings::libavutil::libavutil::frame::{Frame, RegionOfInterest, SideDataType};

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    m.add_regions_of_interest(&[RegionOfInterest::default()])
        .unwrap();
    let view = m
        .as_ref()
        .find_side_data(SideDataType::REGIONS_OF_INTEREST)
        .unwrap()
        .regions_of_interest()
        .unwrap();
    m.remove_side_data(SideDataType::REGIONS_OF_INTEREST);
    assert_eq!(view.len(), 1);
}
