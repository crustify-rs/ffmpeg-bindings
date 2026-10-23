// C's static description of a gamut is lent as a shared handle only.
use ffmpeg_bindings::libavutil::libavutil::csp::CieXy;
use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;

fn main() {
    let d = ColorPrimaries::BT709.descriptor().unwrap();
    d.white_point_mut().set(CieXy::default());
}
