// An iterator over a dictionary cannot outlive its owner.
use ffmpeg_bindings::libavutil::libavutil::dict::{Dict, Iter};

fn entries() -> Iter<'static> {
    let d = Dict::from_entries([(c"a", c"1")]).unwrap();
    d.iter()
}

fn main() {
    let _ = entries().count();
}
