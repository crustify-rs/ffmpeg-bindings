// A shared view of the string excludes the exclusive handle that rewrites
// its terminator.
use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;

fn main() {
    let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    let s = b.as_ref().as_c_str();
    b.as_mut().truncate(0);
    println!("{s:?}");
}
