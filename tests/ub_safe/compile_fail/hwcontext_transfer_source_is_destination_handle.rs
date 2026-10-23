// Nor through one exclusive handle reborrowed shared for the source.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    let _ = unsafe { m.transfer_data_from(m.as_ref()) };
}
