// Two exclusive primary handles cannot be held at once: each borrows the
// parent handle exclusively.
use ffmpeg::libavutil::csp::PrimaryCoefficients;

fn main() {
    let mut p = PrimaryCoefficients::default();
    let mut m = p.as_mut();
    let mut r = m.r_mut();
    let mut g = m.g_mut();
    r.x_mut().set_num(1);
    g.x_mut().set_num(2);
}
