// Two exclusive handles to one chromaticity cannot be made by copying one.
use ffmpeg::libavutil::csp::CieXy;
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let mut v = CieXy::default();
    let mut a = v.as_mut();
    let mut b = a;
    b.set_x(Rational::new(1, 2));
    a.set_y(Rational::new(1, 3));
}
