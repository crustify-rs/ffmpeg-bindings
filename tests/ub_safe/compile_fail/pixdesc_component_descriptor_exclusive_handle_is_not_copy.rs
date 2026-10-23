// Two exclusive handles to one descriptor cannot be made by copying one.
use ffmpeg_bindings::libavutil::libavutil::pixdesc::ComponentDescriptor;

fn main() {
    let mut d = ComponentDescriptor::new();
    let mut a = d.as_mut();
    let mut b = a;
    b.set_plane(1);
    a.set_plane(2);
}
