// A layout owning a map is not byte-copied: copies go through
// `try_clone` (`av_channel_layout_copy`).
use ffmpeg::libavutil::channel_layout::ChannelLayout;

fn main() {
    let layout = ChannelLayout::parse(c"FL@a+FR@b").unwrap().into_inner();
    let twin = layout.clone();
    drop((layout, twin));
}
