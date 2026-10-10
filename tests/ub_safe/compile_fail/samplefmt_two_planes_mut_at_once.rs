// `plane_mut` borrows the whole view: two planes are written at once only
// through `planes_mut`, whose planes are disjoint.
use ffmpeg::libavutil::samplefmt::{SampleBuffer, SampleFormat};

fn main() {
    let mut buffer = SampleBuffer::new(2, 16, SampleFormat::U8P, 0).unwrap();
    let mut view = buffer.as_mut();
    let first = view.plane_mut(0).unwrap();
    let second = view.plane_mut(1).unwrap();
    first.copy_from_slice(second);
}
