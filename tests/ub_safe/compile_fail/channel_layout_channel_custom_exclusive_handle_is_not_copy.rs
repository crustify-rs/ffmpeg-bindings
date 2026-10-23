// An exclusive handle cannot be duplicated into two writers.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::{Channel, ChannelCustom};

fn main() {
    let mut ch = ChannelCustom::new(Channel::FRONT_LEFT);
    let mut a = ch.as_mut();
    let mut b = a;
    a.set_id(Channel::FRONT_RIGHT);
    b.set_id(Channel::BACK_LEFT);
}
