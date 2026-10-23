// A class's name borrows the class, not 'static, through a handle.
use core::ffi::CStr;

use ffmpeg_bindings::libavutil::libavutil::log::ClassRef;

fn name<'a>(c: ClassRef<'a>) -> &'static CStr {
    c.name().unwrap()
}

fn main() {}
