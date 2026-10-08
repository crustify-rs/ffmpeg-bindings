// A slice of a `DynArray` cannot outlive the array, which frees its block.
use ffmpeg::libavutil::mem::DynArray;

fn main() {
    let s = {
        let mut a = DynArray::<u32>::new();
        let _ = a.push_or_clear(1);
        a.as_slice()
    };
    println!("{}", s[0]);
}
