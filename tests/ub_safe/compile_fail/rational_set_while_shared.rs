// The exclusive handle cannot write while a shared reborrow of it lives.
use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

fn main() {
    let mut q = Rational::new(1, 2);
    let mut m = q.as_mut();
    let r = m.as_ref();
    m.set_den(3);
    println!("{}", r.den());
}
