use ffmpeg_bindings::libavutil::libavutil::murmur3::{Murmur3, Murmur3Ref};
fn escaped<'a>() -> Murmur3Ref<'a> {
    let owner = Murmur3::new().unwrap();
    owner.as_ref()
}
fn main() {}
