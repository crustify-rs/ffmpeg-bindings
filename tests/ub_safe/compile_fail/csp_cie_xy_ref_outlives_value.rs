// A shared handle cannot outlive the inline value it borrows.
use ffmpeg::libavutil::csp::{CieXy, CieXyRef};

fn dangling() -> CieXyRef<'static> {
    let v = CieXy::default();
    v.as_ref()
}

fn main() {
    println!("{:?}", dangling());
}
