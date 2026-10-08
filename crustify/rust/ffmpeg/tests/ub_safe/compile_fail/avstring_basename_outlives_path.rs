// `basename` borrows its result from the path: it cannot outlive it.
use ffmpeg::libavutil::avstring::basename;

fn main() {
    let base;
    {
        let path = std::ffi::CString::new("dir/file").unwrap();
        base = basename(&path);
    }
    println!("{base:?}");
}
