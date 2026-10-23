// The shared handle only reads: setters are on the exclusive handle.
use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataDescriptor, SideDataProps};

fn main() {
    let d = SideDataDescriptor::new();
    let r = d.as_ref();
    r.set_props(SideDataProps::GLOBAL);
}
