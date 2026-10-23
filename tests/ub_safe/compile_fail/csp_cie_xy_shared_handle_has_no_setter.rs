// A shared handle only reads, and so do the coordinate handles it lends: the
// setters are on `CieXyMut` and `RationalMut`.
use ffmpeg_bindings::libavutil::libavutil::csp::CieXy;
use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

fn main() {
    let v = CieXy::default();
    v.as_ref().set_x(Rational::new(1, 2));
    v.as_ref().x().set_num(3);
    let _ = v.as_ref().x_mut();
}
