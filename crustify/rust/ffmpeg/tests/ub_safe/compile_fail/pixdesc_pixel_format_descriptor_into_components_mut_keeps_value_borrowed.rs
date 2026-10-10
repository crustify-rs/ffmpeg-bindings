// A run taken from a consumed handle keeps the descriptor exclusively
// borrowed: the descriptor cannot be read while the run lives.
use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let mut d = PixelFormatDescriptor::new();
    let mut run = d.as_mut().into_components_mut();
    println!("{}", d.as_ref().component_count());
    run.get_mut(0).unwrap().set_depth(8);
}
