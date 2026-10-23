// An entry an allocation returns cannot outlive its array.
use ffmpeg_bindings::libavutil::libavutil::frame::SideDataType;

use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

fn main() {
    let mut array = SideDataArray::new();
    let entry = array.alloc_zeroed(SideDataType::AFD, 4, SideDataFlags::NONE);
    drop(array);
    drop(entry);
}
