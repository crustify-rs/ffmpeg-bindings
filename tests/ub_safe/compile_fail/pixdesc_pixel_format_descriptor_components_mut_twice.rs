// Two exclusive component runs cannot be taken from one handle at once.
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let mut d = PixelFormatDescriptor::new();
    let mut m = d.as_mut();
    let mut a = m.components_mut();
    let mut b = m.components_mut();
    a.get_mut(0).unwrap().set_depth(8);
    b.get_mut(0).unwrap().set_depth(9);
}
