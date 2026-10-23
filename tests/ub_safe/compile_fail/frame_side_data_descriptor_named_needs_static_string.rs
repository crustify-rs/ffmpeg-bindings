// A side data descriptor built from a name borrows it for the whole program:
// a string the function owns cannot name one.
use std::ffi::CString;

use ffmpeg_bindings::libavutil::libavutil::frame::{SideDataDescriptor, SideDataProps};

fn main() {
    let name = CString::new("short-lived").unwrap();
    let d = SideDataDescriptor::named(&name, SideDataProps::GLOBAL);
    println!("{d:?}");
}
