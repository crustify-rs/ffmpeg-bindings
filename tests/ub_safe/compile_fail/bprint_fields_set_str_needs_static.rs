// The safe `str` setter takes a `'static` string: a value may outlive any
// shorter borrow, which it cannot record.
use std::ffi::CString;

use ffmpeg::libavutil::bprint::BPrintFields;

fn main() {
    let mut f = BPrintFields::new();
    {
        let s = CString::new("so far").unwrap();
        f.as_mut().set_str(Some(&s));
    }
    println!("{:?}", f.as_ref().str());
}
