// A value cannot be written while a coordinate handle reads it.
use ffmpeg::libavutil::csp::CieXy;
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let mut v = CieXy::default();
    let x = v.as_ref().x();
    v.as_mut().set_x(Rational::new(1, 2));
    println!("{}", x.num());
}
