// A shared handle cannot outlive the `IntFloat64` value it borrows.
use ffmpeg_bindings::libavutil::libavutil::intfloat_h::{IntFloat64, IntFloat64Ref};

fn dangling() -> IntFloat64Ref<'static> {
    let v = IntFloat64::from_bits(1);
    v.as_ref()
}

fn main() {
    println!("{}", dangling().bits());
}
