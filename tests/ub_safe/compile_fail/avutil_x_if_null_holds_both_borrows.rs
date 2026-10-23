// The result of `x_if_null` may be either argument, so both stay borrowed.
use ffmpeg_bindings::libavutil::libavutil::avutil_h::x_if_null;

fn main() {
    let a = Box::new(1u32);
    let b = Box::new(2u32);
    let r = x_if_null(Some(&*a), Some(&*b)).unwrap();
    drop(b);
    println!("{r}");
}
