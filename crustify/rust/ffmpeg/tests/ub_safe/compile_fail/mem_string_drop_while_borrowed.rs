// Moving a `HeapString` into C while a `&str` view of it lives is rejected.
use ffmpeg::libavutil::mem::string_from_c_str;

fn main() {
    let s = string_from_c_str(c"held").unwrap();
    let v = s.to_str().unwrap();
    let _raw = s.into_raw();
    println!("{v}");
}
