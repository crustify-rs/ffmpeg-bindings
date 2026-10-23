// The buffer cannot be written while its tokens are being found.
use ffmpeg_bindings::libavutil::libavutil::avstring::tokens;

fn main() {
    let mut buf = *b"a b c\0";
    let mut it = tokens(&mut buf, c" ").unwrap();
    let _ = it.next();
    buf[2] = 0;
    let _ = it.next();
    println!("{buf:?}");
}
