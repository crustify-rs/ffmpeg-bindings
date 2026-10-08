// A reborrowed exclusive handle freezes its parent while it lives.
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let mut q = Rational::new(1, 2);
    let mut m = q.as_mut();
    let mut inner = m.as_mut();
    m.set_num(3);
    inner.set_den(4);
}
