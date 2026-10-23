// A shared handle only reads: the setters are on `ComponentDescriptorMut`.
use ffmpeg_bindings::libavutil::libavutil::pixdesc::ComponentDescriptor;

fn main() {
    let d = ComponentDescriptor::new();
    d.as_ref().set_step(1);
}
