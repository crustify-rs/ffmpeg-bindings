// An error description cannot outlive the buffer it was written into.
use ffmpeg::libavutil::error::describe;

fn main() {
    let text = {
        let mut buf = [0u8; 64];
        describe(-12, &mut buf).unwrap_or_else(|e| e)
    };
    println!("{text:?}");
}
