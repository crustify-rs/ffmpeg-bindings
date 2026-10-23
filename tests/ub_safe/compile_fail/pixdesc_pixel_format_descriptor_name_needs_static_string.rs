// A descriptor's name is borrowed for the whole program: a string the
// function owns cannot be set.
use std::ffi::CString;

use ffmpeg_bindings::libavutil::libavutil::pixdesc::PixelFormatDescriptor;

fn main() {
    let mut d = PixelFormatDescriptor::new();
    let name = CString::new("short-lived").unwrap();
    d.as_mut().set_name(Some(&name));
    println!("{:?}", d.as_ref().name());
}
