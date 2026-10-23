// An array cannot clone one of its own entries into itself while lending it.
use ffmpeg_bindings::libavutil::libavutil::side_data::{SideDataArray, SideDataFlags};

fn main() {
    let mut array = SideDataArray::new();
    let src = array.as_ref().get(0).unwrap();
    let _ = array.as_mut().add_clone(src, SideDataFlags::NONE);
}
