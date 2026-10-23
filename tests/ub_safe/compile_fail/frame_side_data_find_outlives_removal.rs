// An entry found by kind cannot outlive a removal from its array.
use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;

use ffmpeg_bindings::libavutil::libavutil::side_data::SideDataArray;

fn main() {
    let mut array = SideDataArray::new();
    let found = array.as_ref().find(SideDataType::AFD);
    array.as_mut().remove(SideDataType::AFD);
    println!("{:?}", found.map(|e| e.len()));
}
