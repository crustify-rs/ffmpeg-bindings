// The delimiter set must outlive the iterator that keeps reading it.
use ffmpeg::libavutil::avstring::tokens;
use std::ffi::CString;

fn main() {
    let mut buf = *b"a,b\0";
    let mut it = {
        let delim = CString::new(",").unwrap();
        tokens(&mut buf, &delim).unwrap()
    };
    let _ = it.next();
}
