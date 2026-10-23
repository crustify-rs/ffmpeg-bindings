// A handle cannot outlive the `av_malloc` block its `HeapBox` frees.
use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;
use ffmpeg_bindings::libavutil::libavutil::rational::Rational;

fn main() {
    let boxed = HeapBox::<Rational>::new_zeroed().unwrap();
    let r = boxed.as_ref();
    drop(boxed);
    println!("{}", r.den());
}
