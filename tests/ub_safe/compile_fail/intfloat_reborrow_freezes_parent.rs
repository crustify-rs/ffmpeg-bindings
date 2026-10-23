// A reborrowed exclusive handle freezes its parent while it lives.
use ffmpeg_bindings::libavutil::libavutil::intfloat::IntFloat32;

fn main() {
    let mut a = IntFloat32::default();
    let mut m = a.as_mut();
    let mut inner = m.as_mut();
    m.set_bits(1);
    inner.set_float(2.0);
}
