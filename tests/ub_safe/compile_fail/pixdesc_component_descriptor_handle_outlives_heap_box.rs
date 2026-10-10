// A handle into an `av_malloc`-owned descriptor cannot outlive its owner.
use ffmpeg::libavutil::mem::HeapBox;
use ffmpeg::libavutil::pixdesc::ComponentDescriptor;

fn main() {
    let boxed = HeapBox::<ComponentDescriptor>::new_zeroed().unwrap();
    let r = boxed.as_ref();
    drop(boxed);
    println!("{}", r.offset());
}
