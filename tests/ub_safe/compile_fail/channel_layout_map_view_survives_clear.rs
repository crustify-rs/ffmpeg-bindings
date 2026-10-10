// The map lent by `custom_map_mut` borrows the layout exclusively: the layout
// cannot be cleared (freeing the map) while the view lives.
use ffmpeg::libavutil::channel_layout::ChannelLayout;

fn main() {
    let mut layout = ChannelLayout::custom(2).unwrap();
    let mut handle = layout.as_mut();
    let mut map = handle.custom_map_mut().unwrap();
    handle.clear();
    map.get_mut(0).unwrap().set_name(c"x").unwrap();
}
