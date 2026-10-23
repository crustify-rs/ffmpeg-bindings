use ffmpeg_bindings::libavutil::libavutil::{aes::Direction, xtea::Xtea};
fn main() {
    let ctx = Xtea::new(&[0; 16]);
    let mut buffer = [0; 8];
    ctx.as_ref().crypt(&mut buffer, &buffer, None, Direction::Encrypt).unwrap();
}
