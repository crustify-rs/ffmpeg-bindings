// The shared view only reads: storing needs the exclusive one.
use ffmpeg::libavutil::intreadwrite::Unaligned32Ref;

fn main() {
    let bytes = [0u8; 4];
    Unaligned32Ref::from_bytes(&bytes).set_value(1);
}
