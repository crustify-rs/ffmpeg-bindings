use ffmpeg_bindings::libavutil::libavutil::refstruct::{PoolFlags, RefStructPool};
fn main() {
    let owner = RefStructPool::new(16, PoolFlags::NONE).unwrap();
    let shared = owner.as_ref();
    drop(owner);
    std::hint::black_box(shared);
}
