// A shared view of the string excludes the exclusive handle whose appends
// may move it to a new allocation.
use ffmpeg::libavutil::bprint::BPrint;

fn main() {
    let mut b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    b.as_mut().append_bytes(b"inline");
    let s = b.as_ref().as_bytes();
    b.as_mut().append_repeated(b'x', 5000);
    println!("{s:?}");
}
