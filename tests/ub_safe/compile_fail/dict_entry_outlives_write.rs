// An entry borrowed from a dictionary cannot be read after a write, which
// may move or free it.
use ffmpeg::libavutil::dict::{Dict, Flags};

fn main() {
    let mut d = Dict::from_entries([(c"a", c"1")]).unwrap();
    let e = d.get(c"a", Flags::NONE).unwrap();
    d.set(c"a", c"2", Flags::NONE).unwrap();
    let _ = e.value();
}
