// Two exclusive field handles cannot be held at once: each borrows the
// parent handle exclusively.
use ffmpeg_bindings::libavutil::libavutil::csp::LumaCoefficients;

fn main() {
    let mut c = LumaCoefficients::default();
    let mut m = c.as_mut();
    let mut cr = m.cr_mut();
    let mut cg = m.cg_mut();
    cr.set_num(1);
    cg.set_num(2);
}
