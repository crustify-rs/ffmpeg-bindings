use ffmpeg::libavutil::blowfish::Blowfish;
fn main() {
    let mut owner = Blowfish::new_boxed(b"key").unwrap();
    let mut outer = owner.as_mut();
    let shared = outer.as_ref();
    outer.as_mut().set_p([0; 18]);
    let _ = shared.p();
}
