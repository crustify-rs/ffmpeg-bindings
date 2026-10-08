// A shared handle cannot outlive the `Alias32` value it borrows.
use ffmpeg::libavutil::intreadwrite::{Alias32, Alias32Ref};

fn leak<'a>() -> Alias32Ref<'a> {
    let a = Alias32::from_u32(1);
    a.as_ref()
}

fn main() {
    println!("{}", leak().u32());
}
