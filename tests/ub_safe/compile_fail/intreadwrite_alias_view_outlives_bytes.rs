// An `Alias32Ref` cannot outlive the bytes it views.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::Alias32Ref;

fn main() {
    let view;
    {
        let bytes = [0u8; 4];
        view = Alias32Ref::from_bytes(&bytes);
    }
    println!("{:?}", view.map(|v| v.u32()));
}
