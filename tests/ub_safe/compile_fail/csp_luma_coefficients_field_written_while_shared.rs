// A coefficient cannot be written while a shared field handle to it lives.
use ffmpeg::libavutil::csp::LumaCoefficients;

fn main() {
    let mut c = LumaCoefficients::default();
    let cr = c.as_ref().cr();
    c.as_mut().cr_mut().set_num(1);
    println!("{}", cr.num());
}
