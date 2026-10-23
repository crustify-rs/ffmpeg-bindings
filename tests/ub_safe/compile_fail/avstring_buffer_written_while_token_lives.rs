// The buffer cannot be written while a token still borrows it, even after the
// iterator is gone.
use ffmpeg_bindings::libavutil::libavutil::avstring::tokens;

fn main() {
    let mut buf = *b"ab cd\0";
    let first = tokens(&mut buf, c" ").unwrap().next().unwrap();
    buf[0] = b'x';
    println!("{first:?} {buf:?}");
}
