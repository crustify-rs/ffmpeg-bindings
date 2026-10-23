// An `Alias16` cannot be written while a shared handle to it is live.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::Alias16;

fn main() {
    let mut a = Alias16::from_u16(1);
    let r = a.as_ref();
    a.as_mut().set_u16(2);
    println!("{}", r.u16());
}
