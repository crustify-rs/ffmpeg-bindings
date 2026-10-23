// `dirname`'s result borrows the path buffer it cut, which stays borrowed.
use ffmpeg_bindings::libavutil::libavutil::avstring::dirname;

fn main() {
    let mut path = *b"dir/file\0";
    let d = dirname(&mut path).unwrap();
    path[0] = b'x';
    println!("{d:?}");
}
