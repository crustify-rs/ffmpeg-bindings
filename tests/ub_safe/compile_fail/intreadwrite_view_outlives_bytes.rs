// An `Unaligned64Ref` cannot outlive the bytes it views.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::Unaligned64Ref;

fn main() {
    let view;
    {
        let bytes = [0u8; 8];
        view = Unaligned64Ref::from_bytes(&bytes);
    }
    println!("{}", view.value());
}
