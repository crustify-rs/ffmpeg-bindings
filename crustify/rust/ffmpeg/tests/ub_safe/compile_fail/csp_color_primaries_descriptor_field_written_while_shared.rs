// The description cannot be written while a shared field handle reads it.
use ffmpeg::libavutil::csp::ColorPrimariesDescriptor;

fn main() {
    let mut d = ColorPrimariesDescriptor::default();
    let p = d.as_ref().primaries();
    d.as_mut().set_primaries(Default::default());
    println!("{}", p.r().x().num());
}
