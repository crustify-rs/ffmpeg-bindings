use ffmpeg::libavutil::camellia::Camellia;
fn main() {
    let mut owner = Camellia::new(&[0; 16]).unwrap();
    let mut exclusive = owner.as_mut();
    drop(owner);
    exclusive.init(&[0; 24]).unwrap();
}
