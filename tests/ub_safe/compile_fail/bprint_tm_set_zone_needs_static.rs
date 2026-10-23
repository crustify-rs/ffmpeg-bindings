// The zone setter takes a `'static` string: a time may outlive any shorter
// borrow, which it cannot record.
use std::ffi::CString;

use ffmpeg_bindings::libavutil::libavutil::bprint::Tm;

fn main() {
    let mut tm = Tm::new();
    {
        let s = CString::new("CET").unwrap();
        tm.as_mut().set_zone(Some(&s));
    }
    println!("{:?}", tm.as_ref().zone());
}
