// A primary's handle cannot outlive the primaries it projects.
use ffmpeg_bindings::libavutil::libavutil::csp::PrimaryCoefficients;

fn main() {
    let r = {
        let p = PrimaryCoefficients::default();
        p.as_ref().r()
    };
    println!("{}", r.x().num());
}
