// Dropping a `HeapVec` while a slice of it lives is rejected.
use ffmpeg::libavutil::mem::HeapVec;

fn main() {
    let a = HeapVec::<u32>::from_slice(&[1, 2, 3]).unwrap();
    let s = a.as_slice();
    drop(a);
    println!("{}", s[0]);
}
