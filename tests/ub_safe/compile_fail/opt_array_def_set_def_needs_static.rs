// A definition keeps its default's pointer for as long as a table may publish
// it, so only a `'static` string is accepted.
use ffmpeg::libavutil::opt::OptionArrayDef;
use std::ffi::CString;

fn main() {
    let mut d = OptionArrayDef::new();
    let s = CString::new("1,2").unwrap();
    d.as_mut().set_def(Some(&s));
}
