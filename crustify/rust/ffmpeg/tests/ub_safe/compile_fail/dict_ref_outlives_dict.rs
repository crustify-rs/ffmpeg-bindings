// A dictionary's shared handle borrows it: the dictionary cannot drop while
// the handle lives.
use ffmpeg::libavutil::dict::Dict;

fn main() {
    let d = Dict::from_entries([(c"k", c"v")]).unwrap();
    let r = d.as_ref();
    drop(d);
    let _ = r;
}
