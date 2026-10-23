// Storing a string of any lifetime is `unsafe`: the caller must keep it alive
// while the value and its copies can read it.
use ffmpeg_bindings::libavutil::libavutil::bprint::BPrintFields;

fn main() {
    let mut f = BPrintFields::new();
    f.as_mut().set_str_borrowed(Some(c"so far"));
}
