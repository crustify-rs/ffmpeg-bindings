// A handle cannot outlive the `av_malloc` block its `HeapBox` frees.
use ffmpeg::libavutil::mem::HeapBox;
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let boxed = HeapBox::<Rational>::new_zeroed().unwrap();
    let r = boxed.as_ref();
    drop(boxed);
    println!("{}", r.den());
}
