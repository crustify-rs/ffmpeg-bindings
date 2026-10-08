// Bytes viewed by a shared `Unaligned64Ref` cannot be viewed exclusively.
use ffmpeg::libavutil::intreadwrite::{Unaligned64Mut, Unaligned64Ref};

fn main() {
    let mut bytes = [0u8; 8];
    let shared = Unaligned64Ref::from_bytes(&bytes);
    Unaligned64Mut::from_bytes(&mut bytes).set_value(1);
    println!("{}", shared.value());
}
