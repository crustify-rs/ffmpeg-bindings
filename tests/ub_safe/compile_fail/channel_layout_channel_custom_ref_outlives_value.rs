// A shared handle cannot outlive the channel it borrows.
use ffmpeg_bindings::libavutil::libavutil::channel_layout::{Channel, ChannelCustom};

fn main() {
    let r = {
        let ch = ChannelCustom::new(Channel::FRONT_LEFT);
        ch.as_ref()
    };
    println!("{:?}", r.id());
}
