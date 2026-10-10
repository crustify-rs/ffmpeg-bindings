// A coefficient reached through a shared handle cannot be written.
use ffmpeg::libavutil::csp::LumaCoefficients;

fn main() {
    let c = LumaCoefficients::default();
    c.as_ref().cr().set_num(1);
}
