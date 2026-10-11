use ffmpeg::libavutil::buffer::BufferReference;
fn main() {
    let mut buf = BufferReference::alloc_zeroed(8).unwrap();
    let mut handle = buf.as_mut();
    let src = handle.as_ref();
    let _ = handle.assign(src);
}
