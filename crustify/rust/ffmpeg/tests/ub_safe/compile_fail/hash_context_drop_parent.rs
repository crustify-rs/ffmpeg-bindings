use ffmpeg::libavutil::{md5::Md5, murmur3::Murmur3};
fn md5_drop() {
    let mut owner = Md5::new().unwrap();
    let mut handle = owner.as_mut();
    drop(owner);
    handle.finalize();
}
fn murmur3_drop() {
    let mut owner = Murmur3::new().unwrap();
    let mut handle = owner.as_mut();
    drop(owner);
    handle.finalize();
}
fn main() {}
