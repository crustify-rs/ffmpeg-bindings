// Only a buffer that owns its string is finalized: one printing into caller
// storage would have C free that storage.
use ffmpeg_bindings::libavutil::libavutil::bprint::BPrint;

fn main() {
    let mut out = [0u8; 16];
    let buf = BPrint::for_buffer(&mut out).unwrap();
    let _ = BPrint::into_string(buf);
}
