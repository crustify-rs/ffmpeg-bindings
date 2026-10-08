// A view of a range list cannot outlive the owner of its elements.
use ffibox::CSlice;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::opt::OptionRangesBox;

fn view(b: OptionRangesBox) -> CSlice<'static, *const ffi::AVOptionRange> {
    b.as_ref().ranges()
}

fn main() {}
