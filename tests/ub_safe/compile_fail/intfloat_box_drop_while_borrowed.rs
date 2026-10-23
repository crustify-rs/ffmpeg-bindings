// Dropping a `HeapBox<IntFloat64>` while a handle to it lives is rejected.
use ffmpeg_bindings::libavutil::libavutil::intfloat::IntFloat64;
use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;

fn main() {
    let b = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    let r = b.as_ref();
    drop(b);
    println!("{}", r.float());
}
