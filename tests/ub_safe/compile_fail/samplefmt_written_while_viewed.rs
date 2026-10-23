// A shared view keeps the buffer frozen: no exclusive view can set its
// samples to silence while the shared one is still read.
use ffmpeg_bindings::libavutil::libavutil::samplefmt::{SampleBuffer, SampleFormat};

fn main() {
    let mut buffer = SampleBuffer::new(1, 16, SampleFormat::U8, 0).unwrap();
    let plane = buffer.as_ref().plane(0).unwrap();
    buffer.as_mut().set_silence(0, 16).unwrap();
    println!("{}", plane[0]);
}
