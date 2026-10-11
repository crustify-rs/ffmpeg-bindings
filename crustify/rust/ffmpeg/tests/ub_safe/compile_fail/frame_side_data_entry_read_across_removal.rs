// A side data entry found on a frame is borrowed from it, so it cannot live
// across a removal that may free it.
use ffmpeg::libavutil::frame::{Frame, SideDataType};

fn main() {
    let mut frame = Frame::new().unwrap();
    frame
        .as_mut()
        .alloc_side_data_from_slice(SideDataType::AFD, &[1])
        .unwrap();
    let entry = frame.as_ref().find_side_data(SideDataType::AFD).unwrap();
    frame.as_mut().remove_side_data(SideDataType::AFD);
    assert_eq!(entry.data(), [1]);
}
