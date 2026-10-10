// A white point reached through the shared handle is read-only.
use ffmpeg::libavutil::csp::ColorPrimariesDescriptor;

fn main() {
    let d = ColorPrimariesDescriptor::default();
    let wp = d.as_ref().white_point();
    wp.x_mut().set_num(1);
}
