use ffmpeg_bindings::libavutil::libavutil::{md5::Md5, murmur3::Murmur3};
fn md5_alias() {
    let mut owner = Md5::new().unwrap();
    let mut outer = owner.as_mut();
    let mut inner = outer.as_mut();
    outer.reset();
    inner.update(b"still borrowed");
}
fn murmur3_alias() {
    let mut owner = Murmur3::new().unwrap();
    let mut outer = owner.as_mut();
    let shared = outer.as_ref();
    outer.update(b"conflicting write");
    let _copy = shared;
}
fn main() {}
