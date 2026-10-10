// An exclusive coordinate handle freezes the struct handle it came from.
use ffmpeg::libavutil::csp::CieXy;
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let mut v = CieXy::default();
    let mut m = v.as_mut();
    let mut x = m.x_mut();
    m.set(CieXy::new(Rational::new(1, 2), Rational::new(1, 3)));
    x.set_num(3);
}
