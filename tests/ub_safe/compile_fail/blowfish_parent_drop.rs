use ffmpeg_bindings::libavutil::libavutil::blowfish::Blowfish;
fn main() {
    let owner = Blowfish::new_boxed(b"key").unwrap();
    let shared = owner.as_ref();
    drop(owner);
    let _ = shared.p();
}
