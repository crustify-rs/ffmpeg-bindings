// The exclusive handle is move-only, so two never edit one definition.
use ffmpeg::libavutil::opt::OptionArrayDef;

fn main() {
    let mut d = OptionArrayDef::new();
    let m = d.as_mut();
    let mut a = m;
    let mut b = m;
    a.set_size_min(1);
    b.set_size_min(2);
}
