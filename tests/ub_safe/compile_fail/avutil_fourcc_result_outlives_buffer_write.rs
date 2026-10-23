// `fourcc_string`'s result borrows the buffer, which cannot be rewritten
// while it lives.
use ffmpeg_bindings::libavutil::libavutil::utils::fourcc_string;

fn main() {
    let mut buf = [0u8; 32];
    let s = fourcc_string(&mut buf, 0x3436_3248);
    buf[0] = 0;
    println!("{s:?}");
}
