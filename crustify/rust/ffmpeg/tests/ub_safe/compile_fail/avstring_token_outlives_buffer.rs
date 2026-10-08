// A token cannot outlive the buffer it was split out of.
use std::ffi::CStr;

use ffmpeg::libavutil::avstring::tokens;

fn main() {
    let first: &CStr;
    {
        let mut buf = *b"a b\0";
        first = tokens(&mut buf, c" ").unwrap().next().unwrap();
    }
    println!("{first:?}");
}
