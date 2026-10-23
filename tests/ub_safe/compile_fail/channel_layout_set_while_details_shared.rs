// A layout cannot be replaced while a view of its map is held.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::{ChannelLayout, LayoutDetails};

fn main() {
    let mut layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap();
    let details = layout.as_ref().details();
    layout.as_mut().set_native(0x3);
    if let LayoutDetails::Custom(map) = details {
        println!("{}", map.len());
    }
}
