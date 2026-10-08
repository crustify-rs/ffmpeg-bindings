// `get_token`'s cursor points into its input, so cannot outlive it.
use ffmpeg::libavutil::avstring::get_token;
use std::ffi::{CStr, CString};

fn main() {
    let cursor: &CStr;
    {
        let input = CString::new("a,b").unwrap();
        let mut c = input.as_c_str();
        let _ = get_token(&mut c, c",");
        cursor = c;
    }
    println!("{cursor:?}");
}
