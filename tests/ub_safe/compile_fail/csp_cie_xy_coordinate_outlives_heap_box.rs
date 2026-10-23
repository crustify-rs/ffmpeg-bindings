// A coordinate handle cannot outlive the `av_malloc` block its `HeapBox`
// frees.
use ffmpeg_bindings::libavutil::libavutil::csp::CieXy;
use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;

fn main() {
    let boxed = HeapBox::<CieXy>::new_zeroed().unwrap();
    let y = boxed.as_ref().y();
    drop(boxed);
    println!("{}", y.den());
}
