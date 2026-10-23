// An exclusive handle is `Send`, but an unscoped thread may outlive the value
// it borrows, so the handle cannot be moved into one.
use ffmpeg_bindings::libavutil::libavutil::intfloat::IntFloat64;

fn main() {
    let mut v = IntFloat64::from_bits(1);
    let mut m = v.as_mut();
    let t = std::thread::spawn(move || m.set_bits(2));
    t.join().unwrap();
}
