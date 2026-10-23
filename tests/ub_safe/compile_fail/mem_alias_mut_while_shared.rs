// A `HeapVec` cannot be written while a shared slice of it is alive.
use ffmpeg_bindings::libavutil::libavutil::mem::alloc_array_from_fn;

fn main() {
    let mut a = alloc_array_from_fn(4, |i| i as u8).unwrap();
    let s = a.as_slice();
    a.as_mut_slice()[0] = 1;
    println!("{}", s[0]);
}
