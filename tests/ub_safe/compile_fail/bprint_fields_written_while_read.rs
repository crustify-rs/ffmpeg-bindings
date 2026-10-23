// A value cannot be written through an exclusive handle while a shared handle
// to it is still used.
use ffmpeg_bindings::libavutil::libavutil::bprint::BPrintFields;

fn main() {
    let mut f = BPrintFields::new();
    let r = f.as_ref();
    f.as_mut().set_len(1);
    println!("{}", r.len());
}
