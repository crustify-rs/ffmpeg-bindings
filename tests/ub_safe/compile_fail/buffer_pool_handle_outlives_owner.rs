use ffmpeg::libavutil::buffer::BufferPool;
fn take<T>(_: T) {}
fn main() {
    let owner = BufferPool::new(64).unwrap();
    let shared = owner.as_ref();
    drop(owner);
    take(shared);
}
