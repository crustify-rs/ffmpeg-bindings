// A layout cannot be retyped, which may free its map, while a view of the
// map is held.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::{ChannelLayout, ChannelOrder, LayoutDetails};

fn main() {
    let mut layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
    let details = layout.as_ref().details();
    let _ = layout.as_mut().retype(ChannelOrder::NATIVE);
    if let LayoutDetails::Custom(map) = details {
        println!("{:?}", map.get(0).map(|c| c.name()));
    }
}
