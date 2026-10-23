// A regrowth may free the block, so no slice of it may live across one.
use ffmpeg_bindings::libavutil::libavutil::mem::ZeroedFastBuffer;

fn main() {
    let mut b = ZeroedFastBuffer::new();
    let _ = b.reserve_discarding(16);
    let s = b.as_slice();
    let _ = b.reserve_discarding(4096);
    println!("{}", s[0]);
}
