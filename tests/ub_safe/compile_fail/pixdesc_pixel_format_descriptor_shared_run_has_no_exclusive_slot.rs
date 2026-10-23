// A shared component run hands out shared handles only.
use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let d = PixelFormatDescriptor::new();
    let run = d.as_ref().components();
    run.get_mut(0).unwrap().set_depth(8);
}
