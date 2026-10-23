// Two exclusive handles to one rational cannot be made by copying one.
use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

fn main() {
    let mut q = Rational::new(1, 2);
    let mut a = q.as_mut();
    let mut b = a;
    b.set_num(3);
    a.set_num(4);
}
