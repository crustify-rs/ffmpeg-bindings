use ffmpeg_bindings::libavutil::libavutil::buffer::BufferPool;
fn take<T>(_: T) {}
fn main() {
    let mut owner = BufferPool::new(64).unwrap();
    let shared = owner.as_ref();
    let exclusive = owner.as_mut();
    take(shared);
    take(exclusive);
}
