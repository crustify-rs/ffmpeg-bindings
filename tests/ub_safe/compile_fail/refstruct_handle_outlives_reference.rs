// A payload handle borrows the reference it came from: dropping the
// reference first, possibly the last one, is rejected.
use ffmpeg::libavutil::intfloat::IntFloat64;
use ffmpeg::libavutil::refstruct::RefStructArc;

fn main() {
    let a = RefStructArc::<IntFloat64>::new_zeroed().unwrap();
    let r = a.as_ref();
    drop(a);
    let _ = r.bits();
}
