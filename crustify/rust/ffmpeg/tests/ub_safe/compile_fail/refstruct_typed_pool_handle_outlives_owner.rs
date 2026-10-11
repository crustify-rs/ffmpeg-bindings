use ffmpeg::libavutil::intfloat::IntFloat64;
use ffmpeg::libavutil::refstruct::{PoolFlags, TypedRefStructPool};
fn main() {
    let owner = TypedRefStructPool::<IntFloat64>::new(PoolFlags::NONE).unwrap();
    let shared = owner.as_ref();
    drop(owner);
    std::hint::black_box(shared);
}
