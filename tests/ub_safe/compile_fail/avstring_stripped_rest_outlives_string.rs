// The rest after a stripped prefix cannot outlive the string it borrows.
use std::ffi::CString;

use ffmpeg::libavutil::avstring::strip_prefix_ignore_ascii_case;

fn main() {
    let rest = {
        let s = CString::new("Prefix-rest").unwrap();
        strip_prefix_ignore_ascii_case(&s, c"prefix").unwrap()
    };
    println!("{rest:?}");
}
