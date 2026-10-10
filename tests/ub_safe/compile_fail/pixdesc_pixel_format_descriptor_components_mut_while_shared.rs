// The component slots cannot be borrowed for writing while a shared run of
// them lives.
use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let mut d = PixelFormatDescriptor::new();
    let mut m = d.as_mut();
    let run = m.as_ref().components();
    m.components_mut().get_mut(0).unwrap().set_depth(8);
    println!("{}", run.get(0).unwrap().depth());
}
