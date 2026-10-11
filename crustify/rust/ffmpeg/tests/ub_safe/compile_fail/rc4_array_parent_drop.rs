use ffmpeg::libavutil::rc4::Rc4;
fn main() {
    let owner = Rc4::new(b"key").unwrap();
    let state = owner.as_ref().state();
    drop(owner);
    let _ = state.elem(0);
}
