// A range's label borrows the list: replacing the label while the old one is
// still read is rejected, since the replacement frees it.
use ffmpeg_bindings::libavutil::libavutil::opt::OptionRangesBox;

fn reread(b: &mut OptionRangesBox) -> usize {
    let label = b.as_ref().get(0, 0).and_then(|r| r.label());
    if let Some(mut m) = b.as_mut().get_mut(0, 0) {
        m.set_label(None);
    }
    label.map_or(0, |l| l.count_bytes())
}

fn main() {}
