// A handle into an `av_malloc` block of an `Unaligned64` cannot outlive the
// box that frees it.
use ffmpeg::libavutil::intreadwrite::Unaligned64;
use ffmpeg::libavutil::mem::HeapBox;

fn main() {
    let b = HeapBox::<Unaligned64>::new_zeroed().unwrap();
    let r = b.as_ref();
    drop(b);
    println!("{}", r.value());
}
