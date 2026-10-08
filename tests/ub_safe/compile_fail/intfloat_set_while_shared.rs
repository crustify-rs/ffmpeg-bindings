// The exclusive handle cannot write while a shared reborrow of it lives.
use ffmpeg::libavutil::intfloat::IntFloat64;

fn main() {
    let mut v = IntFloat64::from_bits(1);
    let mut m = v.as_mut();
    let r = m.as_ref();
    m.set_bits(2);
    println!("{}", r.bits());
}
