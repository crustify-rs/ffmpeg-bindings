// A byte view cannot outlive the bytes it reads.
use ffmpeg::libavutil::intreadwrite::Unaligned32Ref;

fn main() {
    let view = {
        let bytes = [1u8, 2, 3, 4];
        Unaligned32Ref::from_bytes(&bytes)
    };
    println!("{}", view.value());
}
