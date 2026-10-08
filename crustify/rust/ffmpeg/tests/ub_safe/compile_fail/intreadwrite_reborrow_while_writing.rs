// A shared reborrow of an exclusive view freezes it until the reborrow ends.
use ffmpeg::libavutil::intreadwrite::Unaligned32Mut;

fn main() {
    let mut bytes = [0u8; 4];
    let mut m = Unaligned32Mut::from_bytes(&mut bytes);
    let r = m.as_ref();
    m.set_value(1);
    println!("{}", r.value());
}
