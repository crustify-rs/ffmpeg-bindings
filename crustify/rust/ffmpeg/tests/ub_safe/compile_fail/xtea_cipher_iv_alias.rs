use ffmpeg::libavutil::{aes::Direction, xtea::Xtea};
fn main() {
    let ctx = Xtea::new(&[0; 16]);
    let mut buffer = [0; 8];
    ctx.as_ref().crypt_le_in_place(&mut buffer, Some(&mut buffer), Direction::Decrypt).unwrap();
}
