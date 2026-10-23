// A buffer cannot be its own copy source through `copy_from`, whose
// destination borrow excludes the source's; `copy_within` copies inside one
// buffer.
use ffmpeg_bindings::libavutil::libavutil::samplefmt::{SampleBuffer, SampleFormat};

fn main() {
    let mut buffer = SampleBuffer::new(2, 16, SampleFormat::FLTP, 0).unwrap();
    buffer.as_mut().copy_from(buffer.as_ref(), 0, 8, 8).unwrap();
}
