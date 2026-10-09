// The text `make_time_string_by_ref` returns borrows the buffer, not the time
// base handle: the buffer cannot be rewritten while the text is in use.
use ffmpeg::libavutil::{ffi, rational::Rational, timestamp::make_time_string_by_ref};

fn main() {
    let tb = Rational::new(1, 1000);
    let mut buf = [0u8; ffi::AV_TS_MAX_STRING_SIZE as usize];
    let first = make_time_string_by_ref(&mut buf, 1, tb.as_ref());
    let second = make_time_string_by_ref(&mut buf, 2, tb.as_ref());
    println!("{first:?} {second:?}");
}
