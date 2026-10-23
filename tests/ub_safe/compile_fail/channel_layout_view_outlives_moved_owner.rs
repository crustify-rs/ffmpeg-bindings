// The shared view of a layout borrows its owner: the owner cannot be moved
// into a thread while the view is in use.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::ChannelLayout;

fn main() {
    let layout = ChannelLayout::custom(2).unwrap();
    let view = layout.as_ref();
    std::thread::spawn(move || drop(layout));
    println!("{}", view.channel_count());
}
