// The rest `small_strptime` returns borrows the input string, which cannot
// drop while the rest lives.
use std::ffi::CString;

use ffmpeg::libavutil::bprint::Tm;
use ffmpeg::libavutil::parseutils::small_strptime;

fn main() {
    let mut tm = Tm::new();
    let rest = {
        let s = CString::new("12:00:00 rest").unwrap();
        small_strptime(&s, c"%T", tm.as_mut())
    };
    println!("{rest:?}");
}
