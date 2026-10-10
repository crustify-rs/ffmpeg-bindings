// The exclusive handle cannot write while a shared reborrow of it lives.
use ffmpeg::libavutil::pixdesc::ComponentDescriptor;

fn main() {
    let mut d = ComponentDescriptor::new();
    let mut m = d.as_mut();
    let r = m.as_ref();
    m.set_depth(8);
    println!("{}", r.depth());
}
