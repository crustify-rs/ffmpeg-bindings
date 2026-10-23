// The exclusive result of `x_if_null_mut` keeps both arguments borrowed: the
// one not returned cannot be read while the result is alive.
use ffmpeg_bindings::libavutil::libavutil::avutil::x_if_null_mut;

fn main() {
    let mut a = 1u32;
    let mut b = 2u32;
    let r = x_if_null_mut(Some(&mut a), Some(&mut b)).unwrap();
    println!("{a}");
    *r += 1;
}
