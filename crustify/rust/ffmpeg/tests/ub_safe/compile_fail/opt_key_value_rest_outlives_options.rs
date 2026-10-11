// The rest of a parsed options string borrows it: it cannot outlive it.
use ffmpeg::libavutil::opt::parse_key_value;

fn main() {
    let rest;
    {
        let opts = std::ffi::CString::new("a=1:b=2").unwrap();
        rest = parse_key_value(&opts, c"=", c":", false).unwrap().rest;
    }
    println!("{rest:?}");
}
