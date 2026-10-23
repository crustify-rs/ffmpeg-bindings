// A sample format's description is borrowed from the buffer it was written
// into, which cannot be overwritten while it is read.
use ffmpeg_bindings::libavutil::libavutil::samplefmt::SampleFormat;

fn main() {
    let mut buf = [0u8; 16];
    let text = SampleFormat::S16.describe(&mut buf).unwrap();
    buf[0] = b'x';
    println!("{text:?} {}", buf[0]);
}
