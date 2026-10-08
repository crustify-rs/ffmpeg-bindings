// A reborrowed `Unaligned64Mut` freezes its parent while it lives.
use ffmpeg::libavutil::intreadwrite::Unaligned64Mut;

fn main() {
    let mut bytes = [0u8; 8];
    let mut outer = Unaligned64Mut::from_bytes(&mut bytes);
    let mut inner = outer.as_mut();
    outer.set_value(1);
    inner.set_value(2);
}
