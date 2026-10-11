use ffmpeg::libavutil::rc4::Rc4;
fn main() {
    let owner = Rc4::new(b"key").unwrap();
    let shared = owner.as_ref();
    drop(owner);
    let _ = shared.x();
}
