// A coordinate handle keeps the struct handle's lifetime: it cannot outlive
// the inline value either.
use ffmpeg::libavutil::csp::CieXy;

fn main() {
    let x = {
        let v = CieXy::default();
        v.as_ref().x()
    };
    println!("{}", x.num());
}
