// A destination bound into scan arguments cannot be read until they are gone.
use ffmpeg::libavutil::avstring::{ScanArg, scan};

fn main() {
    let mut n = 0i32;
    let mut args: [ScanArg<'_>; 1] = [(&mut n).into()];
    println!("{n}");
    let _ = scan(c"1", c"%d", &mut args);
}
