// The two exclusive coordinate handles each borrow the whole struct handle,
// so they cannot be held at once.
use ffmpeg_bindings::libavutil::libavutil::csp::CieXy;

fn main() {
    let mut v = CieXy::default();
    let mut m = v.as_mut();
    let mut x = m.x_mut();
    let mut y = m.y_mut();
    x.set_num(1);
    y.set_num(2);
}
