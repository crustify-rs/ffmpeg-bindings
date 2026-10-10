// A name's `CStr` borrows the copy `name()` returned, which must be kept.
use ffmpeg::libavutil::channel_layout::{Channel, ChannelCustom};

fn main() {
    let ch = ChannelCustom::new(Channel::FRONT_LEFT);
    let s = ch.as_ref().name().as_c_str();
    println!("{s:?}");
}
