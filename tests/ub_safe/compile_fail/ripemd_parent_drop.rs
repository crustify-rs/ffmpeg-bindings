use ffmpeg_bindings::libavutil::libavutil::ripemd::{DigestBits, Ripemd};
fn main() {
    let mut owner = Ripemd::new(DigestBits::Bits160).unwrap();
    let mut handle = owner.as_mut();
    drop(owner);
    handle.init(DigestBits::Bits128).unwrap();
}
