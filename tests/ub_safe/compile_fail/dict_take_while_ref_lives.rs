// Taking the dictionary out (which a writer may then free) needs the owner
// exclusively: not while a shared handle to it lives.
use ffmpeg::libavutil::dict::Dict;

fn main() {
    let mut d = Dict::from_entries([(c"k", c"v")]).unwrap();
    let r = d.as_ref();
    drop(d.take());
    let _ = r;
}
