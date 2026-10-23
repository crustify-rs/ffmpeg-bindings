// An exclusive handle cannot be taken while a shared one lives.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite_h::Alias64;

fn main() {
    let mut a = Alias64::default();
    let r = a.as_ref();
    a.as_mut().set_u64(2);
    println!("{}", r.u64());
}
