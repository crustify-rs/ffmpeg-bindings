// A C-allocated `Alias32` cannot be freed while a handle to it is live.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite_h::Alias32;
use ffmpeg_bindings::libavutil::libavutil::mem::HeapBox;

fn main() {
    let b = HeapBox::<Alias32>::new_zeroed().unwrap();
    let r = b.as_ref();
    drop(b);
    println!("{}", r.u32());
}
