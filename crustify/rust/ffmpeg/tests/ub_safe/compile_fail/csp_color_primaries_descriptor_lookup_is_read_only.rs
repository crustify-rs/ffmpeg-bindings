// C's static description of a gamut is lent as a shared handle only.
use ffmpeg::libavutil::csp::CieXy;
use ffmpeg::libavutil::pixfmt::ColorPrimaries;

fn main() {
    let d = ColorPrimaries::BT709.descriptor().unwrap();
    d.white_point_mut().set(CieXy::default());
}
