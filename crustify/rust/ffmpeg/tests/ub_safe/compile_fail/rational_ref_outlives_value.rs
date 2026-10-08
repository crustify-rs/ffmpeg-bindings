// A shared handle cannot outlive the `Rational` value it borrows.
use ffmpeg::libavutil::rational::{Rational, RationalRef};

fn dangling() -> RationalRef<'static> {
    let q = Rational::new(1, 2);
    q.as_ref()
}

fn main() {
    println!("{}", dangling().num());
}
