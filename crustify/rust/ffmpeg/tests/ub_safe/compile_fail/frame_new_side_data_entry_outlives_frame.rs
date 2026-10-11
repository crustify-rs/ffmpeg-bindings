// An appended side data entry is lent by its frame for the frame's
// exclusive borrow only.
use ffmpeg::libavutil::frame::{Frame, SideDataType};

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    let entry = m.alloc_side_data_zeroed(SideDataType::AFD, 1).unwrap();
    drop(frame);
    assert_eq!(entry.as_ref().data(), [0]);
}
