use ffmpeg_bindings::libavutil::libavutil::rc4::Rc4;
fn main() {
    let mut owner = Rc4::new(b"key").unwrap();
    let mut outer = owner.as_mut();
    let mut inner = outer.state_mut();
    outer.set_x(1);
    assert!(inner.set_elem(0, 1));
}
