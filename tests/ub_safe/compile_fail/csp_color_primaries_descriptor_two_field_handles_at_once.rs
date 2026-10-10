// The white point and primaries handles cannot be held at once: each borrows
// the parent handle exclusively.
use ffmpeg::libavutil::csp::ColorPrimariesDescriptor;

fn main() {
    let mut d = ColorPrimariesDescriptor::default();
    let mut m = d.as_mut();
    let mut wp = m.white_point_mut();
    let mut p = m.primaries_mut();
    wp.x_mut().set_num(1);
    p.r_mut().x_mut().set_num(2);
}
