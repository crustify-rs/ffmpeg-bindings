// An append may reallocate the block, so no slice of it may live across one.
use ffmpeg_bindings::libavutil::libavutil::mem::DynArray;

fn main() {
    let mut a = DynArray::<u32>::new();
    let _ = a.push_or_clear(1);
    let s = a.as_slice();
    let _ = a.push_or_clear(2);
    println!("{}", s[0]);
}
