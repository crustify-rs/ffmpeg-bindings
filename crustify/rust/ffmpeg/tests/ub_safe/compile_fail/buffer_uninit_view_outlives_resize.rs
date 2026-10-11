use ffmpeg::libavutil::buffer::BufferReference;
fn main() {
    let mut buf = BufferReference::alloc_resizable(8).unwrap();
    let view = buf.as_slice();
    buf.resize(4096).unwrap();
    let _ = view[0];
}
