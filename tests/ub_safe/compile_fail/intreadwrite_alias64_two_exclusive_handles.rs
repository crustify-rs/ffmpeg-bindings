// Two exclusive handles to one `Alias64` cannot coexist.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::Alias64;

fn main() {
    let mut a = Alias64::default();
    let mut m1 = a.as_mut();
    let mut m2 = a.as_mut();
    m1.set_u64(1);
    m2.set_u64(2);
}
