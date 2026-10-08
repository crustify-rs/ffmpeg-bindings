// A value cannot hand out an exclusive handle while a shared one lives.
use ffmpeg::libavutil::intfloat::IntFloat64;

fn main() {
    let mut v = IntFloat64::from_float(1.0);
    let r = v.as_ref();
    v.as_mut().set_float(2.0);
    println!("{}", r.float());
}
