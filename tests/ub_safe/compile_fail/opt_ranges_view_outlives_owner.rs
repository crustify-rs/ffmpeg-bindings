// A view of a range list cannot outlive the owner of its elements.
use ffmpeg_bindings::libavutil::libavutil::opt::{OptionRangesBox, RangeIter};

fn view(b: OptionRangesBox) -> RangeIter<'static> {
    b.as_ref().ranges()
}

fn main() {}
