// An entry found by kind cannot outlive a removal from its array.
use ffmpeg::libavutil::frame::{SideDataArray, SideDataType};

fn main() {
    let mut array = SideDataArray::new();
    let found = array.as_ref().find(SideDataType::AFD);
    array.as_mut().remove(SideDataType::AFD);
    println!("{:?}", found.map(|e| e.len()));
}
