// A view laid over a caller's bytes borrows them: it cannot outlive them.
use ffmpeg::libavutil::samplefmt::{SampleFormat, SamplesMut};

fn main() {
    let mut view = {
        let mut bytes = vec![0u8; 256];
        SamplesMut::from_buffer(&mut bytes, 2, 16, SampleFormat::S32, 1).unwrap()
    };
    view.set_silence(0, 16).unwrap();
}
