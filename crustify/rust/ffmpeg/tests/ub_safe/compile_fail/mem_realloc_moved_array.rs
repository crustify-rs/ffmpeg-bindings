// A resized array is consumed: the old owner, whose block C may have freed,
// cannot be used again.
use ffmpeg::libavutil::mem::{alloc_array, realloc_array};

fn main() {
    let a = alloc_array::<u32>(4).unwrap();
    let b = realloc_array(a, 8);
    println!("{} {}", a.len(), b.is_ok());
}
