// A writable plane excludes every other view of the frame, a shared plane
// included.
use ffmpeg::libavutil::buffer::BufferReference;
use ffmpeg::libavutil::frame::Frame;

fn main() {
    let mut frame = Frame::new().unwrap();
    let mut m = frame.as_mut();
    m.set_buffer(0, Some(BufferReference::alloc_zeroed(8).unwrap()));
    m.set_plane(0, 0, 0).unwrap();
    let shared = m.as_ref().plane(0).unwrap();
    let writable = m.plane_mut(0).unwrap();
    writable[0] = 1;
    assert_eq!(shared[0], 0);
}
