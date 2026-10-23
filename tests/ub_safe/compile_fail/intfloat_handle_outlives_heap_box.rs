// A handle into an `av_malloc` block cannot outlive the box that frees it.
use ffmpeg_bindings::libavutil::libavutil::intfloat_h::IntFloat32;
use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;

fn main() {
    let b = HeapBox::<IntFloat32>::new_zeroed().unwrap();
    let r = b.as_ref();
    drop(b);
    println!("{}", r.bits());
}
