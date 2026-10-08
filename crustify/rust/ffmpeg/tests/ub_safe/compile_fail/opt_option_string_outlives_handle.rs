// An option's strings live as long as its handle's borrow, not longer.
use core::ffi::CStr;
use ffmpeg::libavutil::opt::OptionDefRef;

fn name<'a>(o: OptionDefRef<'a>) -> &'static CStr {
    o.name().unwrap()
}

fn main() {}
