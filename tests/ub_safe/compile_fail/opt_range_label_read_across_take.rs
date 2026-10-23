// A label read through an exclusive handle's reborrow cannot be held across
// a take, which frees it once the taken string drops.
use core::ffi::CStr;
use ffmpeg_bindings::libavutil::libavutil::opt::OptionRangeMut;

fn peek_then_take<'a>(mut m: OptionRangeMut<'a>) -> Option<&'a CStr> {
    let label = m.as_ref().label();
    drop(m.take_label());
    label
}

fn main() {}
