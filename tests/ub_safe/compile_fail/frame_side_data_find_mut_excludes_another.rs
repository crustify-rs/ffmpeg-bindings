// Entries found writable exclude one another, even of the same kind.
use ffmpeg::libavutil::frame::{SideDataArray, SideDataType};

fn main() {
    let mut array = SideDataArray::new();
    let mut entries = array.as_mut();
    let first = entries.find_mut(SideDataType::AFD);
    let second = entries.find_mut(SideDataType::AFD);
    drop((first, second));
}
