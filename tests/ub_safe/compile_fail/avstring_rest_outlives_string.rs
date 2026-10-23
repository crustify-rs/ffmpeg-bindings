// The rest `strip_prefix` returns cannot outlive the string it borrows.
use std::ffi::{CStr, CString};

use ffmpeg_bindings::libavutil::libavutil::avstring::strip_prefix;

fn main() {
    let rest: &CStr;
    {
        let s = CString::new("key=value").unwrap();
        rest = strip_prefix(&s, c"key=").unwrap();
    }
    println!("{rest:?}");
}
