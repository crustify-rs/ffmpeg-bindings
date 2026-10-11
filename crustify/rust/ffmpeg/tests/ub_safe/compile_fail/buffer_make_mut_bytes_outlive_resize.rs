use ffmpeg::libavutil::buffer::BufferReference;
fn main() {
    let mut buf = BufferReference::alloc_resizable(8).unwrap().fill(0);
    let mut handle = buf.as_mut();
    let bytes = handle.make_mut().unwrap();
    // A resize may reallocate the bytes in place.
    handle.resize(4096, 0).unwrap();
    bytes[0] = 1;
}
