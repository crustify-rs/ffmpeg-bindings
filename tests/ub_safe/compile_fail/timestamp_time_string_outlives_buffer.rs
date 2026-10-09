// The text `make_time_string` returns borrows the buffer: it cannot outlive
// it, nor survive a rewrite of it.
use ffmpeg::libavutil::{ffi, rational::Rational, timestamp::make_time_string};

fn main() {
    let tb = Rational::new(1, 1000);
    let text = {
        let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
        make_time_string(&mut buf, 1, tb)
    };
    let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
    let first = make_time_string(&mut buf, 1, tb);
    buf[0] = b'x';
    println!("{text:?} {first:?} {}", buf[0]);
}
