// The exclusive handle borrows the reference mutably: cloning it while the
// handle lives, which would make the handle's writes shared, is rejected.
use ffmpeg_bindings::libavutil::libavutil::intfloat_h::IntFloat64;
use ffmpeg_bindings::libavutil::libavutil::refstruct::RefStructArc;

fn main() {
    let mut a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
    let mut m = a.get_mut().unwrap();
    let b = a.clone();
    m.set_bits(1);
    drop(b);
}
