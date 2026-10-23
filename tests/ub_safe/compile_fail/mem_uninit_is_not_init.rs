// Fresh `av_malloc_array` storage cannot be read as initialized elements.
use ffmpeg_bindings::libavutil::libavutil::mem::alloc_array;

fn main() {
    let a = alloc_array::<u8>(4).unwrap();
    let s: &[u8] = a.as_slice();
    println!("{}", s[0]);
}
