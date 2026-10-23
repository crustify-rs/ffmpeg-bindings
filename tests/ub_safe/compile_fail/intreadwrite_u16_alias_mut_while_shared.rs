// Bytes cannot be viewed exclusively while a shared view of them is alive.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::{Unaligned16Mut, Unaligned16Ref};

fn main() {
    let mut bytes = [0u8; 2];
    let shared = Unaligned16Ref::from_bytes(&bytes);
    Unaligned16Mut::from_bytes(&mut bytes).set_value(1);
    println!("{}", shared.value());
}
