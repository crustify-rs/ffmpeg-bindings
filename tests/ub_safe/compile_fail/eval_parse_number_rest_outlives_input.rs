// The rest `parse_number` returns cannot outlive the string it borrows.
use std::ffi::{CStr, CString};

use ffmpeg::libavutil::eval::parse_number;

fn main() {
    let rest: &CStr;
    {
        let s = CString::new("12k rest").unwrap();
        rest = parse_number(&s).1;
    }
    println!("{rest:?}");
}
