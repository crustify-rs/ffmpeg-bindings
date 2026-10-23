// The stored string borrows the buffer: finalizing it frees what the view
// reads.
use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;

fn main() {
    let b = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
    let bytes = b.as_ref().as_bytes();
    drop(b);
    println!("{}", bytes.len());
}
