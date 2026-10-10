// A descriptor's alias is borrowed for the whole program: a string local to
// a scope cannot be set.
use std::ffi::CString;

use ffmpeg::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let mut d = PixelFormatDescriptor::new();
    {
        let alias = CString::new("a,b").unwrap();
        d.as_mut().set_alias(Some(alias.as_c_str()));
    }
    println!("{:?}", d.as_ref().alias());
}
