// Storing an unchecked plane pointer asserts the memory outlives the frame's
// use of it, so it needs `unsafe`.
use ffmpeg_bindings::libavutil::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut bytes = [0u8; 4];
    frame.as_mut().set_data_ptr(0, bytes.as_mut_ptr());
}
