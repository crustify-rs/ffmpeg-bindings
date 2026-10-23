// A channel cannot be written while a shared handle to it lives.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::{Channel, ChannelCustom};

fn main() {
    let mut ch = ChannelCustom::new(Channel::FRONT_LEFT);
    let r = ch.as_ref();
    ch.as_mut().set_name(c"x").unwrap();
    println!("{:?}", r.name());
}
