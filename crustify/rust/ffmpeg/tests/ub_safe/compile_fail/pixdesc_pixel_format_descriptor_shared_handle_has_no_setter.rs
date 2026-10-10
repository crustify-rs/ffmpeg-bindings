// A shared handle has getters only.
use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let d = PixelFormatDescriptor::new();
    d.as_ref().set_component_count(3);
}
