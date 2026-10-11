// A shared handle freezes the definition: no setter runs while it lives.
use ffmpeg::libavutil::opt::OptionArrayDef;

fn main() {
    let mut d = OptionArrayDef::new();
    let r = d.as_ref();
    d.as_mut().set_size_min(1);
    let _ = r.size_min();
}
