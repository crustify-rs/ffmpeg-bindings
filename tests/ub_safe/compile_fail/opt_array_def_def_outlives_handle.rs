// The default read through a handle borrows for the handle's lifetime, not
// longer.
use ffmpeg_bindings::libavutil::libavutil::opt::OptionArrayDefRef;
use std::ffi::CStr;

fn def<'a>(d: OptionArrayDefRef<'a>) -> Option<&'static CStr> {
    d.def()
}

fn main() {}
