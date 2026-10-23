// Two exclusive range handles of one list cannot coexist, even for two
// different ranges.
use ffmpeg_bindings::libavutil::libavutil::opt::OptionRangesBox;

fn both(b: &mut OptionRangesBox) {
    let mut m = b.as_mut();
    let first = m.get_mut(0, 0);
    let second = m.get_mut(1, 0);
    drop((first, second));
}

fn main() {}
