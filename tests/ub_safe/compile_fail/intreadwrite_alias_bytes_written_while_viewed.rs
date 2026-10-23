// Bytes cannot be viewed exclusively as an `Alias16` while a shared view of
// them is alive.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::{Alias16Mut, Alias16Ref};

fn main() {
    let mut bytes = [0u8; 2];
    let shared = Alias16Ref::from_bytes(&bytes).unwrap();
    Alias16Mut::from_bytes(&mut bytes).unwrap().set_u16(1);
    println!("{}", shared.u16());
}
