// A primary reached through the shared handle is read-only.
use ffmpeg_bindings::libavutil::libavutil::csp::PrimaryCoefficients;

fn main() {
    let p = PrimaryCoefficients::default();
    let r = p.as_ref().r();
    r.x_mut().set_num(1);
}
