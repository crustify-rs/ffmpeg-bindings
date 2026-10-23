// A field handle cannot outlive the description it projects.
use ffmpeg_bindings::libavutil::libavutil::csp::ColorPrimariesDescriptor;

fn main() {
    let p = {
        let d = ColorPrimariesDescriptor::default();
        d.as_ref().primaries()
    };
    println!("{}", p.r().x().num());
}
