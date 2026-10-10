// A coefficient's field handle cannot outlive the coefficients it projects.
use ffmpeg::libavutil::csp::LumaCoefficients;

fn main() {
    let cr = {
        let c = LumaCoefficients::default();
        c.as_ref().cr()
    };
    println!("{}", cr.num());
}
