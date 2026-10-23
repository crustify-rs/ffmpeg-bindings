// A found tag value borrows the caller's buffer: the buffer cannot be written
// while the value is in use.
use ffmpeg_bindings::libavutil::libavutil::parseutils::find_info_tag;

fn main() {
    let mut buf = [0u8; 16];
    let value = find_info_tag(&mut buf, c"a", c"?a=1").unwrap();
    buf.fill(b'x');
    println!("{value:?}");
}
