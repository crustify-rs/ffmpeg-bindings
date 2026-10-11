// A range handle cannot outlive the list that owns the range.
use ffmpeg::libavutil::opt::{OptionRangeRef, OptionRangesBox};

fn first(b: OptionRangesBox) -> Option<OptionRangeRef<'static>> {
    b.as_ref().get(0, 0)
}

fn main() {}
