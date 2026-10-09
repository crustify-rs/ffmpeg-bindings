// The string `BPrintFieldsRef::str` returns is borrowed through the value, so
// it cannot outlive it.
use ffmpeg::libavutil::bprint::BPrintFields;

fn main() {
    let s = {
        let mut f = BPrintFields::new();
        f.as_mut().set_str(Some(c"so far"));
        f.as_ref().str()
    };
    println!("{s:?}");
}
