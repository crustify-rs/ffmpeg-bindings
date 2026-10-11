// A dictionary box hands out its exclusive handle only while no shared one
// lives.
use ffmpeg::libavutil::dict::Dict;

fn main() {
    let mut b = Dict::from_entries([(c"k", c"v")]).unwrap().take().unwrap();
    let r = b.as_ref();
    let _m = b.as_mut();
    let _ = r;
}
