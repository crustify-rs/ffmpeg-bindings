// Dropping a range list while a view of it lives is rejected.
use ffmpeg::libavutil::opt::OptionRangesBox;

fn count(b: OptionRangesBox) -> usize {
    let view = b.as_ref().ranges();
    drop(b);
    view.len()
}

fn main() {}
