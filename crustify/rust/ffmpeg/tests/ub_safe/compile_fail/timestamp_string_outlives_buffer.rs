// The text `make_string` returns borrows the buffer: it cannot be rewritten
// while the text is in use.
use ffmpeg::libavutil::{ffi, timestamp::make_string};

fn main() {
    let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
    let first = make_string(&mut buf, 1);
    let second = make_string(&mut buf, 2);
    println!("{first:?} {second:?}");
}
