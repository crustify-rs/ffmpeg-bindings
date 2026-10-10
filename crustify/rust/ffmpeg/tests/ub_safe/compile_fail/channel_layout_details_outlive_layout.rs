// A custom map read through `details` cannot outlive its layout, which frees
// the map when it drops.
use ffmpeg::libavutil::channel_layout::{ChannelLayout, LayoutDetails};

fn main() {
    let details = {
        let layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
        layout.as_ref().details()
    };
    if let LayoutDetails::Custom(map) = details {
        println!("{:?}", map.get(0).map(|c| c.id()));
    }
}
