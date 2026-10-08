// A handle to an inline value cannot outlive the value.
use ffmpeg::libavutil::intreadwrite::{Unaligned16, Unaligned16Mut};

fn handle<'a>() -> Unaligned16Mut<'a> {
    let mut v = Unaligned16::new(1);
    v.as_mut()
}

fn main() {
    handle().set_value(2);
}
