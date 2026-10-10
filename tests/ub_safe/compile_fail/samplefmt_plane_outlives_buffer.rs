// A plane's samples are borrowed from the buffer that owns them: they cannot
// be read after the buffer (and its `av_malloc` block) is dropped.
use ffmpeg::libavutil::samplefmt::{SampleBuffer, SampleFormat};

fn main() {
    let plane = {
        let buffer = SampleBuffer::new(2, 16, SampleFormat::S16P, 0).unwrap();
        buffer.as_ref().plane(1).unwrap()
    };
    println!("{}", plane[0]);
}
