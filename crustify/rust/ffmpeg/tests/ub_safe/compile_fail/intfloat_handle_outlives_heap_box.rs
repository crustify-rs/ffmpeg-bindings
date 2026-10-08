// A handle into an `av_malloc` block cannot outlive the box that frees it.
use ffmpeg::libavutil::intfloat::IntFloat32;
use ffmpeg::libavutil::mem::HeapBox;

fn main() {
    let b = HeapBox::<IntFloat32>::new_zeroed().unwrap();
    let r = b.as_ref();
    drop(b);
    println!("{}", r.bits());
}
