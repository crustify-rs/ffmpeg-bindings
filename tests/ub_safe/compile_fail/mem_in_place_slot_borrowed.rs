// A slice of an array cannot be held across an in-place resize that may move
// or free its block.
use ffmpeg_bindings::libavutil::libavutil::mem::{alloc_array, realloc_array_in_place};

fn main() {
    let mut slot = alloc_array::<u8>(4).unwrap();
    let view = slot.as_slice();
    let _ = realloc_array_in_place(&mut slot, 4096);
    println!("{}", view.len());
}
