// An exclusive handle moves; it is not duplicated by use.
use ffmpeg::libavutil::intfloat::IntFloat32;

fn main() {
    let mut a = IntFloat32::default();
    let mut m = a.as_mut();
    let mut moved = m;
    moved.set_bits(1);
    m.set_bits(2);
}
