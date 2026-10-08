// A shared handle only reads: the setters are on `RationalMut`.
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let q = Rational::new(1, 2);
    q.as_ref().set_num(3);
}
