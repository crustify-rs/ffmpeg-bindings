use ffmpeg::libavutil::{aes::Direction, xtea::Xtea};
fn main() {
    let mut ctx = Xtea::new(&[0; 16]);
    let view = ctx.as_ref();
    ctx.as_mut().init_le(&[1; 16]);
    view.crypt_in_place(&mut [0; 8], None, Direction::Encrypt).unwrap();
}
