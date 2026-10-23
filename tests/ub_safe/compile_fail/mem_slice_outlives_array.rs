// A slice borrowed from a `HeapVec` cannot outlive the array that frees it.
use ffmpeg_bindings::libavutil::libavutil::mem::alloc_array_zeroed;

fn main() {
    let s = {
        let a = alloc_array_zeroed::<u8>(4).unwrap();
        a.as_slice()
    };
    println!("{}", s[0]);
}
