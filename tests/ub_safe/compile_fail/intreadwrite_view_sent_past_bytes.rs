// A byte view crosses threads only within the bytes' borrow: an unscoped
// thread may outlive them.
use ffmpeg::libavutil::intreadwrite::Unaligned16Ref;

fn main() {
    let bytes = [0u8; 2];
    let view = Unaligned16Ref::from_bytes(&bytes);
    let t = std::thread::spawn(move || view.value());
    t.join().unwrap();
}
