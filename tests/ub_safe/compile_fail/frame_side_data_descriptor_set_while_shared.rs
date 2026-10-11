// An exclusive handle cannot be taken while a shared one is still in use.
use ffmpeg::libavutil::frame::{SideDataDescriptor, SideDataProps};

fn main() {
    let mut d = SideDataDescriptor::new();
    let r = d.as_ref();
    d.as_mut().set_props(SideDataProps::MULTI);
    println!("{:?}", r.props());
}
