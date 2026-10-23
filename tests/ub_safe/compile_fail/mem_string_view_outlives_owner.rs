// A view borrowed from a `HeapString` cannot outlive the owner that frees it.
use ffmpeg_bindings::libavutil::libavutil::mem::string_from_c_str;

fn main() {
    let v = {
        let s = string_from_c_str(c"gone").unwrap();
        s.as_bytes()
    };
    println!("{v:?}");
}
